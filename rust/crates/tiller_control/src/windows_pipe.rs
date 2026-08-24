//! The Windows counterpart of the unix-domain-socket transport: a named
//! pipe (`CreateNamedPipeW`/`ConnectNamedPipe`) carrying the exact same
//! line-delimited JSON protocol. Only this file knows which platform it is
//! on; [`crate::server`] and [`crate::client`] call into these types behind
//! their `#[cfg]` seams.
//!
//! # Why a named pipe is not a drop-in for `UnixListener`
//!
//! One server pipe *instance* serves exactly one client at a time — there
//! is no kernel accept queue like a listening socket's backlog. The
//! accept loop therefore keeps one spare instance bound at all times: when
//! a client connects, the connected instance is handed to a worker thread
//! and a fresh instance is created before the loop waits again, so
//! concurrent clients are served exactly as on unix (one thread per
//! connection).
//!
//! All handles are opened `FILE_FLAG_OVERLAPPED` because the accept loop
//! must poll `ConnectNamedPipe` against the shutdown flag (the unix
//! listener polls `accept` non-blocking for the same reason); every read
//! and write consequently goes through the same overlapped plumbing.
//!
//! # Pipe naming (the mapping from the unix world)
//!
//! Callers hand us the same path they would on Linux/macOS: `$TILLER_SOCKET`
//! verbatim when the environment overrides it, otherwise the platform
//! default from [`crate::protocol::default_socket_path`] (XDG runtime/state
//! directories). Both are mapped deterministically onto the named-pipe
//! namespace, because NT pipe names cannot be arbitrary filesystem paths:
//!
//! - a path already under `\.\pipe\` is used as-is;
//! - anything else becomes `\.\pipe\TillerRust\<user-SID>-<sanitized>-<fnv1a64>`,
//!   where `<user-SID>` scopes the name to the calling user, `<sanitized>`
//!   is the path with characters illegal in a pipe name replaced, and
//!   `<fnv1a64>` is a hash of the *full* path, keeping distinct paths
//!   distinct even where sanitization collides, and keeping the name under
//!   the 256-character NT pipe-name limit.
//!
//! The user-SID component is load-bearing, not cosmetic. Without it the
//! default path (`/tmp/TillerRust/control.sock` when no XDG/HOME env is
//! set — the norm for GUI processes) derives a name that is identical for
//! every user on the machine and guessable from the source: an attacker
//! could pre-create it and silently harvest every Layer-A hook payload
//! from victims' `tillerctl notify`, while Tiller itself would fail to
//! bind and misread the collision as "another instance running". With the
//! SID in the name, users no longer contend for one name at all (a second
//! user's Tiller just works), and nobody can pre-squat a name they cannot
//! derive for their victim.
//!
//! `$TILLER_SOCKET=/tmp/x/control.sock` therefore lands on the same pipe
//! for the app and for `tillerctl` *of the same user*, whatever their
//! working directories — the override semantics carry over unchanged from
//! unix. An explicit `\.\pipe\…` override passes through verbatim and
//! is the operator's own responsibility, exactly as on unix.

use std::cell::Cell;
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GENERIC_READ, GENERIC_WRITE, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
    LocalFree, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetKernelObjectSecurity, GetSecurityDescriptorDacl,
    GetTokenInformation, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, RevertToSelf,
    SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, OPEN_EXISTING,
    PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT, WriteFile,
};
use windows_sys::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, ImpersonateNamedPipeClient,
    PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows_sys::Win32::System::SystemServices::{ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
    ResetEvent, WaitForSingleObject,
};

/// Everything the named-pipe namespace allows: 256 UTF-16 units including
/// the `\.\pipe\` prefix. Our derived names stay comfortably below it.
const MAX_PIPE_NAME_CHARS: usize = 256;

/// Prefix for derived pipe names — the TillerRust directory name carried
/// over from the unix default path (`$XDG_RUNTIME_DIR/TillerRust/…`).
const PIPE_NAMESPACE: &str = r"\\.\pipe\TillerRust\";

// SAFETY: a HANDLE is an opaque integer, not a pointer into anyone's
// memory — moving these types across threads is exactly what the accept/
// worker split does on unix with socket fds (which std marks Send).
unsafe impl Send for PipeStream {}
unsafe impl Send for PipeListener {}

/// How often the accept wait wakes to look at the shutdown flag — the same
/// cadence as the unix listener's non-blocking poll.
const POLL_WAIT_MS: u32 = 5;

// Windows error codes we branch on, spelled out because importing dozens of
// Foundation constants for one use each buys nothing.
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_BROKEN_PIPE: i32 = 109;
const ERROR_PIPE_BUSY: i32 = 231;
const ERROR_PIPE_NOT_CONNECTED: i32 = 233;
const ERROR_PIPE_CONNECTED: i32 = 535;
const ERROR_IO_PENDING: i32 = 997;

fn last_error(context: &str) -> io::Error {
    let code = unsafe { GetLastError() } as i32;
    if code != 0 {
        io::Error::from_raw_os_error(code)
    } else {
        io::Error::other(format!("{context} failed"))
    }
}

/// Why [`PipeListener::bind`] refused to create the pipe.
#[derive(Debug)]
pub(crate) enum ListenerError {
    /// A live server owns this name — the probe connect succeeded, exactly
    /// mirroring the unix `prepare_path` refusal to steal a live socket.
    AlreadyRunning { name: String },
    /// Creation failed for any other reason: the name is squatted by a
    /// process that did not grant us access, the security descriptor could
    /// not be built, or the OS denied the create.
    Bind { detail: String },
}

