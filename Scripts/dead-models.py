#!/usr/bin/env python3
"""Enumerate `pub fn`s that nothing outside their own crate (and their own tests) calls.

WHY THIS EXISTS
---------------
The single most common defect in this rewrite is not a crash. It is a function that was
built, unit-tested, marked PASSED, and never wired to anything — so the row's test proves
the unit works while the feature does not exist. `pireview` found seven of these by
exercising the app; `fable` found two more clusters by reading (notification payloads that
are built but never emitted, `resolve_file_link` with no click path). Both found them one
at a time, by luck and diligence.

A caller count is not a reading. It can be re-run, and disagreed with, mechanically.

WHAT IT REPORTS
---------------
For each `pub fn NAME` defined in crate C, it counts identifier references:
  - OUT  : in any other crate's src/  (the only count that proves cross-crate wiring)
  - IN   : elsewhere in C's own src/, outside #[cfg(test)] and outside test fns
  - TEST : inside #[cfg(test)] regions AND crate-level tests/ dirs (integration tests).
           FABLE-05: the first version read src/ only, so the whole tiller_activity
           cluster printed test=0 while being integration-tested — which understated
           the finding ("tested and unwired" is stronger than "dead code").
DEAD  = OUT 0 and IN 0  -> only its definition and its tests mention it.
LOCAL = OUT 0 and IN >0 -> used within its crate; fine unless the crate itself is dead.
Markers: `[crate-vis]` = pub(crate)/pub(super), so OUT 0 is vacuous — only IN carries
information. `~example=` = referenced from an examples/ dir: a demo constructor
(new_for_demo, new_with_default_context), not an unwired feature.

READ THE OUTPUT AS A TRIAGE LIST, NOT A VERDICT
-----------------------------------------------
Known false positives, all of which must be checked by hand before any row is touched:
  - trait-impl methods, reached through the trait, never by name
  - anything reached by macro expansion or `#[derive]`
  - re-exports (`pub use`) that rename at the boundary
  - names common enough to collide (`new`, `len`, `id`) -- these are suppressed by
    --min-len, which is why the default is 5
  - REDUNDANT VARIANTS: a dead name whose feature is carried by a near-identical
    sibling. The first run of this script reported `save_projects`/`save_worktrees`
    with zero app callers, which reads as "nothing persists projects" -- and the app
    in fact calls the SINGULAR `save_project`/`save_worktree` throughout. Dead API
    surface, not a dead feature. A `~sibling=` marker below means exactly this
    suspicion; treat that row as "delete the variant", never as "the feature is gone".
  - PARALLEL MODEL IN ANOTHER CRATE: the variant trap at crate scale, invisible to any
    name heuristic. FABLE-05: tiller_markdown's MarkdownDocument (set_text /
    refresh_from_disk / has_conflict / is_deleted all DEAD) reads as "the editor's
    document model is unwired" -- but the feature lives in tiller_ui::editor::Editor,
    driven by FileView in production. Different crate, different names, live feature.
    Before reporting a dead *model type*, grep for who owns the feature it models.
  - SAME-NAME FIELD MASKING (false NEGATIVE): a method sharing its name with a field
    (status_bar's `on_refresh`) is masked by the field's own mentions and never appears
    here even when truly unwired. Common-word names (`sorted`, `rows`, `status`) are
    masked the same way. Absence from this list proves nothing.
Zeros here are a question ("who calls this?"), not an answer. A zero-caller count on a
NAME is not a zero-caller count on a FEATURE.

POSITIVE CONTROL
----------------
`--self-test` asserts the pattern finds a function known to be called across crates. A
zero from a broken probe is indistinguishable from a discovery, so the sweep refuses to
print results if the control fails. This is the same rule the evidence standard applies to
every negative grep in this project.

THE MODULE TIER (--modules) — FABLE-06
--------------------------------------
The function census has a structural blind spot: a cluster of functions that call each
other, which nothing outside calls, is invisible to DEAD — every member has in>0, so the
whole subsystem lands in `local` among 200+ rows (`resolve_file_link` was exactly this
shape; only reading caught it). A per-module out/in count would reproduce the same
blindness one tier up: two dead modules citing each other both show out>0.

So the module tier is not a counter. It is REACHABILITY: build the graph "file B mentions
a pub item defined in file A", then BFS from the binary roots (the app's main.rs and the
shipped tillerctl; dev-harness bins are separate roots and marked, not trusted). A file no
root reaches is dead AS A GROUP with everything else in its component — the cluster is the
unit, because it hides a feature, not a function.

Edges flow from CONSUMPTION, not publication: `pub use` lines are stripped from the
referencing side, because a lib.rs re-export must not vivify its own crate's dead module
(tiller_activity/rows.rs: re-exported, zero consumers, caught only after this rule).
Consumer-side `use` lines are kept — they are evidence, and for import-renamed items the
only visible evidence (cosmic/hex.rs lives as `use super::hex::parse as hex;`).

Module-tier false deaths, checked before believing any zero:
  - NO COUNTABLE SURFACE: a file defining no pub item >= --min-len (impl-only files,
    mod.rs aggregators that only re-export) can never be referenced by name. Printed as
    "unmeasurable", never as dead.
  - RENAME RE-EXPORTS (`pub use x as y`) hide the defining file behind the new name.
    None exist in this workspace today (checked 2026-08-13); re-check before trusting.
    Import-side renames (`use path::item as alias`) DO exist (cosmic/hex.rs) — the kept
    use-line carries the edge; the strict graph may still flag such files weakly-live.
  - TRAIT DISPATCH: methods reached via `dyn Trait`/generics never name the file — but
    constructing the value names the TYPE, which is why types count as surface.
  - SHARED NAMES: a name defined in two files vivifies both when either is used.
    Conservative toward live: the dead list stays believable, the live set may lie.
  - CFG-GATED mods (platform splits) compile away; only #[cfg(test)] exists here today.
Liveness flows through production text only: tests/ and examples/ never make a module
live (FABLE-05's rule — "tested and unwired" must stay visible).

Controls (--modules refuses to print without them): the fn-tier control, plus every root
must exist and be parsed, plus BFS from product roots must reach a sane majority of
files, plus the most-cross-referenced file must be inside the reached set.

usage:
    Scripts/dead-models.py                 # dead only
    Scripts/dead-models.py --all           # dead + local
    Scripts/dead-models.py --crate tiller_activity
    Scripts/dead-models.py --modules       # module/cluster reachability tier
"""
from __future__ import annotations

