#!/usr/bin/env python3
"""Apply a fleet's structured verdicts to INVENTORY-LEDGER.md, as the single writer.

Why this exists
---------------
Seven agents adjudicating rows into one markdown table is a check-then-write race: each reads
the file, edits its own rows, and writes back a version that silently reverts whatever landed
in between. The fleet therefore never touches the ledger. Each agent returns structured
verdicts, and this script applies all of them in one pass.

It also enforces the one formatting rule the eye cannot police. A raw `|` inside an evidence
cell shifts every column after it, so the row renders as broken markdown and defeats any
column-wise count — including the Totals gate. Evidence text is escaped here rather than
trusted.

Usage
-----
    python3 Scripts/apply-verdicts.py verdicts.json              # dry run, reports what would change
    python3 Scripts/apply-verdicts.py verdicts.json --write      # apply

`verdicts.json` is a list of objects with at least `id` and `verdict`; `newEvidenceCell`,
`slice` and `why` are used when present.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "docs" / "linux-rewrite" / "INVENTORY-LEDGER.md"

_spec = importlib.util.spec_from_file_location("lt", ROOT / "Scripts" / "ledger-totals.py")
lt = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(lt)

VALID = {
    "PASSED",
    "half-proven",
    "FAILED — absent",
    "FAILED — defective",
    "UNREACHABLE",
    "N/A — platform",
    "NOT EXERCISED",
}


def clean(cell: str) -> str:
    """Make a string safe to sit in one markdown table cell."""
    return " ".join(cell.replace("|", "\\|").split())


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("verdicts", type=Path)
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--source-date", default="2026-08-14")
    args = ap.parse_args()

    incoming = json.loads(args.verdicts.read_text())
    if isinstance(incoming, dict):  # tolerate a wrapper object with a "rows" key
        incoming = incoming.get("rows", [])

    by_id: dict[str, dict] = {}
    dupes: list[str] = []
    for v in incoming:
        rid = v["id"]
        if rid in by_id and by_id[rid]["verdict"] != v["verdict"]:
            dupes.append(f"{rid}: {by_id[rid]['verdict']!r} (slice {by_id[rid].get('slice')}) "
                         f"vs {v['verdict']!r} (slice {v.get('slice')})")
        by_id[rid] = v

    bad = [f"{v['id']}: {v['verdict']!r}" for v in by_id.values() if v["verdict"] not in VALID]
    if bad:
        print("REFUSING — verdicts outside the vocabulary:", file=sys.stderr)
        for b in bad:
            print("  " + b, file=sys.stderr)
        return 2

    lines = LEDGER.read_text().splitlines(keepends=True)
    seen, changed, refreshed, identical = set(), [], [], []
    samples: list[tuple[str, str]] = []

    for i, line in enumerate(lines):
        m = lt.ROW.match(line)
        if not m:
            continue
        rid = m.group(1)
        if rid not in by_id:
            continue
        seen.add(rid)
        v = by_id[rid]
        cells = lt.split_cells(line)
        if len(cells) < 4:
            print(f"SKIP {rid}: row has {len(cells)} cells, expected 4 — repair it by hand", file=sys.stderr)
            continue

        old_verdict = cells[1].strip()
        evidence = clean(v.get("newEvidenceCell") or v.get("why") or cells[2].strip())
        source = clean(f"sweep {v.get('slice', '?')}, {args.source_date}")
        new = f"| `{rid}` | {v['verdict']} | {evidence} | {source} |\n"

        if new == line:
            identical.append(rid)
            continue
        if old_verdict == v["verdict"]:
            refreshed.append(rid)
        else:
            changed.append((rid, old_verdict, v["verdict"]))
        if len(samples) < 3:
            samples.append((line.rstrip("\n"), new.rstrip("\n")))
        lines[i] = new

    missing = sorted(set(by_id) - seen)

    print(f"ledger rows matched  : {len(seen)} of {len(by_id)} incoming verdicts")
    print(f"verdict CHANGED      : {len(changed)}")
    print(f"verdict same, cells refreshed: {len(refreshed)}")
    print(f"byte-identical       : {len(identical)}")
    if missing:
        print(f"NOT FOUND in ledger : {len(missing)} -> {', '.join(missing)}")
    if dupes:
        print("\nCONFLICTING verdicts for the same row (last one wins — resolve before writing):")
        for d in dupes:
            print("  " + d)

    moves = Counter((o, n) for _, o, n in changed)
    if moves:
        print("\ntransitions:")
        for (o, n), c in sorted(moves.items(), key=lambda kv: -kv[1]):
            print(f"  {c:>4}  {o}  ->  {n}")

    if samples:
        print("\nrendered rows (eyeball the escaping — a raw | shifts every column after it):")
        for before, after in samples:
            print(f"  -- {before[:150]}")
            print(f"  ++ {after[:150]}")

    if not args.write:
        print("\n(dry run — pass --write to apply)")
        return 0

    if dupes:
        print("\nREFUSING to write while conflicting verdicts remain.", file=sys.stderr)
        return 3

    LEDGER.write_text("".join(lines))
    print(f"\nwrote {LEDGER.relative_to(ROOT)}")
    print("now run: python3 Scripts/ledger-totals.py --write")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
