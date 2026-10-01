#!/usr/bin/env python3
"""Rename uses of sirio_theme's deprecated colour fields to their new paths.

Usage: apply-renames.py <cargo package> [<cargo package> ...]

Builds the packages (all targets) from rust/ with JSON diagnostics. For every
`deprecated` warning on a `sirio_theme::ThemeColors` field, rewrites the
field name at the warning's primary span to the path the deprecation note
names (`text` -> `ely.fg`). Repeats until a build reports none. A span inside
a macro expansion is printed and left for a person.
"""
import collections
import json
import pathlib
import re
import subprocess
import sys

RUST = pathlib.Path(__file__).resolve().parents[2] / "rust"
WARNING = re.compile(r"use of deprecated field `(?:sirio_theme::)?ThemeColors::(\w+)`: (\S+)")


def warnings(packages):
    cmd = ["cargo", "build", "--all-targets", "--message-format=json"]
    for package in packages:
        cmd += ["-p", package]
    result = subprocess.run(cmd, cwd=RUST, capture_output=True, text=True)
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if message.get("reason") != "compiler-message":
            continue
        diagnostic = message["message"]
        hit = WARNING.search(diagnostic["message"])
        if hit:
            span = next(s for s in diagnostic["spans"] if s["is_primary"])
            yield hit.group(1), hit.group(2), span


def main(packages):
    for _ in range(10):
        edits = collections.defaultdict(set)
        for old, new, span in warnings(packages):
            if span.get("expansion"):
                print(f"MACRO, rename by hand: {span['file_name']}:{span['line_start']} {old} -> {new}")
                continue
            edits[(RUST / span["file_name"]).resolve()].add((span["byte_start"], span["byte_end"], old, new))
        if not edits:
            print("RENAMES DONE")
            return
        for name, spans in edits.items():
            path = name
            data = path.read_bytes()
            for start, end, old, new in sorted(spans, reverse=True):
                text = data[start:end].decode()
                if not text.endswith(old):
                    sys.exit(f"{name}:{start}: span {text!r} does not end in {old}")
                data = data[:start] + (text[: -len(old)] + new).encode() + data[end:]
            path.write_bytes(data)
            print(f"renamed {len(spans)} in {path.relative_to(RUST)}")
    sys.exit("still renaming after 10 rounds")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    main(sys.argv[1:])