import argparse
import re
import sys
from collections import Counter, deque
from pathlib import Path

RUST = Path(__file__).resolve().parent.parent / "rust" / "crates"

PUB_FN = re.compile(r"^\s*pub(?:\s*\(([^)]*)\))?\s+(?:async\s+)?(?:unsafe\s+)?fn\s+([a-z_][a-z_0-9]*)", re.M)
# Module-tier surface: every pub item that consumers must NAME to use. Types are the
# load-bearing entry — trait-dispatched methods never appear at call sites, but the value
# had to be constructed, and construction names the type.
PUB_ITEM = re.compile(
    r"^\s*pub(?:\s*\(([^)]*)\))?\s+(?:async\s+)?(?:unsafe\s+)?"
    r"(?:fn|struct|enum|trait|union|type|const|static)\s+([A-Za-z_][A-Za-z0-9_]*)",
    re.M,
)
MACRO_EXPORT = re.compile(r"#\[macro_export\][^!]*?macro_rules!\s*([A-Za-z_][A-Za-z0-9_]*)", re.S)
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
# For the strict liveness graph only: drop line comments, so a module kept alive purely
# by prose mentioning its type gets flagged weakly-live. Crude (a `//` inside a string
# over-strips), but over-stripping only moves files INTO the hand-check list, never out.
LINE_COMMENT = re.compile(r"//.*")
# A `pub use` is publication, not consumption: lib.rs re-exporting a module's items must
# not keep that module alive, or a crate vivifies its own dead subsystems (tiller_activity
# rows.rs was exactly this — re-exported, zero consumers). Plain `use` lines are KEPT:
# a consumer-side import is real evidence, and sometimes the only visible evidence
# (cosmic/hex.rs is consumed as `use super::hex::parse as hex;` + bare `hex(` calls —
# stripping consumer imports would have killed a live module).
PUB_USE_LINE = re.compile(r"^\s*pub(?:\s*\([^)]*\))?\s+use\b.*", re.M)
# Shipped binaries; every other src/bin/*.rs is a dev harness (changes_preview.rs says so
# in its own docstring) whose reach is reported but never trusted as product wiring.
PRODUCT_BINS = {"main.rs", "tillerctl.rs"}
# A test region: `#[cfg(test)] mod ...` to end of file is the overwhelmingly common shape
# in this workspace, plus `#[test]`/`#[gpui::test]` fns. Being generous here only moves
# references from IN to TEST, which makes DEAD *more* likely -- so it is checked by hand.
CFG_TEST = re.compile(r"^\s*#\[cfg\(test\)\]", re.M)