/// Why a client connect failed. The foreign-owner case gets its own
/// variant because it deserves its own message ("the name is held by a
/// pipe created by someone else") and its own classification at bind time
/// (loud hostile collision, never "another instance is already running").
#[derive(Debug)]
pub(crate) enum ClientConnectError {
    /// The pipe exists but was created by — is owned by — another user:
    /// a hostile squatter or another user's Tiller session. Never talk to
    /// it, whatever its DACL happens to grant us.
    ForeignOwner { name: String, owner: String },
    /// No server, busy timeout, access denied, … — ordinary connect noise.
    Io(io::Error),
}

impl std::fmt::Display for ClientConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientConnectError::ForeignOwner { name, owner } => write!(
                f,
                "control pipe {name} exists but is owned by another user ({owner}); refusing to talk to it"
            ),
            ClientConnectError::Io(error) => write!(f, "{error}"),
        }
    }
}

/// Maps the unix-style socket path onto the named-pipe namespace. See the
/// module doc comment for the full mapping rationale — including why the
/// current user's SID is part of every derived name.
///
/// Pure and deterministic: the app and `tillerctl` derive the same name
/// from the same path with no shared state, which is what makes
/// `$TILLER_SOCKET` overrides work identically on both sides. Fails only
/// when the process token cannot be read — refuse closed: a name derived
/// without the user component would silently reintroduce the machine-global
/// predictability the SID exists to prevent.
pub(crate) fn pipe_name_for_path(path: &Path) -> Result<String, String> {
    let text = path.to_string_lossy().into_owned();
    if text.to_ascii_lowercase().starts_with(r"\\.\pipe\") {
        return Ok(text);
    }
    let user = current_user_sid_string()?;

    // FNV-1a over the full path: sanitization alone is not injective
    // (`a/b` and `a_b` would collide), so every derived name carries the
    // hash of the unsanitized path. 64 bits is ample for per-user local
    // names, and FNV is chosen precisely because it is stable everywhere —
    // unlike `DefaultHasher`, whose algorithm Rust may change between
    // releases (a changed hash would split the app and tillerctl onto
    // different pipes until both were rebuilt).
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }

    // Pipe names cannot contain `/` or `\` beyond the prefix, and `:` would
    // parse as an NT alternate-data-stream separator; everything outside
    // the safe set collapses to `_`. Truncated to leave room for the user
    // SID and the hash suffix under MAX_PIPE_NAME_CHARS.
    let mut sanitized: String = text
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
    let hash_len = format!("{hash:016x}").len();
    let budget = MAX_PIPE_NAME_CHARS
        - PIPE_NAMESPACE.len()
        - user.len()
        - 2 // separators around the sanitized path
        - hash_len;
    sanitized.truncate(budget);

    Ok(format!("{PIPE_NAMESPACE}{user}-{sanitized}-{hash:016x}"))
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// The current process token's user SID, rendered as an SDDL string
/// (`S-1-5-21-…`). Cached: the user cannot change mid-process.
fn current_user_sid_string() -> Result<String, String> {
    static USER_SID: OnceLock<Result<String, String>> = OnceLock::new();

    USER_SID
        .get_or_init(|| unsafe {
            let process = GetCurrentProcess();
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return Err("OpenProcessToken failed".to_string());
            }
            // Already inside this closure's unsafe scope.
            let result = token_user_sid_string(token);
            CloseHandle(token);
            result
        })
        .clone()
}

/// Reads the `TokenUser` SID out of an already-open token as an SDDL string.
unsafe fn token_user_sid_string(token: HANDLE) -> Result<String, String> {
    // Two-call pattern: query the required size, then fetch.
    let mut needed = 0u32;
    unsafe {
        GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut needed);
    }
    if needed == 0 {
        return Err("could not size the token's user information".to_string());
    }
    let mut buffer = vec![0u8; needed as usize];
    if unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err("GetTokenInformation(TokenUser) failed".to_string());
    }
    let user = unsafe { &*(buffer.as_ptr() as *const TOKEN_USER) };
    if user.User.Sid.is_null() {
        return Err("token carries no user SID".to_string());
    }

    let mut string_sid: *mut u16 = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut string_sid) } == 0 {
        return Err("ConvertSidToStringSidW failed".to_string());
    }
    let result = unsafe { string_from_wide(string_sid) };
    unsafe { LocalFree(string_sid.cast()) };
    Ok(result)
}

unsafe fn string_from_wide(pointer: *const u16) -> String {
    let mut len: usize = 0;
    while unsafe { *pointer.add(len) } != 0 {
        len += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(pointer, len) })
}

/// The owner SID of any kernel object, as an SDDL string.
///
/// A kernel object's owner is the user whose token created it, minted by
/// the kernel at creation time and unchangeable without
/// SeTakeOwnershipPrivilege — which is exactly what makes this check
/// unforgeable for the client side: a squatter cannot create a pipe that
/// *verifies as* victim-owned, so verifying ownership before writing a
/// single byte authenticates the server without touching the protocol.
/// This is the mirror image of the server-side [`peer_is_owner`], which
/// authenticates the client the same way.
fn pipe_owner_sid_string(handle: HANDLE) -> Result<String, String> {
    let mut owner: PSID = std::ptr::null_mut();
    let mut descriptor: isize = 0;
    // GetSecurityInfo returns a WIN32_ERROR (0 = success), not a BOOL, and
    // allocates the descriptor backing the returned SID — freed below once
    // the SID has been rendered to its canonical string form.
    let code = unsafe {
        GetSecurityInfo(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor as *mut isize as *mut PSECURITY_DESCRIPTOR,
        )
    };
    if code != 0 {
        return Err(format!("GetSecurityInfo failed with error {code}"));
    }
    let result = (|| {
        if owner.is_null() {
            return Err("object carries no owner SID".to_string());
        }
        let mut string_sid: *mut u16 = std::ptr::null_mut();
        if unsafe { ConvertSidToStringSidW(owner, &mut string_sid) } == 0 {
            return Err("ConvertSidToStringSidW failed".to_string());
        }
        let text = unsafe { string_from_wide(string_sid) };
        unsafe { LocalFree(string_sid.cast()) };
        Ok(text)
    })();
    if descriptor != 0 {
        unsafe { LocalFree(descriptor as *mut core::ffi::c_void) };
    }
    result
}

