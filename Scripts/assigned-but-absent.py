#!/usr/bin/env python3
"""Rank `FAILED — absent` ledger rows by whether a builder was ever ASSIGNED to build them.

WHY THIS EXISTS
---------------
`Scripts/stale-failed.py` ranks absent rows by how the absence was *worded* — scoped,
no-search, bare. That is the best signal obtainable from the ledger alone, and its `scoped`
flag really did describe the shape of the known stale rows.

But the ledger alone is the wrong input, and the wording was never the cause. This script
uses a different and independent input: **the task briefs**. A row named in a builder's
brief is a row someone was told to build. If the ledger still says `FAILED — absent`, then
either the builder never reached it, or it was built and nobody re-adjudicated. The second
case is a stale FAILED, and stale FAILEDs cost exactly what false PASSEDs cost — a false
PASSED ships a hole, a false FAILED sends a builder to rebuild something that exists.

THE OBSERVATION THAT MOTIVATED IT
---------------------------------
Checked by hand on the F-EDIT cluster, whose every verdict dates from pass 3 or 7:

  F-EDIT-04  "no ⌘S binding"                -> main.rs:116 (WindowCommand::SaveFile, "ctrl-s")
  F-EDIT-06  "no save path"                 -> editor.rs:641 pub fn save(), main.rs:5584
  F-EDIT-08  "add_file_tab ... no dedupe"   -> main.rs:4001 collects open_paths and dedupes

Three of eleven. They went stale not because the wording was bad but because **builders
shipped pieces in that exact area afterwards** (P61 "the editor save path", P63's
add_file_tab dedupe) and no pass re-read the rows. The predictor is the clock, not the text.

Note F-EDIT-04's row says "⌘S" while the Linux binding is correctly `ctrl-s`. **A row phrased
in the old platform's vocabulary defeats every search made in that vocabulary** — which is
its own reason a text heuristic could never have found it.

WHAT THIS BUYS, MEASURED
------------------------
Of 141 `FAILED — absent` rows, 61 (43%) are named in some builder brief and 80 are not. All
five rows known to be stale fall in the 61. So the filter roughly halves the search space
while keeping every known positive — a real narrowing, not a verdict.

THE CIRCULARITY, DISCLOSED (the same trap that ruins stale-failed.py's HIT)
--------------------------------------------------------------------------
Some briefs name a row *because it is already known to be stale* — they quote the
known-stale table so the builder does not rebuild it. Counting those would be the script
rediscovering its own inputs and reporting them as finds, exactly the flaw documented in
`stale-failed.py`'s header for HIT.

So META_BRIEFS below are excluded from the evidence, and the exclusion is printed on every
run rather than hidden. Critic and census briefs (CRITIC-*, FABLE-*) are excluded wholesale
for the same reason: they are not assignments to build.

A high rank means READ THIS ROW. It never means the row is wrong. Only the critic changes a
verdict, and only by exercising the feature.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "docs" / "linux-rewrite" / "INVENTORY-LEDGER.md"
TASKS = ROOT / "docs" / "linux-rewrite" / "tasks"
CRATES = ROOT / "rust" / "crates"

# Rows proven stale by hand AND discovered through this signal's own mechanism: a builder
# was assigned the area, went to build, and tripped over the thing already existing. These
# are what the script must find.
KNOWN_STALE = {"F-EDIT-04", "F-EDIT-06", "F-EDIT-08", "F-SET-09"}

# Rows proven stale that this signal legitimately CANNOT speak about, kept here so nobody
# "fixes" the script by loosening its control until they pass.
#
#   F-AGENT-SAFE-01 -- found by `fable`'s module census (DEAD-MODULES.md), not by any
#   builder assignment. Its only P-brief mention is P64 quoting the known-stale table, i.e.
#   the circular hit this script exists to exclude. No builder was ever told to build it, so
#   "was a builder assigned?" has nothing to say, and a control demanding this row would be
#   a control tuned to pass rather than one that tests the claim.
#
# The lesson generalises: when a control fails, the first question is whether the tool's
# claim is too broad, not whether the control is too strict. Use Scripts/stale-failed.py for
# rows in this class -- its signal is independent of assignment.
OUT_OF_SCOPE = {"F-AGENT-SAFE-01"}

# Briefs that name rows *because* they are already known stale. Counting them would be the
# script rediscovering its own input. Printed on every run so the exclusion is never silent.
META_BRIEFS = {
    "P68-the-editor-cluster-and-three-rows-that-lie.md",
    # Names F-EDIT-04 / F-SET-09 / F-AGENT-SAFE-01 only to quote the stale table (lines
    # 19-22), not to assign them. Verified by reading it, not assumed.
    "P64-the-diff-surface.md",
}

ROW = re.compile(r"^\|\s*`(F-[A-Z0-9-]+)`\s*\|\s*([^|]+?)\s*\|\s*(.*?)\s*\|\s*([^|]*?)\s*\|\s*$", re.M)
PASS_NUM = re.compile(r"pass\s*(\d+)")
BACKTICKED = re.compile(r"`([^`]+)`")
IDENT_OK = re.compile(r"^[A-Za-z_][A-Za-z_0-9]*(::[A-Za-z_][A-Za-z_0-9]*)*$")
NOISE = {
    "PASSED", "FAILED", "absent", "defective", "true", "false", "None", "Some", "Ok", "Err",
    "String", "Vec", "bool", "usize", "self", "cx", "new", "get", "set", "run", "open",
    "close", "read", "write", "test", "tests", "main", "lib", "mod",
}


def search(symbol):
    """Word-boundary search across the crate tree. Raises rather than returning a falsy
    'no results' when the backend is missing -- see probe_search()."""
    cmd = ["grep", "-rn", "--include=*.rs", "-w", "-e", symbol, str(CRATES)]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    except FileNotFoundError:
        sys.exit("grep not found -- cannot search. Refusing to report absence.")
    except subprocess.TimeoutExpired:
        sys.exit(f"search for {symbol!r} timed out -- refusing to report it as absent.")
    if r.returncode not in (0, 1):
        sys.exit(f"grep failed for {symbol!r} (rc={r.returncode}): {r.stderr.strip()[:200]}")
    return [ln for ln in r.stdout.splitlines() if ln.strip()]


def probe_search():
    """Prove the search backend works before trusting a single negative from it.

    stale-failed.py v1 used `rg`, rg was not installed, every lookup returned nothing, and
    the run still printed 'positive control ok' -- because the control validated the
    heuristics and never the backend. Do not remove this.
    """
    present, absent = "AgentAdapter", "zzz_symbol_that_must_not_exist_zzz"
    if not search(present):
        sys.exit(f"positive control FAILED: {present!r} not found in {CRATES}. A negative "
                 f"grep from an unproven search is not evidence of absence -- refusing.")
    if search(absent):
        sys.exit(f"negative control FAILED: {absent!r} matched something. Refusing to print.")
    return present


def identifiers(text):
    out = []
    for tok in BACKTICKED.findall(text):
        tok = tok.strip()
        if tok.startswith("F-") or tok in NOISE or len(tok) < 4:
            continue
        if not IDENT_OK.match(tok):
            continue
        out.append(tok)
    return sorted(set(out))


def load_briefs():
    """Builder briefs only. Critic/census briefs are not assignments to build."""
    if not TASKS.is_dir():
        sys.exit(f"task briefs not found: {TASKS}")
    out = {}
    for p in sorted(TASKS.glob("P*.md")):
        if p.name in META_BRIEFS:
            continue
        out[p.name] = p.read_text(encoding="utf-8", errors="replace")
    if not out:
        sys.exit("positive control FAILED: no builder briefs parsed -- refusing to rank on "
                 "an empty evidence base.")
    return out


def main():
    if not LEDGER.exists():
        sys.exit(f"ledger not found: {LEDGER}")
    text = LEDGER.read_text(encoding="utf-8", errors="replace")
    probe = probe_search()
    briefs = load_briefs()

    rows = []
    for rid, verdict, evidence, when in ROW.findall(text):
        if "FAILED" in verdict and "absent" in verdict:
            rows.append((rid, evidence, when))
    if not rows:
        sys.exit("positive control FAILED: no 'FAILED — absent' rows parsed. The ledger "
                 "format changed; fix the ROW regex before trusting any output.")

    all_passes = [int(n) for n in PASS_NUM.findall(text)]
    newest = max(all_passes) if all_passes else 0

    scored = []
    for rid, evidence, when in rows:
        where = sorted(n for n, t in briefs.items() if rid in t)
        if not where:
            continue  # never assigned to a builder: this script has nothing to say

        m = PASS_NUM.search(when) or PASS_NUM.search(evidence)
        p = int(m.group(1)) if m else None
        age = (newest - p) if p is not None else 0

        live = []
        for sym in identifiers(evidence):
            if search(sym):
                live.append(sym)

        # Assigned at all is the signal. More briefs = more chances the work landed. An old
        # verdict is likelier to predate that work.
        #
        # `live` is deliberately NOT scored. Adding it re-created the exact circularity that
        # `stale-failed.py`'s header documents for HIT: a row that cites identifiers as part
        # of its evidence is a *well-evidenced* row, so scoring the rediscovery of those
        # identifiers rewards rows for being specific. Measured here: including it inflated
        # the median enough to push F-SET-09 -- genuinely stale, cited in P58, 7 passes old
        # -- below the median and trip the positive control. The control was right and the
        # formula was wrong. `live` is printed as an annotation only.
        risk = 2 * len(where) + (2 if age >= 4 else 0)
        scored.append((risk, rid, where, age, p, live, evidence))

    scored.sort(key=lambda r: (-r[0], r[1]))

    # --- positive control -------------------------------------------------
    ranked = {rid: risk for risk, rid, *_ in scored}
    present_known = {r for r in KNOWN_STALE if r in {x[0] for x in [(i, e, w) for i, e, w in rows]}}
    missing = [r for r in present_known if r not in ranked]
    if missing:
        sys.exit(f"positive control FAILED: known-stale rows still marked 'FAILED — absent' "
                 f"but not ranked: {', '.join(sorted(missing))}. Either the brief evidence "
                 f"was lost or the parser broke. Refusing to print a triage that cannot find "
                 f"the rows it is known to be right about.")
    if ranked:
        flat = sorted(ranked.values())
        median = flat[len(flat) // 2]
        dull = [r for r in present_known if ranked[r] < median]
        if dull:
            sys.exit(f"positive control FAILED: known-stale rows ranked below the median "
                     f"({', '.join(sorted(dull))}). The heuristic does not detect the shape "
                     f"it was built from. Refusing to print.")
    else:
        sys.exit("positive control FAILED: nothing ranked. Refusing to print.")

    ctl = ", ".join(f"{r}=risk {ranked[r]}" for r in sorted(present_known))
    print(f"positive control ok: search backend live (`{probe}` found); {ctl}; median {median}")
    if OUT_OF_SCOPE:
        print(f"out of scope by construction (found by census, never assigned to a builder): "
              f"{', '.join(sorted(OUT_OF_SCOPE))} -- use Scripts/stale-failed.py for these")
    print(f"{len(rows)} FAILED — absent rows; {len(scored)} were assigned to a builder "
          f"({100 * len(scored) // len(rows)}%); {len(rows) - len(scored)} never were.")
    print(f"excluded as circular (they name rows *because* those are known stale): "
          f"{', '.join(sorted(META_BRIEFS)) or 'none'}")
    print(f"builder briefs read: {len(briefs)}; newest pass in ledger: {newest}\n")
    print("A high rank is a CHEAP CHECK WORTH DOING, never a verdict. Only the critic")
    print("changes a verdict, and only by exercising the feature.\n")

    for risk, rid, where, age, p, live, evidence in scored:
        aged = f"pass {p}, {age} behind" if p is not None else "no pass recorded"
        print(f"[{risk:>3}] {rid:<20} assigned in {len(where)} brief(s); {aged}")
        for n in where:
            print(f"        brief  {n}")
        for sym in live:
            print(f"        LIVE   `{sym}` present in the crate tree")
        print(f"        {evidence[:140]}")

    print("\nRows never named in any builder brief are NOT listed: this script has no")
    print("evidence about them. Absence of evidence here is not evidence the row is right --")
    print("use Scripts/stale-failed.py, whose signal is independent of this one.")


if __name__ == "__main__":
    main()