def crate_sources(crate: Path) -> list[Path]:
    src = crate / "src"
    if not src.is_dir():
        return []
    return sorted(p for p in src.rglob("*.rs"))


def aux_sources(crate: Path, kind: str) -> list[Path]:
    """Crate-level tests/ or examples/ files — outside src/, outside the old scan."""
    d = crate / kind
    if not d.is_dir():
        return []
    return sorted(p for p in d.rglob("*.rs"))


def split_test_region(text: str) -> tuple[str, str]:
    """Return (production_text, test_text) for one file."""
    m = CFG_TEST.search(text)
    if not m:
        return text, ""
    return text[: m.start()], text[m.start() :]


def count_ident(text: str, name: str) -> int:
    return len(re.findall(rf"\b{re.escape(name)}\b", text))


def sibling_names(name: str) -> list[str]:
    """Near-identical names that could be carrying the feature instead.

    Plural/singular first, because that is the variant that actually fooled a reader:
    `save_projects` had zero callers while `save_project` was called everywhere.
    """
    out = []
    if name.endswith("s"):
        out.append(name[:-1])
    else:
        out.append(name + "s")
    # First underscore-segment. FABLE-05: `discard_change_entries`/`discard_untracked_entries`
    # escaped every suffix rule because the live sibling (`discard`) is two segments away.
    if "_" in name:
        out.append(name.split("_", 1)[0])
    if name.endswith("es"):
        out.append(name[:-2])
    for pre in ("try_", "maybe_"):
        if name.startswith(pre):
            out.append(name[len(pre) :])
        else:
            out.append(pre + name)
    for suf in ("_mut", "_ref", "_all", "_inner", "_impl", "_owned", "_entries"):
        if name.endswith(suf):
            out.append(name[: -len(suf)])
        else:
            out.append(name + suf)
    # Drop the last underscore-segment. This is the rule that catches an abandoned API
    # shape sitting beside the live one: tiller_git/actions.rs had EIGHT dead functions
    # (`stage_entries`, `discard_changes`, `discard_untracked`, ...) whose work is done by
    # the shorter `stage`/`unstage`/`discard` the app actually calls. Reading the list
    # without this rule suggests git staging is unwired. It is not.
    if "_" in name:
        out.append(name.rsplit("_", 1)[0])
    return [s for s in dict.fromkeys(out) if s and s != name]


def live_sibling(name: str, prod: dict[str, str], home: str) -> str | None:
    """Return a sibling name that IS referenced outside `home`, if any.

    The sibling must also be DEFINED as a fn in `home` itself. Without that check the
    flag fires on generic-word matches (`should_notify`~should, `build_payload`~build,
    `needs_refresh`~needs) and on cross-crate coincidences (`migrate_file` in
    tiller_agents flagged by persistence's `migrate`) — all noise, FABLE-05.
    """
    for s in sibling_names(name):
        if len(s) < 5:
            continue
        if not re.search(rf"\bfn\s+{re.escape(s)}\b", prod[home]):
            continue
        if sum(count_ident(prod[c], s) for c in prod if c != home) > 0:
            return s
    return None


