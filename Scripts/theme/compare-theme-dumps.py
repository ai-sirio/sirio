#!/usr/bin/env python3
"""Compare two theme dumps written by sirio_ui's `theme_dump` example.

Usage: compare-theme-dumps.py BEFORE_DIR AFTER_DIR

`ely.tsv`, `bezel.txt` and `theme.txt` must be byte-identical: they are what
Ely, bezel-theme and the layout read. `sirio.tsv` in the old vocabulary is
mapped through docs/testing/theme-ely-palette/rename.tsv; every token it
holds must keep its value. In the new vocabulary every `ely.*` row must equal
the same combination's row in `ely.tsv`: Ely is handed Sirio's palette.
Prints THEME DUMPS MATCH, or every difference and exits 1.
"""
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
RENAME = ROOT / "docs/testing/theme-ely-palette/rename.tsv"


def rows(path):
    out = {}
    for line in path.read_text().splitlines():
        combo, token, *value = line.split("\t")
        out[(combo, token)] = tuple(value)
    return out


def is_new(table):
    return any(token.startswith(("ely.", "sirio.")) for _, token in table)


def renamed(table, rename):
    if is_new(table):
        return table
    out = {}
    for (combo, token), value in table.items():
        new = rename[token]
        if (combo, new) in out and out[(combo, new)] != value:
            raise SystemExit(f"{combo}: aliases of {new} disagree")
        out[(combo, new)] = value
    return out


def main(before, after):
    rename = dict(
        line.split("\t")
        for line in RENAME.read_text().splitlines()
        if line and not line.startswith("#")
    )
    problems = []
    for name in ("ely.tsv", "bezel.txt", "theme.txt"):
        if (before / name).read_bytes() != (after / name).read_bytes():
            problems.append(f"{name} differs")
    raw_old, raw_new = rows(before / "sirio.tsv"), rows(after / "sirio.tsv")
    old, new = renamed(raw_old, rename), renamed(raw_new, rename)
    for key, value in sorted(old.items()):
        if key not in new:
            problems.append(f"sirio.tsv: {key} missing after")
        elif new[key] != value:
            problems.append(f"sirio.tsv: {key} {value} -> {new[key]}")
    if is_new(raw_old) and set(new) != set(old):
        problems.append(f"sirio.tsv: tokens added {sorted(set(new) - set(old))}")
    if is_new(raw_new):
        ely = rows(after / "ely.tsv")
        for (combo, token), value in sorted(new.items()):
            if token.startswith("ely.") and ely.get((combo, token[4:])) != value:
                problems.append(f"{combo}: {token} is not what Ely holds")
    if problems:
        print("\n".join(problems))
        sys.exit(1)
    print("THEME DUMPS MATCH")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]))
