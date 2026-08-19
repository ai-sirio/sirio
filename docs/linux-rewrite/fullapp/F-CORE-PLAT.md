# F-CORE-PLAT — finish-line critic pass (independent re-drive)

Fresh, independent critic pass over **F-CORE-PLAT-01**, the sole row in this section
(Package tier / TillerCore domain sub-group: one platform-specific reference note). Run
2026-08-19 on this host, against the warm binary at `/dev/shm/tt/debug/tiller` (built from
the checked-out `linux/gpui-waku` worktree, branch confirmed live via the app's own status
bar reading `linux/gpui-waku · ~/Scrivania/Progetti/tiller-linux`).

I did not write this code and did not echo the ledger's existing verdict. The ledger's
evidence column is not the row's contract, so I went to the actual source
(`docs/linux-rewrite/02-inventory-packages.md`) for the VERIFY clause, quoted in full below,
then independently re-derived every factual claim in it from the checked-out tree and from a
live boot of the app — not from the prior critic pass's transcript.

## Contract text (SRC: `docs/linux-rewrite/02-inventory-packages.md:73`)

> `F-CORE-PLAT-01` — The TillerCore package declares macOS 15 as its only package platform
> even though several domain utilities are otherwise portable. **PLATFORM**: Linux must
> remove or conditionalize this declaration and replace each Apple-only dependency recorded
> above. **VERIFY**: Inspect the Linux build manifest and run the resulting app only after
> the rewrite defines its platform policy; this is a reference gap, not a current-build
> step. SRC: `Package.swift:6`

