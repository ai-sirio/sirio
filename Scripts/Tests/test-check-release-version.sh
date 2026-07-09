#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="$SCRIPT_DIR/../check-release-version.sh"

FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

cat > "$FIXTURE/project.yml" <<'EOF'
targets:
  Tiller:
    info:
      properties:
        CFBundleShortVersionString: "0.3.0"
EOF

OUTPUT=$("$CHECK_SCRIPT" "v0.3.0" "$FIXTURE/project.yml")
if [ "$OUTPUT" != "0.3.0" ]; then
  echo "FAIL: expected '0.3.0', got '$OUTPUT'" >&2
  exit 1
fi
echo "PASS: matching version"

if "$CHECK_SCRIPT" "v9.9.9" "$FIXTURE/project.yml" 2>/dev/null; then
  echo "FAIL: expected non-zero exit for mismatched version" >&2
  exit 1
fi
echo "PASS: mismatched version rejected"

if "$CHECK_SCRIPT" "v0.3.0" "$FIXTURE/does-not-exist.yml" 2>/dev/null; then
  echo "FAIL: expected non-zero exit for missing project.yml" >&2
  exit 1
fi
echo "PASS: missing project.yml rejected"

echo "All tests passed."
