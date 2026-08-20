#!/bin/bash
# Scripts/ci.sh is now a one-line `exec` into Scripts/ci-linux.sh (see the comment at the
# top of ci.sh for why: two names, one implementation, so they cannot drift apart). Running
# the real gate here would just re-run everything Scripts/Tests/test-ci-linux.sh already
# pins down -- cargo build/test/fmt/clippy, cross-target checks, a headless smoke test --
# which takes minutes and needs a real Rust toolchain, not something a fast unit test
# should redo. What this test owns instead is the *delegation* itself: that ci.sh still
# exists, is executable, still names ci-linux.sh as its target, and does not swallow that
# target's exit status or output along the way.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/../.."
CI_SH="$REPO_ROOT/Scripts/ci.sh"
CI_LINUX_SH="$REPO_ROOT/Scripts/ci-linux.sh"

[[ -f "$CI_SH" ]] || { echo "FAIL: Scripts/ci.sh is missing" >&2; exit 1; }
[[ -x "$CI_SH" ]] || { echo "FAIL: Scripts/ci.sh is not executable" >&2; exit 1; }
[[ -f "$CI_LINUX_SH" ]] || { echo "FAIL: Scripts/ci-linux.sh is missing" >&2; exit 1; }

# Must delegate via `exec`, not a plain call: a plain call would leave ci.sh's own shell
# alive afterward, at risk of appending output or overriding the exit code — exactly the
# "swallowed" failure mode a wrapper like this exists to avoid.
grep -Eq '^\s*exec\s+"\$\(dirname "\$\{BASH_SOURCE\[0\]\}"\)/ci-linux\.sh"' "$CI_SH" || {
    echo "FAIL: Scripts/ci.sh does not exec its sibling ci-linux.sh" >&2
    exit 1
}

# set -e (or -eu, -euo pipefail, ...) is required: without it a failing ci-linux.sh could
# still let a later line in ci.sh run and print something that reads as success.
grep -Eq '^\s*set\s+-[a-z]*e[a-z]*\s' "$CI_SH" || {
    echo "FAIL: Scripts/ci.sh does not fail fast (missing set -e)" >&2
    exit 1
}

# Behavioral check: stub ci-linux.sh out for this run only, in a private copy of the
# Scripts/ directory, and confirm ci.sh's relative `dirname "$0"` addressing actually
# reaches the sibling script (not a hardcoded path that happens to work only from the
# real checkout) and that both its stdout and its exit code pass through untouched.
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

mkdir -p "$fixture/Scripts"
cp "$CI_SH" "$fixture/Scripts/ci.sh"
cat > "$fixture/Scripts/ci-linux.sh" <<'EOF'
#!/bin/bash
echo "stub ci-linux ran with args: $*"
echo "CI OK"
exit 0
EOF
chmod +x "$fixture/Scripts/ci.sh" "$fixture/Scripts/ci-linux.sh"

output=$(bash "$fixture/Scripts/ci.sh" --probe-arg)
status=$?

[[ "$status" -eq 0 ]] || { echo "FAIL: ci.sh did not propagate the stub's success exit code" >&2; exit 1; }
grep -qF 'stub ci-linux ran with args: --probe-arg' <<<"$output" || {
    echo "FAIL: ci.sh did not forward its arguments to ci-linux.sh" >&2
    exit 1
}
grep -q '^CI OK$' <<<"$output" || {
    echo "FAIL: ci.sh did not surface the delegate's CI OK line" >&2
    exit 1
}

# And the failure path: a nonzero exit from ci-linux.sh must reach ci.sh's own exit code
# unchanged, or a real gate failure could read as green to whatever calls ci.sh.
cat > "$fixture/Scripts/ci-linux.sh" <<'EOF'
#!/bin/bash
echo "stub ci-linux failing"
exit 3
EOF
chmod +x "$fixture/Scripts/ci-linux.sh"

set +e
bash "$fixture/Scripts/ci.sh" >/dev/null 2>&1
fail_status=$?
set -e
[[ "$fail_status" -eq 3 ]] || {
    echo "FAIL: ci.sh did not propagate the stub's failure exit code (got $fail_status, want 3)" >&2
    exit 1
}

echo "PASS: Scripts/ci.sh delegates to Scripts/ci-linux.sh and passes through its output and exit code"
