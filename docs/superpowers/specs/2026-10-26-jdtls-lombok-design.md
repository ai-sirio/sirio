# jdtls starts with Lombok when the project uses it

**Date:** 2026-10-26
**Status:** design

## §0 Intent

Enzo opened a Spring Boot project, built with Maven, in Sirio's editor. The Problems
drawer listed 1268 errors on code that compiles. Two things were going on, and only
one of them is this design.

- At first every import from a dependency was flagged (`ApiResponse cannot be resolved
  to a type`, `MediaType cannot be resolved to a variable`). These were findings from
  before jdtls had finished importing the Maven project. Hovering the same `MediaType`
  already showed Spring's Javadoc. Once the file was reopened, they were gone. That is a
  separate problem and is out of scope here (§3).
- What stayed, 1191 errors, is Lombok:
  - `The blank final field apiPegaCaller may not have been initialized`: the class has
    `@RequiredArgsConstructor`;
  - `log cannot be resolved`: the class has `@Slf4j`;
  - `The method setTarga(String) is undefined for the type LoginBody`: `LoginBody` gets
    its setters from Lombok.

The cause: Lombok rewrites the compiler's syntax tree while it compiles. Under `javac`
it runs as an annotation processor. jdtls compiles with Eclipse's compiler (ECJ), which
Lombok can only reach through a Java agent loaded when the JVM starts
(`-javaagent:lombok.jar`). Sirio launches the built-in `jdtls` entry with no arguments
at all (`LanguageTable::defaults`, `rust/crates/sirio_lsp/src/config.rs`), so the agent
is never there. VS Code's Java extension adds the agent by itself. That is the prior
art, and the experience Enzo expects.

What the user wants: **Sirio notices that a project uses Lombok and starts jdtls with
the Lombok agent, with nothing to configure.**

Success: a Lombok project opens in Sirio, and the Problems drawer holds no error that
exists only because Lombok was missing. That includes files that are not open.

A manual workaround exists today and keeps working: a `[[language]]` entry for `java` in
`~/.config/sirio/languages.toml` with
`args = ["--jvm-arg=-javaagent:<absolute path to lombok.jar>"]`. This design makes it
unnecessary.

## §1 Decided with the user

In the order they were settled:

1. **Sirio detects it.** Documenting the manual entry was the alternative and was
   rejected ("Voglio che Sirio se ne accorga").
2. **The play-back was approved as a whole** ("va bene così"). It said:
   - the jar comes from the local Maven and Gradle caches and is never downloaded;
   - the newest version wins when there are several;
   - with no jar, jdtls starts exactly as today;
   - a hand-written `languages.toml` entry is never touched;
   - no message is shown to the user;
   - the stale-import findings and the hover's raw Markdown are separate problems.

   The calls in it were the designer's, presented and not objected to.