/// Whether an object's owner SID equals the expected one. Split out pure so
/// the refusal decision itself is unit-testable without a second OS user.
fn ensure_owner_is(actual: &str, expected: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "owner is {actual}, expected {expected} (created by another user)"
        ))
    }
}

/// Verifies the pipe we just opened was created by the current user.
/// Fail closed: unreadable ownership counts as mismatch.
fn verify_pipe_owner(handle: HANDLE) -> Result<(), String> {
    let actual = pipe_owner_sid_string(handle)?;
    let expected = current_user_sid_string()?;
    ensure_owner_is(&actual, &expected)
}

/// Builds the SECURITY_ATTRIBUTES handed to `CreateNamedPipeW`, together
/// with the SDDL-produced descriptor that must outlive the create call
/// (free it via [`DescriptorLease`] afterwards).
///
/// This is the Windows analogue of the unix bind-time chmod-to-0600, and it
/// is strictly stronger than the unix sequence: the DACL is part of the very
/// call that creates the object, so — unlike a unix socket, whose file sits
/// at the umask-derived mode between `bind` and `chmod` — there is no window
/// in which the pipe exists in a permissive state, and nothing to race.
/// The SDDL grants GENERIC_ALL to the current user and to SYSTEM only;
/// `P` marks the DACL protected, so inheritable ACEs from parent objects
/// cannot widen it (the analogue of refusing permissive group/other bits).
fn restricted_security_attributes() -> Result<(SECURITY_ATTRIBUTES, isize), String> {
    let user = current_user_sid_string()
        .map_err(|error| format!("could not determine the current user's SID: {error}"))?;
    // SY is LOCAL_SYSTEM (which launches services that may legitimately
    // administer the session); nobody else gets even READ_CONTROL.
    let sddl = format!("D:P(A;;GA;;;{user})(A;;GA;;;SY)");
    let sddl_wide = wide(&sddl);

    let mut descriptor: isize = 0;
    let ok = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl_wide.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor as *mut isize as *mut PSECURITY_DESCRIPTOR,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(format!("SDDL conversion failed for `{sddl}`"));
    }

    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor as *mut core::ffi::c_void,
        bInheritHandle: 0,
    };
    Ok((attributes, descriptor))
}

/// Frees the SDDL-produced security descriptor once every
/// `CreateNamedPipeW` call has consumed its `SECURITY_ATTRIBUTES`.
struct DescriptorLease(isize);

impl Drop for DescriptorLease {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { LocalFree(self.0 as *mut core::ffi::c_void) };
        }
    }
}

/// Verifies the *created* pipe object's DACL matches what we asked for:
/// present, granting only the current user and LOCAL_SYSTEM, with no deny
/// ACEs.
///
/// This mirrors the unix `post_bind_sanity_check` (stat the bound path:
/// must be a socket owned by the current user with a non-permissive mode).
/// On unix the check exists because the mode is set *after* creation by a
/// separate chmod; here the DACL was supplied atomically at creation, but
/// the check stays as belt-and-braces: it catches an OS/filesystem layer
/// quietly substituting a different descriptor, and it is what a future
/// refactor that drops the explicit SD would have to defeat.
///
/// Parity note: unix additionally verifies the file *type* is a socket.
/// There is no comparable "did someone swap the object under us" exposure
/// here — `FILE_FLAG_FIRST_PIPE_INSTANCE` guarantees we created this exact
/// name — so that half has no Windows failure mode to detect.
pub(crate) fn post_bind_sanity_check(pipe: HANDLE) -> Result<(), String> {
    let user = current_user_sid_string()?;

    let mut needed = 0u32;
    unsafe {
        GetKernelObjectSecurity(
            pipe,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    if needed == 0 {
        return Err("could not size the pipe's security descriptor".to_string());
    }
    let mut buffer = vec![0u8; needed as usize];
    if unsafe {
        GetKernelObjectSecurity(
            pipe,
            DACL_SECURITY_INFORMATION,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err("GetKernelObjectSecurity failed".to_string());
    }
    let descriptor = buffer.as_ptr() as PSECURITY_DESCRIPTOR;

    let mut present = 0;
    let mut defaulted = 0;
    let mut dacl: *mut ACL = std::ptr::null_mut();
    if unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }
        == 0
        || present == 0
        || dacl.is_null()
    {
        return Err("pipe has no DACL".to_string());
    }

    walk_dacl(unsafe { &*dacl }, &[&user, "S-1-5-18"])
}

/// Walks an ACL and asserts every allow/deny ACE grants only one of
/// `trusted_trustees`. A free function so tests can exercise the parser.
fn walk_dacl(acl: &ACL, trusted_trustees: &[&str]) -> Result<(), String> {
    let header_size = 4; // ACE_HEADER: type, flags, size
    let mut ace = unsafe { (acl as *const ACL as *const u8).add(std::mem::size_of::<ACL>()) };
    for _ in 0..acl.AceCount {
        let ace_type = unsafe { *ace };
        let ace_size = unsafe { *ace.add(2) as usize } | ((unsafe { *ace.add(3) } as usize) << 8);
        if ace_size < header_size + 4 + 4 {
            // Smaller than header + mask + a minimal SID: malformed.
            return Err("malformed ACE (too small)".to_string());
        }
        match ace_type as u32 {
            ACCESS_ALLOWED_ACE_TYPE => {
                let trustee = unsafe { trustee_string_of(ace) }?;
                if !trusted_trustees.contains(&trustee.as_str()) {
                    return Err(format!("unexpected grant ACE for {trustee}"));
                }
            }
            ACCESS_DENIED_ACE_TYPE => {
                // Deny ACEs are unexpected by construction; treat them as
                // tampering rather than trying to interpret them.
                let trustee = unsafe { trustee_string_of(ace) }?;
                return Err(format!("unexpected deny ACE for {trustee}"));
            }
            // System-audit / label-style ACEs do not widen access; leave
            // them alone.
            _ => {}
        }
        ace = unsafe { ace.add(ace_size) };
    }
    Ok(())
}

/// The SDDL form of the trustee SID of an allow/deny ACE starting at
/// `ace` (layout: header 4 bytes + mask 4 bytes + SidStart).
unsafe fn trustee_string_of(ace: *const u8) -> Result<String, String> {
    let sid = unsafe { ace.add(8) } as PSID;
    let mut string_sid: *mut u16 = std::ptr::null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut string_sid) } == 0 {
        return Err("an ACE trustee could not be resolved".to_string());
    }
    let result = unsafe { string_from_wide(string_sid) };
    unsafe { LocalFree(string_sid.cast()) };
    Ok(result)
}

