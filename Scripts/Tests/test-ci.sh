#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/../.."
FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

# A fake cargo that records every invocation and succeeds immediately, so this test
# exercises Scripts/ci.sh's own control flow (which commands it runs, in what order, and
# whether "CI OK" only appears after both succeed) without needing a real multi-minute
# workspace build.
mkdir -p "$FIXTURE/bin"
cat > "$FIXTURE/bin/cargo" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$CI_CARGO_ARGS"
exit 0
EOF
chmod +x "$FIXTURE/bin/cargo"

CI_CARGO_ARGS="$FIXTURE/cargo.args" \
PATH="$FIXTURE/bin:$PATH" \
  bash "$REPO_ROOT/Scripts/ci.sh" > "$FIXTURE/ci.log"

build_args=$(sed -n '1p' "$FIXTURE/cargo.args")
test_args=$(sed -n '2p' "$FIXTURE/cargo.args")

case "$build_args" in
  "build --workspace") ;;
  *) echo "FAIL: expected 'cargo build --workspace' first, got: $build_args" >&2; exit 1 ;;
esac
case "$test_args" in
  "test --workspace --no-fail-fast") ;;
  *) echo "FAIL: expected 'cargo test --workspace --no-fail-fast' second, got: $test_args" >&2; exit 1 ;;
esac
# Exactly two cargo invocations: this gate is deliberately narrower than
# Scripts/ci-linux.sh and must not silently grow a fmt/clippy stage that isn't
# verified clean (see the comment at the top of ci.sh for why).
[[ $(wc -l < "$FIXTURE/cargo.args") -eq 2 ]] || {
    echo "FAIL: expected exactly 2 cargo invocations, got:" >&2
    cat "$FIXTURE/cargo.args" >&2
    exit 1
}

grep -q '^CI OK$' "$FIXTURE/ci.log"
echo "PASS: ci.sh runs cargo build --workspace then cargo test --workspace and prints CI OK"

# Failure path: a nonzero exit from either cargo invocation must fail the gate and must
# never let "CI OK" print anyway.
cat > "$FIXTURE/bin/cargo" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$CI_CARGO_ARGS"
echo "boom" >&2
exit 1
EOF
chmod +x "$FIXTURE/bin/cargo"
: > "$FIXTURE/cargo.args"

set +e
CI_CARGO_ARGS="$FIXTURE/cargo.args" \
PATH="$FIXTURE/bin:$PATH" \
  bash "$REPO_ROOT/Scripts/ci.sh" > "$FIXTURE/ci-fail.log" 2>&1
fail_status=$?
set -e

[[ "$fail_status" -ne 0 ]] || { echo "FAIL: ci.sh exited 0 despite cargo failing" >&2; exit 1; }
if grep -q '^CI OK$' "$FIXTURE/ci-fail.log"; then
    echo "FAIL: ci.sh printed CI OK despite cargo failing" >&2
    exit 1
fi

echo "PASS: ci.sh fails without printing CI OK when cargo fails"

# The release-gate flag. .github/workflows/build-release.yml sets
# SIRIO_CI_RELEASE_GATE=1 so the two `sirio_agents` conformance tests that probe the
# agent CLIs installed on the runner cannot stop a release; nothing else sets it. Both
# directions are pinned here because the interesting failure is the silent one -- a
# skip list that leaked into every local run would quietly stop checking the adapters'
# ACP claims, which is the only place those claims are ever checked.
cat > "$FIXTURE/bin/cargo" <<'EOF'
#!/bin/bash
printf '%s
' "$*" >> "$CI_CARGO_ARGS"
exit 0
EOF
chmod +x "$FIXTURE/bin/cargo"
: > "$FIXTURE/cargo.args"

CI_CARGO_ARGS="$FIXTURE/cargo.args" PATH="$FIXTURE/bin:$PATH" SIRIO_CI_RELEASE_GATE=1   bash "$REPO_ROOT/Scripts/ci.sh" > "$FIXTURE/ci-gate.log"

gate_test_args=$(sed -n '2p' "$FIXTURE/cargo.args")
expected="test --workspace --no-fail-fast -- --skip opencode_answers_the_acp_handshake_it_claims --skip oh_my_pi_is_only_claimed_once_it_answers"
case "$gate_test_args" in
  "$expected") ;;
  *) echo "FAIL: release gate should run '$expected', got: $gate_test_args" >&2; exit 1 ;;
esac
grep -q '^CI OK$' "$FIXTURE/ci-gate.log"

# The build stage is untouched by the flag: it is the same workspace either way.
gate_build_args=$(sed -n '1p' "$FIXTURE/cargo.args")
case "$gate_build_args" in
  "build --workspace") ;;
  *) echo "FAIL: the release gate must not change the build stage, got: $gate_build_args" >&2; exit 1 ;;
esac

echo "PASS: SIRIO_CI_RELEASE_GATE=1 skips only the agent-CLI probes, and only then"
