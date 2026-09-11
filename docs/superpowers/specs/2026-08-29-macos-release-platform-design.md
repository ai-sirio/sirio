# macOS as the default release platform

Date: 2026-08-29
Status: approved, not yet implemented

## What this changes

macOS becomes Sirio's reference release platform. Linux and Windows keep being
released, behind it: macOS defines the gate that decides whether a release goes
out at all, and the other two follow it in the workflow's job graph.

This reverses a decision the repo took in writing. `CLAUDE.md` opens with "a
native Linux app", and `.github/workflows/release.yml` was cut down on
2026-08-20 (`20f04dd3`) to a build-and-test stub whose own header explains why:

> A faithful *retarget* of a release pipeline needs a release process to
> retarget it at: where binaries get published, whether they're signed, what a
> "release" artifact even is for this port. None of that has been specified
> anywhere in this repo.

That is the blocker this document removes. The answers are now decided, so the
work is a retarget of the pipeline that already exists in git history, not a
design from scratch.

## Decisions

| Question | Decision |
|---|---|
| Status of Linux and Windows | Still released. macOS is the reference: it gates the release, they follow. Nothing is removed. |
| macOS distribution | Signed `.app` in a notarized DMG, published as a GitHub Release asset. |
| Architectures | Apple Silicon (arm64) only. No universal binary, no Intel. |
| How the `.app` is built | A dedicated bash script in `Scripts/`. Not Xcode, not `cargo-bundle`. |

Prerequisites confirmed as available: an active Apple Developer account, an
issued Developer ID Application certificate, a Mac for development and testing,
and the secrets from the previous Swift pipeline.

### Why a bash script rather than Xcode or a cargo plugin

Xcode used to do three jobs at once: compile, bundle, sign. Removing it lost
exactly one of them — cargo compiles and `codesign` signs; the piece with no
replacement is *bundling*, which is a directory convention plus an `Info.plist`,
not a binary format. Anything that produces the convention is a legitimate
implementation.

Against reintroducing Xcode: it means bringing back XcodeGen and an Xcode
project, deliberately removed, for a target that compiles no code, and pinning
releases to an Xcode version.

Against `cargo-bundle` / `cargo-packager`: signing support is weak or absent, it
adds a build-time tool to pin in CI, and `Info.plist` keys end up described in
`Cargo.toml`, which is the wrong place for them.

For the script: `Scripts/` is already all bash, heavily commented;
`Scripts/build-dmg.sh` is already exactly this shape and is product-agnostic;
and Zed — the codebase gpui comes from — bundles its own macOS app with a shell
script rather than Xcode. That script is worth reading as a reference before
writing ours.

## Phase 0 — the verifying build (blocks everything else)

**`Scripts/ci.sh` must print `CI OK` on a real Mac before any release tag is
pushed.** The workspace has never been compiled for macOS.

> **Closed 2026-09-10.** It prints `CI OK` on the self-hosted Mac, and the
> whole path ran from there: run 34525529495 published
> `nightly-202609102018` — signed and notarized DMG, AppImage, Inno installer,
> and a signed manifest served from `dl.sirioai.app`. Getting there needed the
> gate itself fixed first (it was failing on flaky and on genuinely wrong
> tests, not on the macOS code), and then three bugs in steps that had never
> executed anywhere because every earlier nightly died before reaching them.
> The paragraph above stays as written: it was true when written, and the
> section below is what it bought.

> **Amended 2026-08-29.** This section originally read "no workflow code is
> written until" — Phase 0 gated all implementation. The maintainer moved the
> gate to the first release tag instead, so the scripts and workflow are built
> first. The reason is availability: Phase 0 needs Mac hardware and nothing else
> in the plan does, because every script test stubs the macOS-only executable it
> drives. The cost is that those scripts are written against an unverified
> build, and a structural surprise in Phase 0 would mean reworking some of them.

```bash
rustup target add aarch64-apple-darwin
# Zig must report exactly 0.15.2
Scripts/ci.sh
```

