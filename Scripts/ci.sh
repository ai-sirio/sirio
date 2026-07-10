#!/bin/bash
# Single verification entrypoint: regenerates the project, builds the app,
# and runs every package's tests. Used as the gate for every task.
set -euo pipefail
cd "$(dirname "$0")/.."

xcodegen generate

# Build tillerctl so dev-build fallback exists for pane spawns.
swift build --package-path Packages/TillerControl --product tillerctl
# CODE_SIGNING_ALLOWED=NO: this build only needs to compile, not run or be
# distributed — avoids requiring a "Mac Development" cert on CI runners
# that only carry the Developer ID Application cert used for releases.
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -5

# --- Parallel package tests ---
tmpdir=$(mktemp -d /tmp/tiller-test-XXXXXX) || exit 1
trap 'rm -rf "$tmpdir"' EXIT

: > "$tmpdir/jobs"
for pkg in Packages/*/; do
    name=${pkg%/}; name=${name##*/}
    {
        cd "$pkg"
        set +e
        swift test > "$tmpdir/$name.log" 2>&1
        echo $? > "$tmpdir/$name.status"
    } &
    echo "$!:$name" >> "$tmpdir/jobs"
done

failed_names=""
while IFS=: read -r pid name; do
    wait "$pid" 2>/dev/null || true
    status=$(cat "$tmpdir/$name.status" 2>/dev/null || echo 1)
    echo "==> swift test: Packages/$name/"
    cat "$tmpdir/$name.log"
    if [ "$status" != "0" ]; then
        failed_names="$failed_names $name"
    fi
done < "$tmpdir/jobs"

if [ -n "$failed_names" ]; then
    echo "FAILED packages:$failed_names"
    exit 1
fi

echo "CI OK"