def module_pass(mods: list[dict], prod: dict, test: dict, ex: dict, defs: dict, args) -> int:
    """FABLE-06: reachability over files, so mutually-citing dead groups die together."""
    min_len = args.min_len
    repo = RUST.parent.parent
    if args.crate:
        print("note: --crate is ignored with --modules (clusters cross crates)\n")

    def prep(f: dict) -> None:
        names = {m.group(2) for m in PUB_ITEM.finditer(f["prod"]) if len(m.group(2)) >= min_len}
        names |= {m.group(1) for m in MACRO_EXPORT.finditer(f["prod"]) if len(m.group(1)) >= min_len}
        f["names"] = names
        f["idents"] = Counter(IDENT.findall(f["prod"]))
        consumed = PUB_USE_LINE.sub("", f["prod"])  # publication lines carry no liveness
        f["edge_idents"] = Counter(IDENT.findall(consumed))
        f["stripped"] = LINE_COMMENT.sub("", consumed)
        f["edge_idents_strict"] = Counter(IDENT.findall(f["stripped"]))
        p = f["path"]
        f["is_bin"] = (p.name == "main.rs" and p.parent.name == "src") or p.parent.name == "bin"
        f["is_product_bin"] = f["is_bin"] and p.name in PRODUCT_BINS

    anchored_cache: dict[str, re.Pattern] = {}

    def anchored(nm: str) -> re.Pattern:
        # A reference in call/path/type position. `let mut terminal = ...` is not
        # consumption of ActivityTab::terminal; `AttentionSort::sorted(` is. Bare-word
        # matches of common-word surface names fabricate liveness otherwise.
        if nm not in anchored_cache:
            e = re.escape(nm)
            anchored_cache[nm] = re.compile(
                rf"(?:::|\.)\s*{e}\b|\b{e}\s*(?:::|\(|\{{|<)|:\s*{e}\b|<\s*{e}\b|&\s*{e}\b"
            )
        return anchored_cache[nm]

    def build_graph(recs: list[dict], strict: bool) -> list[set[int]]:
        nmap: dict[str, list[int]] = {}
        for i, f in enumerate(recs):
            for nm in f["names"]:
                nmap.setdefault(nm, []).append(i)
        if strict:  # only names with exactly one definer carry liveness
            nmap = {nm: js for nm, js in nmap.items() if len(js) == 1}
        key = "edge_idents_strict" if strict else "edge_idents"
        out: list[set[int]] = []
        for i, f in enumerate(recs):
            hit: set[int] = set()
            for nm in f[key].keys() & nmap.keys():
                if strict and not anchored(nm).search(f["stripped"]):
                    continue  # bare-word mention only — no syntax-anchored consumption
                hit.update(j for j in nmap[nm] if j != i)
            out.append(hit)
        return out

    for f in mods:
        prep(f)
    name_map: dict[str, list[int]] = {}
    for i, f in enumerate(mods):
        for nm in f["names"]:
            name_map.setdefault(nm, []).append(i)

    # Outside-reference counts per file (production text of every OTHER file).
    n = len(mods)
    xcrate, xmod = [0] * n, [0] * n
    for i, f in enumerate(mods):
        if not f["names"]:
            continue
        for j, g in enumerate(mods):
            if i == j:
                continue
            hits = sum(g["idents"][nm] for nm in f["names"] if nm in g["idents"])
            if hits:
                if g["crate"] == f["crate"]:
                    xmod[i] += hits
                else:
                    xcrate[i] += hits
    tall = Counter(IDENT.findall("\n".join(test.values())))
    eall = Counter(IDENT.findall("\n".join(ex.values())))
    tst = [sum(tall[nm] for nm in f["names"]) for f in mods]
    exn = [sum(eall[nm] for nm in f["names"]) for f in mods]

    # Edges: file i mentions a pub item defined in file j  =>  i keeps j alive.
    refs = build_graph(mods, strict=False)
    refs_strict = build_graph(mods, strict=True)

    def bfs(seeds: set[int], edges: list[set[int]]) -> set[int]:
        seen, q = set(seeds), deque(seeds)
        while q:
            i = q.popleft()
            for j in edges[i]:
                if j not in seen:
                    seen.add(j)
                    q.append(j)
        return seen

    unmeasurable = {i for i, f in enumerate(mods) if not f["names"] and not f["is_bin"]}
    product_roots = {i for i, f in enumerate(mods) if f["is_product_bin"]}
    dev_roots = {i for i, f in enumerate(mods) if f["is_bin"] and not f["is_product_bin"]}
    measurable = [i for i in range(n) if i not in unmeasurable]

    reached = bfs(product_roots | unmeasurable, refs)
    reached_all = bfs(product_roots | unmeasurable | dev_roots, refs)
    dev_only = sorted(reached_all - reached - dev_roots)
    dead = [i for i in measurable if i not in reached_all]
    # Strict tier: liveness must survive comment-stripping and flow through uniquely
    # defined names. Whatever falls out is not dead — it is weakly live, i.e. its
    # liveness evidence is a shared name or prose, which is no evidence at all.
    strict_reached = bfs(product_roots | unmeasurable | dev_roots, refs_strict)
    weak = [i for i in measurable if i in reached_all and i not in strict_reached and not mods[i]["is_bin"]]

    # CONTROLS — refuse to print zeros a broken graph would fake.
    found_bins = {mods[i]["path"].name for i in product_roots}
    if found_bins != PRODUCT_BINS:
        print(f"MODULE CONTROL FAILED: product bins {PRODUCT_BINS} expected, found {found_bins}.", file=sys.stderr)
        print("The roots moved; fix PRODUCT_BINS before trusting any zero.", file=sys.stderr)
        return 3
    frac = len([i for i in measurable if i in reached]) / max(len(measurable), 1)
    mx = max(measurable, key=lambda i: xcrate[i])
    if frac < 0.5 or mx not in reached:
        print(f"MODULE CONTROL FAILED: product roots reach {frac:.0%} of measurable files;", file=sys.stderr)
        print(f"top cross-crate file {mods[mx]['path'].name} reached={mx in reached}.", file=sys.stderr)
        print("A graph this blind would report discoveries that are artifacts. Refusing.", file=sys.stderr)
        return 3
    # DEAD-SIDE CONTROL: a probe that cannot kill makes every zero below vacuous. Plant
    # two synthetic files that reference each other and nothing else — the exact
    # mutually-citing shape this tier exists to catch — and run them through the same
    # prep, the same graph builder, the same BFS. They must come out unreachable in both
    # graphs and mutually linked (so the component pass would print them as ONE cluster).
    syn = [
        {"path": Path("__control__/alpha.rs"), "crate": "__control__",
         "prod": "pub fn ctl_alpha_probe_fable06() { ctl_beta_probe_fable06(); }", "test": ""},
        {"path": Path("__control__/beta.rs"), "crate": "__control__",
         "prod": "pub fn ctl_beta_probe_fable06() { ctl_alpha_probe_fable06(); }", "test": ""},
    ]
    for s in syn:
        prep(s)
    trial = mods + syn
    ia, ib = n, n + 1
    for strict_mode in (False, True):
        tref = build_graph(trial, strict=strict_mode)
        treached = bfs(product_roots | unmeasurable | dev_roots, tref)
        if ia in treached or ib in treached:
            print(f"MODULE CONTROL FAILED: planted dead pair came out alive (strict={strict_mode}).", file=sys.stderr)
            print("The probe cannot kill; every zero it prints would be vacuous. Refusing.", file=sys.stderr)
            return 3
        if strict_mode is False and (ib not in tref[ia] or ia not in tref[ib]):
            print("MODULE CONTROL FAILED: planted pair not mutually linked; edge builder broken.", file=sys.stderr)
            return 3
    print(
        f"module control ok: roots {sorted(found_bins)} + {len(dev_roots)} dev bin(s); "
        f"product roots reach {frac:.0%} of {len(measurable)} measurable files; "
        f"top cross-crate module {mods[mx]['path'].name} (xcrate={xcrate[mx]}) is reached; "
        f"planted mutually-citing pair correctly dies as one cluster\n"
    )
    if args.self_test:
        return 0

    # Cluster the dead: undirected components, so a mutually-citing group prints as one unit.
    dead_set = set(dead)
    comp: dict[int, int] = {}
    cid = 0
    for s in dead:
        if s in comp:
            continue
        stack, members = [s], []
        comp[s] = cid
        while stack:
            i = stack.pop()
            members.append(i)
            for j in (refs[i] & dead_set) | {k for k in dead_set if i in refs[k]}:
                if j not in comp:
                    comp[j] = cid
                    stack.append(j)
        cid += 1
    clusters: dict[int, list[int]] = {}
    for i, c in comp.items():
        clusters.setdefault(c, []).append(i)

    def fline(i: int) -> str:
        f = mods[i]
        rel = f["path"].relative_to(repo)
        return (
            f"  {rel}  surface={len(f['names']):<3} "
            f"xcrate={xcrate[i]:<3} xmod={xmod[i]:<3} test={tst[i]:<4} ex={exn[i]}"
        )

    print(f"=== DEAD SUBSYSTEMS — unreachable from any binary root: {len(dead)} file(s) in {len(clusters)} cluster(s) ===")
    print("(every production CONSUMER of these files is inside the same cluster; xmod/xcrate")
    print(" may still count mentions from `pub use` publication lines — those carry no liveness)")
    for c, members in sorted(clusters.items(), key=lambda kv: -len(kv[1])):
        crates_in = sorted({mods[i]["crate"] for i in members})
        print(f"cluster of {len(members)} — {', '.join(crates_in)}:")
        for i in sorted(members, key=lambda i: str(mods[i]["path"])):
            print(fline(i))
    if not dead:
        print("  (none)")
    print()

    print(f"=== reachable ONLY through dev bin(s) {sorted(mods[i]['path'].name for i in dev_roots)}: {len(dev_only)} file(s) ===")
    for i in dev_only:
        print(fline(i) + "  ~bin-wired, not product-wired")
    if not dev_only:
        print("  (none)")
    print()

    print(f"=== weakly live — alive only via shared names or comment prose: {len(weak)} file(s) ===")
    print("(reached in the loose graph, unreached when liveness must flow through uniquely")
    print(" defined names in comment-stripped text; not dead — evidence-free alive. hand-check)")
    for i in sorted(weak, key=lambda i: str(mods[i]["path"])):
        print(fline(i))
    if not weak:
        print("  (none)")
    print()

    print(f"=== unmeasurable by name-counting (no pub surface >= {min_len} chars) — seeded live, hand-check: {len(unmeasurable)} ===")
    for i in sorted(unmeasurable, key=lambda i: str(mods[i]["path"])):
        print(f"  {mods[i]['path'].relative_to(repo)}")
    print()

    # Collapse: how many fn-tier DEAD/local rows sit inside unreached files?
    dead_paths = {mods[i]["path"] for i in dead}
    dev_paths = {mods[i]["path"] for i in dev_only}
    fd = fl = fd_in = fl_in = fd_dev = fl_dev = 0
    for cname, found in defs.items():
        for name, f, _line, _vis in found:
            if len(name) < min_len:
                continue
            out = sum(count_ident(prod[o], name) for o in prod if o != cname)
            out += sum(count_ident(test[o], name) for o in test if o != cname)
            inn = count_ident(prod[cname], name) - 1
            if out != 0:
                continue
            if inn <= 0:
                fd += 1
                fd_in += f in dead_paths
                fd_dev += f in dev_paths
            else:
                fl += 1
                fl_in += f in dead_paths
                fl_dev += f in dev_paths
    print(f"collapse: fn tier has {fd} DEAD / {fl} local; of those, {fd_in} DEAD + {fl_in} local sit in the")
    print(f"{len(dead)} unreached file(s), {fd_dev} DEAD + {fl_dev} local in dev-bin-only files. The rest live in")
    print("reached modules — crate-local by design, not hidden subsystems.")
    print("\nA zero on a module is a question, not a verdict: check trait dispatch, macros,")
    print("rename re-exports and cfg gates by hand before any of this touches a ledger row.")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--all", action="store_true", help="also list crate-local (OUT 0, IN >0)")
    ap.add_argument("--crate", help="restrict to one crate")
    ap.add_argument("--min-len", type=int, default=5, help="skip names shorter than this (collision guard)")
    ap.add_argument("--self-test", action="store_true", help="run the positive control and exit")
    ap.add_argument("--modules", action="store_true", help="module/cluster reachability tier (FABLE-06)")
    args = ap.parse_args()

    crates = sorted(p for p in RUST.iterdir() if p.is_dir() and (p / "src").is_dir())
    if not crates:
        print(f"error: no crates under {RUST}", file=sys.stderr)
        return 2

    # Load every crate's sources once, split into production and test text.
    # Crate-level tests/ join the TEST bucket; examples/ get their own bucket.
    prod: dict[str, str] = {}
    test: dict[str, str] = {}
    ex: dict[str, str] = {}
    defs: dict[str, list[tuple[str, Path, int, str | None]]] = {}
    mods: list[dict] = []  # per-file records for the module tier
    for c in crates:
        pbuf, tbuf = [], []
        found: list[tuple[str, Path, int, str | None]] = []
        for f in crate_sources(c):
            text = f.read_text(errors="replace")
            p, t = split_test_region(text)
            pbuf.append(p)
            tbuf.append(t)
            mods.append({"path": f, "crate": c.name, "prod": p, "test": t})
            for m in PUB_FN.finditer(p):
                line = p.count("\n", 0, m.start()) + 1
                found.append((m.group(2), f, line, m.group(1)))
        tbuf.extend(f.read_text(errors="replace") for f in aux_sources(c, "tests"))
        prod[c.name] = "\n".join(pbuf)
        test[c.name] = "\n".join(tbuf)
        ex[c.name] = "\n".join(f.read_text(errors="replace") for f in aux_sources(c, "examples"))
        defs[c.name] = found

    # POSITIVE CONTROL: a name defined in one crate and referenced from another must be
    # found by the same counting path the sweep uses. Without this, every zero below is
    # unfalsifiable.
    control = None
    for cname, found in defs.items():
        for name, _f, _l, _v in found:
            if len(name) < 8:
                continue
            out = sum(count_ident(prod[o], name) for o in prod if o != cname)
            if out >= 2:
                control = (cname, name, out)
                break
        if control:
            break
    if control is None:
        print("POSITIVE CONTROL FAILED: the sweep found no pub fn referenced from any", file=sys.stderr)
        print("other crate at all. The pattern is broken, not the codebase. Refusing to", file=sys.stderr)
        print("print zeros -- they would be indistinguishable from discoveries.", file=sys.stderr)
        return 3
    print(f"positive control ok: {control[0]}::{control[1]} seen {control[2]}x outside its crate\n")
    if args.modules:
        return module_pass(mods, prod, test, ex, defs, args)
    if args.self_test:
        return 0

    dead_n = local_n = 0
    variant_n = [0]
    for c in crates:
        if args.crate and c.name != args.crate:
            continue
        rows = []
        for name, f, line, vis in defs[c.name]:
            if len(name) < args.min_len:
                continue
            out = sum(count_ident(prod[o], name) for o in prod if o != c.name)
            out += sum(count_ident(test[o], name) for o in test if o != c.name)
            inn = count_ident(prod[c.name], name) - 1  # minus the definition itself
            tst = count_ident(test[c.name], name)
            exn = sum(count_ident(ex[o], name) for o in ex)
            if out == 0 and inn <= 0:
                rows.append(("DEAD ", name, f, line, out, inn, tst, exn, vis))
                dead_n += 1
            elif out == 0 and args.all:
                rows.append(("local", name, f, line, out, inn, tst, exn, vis))
                local_n += 1
        if not rows:
            continue
        print(f"=== {c.name} ===")
        for kind, name, f, line, out, inn, tst, exn, vis in sorted(rows):
            rel = f.relative_to(RUST.parent.parent)
            sib = live_sibling(name, prod, c.name)
            note = f"  ~sibling={sib}() is live -- likely a redundant variant" if sib else ""
            if sib:
                variant_n[0] += 1
            if exn:
                note += f"  ~example={exn} ref(s) -- demo-wired, not unwired"
            if vis is not None:
                note += f"  [crate-vis: pub({vis}), out=0 vacuous]"
            print(f"  {kind} {name:38} out={out:<3} in={max(inn,0):<3} test={tst:<3} {rel}:{line}{note}")
        print()

    print(f"DEAD (no caller outside its own definition+tests): {dead_n}")
    if args.all:
        print(f"local (used only inside its own crate):           {local_n}")
    print(f"  of which flagged ~sibling (redundant variant):   {variant_n[0]}")
    print("\nEvery line above is a question, not a verdict. Check trait impls, macros and")
    print("re-exports by hand before touching a ledger row. A ~sibling row means the")
    print("feature is probably live under another name -- delete the variant, do not")
    print("report the feature missing.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