`Scripts/ci.sh` needs no changes to serve as the macOS gate. It is portable bash
that checks for Zig 0.15.2, then runs `cargo build --workspace` and
`cargo test --workspace --no-fail-fast`. Everything Linux-specific lives in
`Scripts/ci-linux.sh` instead.

Three places this can fail, in descending order of likelihood:

1. **The `[patch."https://github.com/zed-industries/zed"]` block**
   (`rust/Cargo.toml:89-91`) replaces `gpui_platform` with a local vendored fork,
   unconditionally. That fork has only been verified on Linux and Windows. This
   is the main unknown.
2. **The objc2 tray** (`sirio/Cargo.toml:56-76`) — `NSStatusItem` compiled for
   the first time. Risk is contained because the objc2 versions are deliberately
   aligned with the ones gpui links, but it is not zero.
3. **`#[link(name = "proc")]`** (`sirio_activity/src/process.rs:188`) — the
   libproc link has never been exercised by a real linker.

Failures here are ordinary code fixes. Phase 0 closes before anything else
starts.

Note that macOS support is not hypothetical: `sirio_activity/src/process.rs:179`
carries a `macos_process` module calling `proc_listchildpids`/`proc_name`,
`sirio_control/src/server.rs:453` handles peer identity via `getpeereid`, and
`sirio/Cargo.toml:56` declares a native `NSStatusItem` tray. The doc comment at
`process.rs:5-7` describes Linux as the port of the macOS libproc walk, not the
other way round.

## `Scripts/build-app-bundle.sh`

New. Contract deliberately narrow, matching `build-dmg.sh` beside it:

```
build-app-bundle.sh <binary> <version> <output-app-path>
  -> prints the path of the signed .app
```

It takes an already-compiled binary and never compiles anything. That is what
makes it runnable by hand on a Mac without CI, which is the property that makes
it debuggable.

Produced layout:

```
Sirio.app/Contents/
├── Info.plist
├── MacOS/sirio              # the binary, renamed
└── Resources/icon.icns      # rust/assets/app-icon/icon.icns, already in the repo
```

`Info.plist` keys, carried over from the old `project.yml` and updated for the
rebrand:

| Key | Value |
|---|---|
| `CFBundleIdentifier` | `app.sirioai.sirio` (was `dev.sirio.Sirio`; prefix before that `dev.tiller`) |
| `CFBundleExecutable` | `sirio` |
| `CFBundleShortVersionString` | from `rust/Cargo.toml` (`0.6.0` today) |
| `CFBundleVersion` | identical to `CFBundleShortVersionString` |
| `LSMinimumSystemVersion` | `15.0` (unchanged from the old bundle) |
| `CFBundleIconFile` | `icon` |
| `LSApplicationCategoryType` | `public.app-category.developer-tools` |
| `CFBundleName` | `Sirio` |
| `CFBundlePackageType` | `APPL` |
| `NSHighResolutionCapable` | `true` |

`CFBundleVersion` matching `CFBundleShortVersionString` is not redundancy. They
have different semantics (user-facing version vs monotonic build number), and
the old `project.yml` comment records the rule: Sparkle compares the latter, so
it must increase every release. Keeping the rule costs nothing and leaves the
auto-update door open.

Then `codesign --options runtime` (hardened runtime, required for notarization)
with the Developer ID identity.

**Entitlements: start with none.** An app that spawns shells and reads PTYs does
not need any a priori — the hardened runtime blocks dylib injection, not
`fork`/`exec`. The exceptions in the old `project.yml` (lines 59-73) were about
Xcode's debug dylib and injection bundle, which are test-time concerns. If
notarization or first launch fails, add the specific exception the error names —
not a pre-emptive entitlements file full of permissions nothing uses.

