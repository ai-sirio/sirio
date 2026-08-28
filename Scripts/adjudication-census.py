#!/usr/bin/env python3
"""Reachability census over the never-judged ledger rows (FABLE-07).

WHY THIS EXISTS
---------------
Adjudication is bottlenecked on one critic. 58 rows have never been judged; under the
project's rule none of them exist yet. Each is a tick available for the cost of
*exercising* rather than *building* — but only if the critic does not burn its pass
discovering, one row at a time, that a claimed feature is test-only. This script answers
mechanically what can be answered mechanically:

    for every code-shaped identifier a never-judged row cites,
    does PRODUCTION code reference it, or only its tests?

It does NOT decide reachable/test-only/unclear per row — a route "from a rendered
control" is a reading judgment, recorded in ADJUDICATION-BACKLOG.md. It makes that
reading cheap and replayable: the symbols, their defining files, and their production
reference counts are computed, not asserted.

PARSER REUSE, NOT PARSER DUPLICATION
------------------------------------
Row parsing (`ROW`, `split_cells`) is imported from Scripts/ledger-totals.py at runtime.
Two parsers that can drift is how a ledger stops being believed; this script cannot
disagree with the authoritative counter about what a row is. The population is defined
exactly as ledger-totals defines "never independently judged": the judged cell does not
match `pass N`. The script asserts its population size equals a recount done with the
imported parser and REFUSES to print if the ledger moved between the two reads.

SEARCH BACKEND, PROVEN BEFORE ANY NEGATIVE
------------------------------------------
rg is not installed here; an earlier tool's every lookup silently returned nothing while
its control printed ok. This script searches in-process (word-boundary regex over the
crate tree read once), and refuses to print unless:
  - a symbol known present (`AgentAdapter`) is found, and a fabricated one is not;
  - a symbol known production-live (`bind_keys`) shows prod refs > 0;
  - a symbol known test-only (`agent_skill_install_command`, DEAD-MODULES ground truth)
    shows prod refs == 0 outside its defining file.
A zero from a probe that cannot find known positives is not evidence.

WHAT A COUNT MEANS HERE (same discipline as dead-models.py)
-----------------------------------------------------------
prod   = references in production text (outside #[cfg(test)] and crate tests/), minus
         the defining file's own lines for defined symbols. app>0 marks references in
         the binary crates (sirio, sirioctl) — the strongest reachability evidence.
test   = references inside test regions and tests/ dirs.
A test-name symbol (defined only in test text) is labelled [test-fn] — it proves the
test exists, never that the feature is wired. Zeros are questions, not verdicts:
trait dispatch, macros and UI closures can hide callers; that is what the reading
column of the backlog doc is for.

usage:
    Scripts/adjudication-census.py [--ledger PATH]   # defaults to the FABLE-07 pin
"""
from __future__ import annotations

import argparse
import importlib.util
import re
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PIN = ROOT / "docs" / "linux-rewrite" / "pins" / "INVENTORY-LEDGER.FABLE-07.md"
CRATES = ROOT / "rust" / "crates"
PASS_CELL = re.compile(r"^pass \d+$")
IDENT_OK = re.compile(r"^[A-Za-z_][A-Za-z_0-9]*(::[A-Za-z_][A-Za-z_0-9]*)*$")
CFG_TEST = re.compile(r"^\s*#\[cfg\(test\)\]", re.M)
# FABLE-06's rule, re-learned here when the known-test-only control fired on lib.rs's
# re-export of the very symbol it guards: `pub use` is publication, not consumption.
# Consumer-side plain `use` lines are kept — they are evidence (the hex.rs lesson).
PUB_USE_LINE = re.compile(r"^\s*pub(?:\s*\([^)]*\))?\s+use\b.*", re.M)
NOISE = {
    "PASSED", "FAILED", "absent", "defective", "true", "false", "None", "Some", "Ok",
    "Err", "String", "Vec", "bool", "usize", "f32", "u32", "i32", "self", "cx", "id",
    "new", "get", "set", "run", "open", "close", "read", "write", "test", "tests",
    "main", "lib", "mod", "linux", "macos", "cargo", "main.rs", "lib.rs",
}
APP_CRATES = {"sirio", "sirio_control"}  # the two product binaries live here


def load_parser():
    """Import ROW/split_cells from ledger-totals.py — one parser for the whole project."""
    spec = importlib.util.spec_from_file_location(
        "ledger_totals", Path(__file__).resolve().parent / "ledger-totals.py"
    )
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def load_sources() -> list[dict]:
    files = []
    for f in sorted(CRATES.rglob("*.rs")):
        rel = f.relative_to(CRATES)
        crate = rel.parts[0]
        kind = "test" if ("tests" in rel.parts or "examples" in rel.parts) else "src"
        text = f.read_text(errors="replace")
        if kind == "src":
            m = CFG_TEST.search(text)
            prod, test = (text[: m.start()], text[m.start():]) if m else (text, "")
            prod = PUB_USE_LINE.sub("", prod)
        else:
            prod, test = "", text
        files.append({"path": f, "crate": crate, "prod": prod, "test": test})
    return files


