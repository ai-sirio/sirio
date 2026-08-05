#!/bin/bash
# Single verification entrypoint: regenerates the project, builds the app,
# and runs every package's tests. Used as the gate for every task.
set -euo pipefail
cd "$(dirname "$0")/.."

xcodegen generate
bash Scripts/check-migration-fixtures.sh
bash Scripts/check-module-boundaries.sh

# Build tillerctl so dev-build fallback exists for pane spawns.
swift build --package-path Packages/TillerControl --product tillerctl
# CODE_SIGNING_ALLOWED=NO: this build only needs to compile, not run or be
# distributed — avoids requiring a "Mac Development" cert on CI runners
# that only carry the Developer ID Application cert used for releases.
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates build | tail -5

tmpdir=$(mktemp -d /tmp/tiller-test-XXXXXX) || exit 1
trap 'rm -rf "$tmpdir"' EXIT

# App-target tests (TillerTests, sources in AppTests/). Deliberately NOT
# passing CODE_SIGNING_ALLOWED=NO or -derivedDataPath: with either one the
# test host hangs in dyld before test discovery on managed Macs.
# The package checkout is shared with the build above, but derived data remains
# separate so the test host keeps the managed-Mac workaround.
#
# The log lands in gitignored DerivedData rather than a trap-deleted tmpdir:
# when this step fails, the failure detail is the whole point, and a tail of
# the last lines is usually xcodebuild epilogue, not the failing assertion.
app_test_log=DerivedData/apptests.log
mkdir -p DerivedData
set +e
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages > "$app_test_log" 2>&1
app_test_status=$?
set -e
if [ "$app_test_status" != 0 ]; then
    echo "==> App tests FAILED (full log: $app_test_log)"
    grep -E "✘|error:|Test Case .* failed" "$app_test_log" | head -40
    exit 1
fi
# [1-9][0-9]* not [0-9]+: a misconfigured selector exits "TEST SUCCEEDED"
# having run zero tests, which this assertion exists to catch.
grep -qE "Test run with [1-9][0-9]* tests" "$app_test_log" || {
    echo "FAILED: App test run reported no tests (full log: $app_test_log)"; exit 1; }
tail -3 "$app_test_log"

# --- Parallel package tests ---
# TillerTerminal is excluded from the parallel batch and run on its own afterwards.
# Its PtyProcessTests spawn real PTYs and assert on wall-clock deadlines and on output
# arriving within a timeout, so they fail whenever the machine is saturated — and this
# batch saturates it. They pass consistently when run alone.

serial_pkg=TillerTerminal

: > "$tmpdir/jobs"
for pkg in Packages/*/; do
    name=${pkg%/}; name=${name##*/}
    [ "$name" = "$serial_pkg" ] && continue
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

if [ -d "Packages/$serial_pkg" ]; then
    echo "==> swift test: Packages/$serial_pkg/ (serial — timing-sensitive PTY tests)"
    ( cd "Packages/$serial_pkg" && swift test ) || failed_names="$failed_names $serial_pkg"
fi

if [ -n "$failed_names" ]; then
    echo "FAILED packages:$failed_names"
    exit 1
fi

echo "CI OK"