This is the one row in the whole ledger (alongside its siblings like F-CORE-SET-02,
F-CORE-UI-01/02, F-CORE-AUTH-02/03) that is explicitly about a *build-manifest fact*, not a
user-observable runtime behaviour — the VERIFY clause itself says so ("a reference gap, not
a current-build step"). There is no click, keystroke, or socket call that exposes a SwiftPM
`platforms:` array; it cannot be "driven" through the UI. I still (a) independently verified
the literal source-code claim myself rather than trusting the ledger's transcript of it, and
(b) used a live boot of the actual Linux binary to confirm the operational half of the VERIFY
clause — that the resulting app runs fine regardless of what the stale manifest says.

## What I independently verified

**1. The literal manifest claim**, re-read from the file myself today (not copied from any
prior pass):

```
$ head -20 Packages/TillerCore/Package.swift
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerCore",
    platforms: [.macOS(.v15)],
    ...
```

Confirmed: still `platforms: [.macOS(.v15)]`, unconditional, no `#if os(Linux)` escape
hatch, no companion Linux manifest. `git log -- Packages/TillerCore/Package.swift` shows the
last edit to this file is `7d906690` dated **2026-07-28**, weeks before this branch's Linux
rewrite work — i.e. this is not a stale read, the file is genuinely untouched.

I also checked all nine sibling Swift packages for the same pattern:

```
$ grep -rl "platforms:" Packages/*/Package.swift
Packages/TillerGit/Package.swift Packages/TillerPersistence/Package.swift
Packages/TillerCode/Package.swift Packages/TillerControl/Package.swift
Packages/TillerCore/Package.swift Packages/TillerWorkspace/Package.swift
Packages/TillerAgents/Package.swift Packages/TillerBrowser/Package.swift
Packages/TillerTerminal/Package.swift Packages/TillerACP/Package.swift
```

All ten SwiftPM manifests in the repo are still macOS-only. None was "removed or
conditionalized" the way the PLATFORM clause literally instructs. Taken at face value, that
half of the row is simply not done.

**2. Why that literal gap does not block the Linux app**: the rewrite did not touch
`Packages/*/Package.swift` at all — it lives entirely in a parallel `rust/` Cargo workspace
(`tiller`, `tiller_project`, `tiller_activity`, `tiller_ui`, `tiller_terminal`,
`tiller_control`, `tiller_git`, `tiller_persistence`, `tiller_agents`, `tiller_acp`,
`tiller_markdown`, `tiller_theme`, `tiller_usage` — 13 crates, no crate named `tiller_core`).
Cargo has no SwiftPM-style top-level `platforms:` allow-list to conditionalize in the first
place; platform-specific code is instead cfg-gated per dependency where it's genuinely
needed:

```
$ grep -rn "target_os" rust/crates/*/Cargo.toml
tiller_ui/Cargo.toml:29:   [target.'cfg(target_os = "macos")'.dependencies]
tiller_ui/Cargo.toml:46:   [target.'cfg(target_os = "linux")'.dependencies]
tiller_terminal/Cargo.toml:18: [target.'cfg(target_os = "linux")'.dependencies]
tiller_theme/Cargo.toml:21:    [target.'cfg(target_os = "linux")'.dependencies]
tiller/Cargo.toml:28:      [target.'cfg(target_os = "linux")'.dependencies]
```

`tiller_project` — the crate that actually plays TillerCore's domain role for the Linux
build — declares no platform restriction and no macOS-only dependency at all (only `libc`,
`serde`, `serde_json`), so it already builds and runs everywhere Cargo does. The rewrite's
"platform policy" (the VERIFY clause's own phrase) is real and already in force; it is just
implemented as a structurally separate build system rather than as an edit to the old
SwiftPM file.

**3. The "run the resulting app" half of VERIFY, driven live, by me, today**:

```
export TILLER_WL_BIN=/dev/shm/tt/debug/tiller
export TILLER_WL_LABEL=sweep22plat
Scripts/wayland-drive.sh /dev/shm/sweep-22-F-CORE-PLAT '
  ctl system.ping
  shot 01-boot
  sleep 2
  ctl system.capabilities
'
```

Output: `{"id":"drive","ok":true,"result":{"pong":"true"}}`, then
`{"id":"drive","ok":true,"result":{"enabled":"true","methods":"[...68 methods...]", ...}}`.
Screenshot `/dev/shm/sweep-22-F-CORE-PLAT/02-01-boot.png` (1715x972, 4254 colours) — I looked
at it: full chrome renders correctly (Projects sidebar with Filter, Chat/Terminal tabs, "No
worktree selected" centre placeholder, Files panel, status bar with Claude/Codex badges, and
the bottom-right corner literally reading `linux/gpui-waku · ~/Scrivania/Progetti/tiller-linux`
confirming this is the real branch build, not a cached artefact). The app is fully alive and
correctly rendering on Linux, unaffected by anything `Packages/TillerCore/Package.swift`
says — because that file is not in this binary's build graph at all.

## Verdict

| row | verdict | evidence |
|---|---|---|
| F-CORE-PLAT-01 | N/A — platform | Independently re-verified both source-level claims in the VERIFY clause myself, live, today (not carried from any prior pass): (1) `Packages/TillerCore/Package.swift` still reads `platforms: [.macOS(.v15)]` unconditionally, last touched `7d906690` (2026-07-28), and all nine sibling `Packages/*/Package.swift` manifests are equally macOS-only — literally, "remove or conditionalize this declaration" has **not** happened; the file is untouched. (2) That gap has zero operational effect: the actual Linux build is a structurally separate `rust/` Cargo workspace (confirmed by `cargo`-level inspection — 13 crates, none macOS-restricted, `target_os = "linux"`/`"macos"` cfg-gates used only where a real per-OS dependency exists) which I booted live via `Scripts/wayland-drive.sh` (`system.ping` → `pong:true`, `system.capabilities` → 68 methods, boot screenshot showing full correct rendering and the `linux/gpui-waku` branch tag in the status bar). This matches the row's own VERIFY framing exactly ("a reference gap, not a current-build step") and I agree with it: the row asks for housekeeping on a file the rewrite doesn't touch, not for a behaviour the rewrite is missing. I uphold the ledger's prior `N/A — platform` verdict, but on independently reproduced evidence rather than by trusting the prior transcript, and I want the literal half explicitly on record: the manifest itself was never edited, only superseded. |

## Where I agree / disagree with the prior verdict

**Agree**, on independently reproduced evidence, not by default. The prior evidence text
("Packages/TillerCore/Package.swift still declares platforms: [.macOS(.v15)] only; no Linux
manifest replaces it") is accurate and I reproduced it byte-for-byte myself. I went further
than the prior pass by (a) checking all ten sibling manifests, not just TillerCore's, (b)
pinning the file's last-edit commit and date to prove it is genuinely untouched rather than
recently-touched-and-still-wrong, and (c) actually booting the live Linux binary this pass to
confirm the operational half of VERIFY, which the prior evidence text does not record having
done.

## Defects

None. This is a documentation/build-manifest bookkeeping gap with no user-visible or
build-blocking effect on the live Linux app — the VERIFY clause anticipates and excuses
exactly this outcome ("a reference gap, not a current-build step"). If a human wants the
letter of the PLATFORM clause satisfied too, the fix would be cosmetic: either delete the ten
now-vestigial `Packages/*/Package.swift` / `Packages/` tree (since none of it is in the Linux
build graph any more) or add a comment/README noting the Linux port lives entirely in
`rust/` and the SwiftPM manifests are retained only for the macOS build target. Neither is a
functional bug.

## What I could not reach and why

Nothing in this row is UNREACHABLE — everything the VERIFY clause asks for (inspect the
manifest; run the resulting app) was inspected and driven live this pass. The row has no
UI-drivable component by its own nature (a package manifest is not a click target), so
"driving" it consists of the source inspection plus a live-app sanity boot documented above,
not a UI interaction sequence.
