#!/bin/bash
# Enforces the module boundaries decided in tillerai/tiller#8.
set -euo pipefail
cd "$(dirname "$0")/.."
fail=0
report() { echo "boundary violation: $1"; fail=1; }

# 1. Workspace domain files in TillerCore import Foundation only.
for f in Packages/TillerCore/Sources/TillerCore/Workspace/*.swift; do
    [ -e "$f" ] || continue
    while read -r line; do
        module=${line#import }
        case "$module" in
            Foundation) ;;
            *) report "$f imports $module (workspace domain is Foundation-only)";;
        esac
    done < <(grep -E '^import ' "$f" || true)
done

# 2. TillerWorkspace never imports app/content/persistence packages.
if [ -d Packages/TillerWorkspace ]; then
    if grep -rlE '^import (TillerTerminal|TillerACP|TillerCode|TillerPersistence|TillerControl|TillerAgents|GRDB|GhosttyTerminal)' \
        Packages/TillerWorkspace/Sources >/dev/null 2>&1; then
        report "TillerWorkspace imports a forbidden module"
    fi
fi

# 3. TillerTerminal never imports TillerWorkspace.
if grep -rlE '^import TillerWorkspace' Packages/TillerTerminal/Sources >/dev/null 2>&1; then
    report "TillerTerminal imports TillerWorkspace"
fi

[ "$fail" = 0 ] && echo "module boundaries OK"
exit "$fail"
