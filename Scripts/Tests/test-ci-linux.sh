#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/Scripts/ci-linux.sh"

if [[ ! -f "$SCRIPT" ]]; then
    echo "FAIL: Scripts/ci-linux.sh is missing" >&2
    exit 1
fi
[[ -x "$SCRIPT" ]] || { echo "FAIL: Scripts/ci-linux.sh is not executable" >&2; exit 1; }

header=$(sed -n '1,24p' "$SCRIPT")
grep -Eiq 'visual|display' <<<"$header" || {
    echo "FAIL: header does not disclose that visual behavior is outside the gate" >&2
    exit 1
}

required_markers=(
    'cargo fmt'
    'cargo clippy'
    'cargo build'
    'tiller_control'
    'cargo test --workspace'
    'test-crash-supervise.py'
    'test-crash-freeze-supervise.py'
    'test-visual-sweep.sh'
    'TILLER_SOCKET'
    'TILLER_DB'
    'DISPLAY'
    'WAYLAND_DISPLAY'
    'panel create'
    'panel write'
    'panel read'
    'panel close'
    'base64'
    'nonce'
    'cmp'
    'sha256sum'
    'pgrep'
    'PANE_GROUPS'
    'ps'
    'kill -KILL -- "-$pgid"'
    'kill -TERM -- "-$APP_PID"'
    'trap'
    'CI OK'
)
for marker in "${required_markers[@]}"; do
    grep -Fq "$marker" "$SCRIPT" || {
        echo "FAIL: gate is missing required marker: $marker" >&2
        exit 1
    }
done

line_of() {
    grep -nF "$1" "$SCRIPT" | head -1 | cut -d: -f1
}

fmt_line=$(line_of 'cargo fmt')
clippy_line=$(line_of 'cargo clippy')
build_line=$(line_of 'cargo build')
test_line=$(line_of 'cargo test --workspace')
python_line=$(line_of 'test-crash-supervise.py')
visual_line=$(line_of 'test-visual-sweep.sh')
smoke_line=$(line_of 'panel create')
if ! (( fmt_line < clippy_line && clippy_line < build_line && build_line < test_line &&
    test_line < python_line && python_line < visual_line && visual_line < smoke_line )); then
    echo "FAIL: verification stages are not ordered fastest-failure first" >&2
    exit 1
fi

if grep -Fq -- '-p tillerctl' "$SCRIPT"; then
    echo "FAIL: gate builds the nonexistent tillerctl package" >&2
    exit 1
fi
if grep -Fq 'mktemp -u' "$SCRIPT"; then
    echo "FAIL: gate allocates its socket with a race-prone mktemp -u path" >&2
    exit 1
fi

grep -Fq 'source "$HOME/.cargo/env"' "$SCRIPT" || {
    echo "FAIL: gate must source the rustup cargo environment when present" >&2
    exit 1
}
grep -Fq 'cargo not found; run source ~/.cargo/env' "$SCRIPT" || {
    echo "FAIL: missing cargo needs the actionable environment hint" >&2
    exit 1
}

if grep -Fq 'PRE-EXISTING' "$SCRIPT"; then
    echo "FAIL: gate still grandfather warnings or format drift" >&2
    exit 1
fi
grep -Fq -- '-D warnings' "$SCRIPT" || {
    echo "FAIL: owned-crate clippy must fail on the first warning" >&2
    exit 1
}
grep -Fq -- '--exclude tiller' "$SCRIPT" || {
    echo "FAIL: clippy must leave the tiller main owner boundary untouched" >&2
    exit 1
}
grep -Fq -- '--exclude tiller_ui' "$SCRIPT" || {
    echo "FAIL: clippy must leave the tiller_ui owner boundary untouched" >&2
    exit 1
}

grep -Eq 'git( -C "\$ROOT")? ls-files -co --exclude-standard' "$SCRIPT" || {
    echo "FAIL: fingerprint must use Git's tracked/non-ignored file set" >&2
    exit 1
}
grep -Fq '.remember' "$SCRIPT" || {
    echo "FAIL: fingerprint must exclude runtime .remember paths" >&2
    exit 1
}
grep -Fq 'git -C "$ROOT" rev-parse --is-inside-work-tree' "$SCRIPT" || {
    echo "FAIL: smoke test must state its checkout precondition" >&2
    exit 1
}
grep -Fq 'smoke precondition' "$SCRIPT" || {
    echo "FAIL: checkout precondition needs a clear smoke-test message" >&2
    exit 1
}

echo "PASS: Linux verification gate contract is present and ordered"
