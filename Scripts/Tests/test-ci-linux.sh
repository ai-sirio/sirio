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
# The same honesty property, for ACP. The project's acceptance test is "connect a real
# agent over ACP, send messages, verify streaming and replies" -- and the one real-agent
# ACP test in the tree is #[ignore]d, so `cargo test --workspace` skips it. A gate that
# prints CI OK without disclosing that is claiming more than its evidence supports.
grep -Eiq 'acp' <<<"$header" || {
    echo "FAIL: header does not disclose that the real-agent ACP path is off by default" >&2
    exit 1
}

required_markers=(
    'cargo fmt'
    'cargo clippy'
    'cargo build'
    'tiller_control'
    'cargo test --workspace'
    'TILLER_ACP_REAL'
    # Anchored on the invocation, not the bare name. `real_claude` alone is also present in
    # the explanatory comment above the stage, so repointing the stage at a different test
    # left this marker green -- the check passed on the strength of prose rather than of the
    # command actually run. Verified by negative control: 's/--test real_claude/--test other/'
    # now fails, where 'real_claude' did not.
    '-p tiller_acp --test real_claude'
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
    # `--` is required: markers may begin with a dash (e.g. '-p tiller_acp --test
    # real_claude'), which grep would otherwise parse as its own options.
    grep -Fq -- "$marker" "$SCRIPT" || {
        echo "FAIL: gate is missing required marker: $marker" >&2
        exit 1
    }
done

line_of() {
    local n
    n=$(grep -nF -- "$1" "$SCRIPT" | head -1 | cut -d: -f1)
    # Without this guard a missing anchor yields an empty string, the ordering arithmetic
    # below dies under `set -e`, and the test exits non-zero having printed nothing at all.
    # It still fails, which is the point -- but it fails mutely, and the next agent has to
    # bisect the gate to learn which stage vanished. Say so instead.
    [[ -n "$n" ]] || {
        echo "FAIL: gate has no stage anchored on: $1" >&2
        exit 1
    }
    printf '%s\n' "$n"
}

fmt_line=$(line_of 'cargo fmt')
clippy_line=$(line_of 'cargo clippy')
build_line=$(line_of 'cargo build')
test_line=$(line_of 'cargo test --workspace')
# NOT line_of 'TILLER_ACP_REAL': that string also appears in the header disclosure near
# line 10, and line_of takes the first match, so it would resolve to the header and make
# the ordering assertion below compare the wrong line. Anchor on the stage label instead.
acp_line=$(line_of 'real ACP acceptance')
python_line=$(line_of 'test-crash-supervise.py')
visual_line=$(line_of 'test-visual-sweep.sh')
smoke_line=$(line_of 'panel create')
if ! (( fmt_line < clippy_line && clippy_line < build_line && build_line < test_line &&
    test_line < acp_line && acp_line < python_line &&
    python_line < visual_line && visual_line < smoke_line )); then
    echo "FAIL: verification stages are not ordered fastest-failure first" >&2
    exit 1
fi

# The ACP stage must stay opt-in. A pane without Claude credentials or the adapter download
# has to be able to reach a green gate -- the same reasoning the gate already applies to its
# sccache fallback, because an agent that can never reach a green gate learns to ignore it.
# Making this stage unconditional would turn CI OK from over-claiming into unreachable.
grep -Fq 'TILLER_ACP_REAL:-0' "$SCRIPT" || {
    echo "FAIL: the real-agent ACP stage is not defaulted off (expected \${TILLER_ACP_REAL:-0})" >&2
    exit 1
}
grep -Fq 'SKIP: real ACP acceptance' "$SCRIPT" || {
    echo "FAIL: gate does not announce the skipped ACP stage; a silent skip reads as a pass" >&2
    exit 1
}

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
