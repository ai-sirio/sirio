//! PlantUML through the `plantuml` on PATH (design §3). Found, never
//! shipped — `sirio_lsp`'s model for language servers.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use crate::{DiagramError, Options, Svg, plantuml_document, svg};

/// How long a local render may take: a cold JVM alone takes 1–3 s.
pub(crate) const LOCAL_TIMEOUT: Duration = Duration::from_secs(15);

/// PlantUML learned security profiles in 1.2020.11; older releases ignore
/// `PLANTUML_SECURITY_PROFILE` and run with full file and URL access.
const MIN_SANDBOXABLE: (u32, u32) = (2020, 11);

/// Finds `program` on `search_path` the way a shell would, ignoring relative
/// and empty entries (they would resolve against an untrusted directory).
/// On Windows the `PATHEXT` extensions are tried in order, because package
/// managers install PlantUML as a `.cmd` or `.bat` shim that
/// `Command::new("plantuml")` alone would not find.
pub(crate) fn find_program(program: &str, search_path: &OsStr) -> Option<PathBuf> {
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(|extension| extension.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&absolute_search_path(search_path)) {
        for extension in &extensions {
            let candidate = dir.join(format!("{program}{extension}"));
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

/// `search_path` with only its absolute entries: a relative entry would
/// resolve `plantuml` against an untrusted directory, and an empty one
/// against the child's working directory.
fn absolute_search_path(search_path: &OsStr) -> OsString {
    std::env::join_paths(
        std::env::split_paths(search_path)
            .filter(|dir| !dir.as_os_str().is_empty() && dir.is_absolute()),
    )
    .unwrap_or_default()
}

/// The allowlist root as PlantUML sees it: canonicalised, because PlantUML
/// canonicalises the included file but not this entry. Falls back to the
/// path as given when the worktree cannot be read.
fn allowlist_path(options: &Options) -> OsString {
    let canonical = std::fs::canonicalize(&options.include_root)
        .unwrap_or_else(|_| options.include_root.clone());
    let mut text = canonical.as_os_str().to_string_lossy().into_owned();
    if cfg!(windows) {
        text = text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned();
    }
    text.into()
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Renders with the local `plantuml`, source on stdin and SVG on stdout,
/// from the Markdown file's directory. A release too old to sandbox is
/// refused before anything is rendered.
pub(crate) fn render_local(
    source: &str,
    options: &Options,
    search_path: &OsStr,
    timeout: Duration,
) -> Result<Svg, DiagramError> {
    let Some(program) = find_program("plantuml", search_path) else {
        return Err(DiagramError::NotAvailable);
    };
    sandboxed_program(&program, options, search_path, timeout)?;
    let input = plantuml_document(source).into_owned().into_bytes();
    let (status, stdout, stderr) = run_piped(
        &program,
        &["-tsvg", "-pipe", "-charset", "UTF-8"],
        &input,
        options,
        search_path,
        timeout,
    )?;
    if has_error_marker(&stderr) {
        return Err(syntax_error(&stderr));
    }
    if !status.success() {
        // No `ERROR` marker: PlantUML never got to the diagram (a missing
        // `java`, a crashed JVM), so this is not a syntax error.
        return Err(DiagramError::Io(
            last_stderr_line(&stderr).unwrap_or_else(|| format!("plantuml exited with {status}")),
        ));
    }
    svg::double_for_hidpi(&String::from_utf8_lossy(&stdout))
        .ok_or_else(|| DiagramError::Io("plantuml printed no SVG".into()))
}

/// Runs `program` with piped stdio from the Markdown file's directory:
/// `input` on stdin, stdout and stderr collected on threads of their own (a
/// diagram big enough to fill a pipe would otherwise deadlock against a
/// child blocked writing). Sandboxed (design §3) through the environment —
/// the `ALLOWLIST` security profile blocks URLs and environment variables,
/// and `plantuml.allowlist.path` admits files only under the worktree;
/// PlantUML reads both from the environment as well as from Java system
/// properties, so no JVM flag has to reach the launcher. Past `timeout` the
/// whole process tree is killed. Shared by `-version` and the render.
fn run_piped(
    program: &Path,
    args: &[&str],
    input: &[u8],
    options: &Options,
    search_path: &OsStr,
    timeout: Duration,
) -> Result<(ExitStatus, Vec<u8>, String), DiagramError> {
    let allowed = allowlist_path(options);
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(&options.working_dir)
        // The launcher script finds `java` on the same PATH it was found on.
        .env("PATH", absolute_search_path(search_path))
        .env("PLANTUML_SECURITY_PROFILE", "ALLOWLIST")
        // `/bin/sh` launchers on dash/busybox drop env names with dots.
        .env("plantuml.allowlist.path", &allowed)
        .env("PLANTUML_ALLOWLIST_PATH", &allowed)
        // Nothing the parent smuggles in may widen the sandbox again.
        .env_remove("PLANTUML_INCLUDE_PATH")
        .env_remove("plantuml.include.path")
        .env_remove("PLANTUML_ALLOWLIST_URL")
        .env_remove("plantuml.allowlist.url")
        .env_remove("JAVA_TOOL_OPTIONS")
        .env_remove("_JAVA_OPTIONS")
        .env_remove("JDK_JAVA_OPTIONS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // Its own process group, so the timeout kills the JVM's children too.
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        command.env("NoDefaultCurrentDirectoryInExePath", "1");
    }
    // `find_program` already resolved the program: anything missing now (the
    // working directory, the launcher's interpreter) is an I/O error.
    let mut child = command
        .spawn()
        .map_err(|error| DiagramError::Io(error.to_string()))?;

    let input = input.to_owned();
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let mut stderr = child.stderr.take().expect("stderr is piped");
    let error_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                kill_tree(&mut child);
                let _ = child.wait();
                return Err(DiagramError::Timeout {
                    seconds: timeout.as_secs(),
                });
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                kill_tree(&mut child);
                let _ = child.wait();
                return Err(DiagramError::Io(error.to_string()));
            }
        }
    };
    let _ = writer.join();
    let stdout = reader.join().unwrap_or_default();
    let stderr = error_reader.join().unwrap_or_default();
    Ok((status, stdout, stderr))
}

/// Kills the child's whole process tree: on Unix the child leads its own
/// process group (see `run_piped`), so a negative pid reaches the JVM's
/// children as well; on Windows the tree flag does the same.
fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        // Negative pid: the child's process group.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .output();
        let _ = child.kill();
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = child.kill();
    }
}