3. **jdtls restarts once when the jar arrives late** ("includi anche il riavvio di
   jdtls"). Suppose jdtls was started without the agent because no jar existed yet, and
   one appears later, typically because jdtls's own Maven import downloaded it. Sirio
   then restarts that jdtls once, with the agent.

## §2 Behaviour

### §2.1 When the agent is added

When a jdtls is launched, the agent is added only if all three of these hold:

1. **The entry is Sirio's built-in `java` entry**, not one written in `languages.toml`.
   A hand-written entry is the user's choice of command and arguments, and its launcher
   might not understand `--jvm-arg`. *(designer's call, approved in play-back)* How the
   supervisor tells the two apart is the builder's choice. Today `install` is `Some`
   only on built-in entries, because it is `#[serde(skip)]`, but a marker that means
   exactly this is clearer.
2. **The project uses Lombok.** A build file contains the text `lombok`. The build files
   are those the `java` entry already roots on (`pom.xml`, `build.gradle`,
   `build.gradle.kts`, `settings.gradle`, `settings.gradle.kts`) plus
   `gradle/libs.versions.toml`. They are searched in the server's root and in every
   directory above it up to the worktree root, so a parent POM that declares Lombok for
   all modules counts. The text match covers the `org.projectlombok` dependency, the
   `io.freefair.lombok` Gradle plugin and a version-catalog alias such as
   `libs.lombok`. *(designer's call)* Why: a project that does not use Lombok is never
   touched, so an old jar sitting in a cache can never break jdtls for a project that
   did not need it.
3. **A Lombok jar is found** (§2.2).

What gets added is one argument, appended to the entry's arguments:
`--jvm-arg=-javaagent:<absolute path of the jar>`. It is a single argv element passed
with no shell, so spaces in the path need no quoting. This is how the upstream jdtls
launcher passes a JVM option; that launcher is the Python `jdtls` script installed by
the release tarball, Homebrew and Mason. **Confirmed live (§7):** `jdtls.py` of 1.63.0
appends every `--jvm-arg` to the JVM's arguments, and with the agent the Lombok errors
of the open file are gone.

### §2.2 Where the jar comes from

Sirio looks in two places and takes the newest version across both:

- **Maven's local repository**
  - The directory is `<localRepository>` from `~/.m2/settings.xml` when it is set, with
    `${user.home}` expanded. Otherwise it is `~/.m2/repository`.
  - Inside it, the jar is `org/projectlombok/lombok/<version>/lombok-<version>.jar`.
- **Gradle's cache**
  - The base is `$GRADLE_USER_HOME` when set, otherwise `~/.gradle`.
  - Inside it, the jar is
    `caches/modules-2/files-2.1/org.projectlombok/lombok/<version>/<hash>/lombok-<version>.jar`.

Rules:

- Only the main jar counts. `-sources` and `-javadoc` jars do not. A version directory
  that holds only `.lastUpdated` or partial-download files, or a zero-byte jar, counts as
  having no jar.
- Versions compare by their numeric parts, so `1.18.9` sorts below `1.18.30`. A version
  with a suffix sorts below the same version without one. *(designer's call)* Whatever
  the folder names, a malformed version must never fail the launch: such a folder is
  skipped.
- "Home" is the user's home as the launched server sees it, from the same environment
  `Server::launch` gives the child (`sirio_env::installed()`).
- Nothing leaves the machine and nothing is downloaded.
- The project's own Lombok version is not looked up. It usually comes from Spring Boot's
  dependency management rather than from the project's POM, and only jdtls knows it
  after the import. The newest version in the caches is what VS Code also leans on: it
  uses its bundled jar unless the project's is newer.
- A project that needs a newer Lombok than any jar in the caches usually gets one: its
  own import downloads that version, and §2.3 restarts jdtls with it. The case left
  over is accepted: an agent too old for the project, with no newer jar arriving (for
  example an offline import), leaves Lombok-shaped errors behind and no restart
  follows. *(designer's call, from the builder check)*

### §2.3 The restart

**When.** A jdtls is running for a project that uses Lombok (§2.1, rule 2), and later
in the same session a jar appears that is newer than the one it was launched with, or
any jar when it was launched without one. Two usual cases:

- The first open of a project on a machine whose caches do not hold Lombok yet. jdtls's
  own Maven or Gradle import downloads it.
- The caches hold only an older Lombok, left by another project, and the import
  downloads the version this project asks for. *(designer's call, from the builder
  check)*

**What happens:**

1. Sirio takes the old server's handle out of the supervisor and cancels its router
   first. When the old stream closes, the router today calls `language_server_gone`
   for the key, and `forget` would then remove whatever handle sits under that key,
   including the new server's. The old read loop keeps running until the stop below
   returns, because the answer to `shutdown` arrives through it.
2. Sirio stops the old jdtls the way `Server::stop` does: `shutdown`, then `exit`,
   then a grace period, then a kill. **The restart's grace is 10 s.** In the live runs
   jdtls exited between 1.0 s and 4.1 s after `exit` (§7), so `EXIT_GRACE` (500 ms)
   would kill it every time. Quitting the app keeps 500 ms. The live runs also showed
   that the workspace is saved while jdtls answers `shutdown`, so a kill after `exit`
   loses nothing a relaunch needs.
3. The new jdtls is launched only after the old process has exited and been reaped,
   because under the default launcher the new process uses the same workspace
   directory.
4. Sirio does the same cleanup `language_server_gone` does today:
   - every drawer drops the server's findings;
   - open files of the key clear their gutters;
   - the documents it held are forgotten, so the next server is sent them again.

   The old server's Lombok errors must not survive the restart.
5. Sirio launches jdtls again with the agent, and sends it every open file of that key.
   This is the same path a launch follows today, ending in
   `sync_document_with_server` for each open file.
6. From the moment the old handle is taken out until the new launch answers, the
   status row reads "starting", which is the existing state. No card, no message.

**Bounds:**

- At most one deliberate restart per key per session. *(designer's call)* It follows
  the LSP client design's "no automatic restart loop" rule
  (`docs/superpowers/specs/2026-09-14-lsp-client-design.md`). Together with the single
  fallback launch of §2.5, a key gets at most two launches beyond its first in a
  session.
- This restart is its own path. It is not `LspSupervisor::revive`, whose documented
  single caller is a successful install, and that stays true.
- A server that crashes after the restart lands in `Dead::Failed` like any crash.
- Only a key whose project uses Lombok is ever restarted, so a jar downloaded by another
  project never restarts an unrelated jdtls.

**The trigger, decided from the live log (§7):** Sirio re-checks the caches when that
key publishes diagnostics, at most once every 2 seconds per key. With empty caches the
jar landed in `~/.m2` at 42.1 s, during the import. jdtls said `ServiceReady` at 49.4 s
and published its first diagnostics at 51.4 s, for every file of the project and not
only the open one. The first publish is therefore always after the jar. A later
download of a newer jar, for a project that already used Lombok at launch, is caught
by a later publish. A project that starts using Lombok mid-session, for example after
Lombok is added to the POM, gets the agent the next time its server is launched
(reopening the project or restarting Sirio), because "uses Lombok" is decided at
launch. *(builder's call, during execution)* The re-check runs only for keys that could
still restart: the project uses Lombok, the restart has not been spent, and the key is
not in fallback. It is never a free-running timer. A filesystem watch was not needed.

### §2.4 No stale findings after the agent arrives

Requirement: once jdtls runs with the agent, Problems keeps no Lombok error for any file
of the project, open or not.

The problem is real (§7):
- jdtls keeps a workspace on disk between launches. The launcher's default is
  `~/.cache/jdtls/jdtls-<sha1 of the launch directory's name>` on Linux,
  `~/Library/Caches/jdtls/...` on macOS and `%APPDATA%\jdtls\...` on Windows.
- Relaunched with the agent over a workspace built without it, jdtls cleared the open
  file but republished the three old Lombok errors of `PegaService.java`, which was not
  open, and kept them.

This would hit Enzo's machine on the first launch after the feature ships, and every
restart of §2.3.

**The mechanism, decided from the live check:** after every launch with the agent, at
the launch's first `language/status` notification of type `ServiceReady`, Sirio sends
jdtls `java/buildWorkspace` with `true` (a full build).
- It goes through `request_with_timeout` with a 10-minute timeout, because the default
  30 s is too short for a large project.
- An answer of `1` (built) or `2` (built, with compile errors) is the expected outcome.
  Any other answer, an error or the timeout records a content-free `sirio_perf` event
  and nothing else.
- In the live run the full build cleared `PegaService.java` in 1.7 s on the sample
  project. Its cost scales with the project, and it runs inside jdtls after it is
  ready.
- The router routes `language/status` for this. Today it drops it.

*(builder's call, made during planning)* Rejected: a Sirio-owned `-data` directory with
a marker saying which agent last built it, and a full build only when that changes. It
is cheaper per launch. But it moves the workspace of every Lombok project, it trusts the
marker across a crash that skipped the workspace save, and it is more code than one
request.

### §2.6 Language servers' stderr is discarded

`Server::launch` pipes the child's stderr today and nothing ever reads it. In the live
run an old Lombok (1.18.20) on Java 25 did not stop jdtls, but it wrote about 50 KB of
stack traces to stderr. A pipe holds 64 KB on Linux and less on macOS. Past that, a
server's next write to stderr blocks and the server hangs.

So every language server's stderr goes to the null device. This is a deliberate
exception to §4's "nothing changes for a project that does not use Lombok": it touches
every server, and the only thing it changes is that a server can no longer hang this
way. Nothing reads stderr today, so no message is lost.

### §2.5 When the agent breaks the launch

An old Lombok can fail inside a newer jdtls. jdtls 1.63 itself needs Java 25: its
bundles require `JavaSE 25`, although its launcher still only checks for 21. In the
live run Lombok 1.18.20 did not stop jdtls 1.63 from starting; it only wrote stack
traces to stderr (§2.6). The fallback below covers the releases that do stop it.

If a launch with the agent fails, Sirio launches once more without it, and that key gets
no agent for the rest of the session. *(designer's call)* A failed launch means any of:
- the spawn fails;
- the handshake fails;
- the stream closes before `initialize` answers;
- the stream closes after `initialize` but before jdtls has answered the full build of
  §2.4. Any answer counts: a result, an error object or the timeout. A closed stream
  does not. Lombok patches the compiler when a build runs, so a crash in that window is
  treated as the agent's fault. *(designer's call, from the builder check; the end of
  the window moved from "the first `publishDiagnostics`" during planning)* The first
  publish was the wrong boundary: on a workspace that already exists, jdtls republishes
  its stored markers before it has compiled anything with the agent (§7, about 8 s
  into the relaunch).

Lombok's errors come back, but Java support stays: the user is left where they are
today, not worse. The fallback is silent, like the rest of this feature. It records a
content-free `sirio_perf` event so a trace can show it happened.

A jdtls with the agent that crashes after answering the full build is an ordinary
crash (`Failed`). Sirio cannot tell it apart from any other crash.

## §3 Non-goals

- The stale import findings from §0, kept from jdtls's early phase until the file is
  reopened. They are a separate problem that needs its own investigation.
- Rendering the hover's Markdown (it shows `* **Since:**` raw).
- Downloading or bundling a Lombok jar.
- Changing a hand-written `languages.toml` entry.
- A setting to turn this off. *(designer's call)* Someone who wants no agent writes their
  own `java` entry, which is never touched.
- Finding the project's exact Lombok version.
- Other JVM language servers (Kotlin), and other annotation processors. MapStruct and
  the like are ordinary processors that the Maven import configures without an agent.

## §4 Anti-goals

These would make the feature a failure even if it worked:

- A restart loop, or more than one deliberate restart per key per session.
- Sirio reaching the network for this.
- A card, banner or notification about Lombok.
- A project losing Java support because the agent kept jdtls from starting or from
  finishing its first build (§2.5).
- Anything at all changing for a project that does not use Lombok.

## §5 Constraints

- **Crate rules.** `sirio_lsp/src/config.rs` is pure by construction: it reads no file.
  The jar search touches the filesystem, so it lives beside `loader.rs` or in its own
  module. Its pure parts get their own functions, testable without temporary
  directories, the way `config.rs` and `loader.rs` already split. Those parts are:
  - version ordering;
  - reading `localRepository` out of `settings.xml`;
  - choosing a jar from a directory listing;
  - the "uses Lombok" text test.

  `sirio_lsp` stays a leaf with no new local dependency. Read
  `docs/agents/architecture/crates.md` first.
- `LspSupervisor::revive` keeps its single caller.
- **Platforms.** macOS is the reference platform. Linux has the same layout. On Windows
  the paths are `%USERPROFILE%\.m2` and `%USERPROFILE%\.gradle`, with backslashes and
  possibly spaces. The argument is one argv element, never a shell string.
- **Perf-trace names are content-free.** Never put the jar path, the project path or a
  version into an event name.
- **Version.** This is a `feat:`, so the cycle-version rule in CLAUDE.md applies. If no
  `feat:` has moved the 0.36.1 cycle yet, this one moves it to 0.37.0. If PR #668 lands
  first, it already has.
- **Docs.** Add the agent to the `java` entry's story in
  `docs/agents/architecture/languages.md`. Write the release-notes line at release time.

## §6 Proof

Per CLAUDE.md the proof is end to end and ends in an artifact anyone can re-run.

**E2E with a fake server.**
- A fake `jdtls` on `PATH`, under a private `HOME` and `SIRIO_DB`.
- It wraps `Scripts/Tests/fake_lsp.py` and logs each launch with its argv.
- It is launched as the built-in entry, which is why it must be named `jdtls` and not
  configured through `languages.toml`.
- Whether this is a new script or extends `Scripts/Tests/test-editor-e2e.sh` is the
  builder's call.

Cases:

1. A Lombok project with a jar in the fake `~/.m2`: the launch carries
   `--jvm-arg=-javaagent:<that jar>`.
2. A project whose build files do not mention Lombok, with a jar present: no agent.
3. A Lombok project with no jar at start:
   - no agent at launch;
   - once the jar is placed and the trigger fires, exactly one `shutdown`/`exit` and one
     relaunch with the agent;
   - a newer jar placed afterwards causes no second restart.
4. `localRepository` in `settings.xml` is honoured.
5. A launch with the agent fails: a second launch without it follows, and the file gets
   its server. Two variants: the fake exits during `initialize` when it sees the agent
   argument, and the fake exits after `initialize` but before answering the full build.
6. During a restart the old server's stream closes, and the new server under the same
   key keeps running: the old router does not remove it.
7. A hand-written `java` entry in `languages.toml`: its argv is exactly what was
   written.
8. A launch with the agent sends one `java/buildWorkspace` after its first
   `ServiceReady`; a launch without it sends none.
9. A server that writes 200 KB to stderr before answering `initialize` reaches ready
   (§2.6). Today it hangs and ends `Failed` when the handshake times out.

It prints one OK line and is listed in `docs/agents/e2e-tests.md`.

**Unit tests** cover only the pure parts, with the ways each can fail written down
before the code:
- version ordering (`1.18.9` < `1.18.30`, suffixes);
- a version directory with no jar, with only `.lastUpdated` files, or with a zero-byte
  jar;
- `-sources` and `-javadoc` jars excluded;
- `settings.xml` with and without `localRepository`, with `${user.home}`, and with
  `localRepository` commented out;
- Gradle's hash directories;
- Windows paths.

A test stays only if it goes red when the behaviour changes.

**Real jdtls.** Only the real server proves that Lombok's errors go away in Sirio.
- Run on this Linux development machine. Enzo approved installing a Java runtime and
  jdtls there, at user level (mise or the release tarball, never a system package),
  and downloading dependencies from Maven Central. jdtls 1.63 needs Java 25.
- The project is a small Spring Boot + Lombok Maven project that the script writes
  itself, the one §7 used.
- Java takes `user.home` from the passwd entry on Linux, not from `$HOME`. A private
  `HOME` therefore needs a `jdtls` wrapper on `PATH` that adds
  `--jvm-arg=-Duser.home=$HOME` and execs the real launcher. It is still the built-in
  entry, so the run stays valid.
- Record the Problems counts (open and not-open files) for:
  - the restart case, starting from an empty local Maven repository so the import
    downloads Lombok;
  - a relaunch over the workspace that run left behind.
- The run keeps its transcript and captures as the artifact.

**Hand check on Enzo's Mac.** The work project on the reference platform. This is owed
before release. It also confirms an assumption: how jdtls was installed on that Mac was
asked and not answered, so §2.1 assumes the upstream launcher, which Homebrew and Mason
both install. If the Mac's `jdtls` does not accept `--jvm-arg`, the way the agent is
passed has to change.

## §7 Live findings (2026-10-27)

What the decisions above rest on.

**Setup:**
- jdtls 1.63.0 (`jdt-language-server-1.63.0-202610191322.tar.gz`, sha256 checked) on
  Temurin 25.0.5.
- A four-file Spring Boot 3.5.6 + Lombok project; Lombok 1.18.40 comes from Spring
  Boot's dependency management.
- A client sending Sirio's exact `initialize` capabilities and answering server
  requests like `answer_for`.
- `user.home` pointed at an empty directory.
- Logs and the client live in `~/.cache/sirio-spike/jdtls-lombok/`, which is not in
  the repository.

**Findings:**
- **Java 21 is refused.** jdtls 1.63 exits at once: its bundles require `JavaSE 25`.
- **Without the agent, empty caches:**
  - `language/status` messages arrived without any client capability declared;
  - the Lombok jar landed at 42.1 s;
  - `ServiceReady` came at 49.4 s;
  - the first `publishDiagnostics` came at 51.4 s, for all three Java files.
  - Errors in the open file: `The blank final field pegaService may not have been
    initialized`, `log cannot be resolved` and `The method setTarga(String) is
    undefined for the type LoginBody`.
  - Errors in a file that was not open: three Lombok errors.
- **With the agent, same workspace, no full build:** the open file went to 0 errors.
  The file that was not open kept its 3.
- **The same, then `java/buildWorkspace(true)`:** the answer was `1`, and every file
  went to 0 errors.
- **Exit time:** `shutdown` was answered at once, and the process ended 1.0 to 4.1 s
  after `exit`.
- **A full build, then a kill 0.5 s after `exit`, then a relaunch with the agent and no
  build:** the file that was not open stayed at 0. The workspace is saved when jdtls
  answers `shutdown`.
- **Lombok 1.18.20 as the agent, fresh workspace:** jdtls started and the open file had
  0 errors. Stderr carried 50,335 bytes of `Unsupported class file major version 69`
  stack traces.
- **A fresh workspace with the agent and populated caches:** 41 s to `ServiceReady`,
  then 0 errors.

## §8 Left to the builder

- Module layout and names, and how the supervisor marks an entry as built-in.
- Whether the E2E is a new script or extends `test-editor-e2e.sh`.