The binary is self-contained: `ghostty-vt` links statically, `rusqlite` uses its
`bundled` feature, and gpui links system frameworks that are never bundled. No
nested frameworks, no separate dylibs to sign — a single executable in
`Contents/MacOS/`. This is the easy case for notarization; the most common
failure class, unsigned nested items, does not exist here.

## Recovered scripts

Both are in git history and come back from `c47234de^`.

- **`Scripts/build-dmg.sh`** — restored verbatim. It takes an `.app` path, a
  volume name and an output path, stages a copy with an `/Applications` symlink,
  and runs `hdiutil create`. Nothing about it was Swift-specific.
- **`Scripts/check-release-version.sh`** — restored and retargeted. Its source of
  truth moves from `project.yml` to `rust/Cargo.toml`'s
  `[workspace.package] version`. Tag `v0.6.0` must match `version = "0.6.0"`.

`Scripts/generate-changelog.sh` is still in the tree, but it did need one change.
Its no-previous-tag fallback sets the range to the bare tag, which on a repository
with no tags means `git log <tag>` walks the entire history: 2552 commits, a
140,547-character body, against GitHub's 125,000-character release-body limit.
The first release would have failed at `gh release create` — after signing and
notarization had already succeeded. It now caps that path at the 100 most recent
commits with a note saying how many were omitted; the with-previous-tag path is
unchanged.

## The gate, and what "macOS is the reference" means

"macOS is the reference" is about *releases*, not the daily loop. Development
happens on Windows; a gate that required a Mac would make every task depend on
hardware that is not at hand. `Scripts/ci.sh` stays the portable local gate that
`CLAUDE.md` already tells contributors to run before considering a task done —
it works unchanged under Git Bash on Windows, on Linux and on macOS.

Two roles for one script, not two competing scripts.

### Job graph

```
tag v*.*.*
    │
    ▼
┌─────────────────────────────────────────┐
│ macos  [self-hosted, macOS, arm64]      │  gates everything
│ Scripts/ci.sh → build-app-bundle.sh     │
│ → notarize → build-dmg.sh               │
└───────────────┬─────────────────────────┘
                │ needs: macos
        ┌───────┴────────┐
        ▼                ▼
┌───────────────┐  ┌───────────────┐
│ linux         │  │ windows       │
│ ubuntu-22.04  │  │ windows-latest│
│ setup-zig     │  │ setup-zig     │
└───────┬───────┘  └───────┬───────┘
        └────────┬─────────┘
                 ▼
        ┌─────────────────┐
        │ publish         │
        │ gh release      │
        └─────────────────┘
```

`needs: macos` makes the priority literal rather than documented. A comment
explaining which platform leads can be ignored; a dependency edge cannot. It
also means a broken macOS build does not burn runner minutes producing artifacts
that will not be published.

### Runners

**macOS: self-hosted, on the maintainer's Mac.** This is what the repo already
chose: `73f0a727 ci(release): use self-hosted runner instead of GitHub-hosted
macos-15`, followed by `da9d18da fix(release): unlock login keychain for
self-hosted runner CI gate`. Those two commits together state the trade-off — a
self-hosted runner has the certificate already in the login keychain and costs
no minutes, but it is not a clean room, and it is precisely because the keychain
persists that an unlock step was needed.

It also keeps `~/.cargo` and `target/` warm between releases, which for a
workspace that rebuilds ~400 crates including gpui is the difference between a
release taking minutes and taking tens of minutes.

The price: the Mac must be online with the runner active for a release to go.
If that becomes a problem, GitHub-hosted `macos-14` is a one-line change — the
workflow already creates an ephemeral keychain and imports the certificate, so
it works either way.

**Linux and Windows: GitHub-hosted**, both with `mlugg/setup-zig@v2` pinned to
`0.15.2`. On the self-hosted Mac, Zig is installed once; `Scripts/ci.sh` catches
a version mismatch with a named error.

Linux pins `ubuntu-22.04` explicitly, never `ubuntu-latest`: the binary links
against the runner's glibc, which becomes the minimum requirement for users.
With `latest` that requirement changes underneath you when GitHub updates the
image, and you find out from a bug report.