/// The `-version` verdict per program path and modification time, so the
/// JVM starts for `-version` once per program, not per diagram: `Ok` the
/// sandboxable banner, `Err` the banner too old to sandbox, or `"unknown"`.
type VersionVerdict = Result<String, String>;
type VersionCache = Mutex<HashMap<(PathBuf, SystemTime), VersionVerdict>>;

fn version_cache() -> &'static VersionCache {
    static CACHE: OnceLock<VersionCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Refuses a `plantuml` too old to sandbox: security profiles exist only
/// from 1.2020.11, and older releases silently run with full file and URL
/// access. Deliberately no server fallback here: only `NotAvailable` falls
/// back, never a program that cannot run sandboxed.
fn sandboxed_program(
    program: &Path,
    options: &Options,
    search_path: &OsStr,
    timeout: Duration,
) -> Result<(), DiagramError> {
    let cache_key = std::fs::metadata(program)
        .and_then(|meta| meta.modified())
        .ok()
        .map(|modified| (program.to_path_buf(), modified));
    let verdict = cache_key
        .clone()
        .and_then(|key| {
            version_cache()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&key)
                .cloned()
        })
        .unwrap_or_else(|| {
            let fresh = check_version(program, options, search_path, timeout);
            if let Some(key) = cache_key {
                version_cache()
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(key, fresh.clone());
            }
            fresh
        });
    verdict
        .map(|_| ())
        .map_err(|version| DiagramError::Unsandboxed { version })
}