/// Creates one pipe server instance carrying `security`. Every instance is
/// its own kernel object, so *each* one must be created behind the
/// restricted DACL — restricting only the first would leave later instances
/// at the default (machine-wide accessible) descriptor. `first` adds
/// `FILE_FLAG_FIRST_PIPE_INSTANCE`: the create *fails* if the name already
/// exists, which is what makes squatting the control-pipe name impossible
/// for anyone who got here first — the analogue of the unix stale/live
/// socket takeover dance, minus the stale case: a pipe object dies with the
/// process that owns it, so a crashed previous run leaves nothing behind to
/// clean up.
fn create_instance(
    name_wide: &[u16],
    security: &SECURITY_ATTRIBUTES,
    first: bool,
) -> io::Result<HANDLE> {
    let mut open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // Byte-mode + PIPE_WAIT: the framing code (line scan, cap, dispatch)
    // is byte-oriented exactly like the unix stream implementation, so the
    // protocol behaves identically on both transports. Buffers are sized
    // above the largest single request line the protocol accepts, so a
    // writer can never stall on buffer backpressure mid-line.
    let handle = unsafe {
        CreateNamedPipeW(
            name_wide.as_ptr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            crate::server::MAX_BUFFER_BYTES as u32,
            crate::server::MAX_BUFFER_BYTES as u32,
            0,
            security,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(last_error("CreateNamedPipeW"));
    }
    Ok(handle)
}

/// The control pipe for `path`, holding the first spare instance.
pub(crate) struct PipeListener {
    name_wide: Vec<u16>,
    security: SECURITY_ATTRIBUTES,
    _lease: DescriptorLease,
    /// The spare instance waiting for the next client.
    instance: HANDLE,
}

impl PipeListener {
    /// The handle of the first (spare) instance, for the post-bind DACL
    /// sanity check — the object the server will actually serve on.
    pub(crate) fn instance_handle(&self) -> HANDLE {
        self.instance
    }

    /// Creates the pipe with a restricted DACL and first-instance
    /// exclusivity, distinguishing a live sibling Tiller from a hostile
    /// name squatter exactly like the unix `prepare_path` does with its
    /// probe connect.
    pub(crate) fn bind(path: &Path) -> Result<Self, ListenerError> {
        let name = pipe_name_for_path(path).map_err(|detail| ListenerError::Bind { detail })?;
        let name_wide = wide(&name);
        let (security, descriptor) =
            restricted_security_attributes().map_err(|detail| ListenerError::Bind { detail })?;
        let lease = DescriptorLease(descriptor);

        match create_instance(&name_wide, &security, true) {
            Ok(instance) => Ok(Self {
                name_wide,
                security,
                _lease: lease,
                instance,
            }),
            Err(error) => {
                // ERROR_ACCESS_DENIED with FIRST_PIPE_INSTANCE means the
                // name already exists and we were not allowed to replace
                // it. Probe it as a client: if we can connect, a live
                // server owns it (refuse to steal, like unix); if we
                // cannot, some process created a pipe on this name with a
                // DACL that locks us out — report that loudly rather than
                // silently serving nobody.
                if error.raw_os_error() == Some(ERROR_ACCESS_DENIED) {
                    match open_client(path) {
                        // Probe connected AND the pipe verifies as ours: a
                        // live sibling Tiller — or a same-user squatter,
                        // which is indistinguishable from a sibling within
                        // one uid, exactly as on unix where a same-uid
                        // process can bind the socket file first. Refuse to
                        // steal, like unix.
                        Ok(_) => Err(ListenerError::AlreadyRunning { name }),
                        // The probe reached a pipe owned by ANOTHER user:
                        // hostile squat or another user's session. This must
                        // never be reported as "already running" — the app
                        // would start silently socket-less while agents talk
                        // to the impostor. Loud failure instead.
                        Err(ClientConnectError::ForeignOwner { owner, .. }) => {
                            Err(ListenerError::Bind {
                                detail: format!(
                                    "pipe name {name} is held by a pipe owned by \
                                     another user ({owner}) — hostile name collision; refusing"
                                ),
                            })
                        }
                        Err(probe_error @ ClientConnectError::Io(_)) => Err(ListenerError::Bind {
                            detail: format!(
                                "pipe name {name} is held by another process \
                                 that denies us access (probe connect: {probe_error})"
                            ),
                        }),
                    }
                } else {
                    Err(ListenerError::Bind {
                        detail: error.to_string(),
                    })
                }
            }
        }
    }

    /// Waits for the next client, hands back the connected stream, and
    /// binds a fresh spare instance. Returns `None` when shut down (or
    /// when a fresh instance can no longer be created — the analogue of a
    /// unix listener whose descriptor went bad).
    pub(crate) fn accept(&mut self, shutdown: &AtomicBool) -> Option<PipeStream> {
        loop {
            match self.wait_for_connection(shutdown) {
                WaitOutcome::Connected(stream) => {
                    // Replace the consumed instance before returning, so
                    // there is no window without a spare (a second client
                    // connecting while the first is being dispatched waits
                    // microseconds instead of failing).
                    match create_instance(&self.name_wide, &self.security, false) {
                        Ok(next) => self.instance = next,
                        Err(_) => {
                            // Serve the client we already have; the next
                            // accept call reports the dead listener.
                            self.instance = INVALID_HANDLE_VALUE;
                        }
                    }
                    return Some(stream);
                }
                WaitOutcome::ForeignPeer => {
                    // Same policy as the unix accept loop: refuse and keep
                    // serving. The instance is reset for the next client.
                    unsafe { DisconnectNamedPipe(self.instance) };
                }
                WaitOutcome::Error(error) => {
                    // Recreate the spare; if the OS refuses, stop
                    // accepting — matching a unix listener that starts
                    // permanently failing accept().
                    match create_instance(&self.name_wide, &self.security, false) {
                        Ok(next) => self.instance = next,
                        Err(_) => {
                            let _ = error;
                            return None;
                        }
                    }
                }
                WaitOutcome::Shutdown => return None,
            }
        }
    }

    /// Overlapped `ConnectNamedPipe`, polled against the shutdown flag at
    /// the same cadence as the unix accept loop.
    fn wait_for_connection(&mut self, shutdown: &AtomicBool) -> WaitOutcome {
        let event = unsafe {
            CreateEventW(
                std::ptr::null(),
                1, // manual reset: the cancelled completion may signal late
                0,
                std::ptr::null(),
            )
        };
        if event.is_null() || event == INVALID_HANDLE_VALUE {
            return WaitOutcome::Error(last_error("CreateEventW"));
        }
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = event;

        let outcome = unsafe { ConnectNamedPipe(self.instance, &mut overlapped) };
        let mut connected = outcome != 0; // client connected synchronously
        let mut error: Option<io::Error> = None;
        if !connected {
            match last_error("ConnectNamedPipe").raw_os_error() {
                Some(code) if code == ERROR_IO_PENDING => loop {
                    if shutdown.load(Ordering::SeqCst) {
                        // Cancel the pending wait before tearing down, and
                        // drain the completion so the kernel state of this
                        // instance is well-defined afterwards.
                        unsafe { CancelIoEx(self.instance, &overlapped) };
                        let mut transferred = 0u32;
                        unsafe {
                            GetOverlappedResult(self.instance, &overlapped, &mut transferred, 1);
                            ResetEvent(event);
                        }
                        unsafe { CloseHandle(event) };
                        return WaitOutcome::Shutdown;
                    }
                    match unsafe { WaitForSingleObject(event, POLL_WAIT_MS) } {
                        WAIT_OBJECT_0 => {
                            connected = true;
                            break;
                        }
                        WAIT_TIMEOUT => continue,
                        status => {
                            error = Some(io::Error::other(format!(
                                "WaitForSingleObject returned {status:#x}"
                            )));
                            break;
                        }
                    }
                },
                Some(code) if code == ERROR_PIPE_CONNECTED => {
                    // A client slipped in between CreateNamedPipeW and
                    // ConnectNamedPipe — the good kind of race.
                    connected = true;
                }
                Some(code) => error = Some(io::Error::from_raw_os_error(code)),
                None => error = Some(last_error("ConnectNamedPipe")),
            }
        }

        unsafe { CloseHandle(event) };
        let Some(error) = error else {
            debug_assert!(connected);
            return match wrap_stream(self.instance) {
                Some(stream) => {
                    if peer_is_owner(stream.handle) {
                        WaitOutcome::Connected(stream)
                    } else {
                        // A foreign identity reached the pipe however it
                        // managed to — the DACL should have kept it out,
                        // so treat every failure here as hostile and drop
                        // the connection, exactly like the unix accept
                        // loop drops a peer whose uid differs.
                        WaitOutcome::ForeignPeer
                    }
                }
                None => WaitOutcome::Error(io::Error::other("pipe stream wrap failed")),
            };
        };
        WaitOutcome::Error(error)
    }
}

impl Drop for PipeListener {
    fn drop(&mut self) {
        if self.instance != INVALID_HANDLE_VALUE && !self.instance.is_null() {
            unsafe {
                DisconnectNamedPipe(self.instance);
                CloseHandle(self.instance);
            }
        }
    }
}

enum WaitOutcome {
    Connected(PipeStream),
    ForeignPeer,
    Error(io::Error),
    Shutdown,
}

/// Whether the client at the far end of `pipe` runs as this process's own
/// user — the Windows `peer_is_owner`.
///
/// Mechanism: the connecting client must open the pipe with the
/// `SECURITY_IDENTIFICATION` SQOS level (our client does; see
/// [`open_client`]), which authorizes the server to *identify* — but not
/// act as — the client. `ImpersonateNamedPipeClient` binds the server
/// thread to the client's actual communication-session identity, and the
/// thread token's user SID is compared against our own process-token SID.
///
/// This is the direct analogue of `SO_PEERCRED`/`getpeereid`: the kernel
/// vouches for who is on the other end, independent of any file-mode/DACL
/// gate. It is deliberately *not* implemented via
/// `GetNamedPipeClientProcessId` + `OpenProcess(pid)`: between learning a
/// PID and opening it, the client can exit and an owner-user process can
/// be assigned the recycled PID — a TOCTOU the credential-based check
/// below does not have, because the impersonated token comes from the
/// pipe session itself.
///
/// Any failure (impersonation refused, token unreadable) counts as
/// not-owner: refuse closed, like every unix variant.
fn peer_is_owner(pipe: HANDLE) -> bool {
    unsafe {
        if ImpersonateNamedPipeClient(pipe) == 0 {
            return false;
        }
        let verified = verify_impersonated_identity();
        // Never leave the thread running as the client, whatever happened.
        RevertToSelf();
        verified
    }
}

/// Compares the current thread's (impersonated) token user SID against the
/// process token's. Must be called while impersonating.
unsafe fn verify_impersonated_identity() -> bool {
    let mut token: HANDLE = std::ptr::null_mut();
    // OpenAsSelf stays 0 deliberately: we want the impersonation context's
    // token, not the process's own.
    if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 0, &mut token) } == 0 {
        return false;
    }
    let client = unsafe { token_user_sid_string(token) };
    unsafe { CloseHandle(token) };

    match (client, current_user_sid_string()) {
        (Ok(client), Ok(us)) => client == us,
        _ => false,
    }
}

