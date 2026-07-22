#!/bin/bash
# Records one Claude stream-json conversation turn into a fixture file.
# Requires: claude CLI installed and authenticated. Run from repo root.
set -euo pipefail
out="Packages/TillerACP/Tests/TillerACPTests/Fixtures/claude-init-turn.ndjson"
mkdir -p "$(dirname "$out")"
printf '{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Run: echo hello. Then reply with one word."}]}}\n' \
  | claude -p --input-format stream-json --output-format stream-json --verbose \
      --permission-mode acceptEdits --model claude-haiku-4-5-20251001 \
  > "$out"
echo "captured $(wc -l < "$out") lines -> $out"
