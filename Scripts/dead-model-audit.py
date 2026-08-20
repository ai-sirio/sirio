#!/usr/bin/env python3
"""Find `pub fn`s in a crate that no production code anywhere can reach.

Motivation
----------
The Linux port mirrors the Swift core closely, and a faithfully ported model
function can pass its own unit tests forever while the app reaches the same
behaviour by a completely different path -- or not at all. F-CORE-WSP-05 is the
worked example: `classify_layout_command` mirrors Swift exactly, is unit-tested,
and had a single production caller covering one of its eight command variants.

A dead model has three legitimate resolutions, all of which the ledger has used:

  (a) the behaviour lives elsewhere under another name -> prove it live and say
      so in the row (F-CORE-DOM-05: `move_item` is dead, `Sidebar::reorder_rows`
      is the real thing, closed with an OS-level drag);
  (b) the wired duplicate is the *worse* implementation -> delete it and promote
      the model (F-CORE-DOM-06: `numeric_tab_selection` clamped correctly where
      the wired `TabSelection::jump` did not);
  (c) the affordance exists but bypasses the model -> route it through
      (F-CORE-WSP-05).

"Dead" is therefore a prompt to investigate, never a verdict on its own.

Reading the output
------------------
  called by another crate        -- live public surface, nothing to do
  called ONLY inside the crate   -- over-public; a visibility question, not a
                                    reachability one
  NO production caller anywhere  -- the candidates worth a look

Both controls must print PASS. If they do not, the numbers are meaningless --
see the note on `#[cfg(test)]` below.

Usage
-----
    python3 Scripts/dead-model-audit.py [crate-src-dir] [--control SYMBOL]

    # default: audit tiller_project against the rest of the workspace
    python3 Scripts/dead-model-audit.py

    python3 Scripts/dead-model-audit.py rust/crates/tiller_git/src \
        --control run_streaming
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys

CFG_TEST = re.compile(r"#\[cfg\(test\)\]")
PUB_FN = re.compile(r"^\s*pub fn (\w+)", re.M)

# Names too generic for a textual call-site search to say anything useful about.
UNINFORMATIVE = {"new", "default", "fmt", "clone", "from"}


def strip_test_mods(src: str) -> str:
    """Return `src` with every `#[cfg(test)]` item removed.

    Brace-matched on purpose. Truncating at the *first* `#[cfg(test)]` -- the
    obvious shortcut -- silently discards every line after the first inline test
    module. In `main.rs` that is thousands of lines of production code, which
    made a first version of this script report `classify_layout_command` as
    having no callers when it plainly has one. That is what the positive control
    below exists to catch.
    """
    out: list[str] = []
    i = 0
    while True:
        match = CFG_TEST.search(src, i)
        if not match:
            out.append(src[i:])
            return "".join(out)
        out.append(src[i : match.start()])
        brace = src.find("{", match.end())
        if brace == -1:  # attribute on a non-block item; skip the attribute only
            i = match.end()
            continue
        depth, j, in_string = 0, brace, False
        while j < len(src):
            char = src[j]
            if in_string:
                if char == "\\":
                    j += 2
                    continue
                if char == '"':
                    in_string = False
            elif char == '"':
                in_string = True
            elif char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
                if depth == 0:
                    j += 1
                    break
            j += 1
        i = j


def call_count(name: str, text: str) -> int:
    return len(re.findall(r"\b" + re.escape(name) + r"\s*\(", text))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("crate_src", nargs="?", default="rust/crates/tiller_project/src")
    parser.add_argument("--workspace", default="rust/crates")
    parser.add_argument(
        "--control",
        default="classify_layout_command",
        help="a symbol known to HAVE an external caller; the audit aborts if it "
        "is not found, because that means the parser is broken",
    )
    args = parser.parse_args()

    root = pathlib.Path(args.crate_src)
    if not root.is_dir():
        print(f"no such directory: {root}", file=sys.stderr)
        return 2

    production = {
        path: strip_test_mods(path.read_text(encoding="utf-8", errors="replace"))
        for path in root.rglob("*.rs")
    }
    declarations: dict[str, set[pathlib.Path]] = {}
    for path, text in production.items():
        for match in PUB_FN.finditer(text):
            declarations.setdefault(match.group(1), set()).add(path)

    external = "\n".join(
        strip_test_mods(path.read_text(encoding="utf-8", errors="replace"))
        for path in pathlib.Path(args.workspace).rglob("*.rs")
        if str(root) not in str(path) and "/target/" not in str(path)
    )

    positive = bool(call_count(args.control, external))
    negative = not call_count("zzz_definitely_not_a_real_symbol", external)
    print(f"CONTROL positive ({args.control} must be found): "
          f"{'PASS' if positive else 'FAIL'}")
    print(f"CONTROL negative (fabricated symbol must be absent): "
          f"{'PASS' if negative else 'FAIL'}")
    if not (positive and negative):
        print("\nThe instrument failed its own controls. Numbers withheld.",
              file=sys.stderr)
        return 1
    print()

    unreachable: list[tuple[str, str]] = []
    internal_only: list[tuple[str, str, int]] = []
    for name, files in sorted(declarations.items()):
        if name in UNINFORMATIVE or call_count(name, external):
            continue
        declared = sum(
            len(re.findall(r"^\s*pub fn " + re.escape(name) + r"\b", text, re.M))
            for text in production.values()
        )
        internal = sum(call_count(name, text) for text in production.values()) - declared
        where = sorted(files)[0].name
        if internal > 0:
            internal_only.append((name, where, internal))
        else:
            unreachable.append((name, where))

    reachable = len(declarations) - len(unreachable) - len(internal_only)
    print(f"pub fns in {root} (production): {len(declarations)}")
    print(f"  called by another crate       : {reachable}")
    print(f"  called ONLY inside this crate : {len(internal_only)}")
    print(f"  NO production caller anywhere : {len(unreachable)}\n")

    if internal_only:
        print("=== called only internally (over-public, but live) ===")
        for name, where, count in internal_only:
            print(f"  {name:34s} {where:16s} {count} internal call(s)")
        print()
    if unreachable:
        print("=== NO production caller anywhere — investigate, do not assume dead ===")
        for name, where in unreachable:
            print(f"  {name:34s} {where}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