/// Opens the pipe as a client would, with `SECURITY_IDENTIFICATION` SQOS so
/// the server can verify who we are (see [`peer_is_owner`]). Handles
/// `ERROR_PIPE_BUSY` by waiting, like every well-behaved pipe client.
///
/// Before a single byte is written, the opened pipe object's OWNER is
/// verified against the current user ([`verify_pipe_owner`]). A pipe's
/// DACL only says who may connect to the object its creator made — it says
/// nothing about *who the creator was*, and a squatter happily grants the
/// whole world access. Ownership is the one creator identity the kernel
/// vouches for, so this is what turns "we connected" into "we connected to
/// our own Tiller". No TOCTOU either: we hold a handle to that exact
/// object, and its owner cannot change without SeTakeOwnershipPrivilege.
// `pub(crate)` rather than `pub`: it returns [`ClientConnectError`], which is
// crate-private because the ForeignOwner/Io distinction is an implementation
// detail of this transport. `client.rs` is the only caller and it flattens
// that distinction into `io::Error` for the outside world.
pub(crate) fn open_client(path: &Path) -> Result<PipeStream, ClientConnectError> {
    let name = pipe_name_for_path(path)
        .map_err(|error| ClientConnectError::Io(io::Error::other(error)))?;
    let name_wide = wide(&name);

    // Bounded retry: a busy pipe means the server's spare instance is
    // momentarily taken; WaitNamedPipeW releases us when the next instance
    // appears (or after its own timeout, whichever first). Two seconds
    // covers any realistic dispatch delay; past that we report the busy
    // error instead of hanging tillerctl.
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let handle = unsafe {
            CreateFileW(
                name_wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0, // share mode: exclusively ours, like a connected socket
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                std::ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            // Authenticate the server before any protocol byte flows.
            return match verify_pipe_owner(handle) {
                Ok(()) => Ok(PipeStream {
                    handle,
                    event: std::ptr::null_mut(),
                    read_timeout: Cell::new(None),
                }),
                Err(_) => {
                    let owner = pipe_owner_sid_string(handle).unwrap_or_default();
                    unsafe { CloseHandle(handle) };
                    Err(ClientConnectError::ForeignOwner { name, owner })
                }
            };
        }
        let error = last_error("CreateFileW");
        if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < deadline {
            unsafe { WaitNamedPipeW(name_wide.as_ptr(), 250) };
            continue;
        }
        // ERROR_FILE_NOT_FOUND (no server), ERROR_ACCESS_DENIED (a DACL we
        // do not satisfy), … — all surface as plain connect failures, the
        // same way UnixStream::connect reports them.
        return Err(ClientConnectError::Io(error));
    }
}

