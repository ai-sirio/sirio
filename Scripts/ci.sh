#!/bin/bash
# Single verification gate for the whole repo. This filename and its "CI OK" contract
# predate the Rust port -- CLAUDE.md, AGENTS.md, README.md, and Scripts/Tests/test-ci.sh
# all quote `Scripts/ci.sh` as *the* gate, and that contract does not change just because
# the Swift/Xcode project it used to build is gone. The only build target left in this
# repo is the Rust/gpui workspace under rust/, whose gate was already built, hardened,
# and independently tested under the name Scripts/ci-linux.sh (see
# docs/superpowers/plans/2026-08-13-linux-verification-gate.md and
# Scripts/Tests/test-ci-linux.sh) while the Swift gate above was still the one most of
# this repo's history calls "the" CI script.
#
# Rather than fork that ~500-line gate under this name -- which would give the repo two
# copies of the same logic to keep in sync, or worse, two gates that quietly disagree --
# this script simply *is* Scripts/ci-linux.sh: `exec` replaces this process with it, so
# stdout, stderr, and the exit code all pass through completely unchanged. Every doc under
# docs/ that already tells an agent to run `./Scripts/ci-linux.sh` keeps working exactly as
# written; this name is just an alias that also keeps working for anything that still
# expects `Scripts/ci.sh` to be the one gate to run.
set -euo pipefail
exec "$(dirname "${BASH_SOURCE[0]}")/ci-linux.sh" "$@"
