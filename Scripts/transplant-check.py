#!/usr/bin/env python3
"""Find code transplanted from the reference implementations.

The project's rule is that every line of Sirio is written from scratch: the references
are read for dimensions and behaviour, never copied. This checks that claim mechanically
so it does not have to be taken on trust.

**Counting shared lines is the wrong metric and produces a false alarm every time.** Any
two GPUI applications share hundreds of identical lines, because the framework dictates
them: `fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl
IntoElement` cannot be written any other way, and neither can an `on_mouse_event` closure
signature. Isolated matches are the *expected* background of independent work.

What distinguishes a transplant is **consecutive** lines. Two people solving the same
problem against the same API land on the same signatures; they do not land on the same
three substantive statements in the same order. So this looks for runs, and reports the
longest one per file pair.

Usage:
    Scripts/transplant-check.py [--refs DIR] [--min-run N] [--quiet]

Exit: 0 nothing above threshold · 1 candidate transplants found · 2 bad invocation
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

# Lines that carry no authorship: closing braces, lone keywords, short punctuation.
# A run made of these means nothing, so they never start or extend a run.
TRIVIAL = {
    "}", "};", "});", ")", ");", "],", "]", "},", "{", "else {", "});\n",
    ")?;", ".unwrap();", "return;", "break;", "continue;", "Ok(())", "Ok(())?",
    "#[test]", "#[cfg(test)]", "use super::*;", "mod tests {", "*/", "/*",
}


def substantive(line: str) -> str | None:
    """Normalise a line, or return None if it carries no authorship signal."""
    s = line.strip()
    if len(s) < 12:
        return None
    if s in TRIVIAL:
        return None
    # Comments are prose, not code, and the project's comments are its own voice —
    # a shared comment is worth knowing about but is not a code transplant.
    if s.startswith(("//", "#", "*")):
        return None
    return s


def shingles(path: Path, n: int) -> dict[tuple[str, ...], int]:
    """Map each run of n consecutive substantive lines to its first line number."""
    try:
        raw = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        return {}
    kept = [(i + 1, s) for i, line in enumerate(raw) if (s := substantive(line))]
    out: dict[tuple[str, ...], int] = {}
    for i in range(len(kept) - n + 1):
        window = kept[i : i + n]
        # Require real substance across the run, so three short similar lines
        # (`let x = 1;`) cannot trip it.
        if sum(len(s) for _, s in window) < 90:
            continue
        out.setdefault(tuple(s for _, s in window), window[0][0])
    return out


def main() -> int:
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--refs", default="/home/enzopalmisano/Scrivania/Progetti/_sirio-refs")
    ap.add_argument("--tree", default="rust/crates")
    ap.add_argument("--min-run", type=int, default=3)
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args()

    refs_root = Path(args.refs)
    if not refs_root.is_dir():
        print(f"FAIL: no reference checkouts at {refs_root}", file=sys.stderr)
        print("      This check cannot pass by default — absent references mean", file=sys.stderr)
        print("      it proved nothing, which is not the same as a clean result.", file=sys.stderr)
        return 2

    ours = sorted(Path(args.tree).rglob("*.rs"))
    if not ours:
        print(f"FAIL: no sources under {args.tree}", file=sys.stderr)
        return 2

    # Index every reference run once, mapping it back to where it came from.
    index: dict[tuple[str, ...], tuple[str, int]] = {}
    ref_files = 0
    for ref in sorted(p for p in refs_root.iterdir() if p.is_dir()):
        for f in ref.rglob("*.rs"):
            if "/target/" in str(f):
                continue
            ref_files += 1
            for run, line in shingles(f, args.min_run).items():
                index.setdefault(run, (f"{ref.name}:{f.name}", line))

    if not args.quiet:
        print(f"references: {ref_files} .rs files under {refs_root}")
        print(f"ours:       {len(ours)} .rs files under {args.tree}")
        print(f"run length: {args.min_run} consecutive substantive lines\n")

    hits = []
    for f in ours:
        for run, line in shingles(f, args.min_run).items():
            if run in index:
                src, srcline = index[run]
                hits.append((str(f), line, src, srcline, run))

    if not hits:
        print(f"CLEAN: no run of {args.min_run}+ consecutive lines is shared with any reference.")
        print("Isolated shared lines are expected and not reported — the framework dictates them.")
        return 0

    print(f"CANDIDATE TRANSPLANTS: {len(hits)}\n")
    for path, line, src, srcline, run in sorted(hits):
        print(f"{path}:{line}  <-  {src}:{srcline}")
        for s in run:
            print(f"    {s}")
        print()
    print("Each needs a human read: a shared run can still be convergent when the API")
    print("forces the order. What it can never be is unexamined.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
