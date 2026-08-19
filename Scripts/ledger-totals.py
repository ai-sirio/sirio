#!/usr/bin/env python3
"""Recompute INVENTORY-LEDGER.md's Totals block from its own body.

Why this exists
---------------
The Totals block has now been wrong three times. Twice it was simply stale (it read 88
against a 146-row body for two passes). The third time is the interesting one: the critic
recomputed it correctly at the end of pass 12, and it was wrong again within minutes —
because builders append rows continuously while only the critic recomputes. A hand-maintained
aggregate over a concurrently-edited table is not "sometimes stale", it is guaranteed stale.

So the fix is not to recompute it more carefully. It is to stop maintaining it by hand.

It also catches a second defect the eye cannot: a row whose evidence contains a raw `|`
(for example the no-op handler `|_, _, _| {}`) silently shifts every column after it. Those
rows render as broken markdown and defeat any column-wise count, including this one — so
they are reported rather than guessed at.

Usage
-----
    python3 Scripts/ledger-totals.py                 # report, exit 1 if the block is stale
    python3 Scripts/ledger-totals.py --write         # rewrite the Totals block in place
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import Counter
from pathlib import Path

LEDGER = Path(__file__).resolve().parent.parent / "docs" / "linux-rewrite" / "INVENTORY-LEDGER.md"

# A row looks like:  | `F-CHAT-08` | FAILED — absent | evidence… | pass 12 |
ROW = re.compile(r"^\|\s*`(F-[A-Z0-9-]+)`\s*\|")

# Rows that look like inventory rows but are not in the denominator.
#
# The `ACP-*` appendix is the only such family today, and it is excluded **by the user's
# explicit ruling**, recorded in the ledger's own section heading: "these 14 rows are an
# appendix … the denominator stays 389". It is a decision, not an oversight — and it was
# nearly "fixed" as an oversight, because the counter simply did not match them and so said
# nothing about them at all. Silence is what made a deliberate exclusion look like a bug.
#
# So they are matched and reported, never added: the footer states how many rows sit outside
# the denominator and under which family, which is also what the ledger asks progress reports
# to say ("plus 14 newly-found ACP rows not yet in the denominator"). Their table is 3-column
# by design — no `judged` cell — so they are kept out of the malformed-row check too.
APPENDIX_ROW = re.compile(r"^\|\s*`((?!F-)[A-Z][A-Z0-9]*(?:-[A-Z0-9]+)+)`\s*\|")
FAMILY = re.compile(r"^([A-Z]+)-")

# Which `judged` values count as an independent critic pass.
#
# The criterion is structural, not reputational: the agent that set the verdict neither built
# the row nor produced the evidence it rests on. Two source shapes satisfy that by
# construction and nothing else is admitted here — a source naming the orchestrator, or a
# builder's own report, stays in the "never judged" column on purpose.
#
#   pass 12                 — a numbered critic pass
#   sweep A1-P109, 2026-08-14  — an exercise-sweep slice, where the workflow guarantees the
#                                adjudicating agent is not the one that drove or built it
#                                (docs/linux-rewrite/sweep/PLAN.md)
CRITIC_PASS = re.compile(r"pass \d+|sweep \S+, \d{4}-\d{2}-\d{2}")

# Verdicts that carry a free-text tail we fold into one bucket for counting.
#
# "PASSED" is here for `PASSED (measured)` — a pass whose evidence is a number rather than a
# screenshot. It is strictly stronger than a bare PASSED, so folding it loses nothing a count
# of "is this done" cares about; the qualifier stays visible in the row itself. No other
# verdict in ORDER has "PASSED" as a prefix, so the fold cannot swallow anything else.
PREFIX_BUCKETS = ("UNREACHABLE", "NOT EXERCISED — blocked on display", "half-proven", "PASSED")

ORDER = [
    "PASSED",
    "half-proven",
    "FAILED — absent",
    "FAILED — defective",
    "UNREACHABLE",
    "N/A — platform",
    "NOT EXERCISED",
    "NOT EXERCISED — blocked on display",
    "builder-claimed, unverified",
]


def split_cells(line: str) -> list[str]:
    """Split a markdown table row on `|`, ignoring pipes inside `backtick spans`.

    Markdown requires a literal pipe in a cell to be escaped as `\\|`; several rows carry a
    raw one inside a code span instead. Those rows are malformed, but they are malformed in a
    recoverable way, so we honour backticks and report the row rather than miscounting it.
    """
    cells: list[str] = []
    buf: list[str] = []
    in_code = False
    i = 0
    while i < len(line):
        ch = line[i]
        if ch == "\\" and i + 1 < len(line) and line[i + 1] == "|":
            buf.append("|")
            i += 2
            continue
        if ch == "`":
            in_code = not in_code
            buf.append(ch)
        elif ch == "|" and not in_code:
            cells.append("".join(buf).strip())
            buf = []
        else:
            buf.append(ch)
        i += 1
    cells.append("".join(buf).strip())
    # A well-formed row starts and ends with `|`, producing empty first/last cells.
    if cells and cells[0] == "":
        cells.pop(0)
    if cells and cells[-1] == "":
        cells.pop()
    return cells


def bucket(verdict: str) -> str:
    for prefix in PREFIX_BUCKETS:
        if verdict.startswith(prefix):
            return prefix
    return verdict


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="rewrite the Totals block in place")
    ap.add_argument("--ledger", type=Path, default=LEDGER)
    args = ap.parse_args()

    text = args.ledger.read_text(encoding="utf-8")
    lines = text.splitlines()

    counts: Counter[str] = Counter()
    families: Counter[str] = Counter()
    malformed: list[tuple[int, str, int]] = []
    raw_pipe: list[tuple[int, str]] = []
    total = 0

    for n, line in enumerate(lines, 1):
        m = ROW.match(line)
        if not m:
            appendix = APPENDIX_ROW.match(line)
            if appendix:
                fam = FAMILY.match(appendix.group(1))
                families[fam.group(1) if fam else "?"] += 1
            continue
        total += 1
        row_id = m.group(1)
        cells = split_cells(line)
        if len(cells) != 4:
            malformed.append((n, row_id, len(cells)))
            # Still count the verdict: it is cell 2 and precedes any evidence damage.
        if "|" in "".join(cells[3:]) or (len(cells) > 4):
            raw_pipe.append((n, row_id))
        verdict = bucket(cells[1]) if len(cells) > 1 else "<unparsed>"
        counts[verdict] += 1

    width = max(len(k) for k in ORDER)
    print(f"{args.ledger.name}: {total} F- rows\n")
    print("verdict counts, computed from the body:")
    for key in ORDER:
        print(f"  {key:<{width}}  {counts.get(key, 0)}")
    for key in sorted(set(counts) - set(ORDER)):
        print(f"  {key:<{width}}  {counts[key]}   <-- not in the documented vocabulary")
    print(f"  {'TOTAL':<{width}}  {sum(counts.values())}")

    if families:
        outside = ", ".join(f"{n} {fam}-*" for fam, n in sorted(families.items()))
        print(f"\noutside the denominator by the user's ruling: {outside}")
        print("report these as \"plus N rows not yet in the denominator\", never folded in.")

    if malformed:
        print(f"\n{len(malformed)} row(s) do not have exactly 4 cells — a raw `|` in the")
        print("evidence shifts every column after it, so the `judged` column cannot be")
        print("counted for these. Escape it as \\| :")
        for n, row_id, got in malformed:
            print(f"  line {n}: {row_id} has {got} cells")

    # Cross-cut on the `judged` column: which rows no critic pass has ever established.
    # This is only trustworthy once every row has exactly 4 cells, so it is suppressed
    # while any row is malformed rather than reported at an unknown accuracy.
    if not malformed:
        judged: Counter[str] = Counter()
        for line in lines:
            if not ROW.match(line):
                continue
            cells = split_cells(line)
            j = cells[3] if len(cells) > 3 else ""
            # `search`, not `fullmatch`: real stamps annotate the pass they name
            # ("pass 19 (P92 live drive)", "fable drive, 2026-08-14, pass 18"). Requiring the
            # marker to be the *whole* cell rejected 21 rows that a numbered pass had in fact
            # judged, which overstated the never-judged count by ~80% (47 reported vs 26 real).
            # A cell with no pass marker at all is still never-judged, so this credits
            # provenance without admitting orchestrator- or builder-sourced rows.
            judged["critic pass" if CRITIC_PASS.search(j) else j or "<empty>"] += 1
        never = sum(v for k, v in judged.items() if k != "critic pass")
        print(f"\nnever independently judged by a critic pass: {never}")
        for key, n in judged.most_common():
            if key != "critic pass":
                print(f"  {key:<32}  {n}")

    block = ["| verdict | count |", "|---|---|"]
    for key in ORDER:
        block.append(f"| {key} | **{counts.get(key, 0)}** |")
    block.append(f"| **total** | **{sum(counts.values())}** |")
    rendered = "\n".join(block)

    start = next((i for i, l in enumerate(lines) if l.strip() == "## Totals"), None)
    if start is None:
        print("\nNo '## Totals' heading found.", file=sys.stderr)
        return 2

    tbl_start = next(i for i in range(start, len(lines)) if lines[i].startswith("| verdict"))
    tbl_end = tbl_start
    while tbl_end < len(lines) and lines[tbl_end].startswith("|"):
        tbl_end += 1
    current = "\n".join(lines[tbl_start:tbl_end])

    if current.strip() == rendered.strip():
        print("\nTotals block matches the body.")
        return 0

    if args.write:
        lines[tbl_start:tbl_end] = rendered.splitlines()
        args.ledger.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print("\nTotals block rewritten from the body.")
        return 0

    print("\nTotals block does NOT match the body. Current:\n")
    print(current)
    print("\nShould be:\n")
    print(rendered)
    print("\nRe-run with --write to fix.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
