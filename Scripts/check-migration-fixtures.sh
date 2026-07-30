#!/bin/bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
fixture_dir="$repo_root/AppTests/Fixtures/WorkspaceMigration"

for fixture in "$fixture_dir"/*.json; do
    /usr/bin/python3 -c 'import json, sys; json.load(open(sys.argv[1]))' "$fixture"
    name="$(basename "$fixture")"
    if ! rg --files "$repo_root/AppTests" --glob '*Tests.swift' \
        | xargs rg -l --fixed-strings "$name" >/dev/null; then
        echo "fixture is not referenced by a test: $name" >&2
        exit 1
    fi
done

echo "Workspace migration fixtures OK"