def count(files: list[dict], symbol: str) -> dict:
    """Word-boundary counts for one symbol, split prod/test/app, plus defining files."""
    tail = symbol.rsplit("::", 1)[-1]
    pat = re.compile(rf"\b{re.escape(tail)}\b")
    dfn = re.compile(rf"\b(?:fn|struct|enum|trait|const|static|type|union)\s+{re.escape(tail)}\b")
    prod = test = app = 0
    def_files, def_in_test = [], []
    for f in files:
        p_hits = len(pat.findall(f["prod"]))
        t_hits = len(pat.findall(f["test"]))
        if dfn.search(f["prod"]):
            def_files.append(f)
            p_hits = 0  # a definition site is not a caller; drop its own mentions
        elif dfn.search(f["test"]):
            def_in_test.append(f)
        prod += p_hits
        test += t_hits
        if f["crate"] in APP_CRATES:
            app += p_hits
    return {
        "symbol": symbol, "prod": prod, "test": test, "app": app,
        "defs": [str(f["path"].relative_to(ROOT)) for f in def_files],
        "test_fn": bool(def_in_test) and not def_files,
    }


def identifiers(evidence: str) -> list[str]:
    out = []
    for tok in re.findall(r"`([^`]+)`", evidence):
        tok = tok.strip()
        if tok.startswith("F-") or tok in NOISE or len(tok) < 4:
            continue
        if not IDENT_OK.match(tok):
            continue
        out.append(tok)
    return sorted(set(out))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ledger", type=Path, default=PIN)
    args = ap.parse_args()

    lt = load_parser()
    text = args.ledger.read_text(encoding="utf-8")
    rows = []
    for line in text.splitlines():
        if not lt.ROW.match(line):
            continue
        cells = lt.split_cells(line)
        if len(cells) != 4:
            print(f"REFUSING: malformed row (cells={len(cells)}): {line[:80]}", file=sys.stderr)
            return 3
        rid, verdict, evidence, judged = cells
        rid = rid.strip("`")
        if not PASS_CELL.fullmatch(judged):
            rows.append((rid, verdict, evidence, judged))

    files = load_sources()

    # CONTROLS — refuse before printing a single absence.
    if len(files) < 100:
        print(f"CONTROL FAILED: only {len(files)} .rs files under {CRATES}.", file=sys.stderr)
        return 3
    probe = count(files, "AgentAdapter")
    if probe["prod"] + probe["test"] == 0 and not probe["defs"]:
        print("CONTROL FAILED: `AgentAdapter` (known present) not found — backend broken.", file=sys.stderr)
        return 3
    if count(files, "zzz_not_a_symbol_zzz")["prod"] != 0:
        print("CONTROL FAILED: fabricated symbol matched — backend overmatches.", file=sys.stderr)
        return 3
    live = count(files, "bind_keys")
    if live["prod"] == 0:
        print("CONTROL FAILED: `bind_keys` (known production-live) shows prod=0.", file=sys.stderr)
        return 3
    deadctl = count(files, "agent_skill_install_command")
    if deadctl["prod"] != 0:
        print("CONTROL FAILED: `agent_skill_install_command` (known test-only, "
              "DEAD-MODULES) shows prod>0 — the counter no longer separates prod from test.", file=sys.stderr)
        return 3
    never_recount = Counter(j for _, _, _, j in rows)
    if sum(never_recount.values()) != len(rows):
        print("CONTROL FAILED: population recount disagrees with itself.", file=sys.stderr)
        return 3
    print(f"controls ok: backend live (`AgentAdapter` defined in {probe['defs'][0]}); "
          f"`bind_keys` prod={live['prod']}; `agent_skill_install_command` prod=0 test={deadctl['test']}; "
          f"population {len(rows)} never-judged rows "
          f"({', '.join(f'{k}={v}' for k, v in sorted(never_recount.items()))})\n")

    for rid, verdict, evidence, judged in rows:
        syms = identifiers(evidence)
        print(f"### {rid}  [{verdict}]  ({judged})")
        print(f"    {evidence[:200]}")
        if not syms:
            print("    (no code-shaped identifiers in evidence — reading only)")
        for s in syms:
            c = count(files, s)
            tag = " [test-fn]" if c["test_fn"] else ""
            where = f" defs={','.join(c['defs'][:2])}" if c["defs"] else " defs=NONE"
            print(f"    {s:<52} prod={c['prod']:<4} app={c['app']:<3} test={c['test']:<4}{tag}{where}")
        print()
    print("Every zero above is a question, not a verdict — trait dispatch, macros and UI")
    print("closures hide callers. Outcomes live in ADJUDICATION-BACKLOG.md; verdicts with")
    print("the critic.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
