#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
echo "Pane drag prototype: http://localhost:4173/?variant=edge"
exec python3 -m http.server 4173 --bind 127.0.0.1 --directory "$ROOT/Prototypes/PaneDragDrop"
