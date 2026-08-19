#!/bin/bash
# F-CHG-15 diff-load-fail repro: an untracked file `git status` can list
# without reading (a plain stat), but that `git diff --no-index` genuinely
# cannot open -- isolates diff_entry()'s own error path from status()/
# stats(), which is exactly the discrimination two prior cheap attempts
# (self-referential symlink, FIFO-timeout) failed to isolate.
set -euo pipefail
DIR="${1:-/tmp/chg15-repro}"
rm -rf "$DIR"
mkdir -p "$DIR"
cd "$DIR"
git init -q
echo "hello" > tracked.txt
git add tracked.txt
git commit -q -m init
printf 'secret content\n' > noperm.txt
chmod 000 noperm.txt
echo "Repro ready at $DIR -- add it as a Tiller project, open Changes,"
echo "expand noperm.txt: renders 'diff unavailable: git exited with status"
echo "128: ...Permission denied' while tracked.txt is unaffected (status()/"
echo "stats() for the whole snapshot never fail)."