/// Runs `<program> -version`: `Ok` the banner of a PlantUML new enough to
/// sandbox, `Err` the banner (or `"unknown"`) of one that is not.
fn check_version(
    program: &Path,
    options: &Options,
    search_path: &OsStr,
    timeout: Duration,
) -> Result<String, String> {
    let (status, stdout, _) = run_piped(program, &["-version"], &[], options, search_path, timeout)
        .map_err(|_| "unknown".to_string())?;
    if !status.success() {
        return Err("unknown".into());
    }
    match parse_version(&String::from_utf8_lossy(&stdout)) {
        Some((year, number)) if (year, number) >= MIN_SANDBOXABLE => {
            Ok(format!("1.{year}.{number}"))
        }
        Some((year, number)) => Err(format!("1.{year}.{number}")),
        None => Err("unknown".into()),
    }
}

/// The first `1.YYYY.N` in `banner`: its year and release number.
fn parse_version(banner: &str) -> Option<(u32, u32)> {
    let bytes = banner.as_bytes();
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at] == b'1' && bytes[at + 1] == b'.' {
            let mut year_end = at + 2;
            while year_end < bytes.len() && bytes[year_end].is_ascii_digit() {
                year_end += 1;
            }
            if year_end > at + 2 && year_end < bytes.len() && bytes[year_end] == b'.' {
                let mut number_end = year_end + 1;
                while number_end < bytes.len() && bytes[number_end].is_ascii_digit() {
                    number_end += 1;
                }
                if number_end > year_end + 1 {
                    let year = banner[at + 2..year_end].parse().ok()?;
                    let number = banner[year_end + 1..number_end].parse().ok()?;
                    return Some((year, number));
                }
            }
        }
        at += 1;
    }
    None
}

fn has_error_marker(stderr: &str) -> bool {
    stderr.lines().any(|line| line.trim() == "ERROR")
}

/// The last non-empty stderr line, trimmed, for failure reports.
fn last_stderr_line(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_string)
}

