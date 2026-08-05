#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/../.."
FIXTURE=$(mktemp -d)
trap 'rm -rf "$FIXTURE"' EXIT

mkdir -p "$FIXTURE/bin"
cat > "$FIXTURE/bin/xcodegen" <<'EOF'
#!/bin/bash
exit 0
EOF
cat > "$FIXTURE/bin/swift" <<'EOF'
#!/bin/bash
exit 0
EOF
cat > "$FIXTURE/bin/xcodebuild" <<'EOF'
#!/bin/bash
printf '%s\n' "$*" >> "$CI_XCODEBUILD_ARGS"
case " $* " in
  *" test "*) printf 'Test run with 1 tests\n' ;;
  *) printf '** BUILD SUCCEEDED **\n' ;;
esac
EOF
chmod +x "$FIXTURE/bin/"*

CI_XCODEBUILD_ARGS="$FIXTURE/xcodebuild.args" \
PATH="$FIXTURE/bin:$PATH" \
  bash "$REPO_ROOT/Scripts/ci.sh" > "$FIXTURE/ci.log"

build_args=$(sed -n '1p' "$FIXTURE/xcodebuild.args")
test_args=$(sed -n '2p' "$FIXTURE/xcodebuild.args")

case "$build_args" in
  *" -skipPackageUpdates "*) ;;
  *) echo "FAIL: app build does not skip remote package updates" >&2; exit 1 ;;
esac
case "$test_args" in
  *" -skipPackageUpdates "*) ;;
  *) echo "FAIL: app tests do not skip remote package updates" >&2; exit 1 ;;
esac
case "$test_args" in
  *" -clonedSourcePackagesDirPath DerivedData/SourcePackages"*) ;;
  *) echo "FAIL: app tests do not use the build package checkout" >&2; exit 1 ;;
esac
case "$test_args" in
  *" -derivedDataPath "*) echo "FAIL: app tests share derived data" >&2; exit 1 ;;
esac

grep -q '^CI OK$' "$FIXTURE/ci.log"
echo "PASS: app xcodebuild actions use bounded local package resolution"
