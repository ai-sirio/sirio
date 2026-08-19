#!/bin/bash
# F-CORE-DOM-02 branch-precedence repro: a project whose PRIMARY worktree is
# checked out on a non-default branch (git's own init default is "master";
# the primary worktree here is on "primary-feature", one commit ahead), so
# blank-vs-explicit base branch resolution is a real discriminator instead
# of accidentally being the same ref.
set -euo pipefail
DIR="${1:-/tmp/dom02-repro}"
rm -rf "$DIR"
mkdir -p "$DIR"
cd "$DIR"
git init -q -b master
echo "root" > README.md
git add README.md
git commit -q -m init
git checkout -q -b primary-feature
echo "feature work" >> README.md
git add README.md
git commit -q -m "feature commit"
echo "Repro ready at $DIR (primary worktree on primary-feature, master one"
echo "commit behind). Add as a Tiller project, New Worktree twice:"
echo "  1) branch name only, base branch left BLANK -> should land on"
echo "     primary-feature's tip, not master's."
echo "  2) branch name + base branch = 'master' explicitly -> should land"
echo "     on master's tip, overriding the primary-branch fallback."