fn wrap_stream(handle: HANDLE) -> Option<PipeStream> {
    if handle == INVALID_HANDLE_VALUE || handle.is_null() {
        return None;
    }
    Some(PipeStream {
        handle,
        event: std::ptr::null_mut(),
        read_timeout: Cell::new(None),
    })
}

/// One end of a control conversation — the server-side instance handed out
/// by [`PipeListener::accept`], or the client end from [`open_client`].
/// Byte-stream semantics identical to a connected unix socket: ordered,
/// reliable, EOF on peer disconnect.
#[derive(Debug)]
pub struct PipeStream {
    handle: HANDLE,
    /// Reusable manual-reset event for overlapped I/O. Guarded by `&mut`
    /// access: the `Read`/`Write` impls take `&mut self`, so operations on
    /// one end are exclusive by construction.
    event: HANDLE,
    read_timeout: Cell<Option<Duration>>,
}

impl PipeStream {
    /// Hard-aborts the conversation: the peer sees EOF/error immediately,
    /// like `shutdown(Both)` on a unix socket. Used by the server when a
    /// request line violates the protocol cap.
    pub fn abort(&mut self) {
        unsafe { DisconnectNamedPipe(self.handle) };
    }

    /// Mirrors `UnixStream::set_read_timeout`: bounds how long a single
    /// read may block. `None` waits forever (the server side).
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.read_timeout.set(timeout);
        Ok(())
    }

    fn ensure_event(&mut self) -> io::Result<()> {
        if self.event.is_null() {
            let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
            if event.is_null() || event == INVALID_HANDLE_VALUE {
                return Err(last_error("CreateEventW"));
            }
            self.event = event;
        }
        Ok(())
    }

    /// One overlapped transfer in either direction. `deadline` bounds the
    /// wait (`None` = forever, matching the unix blocking-stream default);
    /// on expiry the pending I/O is cancelled so neither the caller's view
    /// nor the kernel state leaks into the next operation.
    fn transfer(
        &mut self,
        buf: &mut [u8],
        read_direction: bool,
        deadline: Option<Instant>,
    ) -> io::Result<usize> {
        self.ensure_event()?;
        let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
        overlapped.hEvent = self.event;
        unsafe { ResetEvent(self.event) };

        let initiated = if read_direction {
            unsafe {
                ReadFile(
                    self.handle,
                    buf.as_mut_ptr(),
                    buf.len().min(u32::MAX as usize) as u32,
                    std::ptr::null_mut(),
                    &mut overlapped,
                )
            }
        } else {
            unsafe {
                WriteFile(
                    self.handle,
                    buf.as_ptr(),
                    buf.len().min(u32::MAX as usize) as u32,
                    std::ptr::null_mut(),
                    &mut overlapped,
                )
            }
        };

        let mut pending = false;
        if initiated == 0 {
            match last_error(if read_direction {
                "ReadFile"
            } else {
                "WriteFile"
            })
            .raw_os_error()
            {
                Some(code) if code == ERROR_IO_PENDING => pending = true,
                // The peer is gone. For reads this *is* EOF (Ok(0)), the
                // same contract a unix socket honours on FIN.
                Some(code)
                    if read_direction
                        && (code == ERROR_BROKEN_PIPE || code == ERROR_PIPE_NOT_CONNECTED) =>
                {
                    return Ok(0);
                }
                Some(code) => return Err(io::Error::from_raw_os_error(code)),
                None => return Err(last_error("overlapped init")),
            }
        }

        if pending {
            let wait_ms = match deadline {
                None => u32::MAX, // INFINITE: block until data or disconnect
                Some(deadline) => deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min((u32::MAX - 1) as u128) as u32,
            };
            match unsafe { WaitForSingleObject(self.event, wait_ms) } {
                WAIT_OBJECT_0 => {}
                WAIT_TIMEOUT => {
                    unsafe { CancelIoEx(self.handle, &overlapped) };
                    let mut transferred = 0u32;
                    unsafe {
                        GetOverlappedResult(self.handle, &overlapped, &mut transferred, 1);
                    }
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "named pipe operation timed out",
                    ));
                }
                status => {
                    return Err(io::Error::other(format!(
                        "WaitForSingleObject returned {status:#x}"
                    )));
                }
            }
        }

        let mut transferred = 0u32;
        if unsafe { GetOverlappedResult(self.handle, &overlapped, &mut transferred, 0) } == 0 {
            let error = last_error("GetOverlappedResult");
            if read_direction {
                match error.raw_os_error() {
                    // Broken pipe surfacing at completion time is a clean
                    // EOF, not a fault.
                    Some(code) if code == ERROR_BROKEN_PIPE || code == ERROR_PIPE_NOT_CONNECTED => {
                        return Ok(0);
                    }
                    _ => {}
                }
            }
            return Err(error);
        }
        Ok(transferred as usize)
    }

    /// Full-write loop, partial-write-safe: byte-mode pipes may accept
    /// fewer bytes than requested.
    fn write_all_impl(&mut self, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            // WriteFile needs a *mut buffer even though it never writes to it;
            // copy into scratch memory rather than lie about mutability.
            let mut scratch = buf.to_vec();
            let written = self.transfer(&mut scratch, false, None)?;
            if written == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "named pipe accepted zero bytes",
                ));
            }
            buf = &buf[written..];
        }
        Ok(())
    }
}

