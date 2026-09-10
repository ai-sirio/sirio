#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/../.."
FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# A fake cargo that records every invocation and succeeds immediately, so this
# test exercises Scripts/ci.sh's own control flow (which commands it runs, in
# what order, in what environment, and whether "CI OK" only appears after both
# succeed) without needing a real multi-minute workspace build. The fake
# cargo-nextest next to it exists only to satisfy ci.sh's `command -v`
# preflight; the gate invokes nextest through `cargo nextest`, which the fake
# cargo above already records.
mkdir -p "$FIXTURE/bin"
cat > "$FIXTURE/bin/cargo" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$CI_CARGO_ARGS"
printf 'CARGO_INCREMENTAL=%s\n' "${CARGO_INCREMENTAL:-unset}" >> "$CI_CARGO_ENV"
exit 0
EOF
chmod +x "$FIXTURE/bin/cargo"
printf '#!/bin/bash\nexit 0\n' > "$FIXTURE/bin/cargo-nextest"
chmod +x "$FIXTURE/bin/cargo-nextest"

run_gate() {
    CI_CARGO_ARGS="$FIXTURE/cargo.args" \
    CI_CARGO_ENV="$FIXTURE/cargo.env" \
    PATH="$FIXTURE/bin:$PATH" \
    "$@" bash "$REPO_ROOT/Scripts/ci.sh"
}

expect_line() {
    local which=$1 expected=$2 got
    got=$(sed -n "${which}p" "$FIXTURE/cargo.args")
    [[ "$got" == "$expected" ]] || {
        echo "FAIL: expected cargo invocation $which to be '$expected', got: $got" >&2
        exit 1
    }
}

: > "$FIXTURE/cargo.args"
: > "$FIXTURE/cargo.env"
run_gate env > "$FIXTURE/ci.log"

# --all-targets on the build stage is load-bearing, not tidiness: nextest
# builds only what it can run, so without it the examples under
# crates/*/examples -- Scripts/Tests/test-update-e2e.sh's probe among them --
# would stop being compiled by this gate at all.
expect_line 1 "build --workspace --all-targets"
expect_line 2 "nextest run --workspace --no-fail-fast --profile default"

# Exactly two cargo invocations: this gate is deliberately narrower than
# Scripts/ci-linux.sh and must not silently grow a fmt/clippy stage that isn't
# verified clean (see the comment at the top of ci.sh for why).
[[ $(wc -l < "$FIXTURE/cargo.args") -eq 2 ]] || {
    echo "FAIL: expected exactly 2 cargo invocations, got:" >&2
    cat "$FIXTURE/cargo.args" >&2
    exit 1
}

grep -q '^CI OK$' "$FIXTURE/ci.log"
echo "PASS: ci.sh builds every target, runs the workspace under nextest, and prints CI OK"

# Failure path: a nonzero exit from either cargo invocation must fail the gate
# and must never let "CI OK" print anyway.
cat > "$FIXTURE/bin/cargo" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$CI_CARGO_ARGS"
echo "boom" >&2
exit 1
EOF
chmod +x "$FIXTURE/bin/cargo"
: > "$FIXTURE/cargo.args"
: > "$FIXTURE/cargo.env"

set +e
run_gate env > "$FIXTURE/ci-fail.log" 2>&1
fail_status=$?
set -e

[[ "$fail_status" -ne 0 ]] || { echo "FAIL: ci.sh exited 0 despite cargo failing" >&2; exit 1; }
if grep -q '^CI OK$' "$FIXTURE/ci-fail.log"; then
    echo "FAIL: ci.sh printed CI OK despite cargo failing" >&2
    exit 1
fi
echo "PASS: ci.sh fails without printing CI OK when cargo fails"

# The release-gate flag. .github/workflows/build-release.yml sets
# SIRIO_CI_RELEASE_GATE=1 and nothing else does. Both directions are pinned
# here because the interesting failure is the silent one -- a skip list or a
# retry count that leaked into every local run would quietly stop checking the
# adapters' ACP claims and would hide flakes from the developer who introduced
# them, and neither would announce itself.
cat > "$FIXTURE/bin/cargo" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$CI_CARGO_ARGS"
printf 'CARGO_INCREMENTAL=%s\n' "${CARGO_INCREMENTAL:-unset}" >> "$CI_CARGO_ENV"
exit 0
EOF
chmod +x "$FIXTURE/bin/cargo"
: > "$FIXTURE/cargo.args"
: > "$FIXTURE/cargo.env"

run_gate env SIRIO_CI_RELEASE_GATE=1 > "$FIXTURE/ci-gate.log"

expect_line 1 "build --workspace --all-targets"
expect_line 2 "nextest run --workspace --no-fail-fast --profile ci"
grep -q '^CI OK$' "$FIXTURE/ci-gate.log"

# Incremental compilation off, but only here: rust/.cargo/config.toml keeps it
# on for the edit-test loop, which is the wrong trade on a machine that has no
# edit-test loop and a cold sccache.
if grep -q '^CARGO_INCREMENTAL=0$' "$FIXTURE/cargo.env"; then :; else
    echo "FAIL: the release gate should export CARGO_INCREMENTAL=0, got:" >&2
    cat "$FIXTURE/cargo.env" >&2
    exit 1
fi
if grep -q '^CARGO_INCREMENTAL=unset$' "$FIXTURE/cargo.env"; then
    echo "FAIL: the release gate leaked an unset CARGO_INCREMENTAL into a cargo call" >&2
    exit 1
fi

echo "PASS: SIRIO_CI_RELEASE_GATE=1 selects the ci nextest profile and disables incremental, and only then"

# And the local run must not: the same two knobs, checked from the other side.
: > "$FIXTURE/cargo.args"
: > "$FIXTURE/cargo.env"
run_gate env > /dev/null
if grep -q '^CARGO_INCREMENTAL=0$' "$FIXTURE/cargo.env"; then
    echo "FAIL: a local run must not disable incremental compilation" >&2
    exit 1
fi
echo "PASS: a local run keeps incremental compilation and the default profile"
