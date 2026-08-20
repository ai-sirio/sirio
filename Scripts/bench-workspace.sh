#!/bin/bash
# Reference benchmark job for the workspace signpost intervals -- macOS original.
#
# This script measured five named os_signpost intervals (workspaceCommandApply,
# workspaceReconcile, workspaceDragFrame, workspaceStructuralCommit, workspaceRestore) by
# enabling `debug.signpostMetrics` in the app's UserDefaults domain and reading them back
# with macOS's `log show --predicate ... --signpost`. Both of those are macOS-only: there is
# no os_signpost, no unified logging `log show`, and no UserDefaults domain on Linux/gpui,
# and grep found no equivalent instrumentation anywhere under rust/ or docs/linux-rewrite/ --
# this was checked, not assumed, before writing this notice.
#
# The Swift app that emitted these signposts was removed once the Rust/gpui port covered
# the inventory (see docs/linux-rewrite/README.md for the exact commit and how to read the
# retired source). A faithful retarget of this script would mean inventing a new
# benchmarking mechanism for the Rust port from scratch -- a new instrumentation point in
# gpui's event loop, a new way to read it back, new pass/fail thresholds -- none of which
# anyone has specified. Rather than fabricate that and risk it being read as an existing,
# validated benchmark, this script now says so plainly and stops instead of pretending to
# measure something that no longer exists.
#
# If workspace-selection performance ever needs measuring on the Rust port, the five
# interval names and their PASS/FAIL budgets above are still the ones product care about
# (workspaceCommandApply p95<=1ms, workspaceReconcile p95<=8ms, workspaceDragFrame
# p95<=16.7ms with no two consecutive frames over budget, workspaceStructuralCommit
# p95<=100ms/p99<=250ms, workspaceRestore p95<=250ms) -- reuse those, not this file's
# mechanism, which cannot run here.
echo "bench-workspace.sh measured macOS-only os_signpost/log-show instrumentation that" >&2
echo "does not exist in the Rust/gpui port and has no Linux equivalent in this tree." >&2
echo "Nothing to run here; see the comment at the top of this file." >&2
exit 1