impl Read for PipeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.transfer(
            buf,
            true,
            self.read_timeout.get().map(|t| Instant::now() + t),
        )
    }
}

impl Write for PipeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Delegate to the full-write loop and report the whole slice: the
        // framing callers use write_all anyway.
        self.write_all_impl(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(()) // byte-mode pipes deliver eagerly; nothing to flush
    }
}

impl Drop for PipeStream {
    fn drop(&mut self) {
        if !self.event.is_null() {
            unsafe { CloseHandle(self.event) };
        }
        unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("tcw{}-{unique}-{tag}", std::process::id()));
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn pipe_name_is_deterministic_and_injective_enough() {
        let a = Path::new(r"C:\Users\me\AppData\Local\Temp\control.sock");
        let b = Path::new(r"C:\Users\you\AppData\Local\Temp\control.sock");

        let name_a = pipe_name_for_path(a).expect("derive a name");
        assert_eq!(
            name_a,
            pipe_name_for_path(a).expect("derive a name"),
            "same path, same name"
        );
        assert_ne!(
            name_a,
            pipe_name_for_path(b).expect("derive a name"),
            "distinct paths, distinct names"
        );
        assert!(name_a.starts_with(r"\\.\pipe\TillerRust\"));
        assert!(name_a.len() <= MAX_PIPE_NAME_CHARS, "{name_a}");

        // The security property, and the reason this function became
        // fallible: the name carries the CURRENT USER's SID. Without it the
        // default path — no XDG_*/HOME exists on native Windows, so it falls
        // back to the literal "/tmp/TillerRust/control.sock" — derived to ONE
        // machine-global name. That was predictable enough for a local user
        // to create the pipe first and harvest everything tillerctl wrote to
        // it, and it also broke a second user's Tiller, which contended for
        // the same name.
        let user = current_user_sid_string().expect("resolve our own SID");
        assert!(
            name_a.contains(&user),
            "the pipe name must be per-user, got {name_a} for SID {user}"
        );

        // An explicit pipe path passes through untouched — the Windows
        // counterpart of $TILLER_SOCKET naming an exact socket file.
        assert_eq!(
            pipe_name_for_path(Path::new(r"\\.\pipe\custom")).expect("passthrough"),
            r"\\.\pipe\custom"
        );

        // Sanitization collapses characters, the hash keeps names apart.
        assert_ne!(
            pipe_name_for_path(Path::new("/a/b")).expect("derive a name"),
            pipe_name_for_path(Path::new("/a_b")).expect("derive a name")
        );

        // A path long enough to blow the NT limit still derives a legal name.
        let deep = PathBuf::from("/".to_string() + &"d".repeat(600));
        let derived = pipe_name_for_path(&deep).expect("derive a name");
        assert!(derived.len() <= MAX_PIPE_NAME_CHARS, "{derived}");
    }

    #[test]
    fn bound_pipe_has_a_restricted_dacl() {
        let dir = TempDir::new("dacl");
        let listener = PipeListener::bind(&dir.0.join("control.sock")).expect("bind");
        // The full parity of the unix post-bind stat: the object that ended
        // up on the wire name grants only us (and SYSTEM).
        assert!(!listener.instance.is_null());
        post_bind_sanity_check(listener.instance).expect("DACL restricted to owner");
    }

    /// The refusal decision behind the client-side owner check, which is what
    /// gives Windows the authenticity unix gets for free from a socket living
    /// in a user-owned directory. `\\.\pipe\` is a flat global namespace, so
    /// without this a local user can create the name first and harvest
    /// whatever tillerctl writes; a DACL only says who may CONNECT to a pipe
    /// we created, and says nothing about a pipe someone else created.
    ///
    /// A genuine cross-user squat cannot be staged in-process — that needs a
    /// second OS user — which is exactly why the decision was split out pure.
    /// This pins the decision; the sibling test below pins that our OWN pipe
    /// is still accepted, so a check that simply refused everything (secure
    /// and useless) cannot pass both.
    #[test]
    fn a_pipe_owned_by_someone_else_is_refused_and_the_owner_is_named() {
        let us = current_user_sid_string().expect("resolve our own SID");
        ensure_owner_is(&us, &us).expect("our own pipe must be accepted");

        let stranger = "S-1-5-21-0-0-0-1234";
        let refusal = ensure_owner_is(stranger, &us)
            .expect_err("a pipe owned by another user must be refused");
        assert!(
            refusal.contains(stranger),
            "the refusal must name the actual owner so the cause is diagnosable, got: {refusal}"
        );
    }

    /// The other half of the pair above: the owner check must not reject the
    /// pipe we ourselves just bound. Without this, `verify_pipe_owner` could
    /// fail closed on everything and the refusal test would still pass.
    #[test]
    fn our_own_bound_pipe_passes_the_client_owner_check() {
        let dir = TempDir::new("owner");
        let path = dir.0.join("control.sock");
        let _listener = PipeListener::bind(&path).expect("bind");
        open_client(&path).expect("a pipe we own must be accepted by the client check");
    }

    #[test]
    fn second_bind_on_a_live_pipe_is_refused_not_stolen() {
        let dir = TempDir::new("squat");
        let path = dir.0.join("control.sock");
        let _listener = PipeListener::bind(&path).expect("first bind");

        match PipeListener::bind(&path) {
            Err(ListenerError::AlreadyRunning { .. }) => {} // the live-server case
            Err(ListenerError::Bind { detail }) => {
                panic!("expected AlreadyRunning, got Bind({detail})")
            }
            Ok(_) => panic!("second bind on a live pipe must not succeed"),
        }
    }

    #[test]
    fn round_trip_over_a_real_pipe_pair() {
        let dir = TempDir::new("pair");
        let path = dir.0.join("control.sock");
        let mut listener = PipeListener::bind(&path).expect("bind");
        let shutdown = AtomicBool::new(false);

        let server = std::thread::spawn(move || {
            let mut stream = listener.accept(&shutdown).expect("accepted");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            let mut buffer = vec![0u8; 256];
            let n = stream.read(&mut buffer).expect("read request");
            stream.write_all(&buffer[..n]).expect("echo response");
        });

        let mut client = open_client(&path).expect("client connects");
        client
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        client.write_all(b"ping").expect("write request");
        let mut buffer = vec![0u8; 256];
        let n = client.read(&mut buffer).expect("read echo");
        assert_eq!(&buffer[..n], b"ping");
        server.join().expect("server thread");
    }

    #[test]
    fn client_read_times_out_instead_of_hanging_forever() {
        let dir = TempDir::new("timeout");
        let path = dir.0.join("control.sock");
        let mut listener = PipeListener::bind(&path).expect("bind");
        let shutdown = AtomicBool::new(false);

        let server = std::thread::spawn(move || {
            // Accept, send nothing, and crucially *keep the server end
            // alive*: dropping it would close the pipe and surface as a
            // clean EOF (Ok(0)) on the client, which is correct behavior,
            // not a timeout.
            let _held = listener.accept(&shutdown).expect("accepted");
            std::thread::sleep(Duration::from_secs(1));
        });

        let mut client = open_client(&path).expect("client connects");
        client
            .set_read_timeout(Some(Duration::from_millis(250)))
            .expect("timeout");
        let started = Instant::now();
        let mut buffer = [0u8; 16];
        let error = client.read(&mut buffer).expect_err("read must time out");
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() >= Duration::from_millis(200));
        assert!(started.elapsed() < Duration::from_secs(5));
        server.join().expect("server thread");
    }
}
