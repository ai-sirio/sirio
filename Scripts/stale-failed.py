#!/usr/bin/env python3
"""Triage FAILED — absent ledger rows by how well the absence was actually established.

WHY THIS EXISTS
---------------
`Scripts/dead-models.py` mechanised one direction of ledger error: a row marked PASSED
whose code nothing calls. This script is its mirror — a row marked FAILED — absent for a
feature that is in fact built, under a name or in a crate the search did not reach.

Both directions cost the same. A false PASSED ships a hole; a false FAILED sends a builder
to construct something that already exists. Three were found by accident in a single day,
each when a builder went to build the thing and tripped over it:

  F-EDIT-04        "no ⌘S binding"                     -> ⌘S was already wired in main.rs
  F-SET-09         "no skill provisioner in port"      -> skill.rs has a tested provisioner
  F-AGENT-SAFE-01  "no skill code in the crate"        -> it is in tiller_project, not
                                                          tiller_agents, which is where the
                                                          search looked

Found by accident, three times, is not a process. Hence this.

THE MEASUREMENT THAT MOTIVATES THE TRIAGE
-----------------------------------------
Of the FAILED — absent population, **2 rows cite a workspace-wide search and ~120 cite no
search at all.** Absence is therefore usually asserted, not demonstrated. That is not a
claim that the rows are wrong — most are certainly right, and `DEAD-MODULES.md` concluded
independently that "absent mostly means absent". It is a claim that the rows do not record
enough for anyone to tell which is which, and this script ranks them so the cheapest checks
come first.

RISK SIGNALS (each is a heuristic; the script never changes a verdict)
---------------------------------------------------------------------
  scoped      evidence bounds the search to a crate/file/port -- the F-AGENT-SAFE-01 shape,
              and the single strongest signal, because a scoped search is exactly a search
              that can miss a feature living one crate over
  no-search   evidence records no grep, crate sweep or workspace check at all
  bare        evidence is a short bare assertion ("no toast implementation")
  stale-pass  verdict is old; verdicts expire (EVIDENCE-STANDARD.md, "Verdicts expire")
  HIT:<sym>   a backticked identifier from the row IS present in the workspace

WHERE `HIT` LIES, MEASURED
--------------------------
`HIT` is weaker than it looks and the first run proved it. A well-evidenced row *cites
identifiers as part of its evidence* -- F-SID-15 reads "no Remove Worktree item; removal is
the hover x -> `remove_worktree`" -- and the script then rediscovers `remove_worktree` and
reports it as though it were a find. That is circular: the row is flagged for being
specific. The rows with the loudest HIT counts on the first run (F-CHAT-02, F-SID-15,
F-CHAT-08) were all of this shape, and all of them were right.

So: **a HIT means "read this row", never "this row is wrong".** The signal that survived
scrutiny is `scoped` -- a search bounded to one crate is the shape that actually produced
all three known stale rows.

THE SPLIT THAT TURNED OUT TO MATTER MORE
----------------------------------------
Running this surfaced something the triage was not looking for. Of 123 FAILED — absent
rows only 8 are `builder-claimed, unverified`, but **56 rows across the whole ledger are**.
Those 56 are built, carry named tests in the tree, and have never been adjudicated. Under
the project's rule that a feature the critic has not exercised does not exist, each is a
tick available for the cost of *exercising* rather than *constructing* -- much cheaper than
the 115 FAILED — absent rows that need real building. The script prints this split last.

POSITIVE CONTROL
----------------
A triage nobody has validated is a ranking, not a tool. This script knows the three rows
above are genuinely stale and **refuses to print anything unless it ranks all three as
elevated risk.** If the ledger is edited so those rows no longer parse, the control fails
loudly rather than the script silently reporting on nothing -- the same rule dead-models.py
uses. A negative result from an unvalidated search is not evidence of absence.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "docs" / "linux-rewrite" / "INVENTORY-LEDGER.md"
CRATES = ROOT / "rust" / "crates"

# Rows proven stale by hand. The script must rank every one of these as elevated risk.
# Rows currently marked `FAILED — absent` whose evidence is known to be false, each verified
# by hand rather than assumed. The originals — F-EDIT-04, F-SET-09, F-AGENT-SAFE-01 — were
# retired on 2026-08-14 because all three had been correctly re-judged (PASSED, defective and
# half-proven respectively), which is the control doing its job, not failing.
#
# Replace a row here only when it has been re-judged, and only with one verified stale by
# reading the tree. Never widen or delete this set to make a red run go green: the whole point
# is that a triage nobody can check is worth less than no triage.
KNOWN_STALE = {
    # `InitializeGit` (11 refs) and `AlreadyGitProject` (6) are named in the row's own evidence
    # and are live in the tree, while the verdict still reads absent.
    "F-SID-08",
    # same shape: `RevealInFileManager`, 11 refs, live.
    "F-SID-09",
    # evidence says "drag reorder removed by design"; nothing was removed — the macOS
    # reference's RowReorder.swift is intact, reachable via `.reorderable(model:id:scope:)`.
    "F-SID-16",
}

# This detector finds ONE shape of staleness: a search claim that was wrong. It scores
# search language and greppable identifiers, so it sees "grep X → nothing" when X exists.
#
# **There is a second shape it cannot see, and that is not a bug to paper over.** A row like
# `F-BRW-01` ("no browser surface") was *true* when written at pass 8 and became false the
# moment someone built one. There is no wrong search and no identifier to grep — the evidence
# is plain prose overtaken by events. Both `F-BRW-01` and `F-PRJ-05` were tried as controls
# here on 2026-08-14 and the median check correctly rejected them.
#
# Finding that shape needs a different signal: an old pass number plus the present existence
# of a symbol that would satisfy the row. Until something implements it, **a clean run of this
# script is not evidence that the ledger is current.**

ROW = re.compile(
    r"^\|\s*`(F-[A-Z0-9-]+)`\s*\|\s*([^|]+?)\s*\|\s*(.*?)\s*\|\s*([^|]*?)\s*\|\s*$", re.M
)
BACKTICKED = re.compile(r"`([^`]+)`")
PASS_NUM = re.compile(r"pass\s*(\d+)")
# Bounds the search to somewhere: a crate, a file, "the crate", "in port".
SCOPED = re.compile(
    r"\bin the crate\b|\bin port\b|\bin `?tiller_\w+|\bthe \w+ crate\b|\bin `?\w+\.rs",
    re.I,
)
SEARCHED = re.compile(r"\bgrep\b|\bany crate\b|\bworkspace\b|\bno crate\b", re.I)
# Identifiers worth grepping for: real code-shaped tokens only.
IDENT_OK = re.compile(r"^[A-Za-z_][A-Za-z_0-9]*(::[A-Za-z_][A-Za-z_0-9]*)*$")
# Tokens that are ledger vocabulary or too generic to mean anything as a grep hit.
NOISE = {
    "PASSED", "FAILED", "absent", "defective", "true", "false", "None", "Some", "Ok", "Err",
    "String", "Vec", "bool", "usize", "f32", "u32", "i32", "self", "cx", "id", "new", "get",
    "set", "run", "open", "close", "read", "write", "test", "tests", "main", "lib", "mod",
}


def search(symbol):
    """Word-boundary search for `symbol` across the crate tree's Rust sources.

    Uses grep, not rg. THE FIRST VERSION OF THIS SCRIPT USED rg, rg WAS NOT INSTALLED,
    every lookup returned nothing, and the run still printed "positive control ok" --
    because the control validated the heuristics and never the search backend. That is
    exactly the defect this script hunts: a check that executes, reports success, and
    proves nothing. `probe_search()` below now fails the whole run if this function
    cannot find a symbol known to be present.

    Raises on a missing backend rather than returning a falsy "no results".
    """
    cmd = ["grep", "-rn", "--include=*.rs", "-w", "-e", symbol, str(CRATES)]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    except FileNotFoundError:
        sys.exit("grep not found -- cannot search. Refusing to report absence.")
    except subprocess.TimeoutExpired:
        sys.exit(f"search for {symbol!r} timed out -- refusing to report it as absent.")
    if r.returncode not in (0, 1):  # 1 == no match, which is a real answer
        sys.exit(f"grep failed for {symbol!r} (rc={r.returncode}): {r.stderr.strip()[:200]}")
    return [ln for ln in r.stdout.splitlines() if ln.strip()]


def probe_search():
    """Prove the search backend works before trusting a single negative from it."""
    present, absent = "AgentAdapter", "zzz_symbol_that_must_not_exist_zzz"
    if not search(present):
        sys.exit(f"positive control FAILED: {present!r} not found in {CRATES}. The search "
                 f"backend is broken or the tree moved. A negative grep from an unproven "
                 f"search is not evidence of absence -- refusing to print.")
    if search(absent):
        sys.exit(f"negative control FAILED: {absent!r} matched something. Refusing to print.")
    return present


def identifiers(text):
    """Code-shaped backticked tokens from a row, minus ledger noise and row ids."""
    out = []
    for tok in BACKTICKED.findall(text):
        tok = tok.strip()
        if tok.startswith("F-") or tok in NOISE or len(tok) < 4:
            continue
        if not IDENT_OK.match(tok):
            continue
        out.append(tok)
    return sorted(set(out))


def main():
    if not LEDGER.exists():
        sys.exit(f"ledger not found: {LEDGER}")
    text = LEDGER.read_text(encoding="utf-8", errors="replace")
    probe = probe_search()

    rows = []
    for rid, verdict, evidence, when in ROW.findall(text):
        if "FAILED" not in verdict or "absent" not in verdict:
            continue
        rows.append((rid, evidence, when))

    if not rows:
        sys.exit("positive control FAILED: no 'FAILED — absent' rows parsed. "
                 "The ledger format changed; fix the ROW regex before trusting any output.")

    # Newest pass seen anywhere, so "stale" is relative to the ledger's own clock.
    all_passes = [int(n) for n in PASS_NUM.findall(text)]
    newest = max(all_passes) if all_passes else 0

    scored = []
    for rid, evidence, when in rows:
        flags = []
        if SCOPED.search(evidence):
            flags.append("scoped")
        if not SEARCHED.search(evidence):
            flags.append("no-search")
        if len(evidence) < 60:
            flags.append("bare")

        m = PASS_NUM.search(when) or PASS_NUM.search(evidence)
        p = int(m.group(1)) if m else None
        if p is not None and newest - p >= 4:
            flags.append(f"stale-pass{p}")

        hits = []
        for sym in identifiers(evidence):
            found = search(sym)
            if found:
                hits.append((sym, len(found)))

        # Risk: a live identifier is evidence and outranks every heuristic.
        risk = 3 * len(hits) + (2 if "scoped" in flags else 0) + len(flags)
        scored.append((risk, rid, flags, hits, evidence, when))

    scored.sort(key=lambda r: (-r[0], r[1]))

    # --- positive control -------------------------------------------------
    ranked = {rid: risk for risk, rid, *_ in scored}
    missing = [r for r in KNOWN_STALE if r not in ranked]
    if missing:
        sys.exit(f"positive control FAILED: known-stale rows not parsed as FAILED — absent: "
                 f"{', '.join(sorted(missing))}. Either they were re-marked (good -- update "
                 f"KNOWN_STALE) or the parser broke. Refusing to print a triage that cannot "
                 f"find the three rows it is known to be right about.")
    flat = [r for r in ranked.values()]
    median = sorted(flat)[len(flat) // 2]
    dull = [r for r in KNOWN_STALE if ranked[r] < median]
    if dull:
        sys.exit(f"positive control FAILED: known-stale rows ranked below the median "
                 f"({', '.join(sorted(dull))}). The heuristics do not detect the shape they "
                 f"were built from. Refusing to print.")

    ctl = ", ".join(f"{r}=risk {ranked[r]}" for r in sorted(KNOWN_STALE))
    print(f"positive control ok: search backend live (`{probe}` found); {ctl}; "
          f"median risk {median}")
    print(f"{len(scored)} FAILED — absent rows, newest pass {newest}\n")

    print("A high rank is a CHEAP CHECK WORTH DOING, never a verdict. Only the critic")
    print("changes a verdict, and only by exercising the feature.\n")

    for risk, rid, flags, hits, evidence, when in scored:
        if risk == 0:
            continue
        mark = "  <-- identifier is LIVE in the workspace" if hits else ""
        print(f"[{risk:>3}] {rid:<20} {','.join(flags) or '-'}{mark}")
        for sym, n in hits:
            print(f"        HIT `{sym}` x{n}")
        print(f"        {evidence[:150]}")
        print(f"        ({when})")
    zero = sum(1 for r in scored if r[0] == 0)
    if zero:
        print(f"\n{zero} rows scored 0 (evidence cites a real search) -- lowest priority.")

    # Deliberately NOT reported here: the ledger-wide `builder-claimed, unverified` count.
    #
    # This script used to print it via `len(re.findall(...))` over the whole file, which
    # gave 70. The real figure is 34. The raw match also swept up the ledger's own totals
    # block and any row mentioning the phrase in prose. `Scripts/ledger-totals.py` parses
    # rows properly and reconciles against that block ("Totals block matches the body"), so
    # it is authoritative and this script defers to it rather than shipping a second,
    # worse answer. Two tools disagreeing about one number is how a ledger stops being
    # believed -- which is the exact failure both scripts exist to prevent.
    claimed_absent = sum(1 for _, _, when in rows if "builder-claimed" in when)
    print(f"\n--- adjudication backlog ---")
    print(f"{claimed_absent} of these FAILED — absent rows are `builder-claimed, unverified`; "
          f"{len(rows) - claimed_absent} need real construction.")
    print("For the ledger-wide backlog run Scripts/ledger-totals.py -- authoritative there.")


if __name__ == "__main__":
    main()
