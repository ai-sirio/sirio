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
the release tarball, Homebrew and Mason. The builder confirms on the real launcher that
the flag reaches the JVM (§6).

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
  uses its bundled jar unless the project's is newer. The risk this leaves is covered
  in §2.5.

### §2.3 The restart

**When.** A jdtls was launched without the agent because the project uses Lombok
(§2.1, rule 2) but no jar was found. Later in the same session a jar appears. The usual
case is the first open of a project on a machine whose caches do not hold Lombok yet,
where jdtls's own Maven or Gradle import downloads it.

**What happens:**

1. Sirio stops that jdtls deliberately with `shutdown` and `exit`, the way
   `sirio_lsp::shutdown` already does. It does not kill the process.
2. Sirio does the same cleanup `language_server_gone` does today:
   - every drawer drops the server's findings;
   - open files of the key clear their gutters;
   - the documents it held are forgotten, so the next server is sent them again.

   The old server's Lombok errors must not survive the restart.
3. Sirio launches jdtls again with the agent, and sends it every open file of that key.
   This is the same path a launch follows today, ending in
   `sync_document_with_server` for each open file.
4. During the restart the status row reads "starting", which is the existing state. No
   card, no message.

**Bounds:**

- At most one deliberate restart per key per session. *(designer's call)* It follows
  the LSP client design's "no automatic restart loop" rule
  (`docs/superpowers/specs/2026-09-14-lsp-client-design.md`).
- This restart is its own path. It is not `LspSupervisor::revive`, whose documented
  single caller is a successful install, and that stays true.
- A server that crashes after the restart lands in `Dead::Failed` like any crash.
- Only a key whose project uses Lombok is ever restarted, so a jar downloaded by another
  project never restarts an unrelated jdtls.

**The trigger** (how Sirio learns that the jar appeared) **is the builder's call, made
after a live log.** Today the router drops every notification except
`textDocument/publishDiagnostics` (the `lsp.message.dropped` event in `spawn_router`),
and the client declares no `window.workDoneProgress`. Before choosing, run real jdtls on
a Lombok project with empty caches and log the method names it sends while importing.
Candidates:

- **Re-check the caches when that key publishes diagnostics, throttled.** This is the
  designer's preference. Diagnostics arrive after the build that follows the import,
  which is when the jar exists. It needs no timer and no non-standard protocol.
- **jdtls's own `language/status` notification**, if the log shows it arrives reliably
  once the import is done.
- **A filesystem watch on the cache directories.**

Whatever the trigger, it runs only for keys waiting for a jar, and never as a
free-running timer.

### §2.4 No stale findings after the agent arrives

Requirement: once jdtls runs with the agent, Problems keeps no Lombok error for any file
of the project, open or not.

The risk:

- jdtls keeps a workspace on disk (its `-data` directory) between launches.
- By default the launcher derives that directory from the name of the directory it was
  launched in. The builder confirms this on the real launcher.
- A workspace built without Lombok may hold the old error markers through an
  incremental build that sees no changed source.

This would hit Enzo's machine on the first launch after the feature ships, and it would
hit every restart of §2.3.

**The mechanism is the builder's call, after a live check.** First establish with real
jdtls whether the problem exists: relaunch with the agent over a workspace built
without it, and count Problems for files that are not open. If it exists, there are two
candidates:

- **A full build after a launch with the agent**, through jdtls's `java/buildWorkspace`
  request. Verify that the installed jdtls has it. The cost is a full build on every
  such launch, unless Sirio can tell that the workspace was already built with the
  agent.
- **Sirio passes its own `-data` directory**, keyed by the server's root and by the
  agent jar, so a change of agent means a fresh import. This also ends the default
  launcher sharing one workspace between two roots whose directories have the same
  name. But it moves every jdtls workspace and costs one full import per project the
  first time.

### §2.5 When the agent breaks the launch

An old Lombok can fail inside a newer jdtls. jdtls itself needs Java 21 or newer.

If a launch with the agent fails, Sirio launches once more without it, and that key gets
no agent for the rest of the session. *(designer's call)* A failed launch means any of:
- the spawn fails;
- the handshake fails;
- the stream closes before `initialize` answers.

Lombok's errors come back, but Java support stays: the user is left where they are
today, not worse. The fallback is silent, like the rest of this feature. It records a
content-free `sirio_perf` event so a trace can show it happened.

A jdtls that starts with the agent and crashes later is an ordinary crash (`Failed`).

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
- A project losing Java support because of the agent (§2.5).
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
5. A launch with the agent fails (the fake exits when it sees the agent argument): a
   second launch without it follows, and the file gets its server.
6. A hand-written `java` entry in `languages.toml`: its argv is exactly what was
   written.

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

**Real jdtls.** Only the real server proves that Lombok's errors go away, and only the
real server can settle §2.3's trigger and §2.4's stale markers.
- Run on Linux with a Java 21+ runtime and jdtls installed at user level (mise or the
  release tarball, never a system package).
- Use a small public Spring Boot + Lombok Maven project.
- Record:
  - Problems counts, for open and for not-open files, before and after;
  - the restart case, starting from an empty local Maven repository so the import
    downloads Lombok;
  - the method log the trigger was chosen from.
- The run keeps its transcript and captures as the artifact.

**Hand check on Enzo's Mac.** The work project on the reference platform. This is owed
before release.

## §7 Left to the builder

- Module layout and names, and how the supervisor marks an entry as built-in.
- The trigger (§2.3), chosen from the live log.
- The stale-marker mechanism (§2.4), chosen from the live check.
- Throttle values for the re-check.
- Whether the E2E is a new script or extends `test-editor-e2e.sh`.
