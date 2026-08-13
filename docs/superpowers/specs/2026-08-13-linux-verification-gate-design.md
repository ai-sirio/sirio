# Linux Verification Gate Design

## Goal

Add `Scripts/ci-linux.sh`, a single Linux verification command that checks the
Rust workspace, the two Python supervisor suites, and a headless control-socket
round trip without claiming that visual rendering has been verified.

## Design

The script runs from the repository root and executes stages in failure-fast
order: Rust formatting, Rust clippy, the two required package builds, the full
workspace tests, both Python suites, and a headless smoke test. Each stage has a
named log under a run-private temporary directory. A failure prints the stage
name, exit status, and useful captured output before returning nonzero.

The Rust workspace is rooted at `rust/`, so Cargo commands use
`--manifest-path rust/Cargo.toml` or run from that directory. `tiller_control`
is built as a package so its `tillerctl` binary is rebuilt as part of the gate.
Clippy warnings are reported as arrival warnings while command errors remain
fatal. Existing format drift is reported separately as pre-existing arrival
drift so the gate can still exercise later behavioral stages in this shared,
already-dirty worktree; the script never runs a formatter or changes Rust
sources.

The smoke test creates its own socket path and database path, exports them to
the app and CLI, removes both display variables, and starts the debug `tiller`
binary in a private process group. It waits for the private socket, requires
`current-workspace` to return non-empty project, path, and id fields, creates a
panel, writes a generated nonce, reads the panel, base64-decodes the returned
payload, and requires the nonce to be present. The app and all run-private
temporary paths are cleaned by an exit trap on success and failure.

The script header explicitly says that it does not cover anything visual: no
display is available for this machine, so `CI OK` is not a claim that the UI
maps, paints, or behaves correctly on screen.

## Warning cleanup

Only the clean, unowned `rust/crates/tiller_usage/src/claude.rs` warning site
will be touched. The task's owned `Scripts/**` surface receives the new gate;
the existing `Scripts/ci.sh` and the other agents' Rust files remain untouched.

## Verification

The new shell contract test checks the script's stage order, required command
names, isolation variables, cleanup trap, nonce round-trip machinery, and
visual-coverage disclaimer. The final verification runs the complete
`Scripts/ci-linux.sh` command and records its exact output and elapsed time.