/// `plantuml -pipe` reports a bad diagram as `ERROR`, the line number, then
/// the message, on stderr. Only call with an `ERROR` marker present: any
/// other failure is the tool's, not the diagram's.
fn syntax_error(stderr: &str) -> DiagramError {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if let Some(at) = lines.iter().position(|line| *line == "ERROR") {
        let line = lines.get(at + 1).and_then(|line| line.parse::<u32>().ok());
        let skip = if line.is_some() { 2 } else { 1 };
        let message = lines
            .get(at + skip..)
            .map(|rest| rest.join(" "))
            .unwrap_or_default();
        return DiagramError::Syntax {
            message: if message.is_empty() {
                "syntax error".into()
            } else {
                message
            },
            line,
        };
    }
    DiagramError::Syntax {
        message: last_stderr_line(stderr).unwrap_or_else(|| "plantuml failed".into()),
        line: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiagramKind, render, scratch_dir, test_options};
    use std::net::TcpListener;
    use std::time::Instant;

    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40\" height=\"20\" viewBox=\"0 0 40 20\"></svg>";

    /// `bin` first, then the real PATH: the fake scripts call `cat`, `env`
    /// and `sleep`, and `render_local` hands the child this same PATH.
    #[cfg(unix)]
    fn path_with(bin: &Path) -> std::ffi::OsString {
        let rest = std::env::var_os("PATH").unwrap_or_default();
        std::env::join_paths(std::iter::once(bin.to_path_buf()).chain(std::env::split_paths(&rest)))
            .expect("joinable PATH")
    }

    #[cfg(unix)]
    fn fake_plantuml(dir: &Path, body: &str) {
        fake_plantuml_with_version(dir, "PlantUML version 1.2024.3 (Sun Jan 28 2024)", body);
    }

    /// A fake answering `-version` with `banner`; every fake answers
    /// `-version` first, because `render_local` checks it before rendering.
    #[cfg(unix)]
    fn fake_plantuml_with_version(dir: &Path, banner: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).expect("bin dir");
        let path = dir.join("plantuml");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then echo '{banner}'; exit 0; fi\n{body}\n"
            ),
        )
        .expect("write fake plantuml");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    #[test]
    fn nothing_on_the_path_is_not_available() {
        let root = scratch_dir("plantuml-none");
        let empty = root.join("bin");
        std::fs::create_dir_all(&empty).expect("bin dir");
        assert_eq!(
            render_local(
                "A -> B",
                &test_options(&root),
                empty.as_os_str(),
                LOCAL_TIMEOUT
            ),
            Err(DiagramError::NotAvailable)
        );
    }

    #[cfg(unix)]
    #[test]
    fn its_svg_is_the_diagram() {
        let root = scratch_dir("plantuml-svg");
        let bin = root.join("bin");
        fake_plantuml(&bin, &format!("cat > /dev/null\nprintf '%s' '{SVG}'"));
        let svg = render_local(
            "A -> B",
            &test_options(&root),
            &path_with(&bin),
            LOCAL_TIMEOUT,
        )
        .expect("the fake prints an SVG");
        assert_eq!((svg.logical_width, svg.logical_height), (40, 20));
    }

    #[cfg(unix)]
    #[test]
    fn it_receives_the_wrapped_source_in_the_files_directory_sandboxed() {
        let root = scratch_dir("plantuml-capture");
        let bin = root.join("bin");
        let docs = root.join("docs");
        let out = root.join("out");
        std::fs::create_dir_all(&docs).expect("docs");
        std::fs::create_dir_all(&out).expect("out");
        let out_dir = out.display();
        fake_plantuml(
            &bin,
            &format!(
                "cat > '{out_dir}/stdin'\nenv > '{out_dir}/env'\npwd > '{out_dir}/pwd'\nprintf '%s' '{SVG}'"
            ),
        );
        let options = Options {
            working_dir: docs.clone(),
            include_root: root.clone(),
            ..test_options(&root)
        };
        render_local("A -> B", &options, &path_with(&bin), LOCAL_TIMEOUT).expect("renders");
        assert_eq!(
            std::fs::read_to_string(out.join("stdin")).expect("stdin"),
            "@startuml\nA -> B\n@enduml\n"
        );
        let env = std::fs::read_to_string(out.join("env")).expect("env");
        assert!(
            env.lines()
                .any(|line| line == "PLANTUML_SECURITY_PROFILE=ALLOWLIST")
        );
        let allowed = allowlist_path(&options);
        assert!(
            env.lines().any(
                |line| line == format!("plantuml.allowlist.path={}", allowed.to_string_lossy())
            )
        );
        assert!(
            env.lines().any(
                |line| line == format!("PLANTUML_ALLOWLIST_PATH={}", allowed.to_string_lossy())
            )
        );
        assert!(
            env.lines()
                .all(|line| !line.starts_with("JAVA_TOOL_OPTIONS=")),
            "the child's JVM flags are scrubbed"
        );
        assert!(
            env.lines()
                .all(|line| !line.starts_with("PLANTUML_INCLUDE_PATH=")),
            "the child's include path is scrubbed"
        );
        let pwd = std::fs::read_to_string(out.join("pwd")).expect("pwd");
        assert_eq!(
            std::fs::canonicalize(pwd.trim()).expect("pwd exists"),
            std::fs::canonicalize(&docs).expect("docs exists")
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_error_exit_is_a_syntax_error_at_its_line() {
        let root = scratch_dir("plantuml-error");
        let bin = root.join("bin");
        fake_plantuml(
            &bin,
            "cat > /dev/null\nprintf 'ERROR\\n3\\nSyntax Error?\\n' >&2\nexit 200",
        );
        assert_eq!(
            render_local(
                "A -> ",
                &test_options(&root),
                &path_with(&bin),
                LOCAL_TIMEOUT
            ),
            Err(DiagramError::Syntax {
                message: "Syntax Error?".into(),
                line: Some(3)
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_error_marker_counts_even_when_the_exit_is_zero() {
        let root = scratch_dir("plantuml-error-zero");
        let bin = root.join("bin");
        fake_plantuml(
            &bin,
            &format!(
                "cat > /dev/null\nprintf 'ERROR\\n1\\nNo diagram found\\n' >&2\nprintf '%s' '{SVG}'"
            ),
        );
        assert_eq!(
            render_local("x", &test_options(&root), &path_with(&bin), LOCAL_TIMEOUT),
            Err(DiagramError::Syntax {
                message: "No diagram found".into(),
                line: Some(1)
            })
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_plantuml_is_killed_at_the_timeout() {
        let root = scratch_dir("plantuml-hang");
        let bin = root.join("bin");
        fake_plantuml(&bin, "exec sleep 5");
        let started = Instant::now();
        let result = render_local(
            "A -> B",
            &test_options(&root),
            &path_with(&bin),
            Duration::from_millis(300),
        );
        assert!(
            matches!(result, Err(DiagramError::Timeout { .. })),
            "got {result:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the child was not waited out"
        );
    }

    #[test]
    fn the_version_is_read_from_its_banner() {
        assert_eq!(
            parse_version("PlantUML version 1.2020.2 (Sat Feb 01 2020)"),
            Some((2020, 2))
        );
        assert_eq!(
            parse_version("PlantUML version 1.2024.3 (Sun Jan 28 2024)"),
            Some((2024, 3))
        );
        assert_eq!(parse_version("nonsense"), None::<(u32, u32)>);
    }

    #[cfg(unix)]
    #[test]
    fn a_plantuml_older_than_1_2020_11_is_refused() {
        let root = scratch_dir("plantuml-old");
        let bin = root.join("bin");
        let out = root.join("out");
        std::fs::create_dir_all(&out).expect("out");
        let marker = out.join("rendered");
        let marker_arg = marker.display();
        fake_plantuml_with_version(
            &bin,
            "PlantUML version 1.2020.2 (Sat Feb 01 2020)",
            &format!("touch '{marker_arg}'\nprintf '%s' '{SVG}'"),
        );
        assert_eq!(
            render_local(
                "A -> B",
                &test_options(&root),
                &path_with(&bin),
                LOCAL_TIMEOUT
            ),
            Err(DiagramError::Unsandboxed {
                version: "1.2020.2".into()
            })
        );
        assert!(!marker.exists(), "the render never ran");
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_plantuml_and_its_children_are_killed_at_the_timeout() {
        let root = scratch_dir("plantuml-tree");
        let bin = root.join("bin");
        let out = root.join("out");
        std::fs::create_dir_all(&out).expect("out");
        let out_dir = out.display();
        fake_plantuml(
            &bin,
            &format!("sleep 30 & echo $! > '{out_dir}/grandchild'\nwait"),
        );
        let started = Instant::now();
        let result = render_local(
            "A -> B",
            &test_options(&root),
            &path_with(&bin),
            Duration::from_millis(300),
        );
        assert!(
            matches!(result, Err(DiagramError::Timeout { .. })),
            "got {result:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the child was not waited out"
        );
        let grandchild: i32 = std::fs::read_to_string(out.join("grandchild"))
            .expect("grandchild pid")
            .trim()
            .parse()
            .expect("pid parses");
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            // Signal 0: succeeds while the process still exists.
            if unsafe { libc::kill(grandchild, 0) } != 0 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "the grandchild survived the timeout"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn relative_path_entries_are_dropped() {
        let (first, second) = if cfg!(windows) {
            ("C:\\tools\\plantuml", "D:\\opt\\x")
        } else {
            ("/usr/bin", "/opt/x")
        };
        let search =
            std::env::join_paths([first, "bin", "", "./tools", second]).expect("joinable PATH");
        let kept = std::env::join_paths([first, second]).expect("joinable PATH");
        assert_eq!(absolute_search_path(&search), kept);
    }

    #[cfg(unix)]
    #[test]
    fn a_failure_without_an_error_marker_is_not_a_syntax_error() {
        let root = scratch_dir("plantuml-failure");
        let bin = root.join("bin");
        fake_plantuml(
            &bin,
            "cat > /dev/null\necho 'java: not found' >&2\nexit 127",
        );
        assert_eq!(
            render_local(
                "A -> B",
                &test_options(&root),
                &path_with(&bin),
                LOCAL_TIMEOUT
            ),
            Err(DiagramError::Io("java: not found".into()))
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_finds_a_cmd_shim_through_pathext() {
        let dir = scratch_dir("plantuml-pathext");
        std::fs::write(dir.join("plantuml.cmd"), "@echo off\r\n").expect("shim");
        assert_eq!(
            find_program("plantuml", dir.as_os_str()),
            Some(dir.join("plantuml.cmd"))
        );
    }

    fn real_plantuml() -> bool {
        let found =
            find_program("plantuml", &std::env::var_os("PATH").unwrap_or_default()).is_some();
        if !found {
            eprintln!("SKIP: plantuml is not on PATH");
        }
        found
    }

    #[test]
    fn real_plantuml_renders_a_sequence_diagram() {
        if !real_plantuml() {
            return;
        }
        let root = scratch_dir("plantuml-real");
        let svg = render(
            DiagramKind::PlantUml,
            "Alice -> Bob: hello",
            &test_options(&root),
        )
        .expect("plantuml renders");
        assert!(svg.markup.contains("Alice"));
        assert!(svg.logical_width > 0);
    }

    /// The design's sandbox requirement (§3): includes only inside the
    /// worktree, and no network at all.
    #[test]
    fn real_plantuml_is_sandboxed_to_the_worktree_and_offline() {
        if !real_plantuml() {
            return;
        }
        let root = scratch_dir("plantuml-sandbox");
        let docs = root.join("docs");
        std::fs::create_dir_all(&docs).expect("docs");
        std::fs::write(root.join("inside.puml"), "Alice -> Bob: INSIDE\n").expect("inside");
        let outside = scratch_dir("plantuml-outside");
        std::fs::write(outside.join("secret.puml"), "Alice -> Bob: SECRET\n").expect("secret");
        let listener = TcpListener::bind("127.0.0.1:0").expect("listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("addr").port();
        let options = Options {
            working_dir: docs,
            include_root: root.clone(),
            ..test_options(&root)
        };

        let inside = render(
            DiagramKind::PlantUml,
            &format!("!include {}\n", root.join("inside.puml").display()),
            &options,
        )
        .expect("an include inside the worktree renders");
        assert!(inside.markup.contains("INSIDE"));

        // An include outside the worktree is refused as a syntax error. If
        // the installed PlantUML predates security profiles, nothing below
        // can run sandboxed: skip instead of failing.
        match render(
            DiagramKind::PlantUml,
            &format!("!include {}\n", outside.join("secret.puml").display()),
            &options,
        ) {
            Err(DiagramError::Syntax { .. }) => {}
            Err(DiagramError::Unsandboxed { .. }) => {
                eprintln!("SKIP: plantuml too old to sandbox");
                return;
            }
            other => panic!("an include outside the worktree must be refused, got {other:?}"),
        }

        let _ = render(
            DiagramKind::PlantUml,
            &format!("!includeurl http://127.0.0.1:{port}/x.puml\nA -> B\n"),
            &options,
        );
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock),
            "PlantUML connected to the loopback listener"
        );
    }

    #[test]
    fn real_plantuml_reports_a_syntax_error() {
        if !real_plantuml() {
            return;
        }
        let root = scratch_dir("plantuml-real-error");
        assert!(
            matches!(
                render(DiagramKind::PlantUml, "A -> ", &test_options(&root)),
                Err(DiagramError::Syntax { .. })
            ),
            "a broken diagram is a syntax error"
        );
    }
}