## The macOS job, step by step

Most of the original pipeline survives unchanged.

| Step | Origin |
|---|---|
| Checkout | verbatim |
| Check version matches tag | retargeted to `rust/Cargo.toml` |
| Verify release scripts | new; runs the four `Scripts/Tests/` release tests on the runner before anything is signed |
| ~~Install XcodeGen~~ | dropped |
| ~~Unlock login keychain~~ | dropped; see below |
| Run CI gate — `Scripts/ci.sh` | verbatim (now the Rust gate) |
| Import signing certificate | verbatim |
| ~~xcodegen + xcodebuild archive + exportArchive~~ | replaced: `cargo build --release --target aarch64-apple-darwin` + `build-app-bundle.sh` |
| Notarize and staple | verbatim; only the input path changes |
| Build DMG | `build-dmg.sh` restored, verbatim |
| Sign, notarize and staple the DMG | new; see below |
| Upload the DMG | new; the artifact hand-off to `publish` |
| ~~Generate appcast~~ | deferred (see Out of scope) |
| ~~Generate changelog~~ | moved to the `publish` job |
| ~~Publish release~~ | moved to the `publish` job |
| Clean up keychain | verbatim |

Six steps unchanged, two retargeted or replaced, three new, two moved to the
`publish` job, three dropped.

**Why the changelog and the release move out of the macOS job.** Publishing is
the one thing that must not happen until *all three* platforms have produced an
artifact, and a step inside `macos` cannot wait on `linux` and `windows` — that
is what a fourth job with `needs: [macos, linux, windows]` expresses. It also
means the write permission the release call needs lives on a job that compiles
nothing and runs no third-party code.

**Why the DMG is signed and notarized too.** The `.app` is notarized and stapled
before the DMG is built, but a notarization ticket is per-artifact: the one on
`Sirio.app` says nothing about the container it ends up inside. The DMG is what
the user downloads, so the DMG is what carries the quarantine flag and what
Gatekeeper evaluates first. Stapling its own ticket makes that check resolve
offline, so first open works without a live call to Apple. It costs a second
round-trip on every release.

**Why the login-keychain unlock is gone.** `da9d18da` added that step because the
Swift app's `KeychainCredentialStore` made the gate fail intermittently against a
locked keychain. That class went with the Swift app. The only macOS keychain use
left in the Rust workspace is `keychain_cookie` in
`rust/crates/sirio_usage/src/opencode_go.rs`, which shells out to
`security find-generic-password` and returns `None` on any failure, so a locked
keychain cannot fail the gate. Meanwhile the step unlocked the maintainer's
personal login keychain before ~400 crates' build scripts and test binaries ran.
Nothing downstream needs it: the workflow creates its own `ci.keychain` and puts
it first in the search list, `codesign` finds the identity there, `notarytool`
authenticates with the API key file, and `stapler` needs no credentials.

The version-vs-tag check sits second on purpose. Ordering steps by increasing
cost of failure is the same principle as `needs: macos` above: a version
mismatch found after notarization costs Apple's server round-trip; found early
it costs two seconds.

## Linux and Windows artifacts

- **Linux** — `sirio-<version>-x86_64-linux.tar.gz` containing `sirio` and
  `sirioctl`
- **Windows** — `sirio-<version>-x86_64-windows.zip` containing `sirio.exe` and
  `sirioctl.exe`

Unsigned. On Windows this means SmartScreen on first launch; fixing it needs an
Authenticode certificate, which is a separate purchase and a separate decision.
Recorded here so it is a known choice rather than a surprise from a bug report.

## Secrets

The repo has no tags yet and the secrets lived in the previous repo, so they
must be recreated in `ai-sirio/sirio`:

```
DEVELOPER_ID_CERTIFICATE_P12         certificate, base64
DEVELOPER_ID_CERTIFICATE_PASSWORD    .p12 password
KEYCHAIN_PASSWORD                    ephemeral CI keychain
ASC_API_KEY_P8                       App Store Connect key, base64
ASC_API_KEY_ID                       key id
ASC_API_ISSUER_ID                    issuer id
```

`MAC_LOGIN_KEYCHAIN_PASSWORD` is deliberately absent from that list. The
Swift-era pipeline needed it; this one does not, for the reasons under "The macOS
job, step by step" above. Do not recreate it — adding the secret back is the
first half of adding the step back.

`SPARKLE_ED_PRIVATE_KEY` is not needed now, but must not be discarded: it is the
only one that cannot be regenerated without invalidating updates for anyone who
already has the app installed.

## Out of scope, deliberately

- **Sparkle and auto-update.** The appcast step is recoverable from git as one
  piece when wanted, and the `CFBundleVersion` rule above keeps the door open.
- **Universal binary.** arm64 only. This also means `zig build` runs once rather
  than twice, and no `lipo` of Zig-produced static libraries — much less
  well-trodden ground than merging two Rust binaries.
- **Homebrew cask.** Wants a stable download URL, which GitHub Releases will
  provide. Adds later without touching any of this.
- **Windows code signing.** Separate certificate, separate cost.
- **Native Linux packages** (.deb, AppImage, Flatpak). The tarball is enough to
  start.

## Risks

1. **Phase 0 is unverified.** The `gpui_platform` patch is the real unknown, and
   everything else depends on it.
2. **The self-hosted runner must have Zig 0.15.2.** `Scripts/ci.sh` catches it,
   but it will fail the first release if forgotten.
3. **The first notarization may need iterations** — not because the bundle is
   complex, but because it is the first non-Xcode binary Apple sees under this
   Developer ID.

## Deliverables

```
Scripts/build-app-bundle.sh                    new
Scripts/build-dmg.sh                           restored from c47234de^, verbatim
Scripts/check-release-version.sh               restored, retargeted to Cargo.toml
Scripts/Tests/test-build-app-bundle.sh         new
Scripts/Tests/test-build-dmg.sh                new
Scripts/Tests/test-check-release-version.sh    new
Scripts/Tests/test-release-workflow.sh         new
Scripts/generate-changelog.sh                  capped for the untagged first release
Scripts/Tests/test-generate-changelog.sh       extended: the untagged case
Scripts/ci-linux.sh                            runs the five new tests
.github/workflows/release.yml                  rewritten: 4 jobs, needs: macos
CLAUDE.md                                      "What this is" and "Commands" updated
```

The four tests follow the repo's own convention ("Tests first"), and every macOS-only
executable they touch — `codesign`, `hdiutil` — is stubbed the way
`Scripts/Tests/test-ci.sh` already stubs `cargo`. That keeps them runnable on any
POSIX host rather than only on the release machine.

The signing identity is **not** an eighth secret. It is read back at run time from
the certificate the workflow has just imported into `ci.keychain`, because the
certificate already carries its own name and a stored copy would be a second source
of truth free to drift from it.

`CLAUDE.md` is not an afterthought. It is the file that instructs every agent
working in this repo, it already lags the code (`process.rs` describes Linux as
the port of the macOS path), and this decision makes its opening line actively
misleading.

## References

| What | Where |
|---|---|
| Original signed/notarized pipeline | `git show 20f04dd3^:.github/workflows/release.yml` |
| `build-dmg.sh`, `check-release-version.sh` | `git show c47234de^:Scripts/<name>` |
| Old bundle keys, deployment target | `git show 3b6fc5b9^:project.yml` |
| Move to self-hosted runner | `73f0a727`, `da9d18da` |
| Windows/Zig/MSVC findings | `docs/prototypes/ghostty-pane-windows.md:148-166` |
| Why cross-compiling from one host fails | `Scripts/ci-linux.sh`, the `CROSS_TARGET_KNOWN_WALL` comment |
