#!/usr/bin/env python3
"""Vendor the Material icon theme into rust/crates/sirio_icons.

The upstream repo (zed-extensions/material-icon-theme) carries PKief's
artwork *and* a flattened mapping, which is why it is the source rather
than PKief's npm package: one commit pins both, so they cannot drift
apart.

Two things this script does that reading the JSON does not suggest:

1. It repairs an upstream defect. `src/theme.ts` renames six icon
   definition keys (git->vcs, template->templ, default->file, code->json,
   coffeescript->coffee, storage->database) and leaves the *values* in
   file_stems/file_suffixes pointing at the old names. Thirty-nine entries
   -- including .gitignore, .gitattributes and .gitmodules -- therefore
   name a definition that does not exist. We apply the same renaming to
   the values and fail if anything is still unresolved afterwards.

2. It lowercases and de-duplicates. Zed blew the name table up into case
   variants because its API is case-sensitive; Sirio lowercases before it
   looks anything up, so 8863 stems collapse to 2020. De-duplication is
   only safe while no two keys differing in case name different icons,
   so the script checks that and refuses rather than picking a winner.

3. It carries two licences, because two projects are involved. The
   artwork is PKief's, published under MIT; the flattened mapping the
   tables are generated from is the Zed packaging repo's own work, under
   Apache-2.0. MIT requires its notice to travel with every copy, and the
   Zed tarball does not contain it, so the script reads which artwork
   release the pinned commit packaged (its package-lock.json) and fetches
   that release's LICENSE.

Usage:  Scripts/vendor-material-icons.py <commit-sha>
"""

import io
import json
import re
import shutil
import subprocess
import sys
import tarfile
import urllib.request
from datetime import date
from pathlib import Path

REPO = "zed-extensions/material-icon-theme"
ARTWORK_REPO = "material-extensions/vscode-material-icon-theme"
CRATE = Path(__file__).resolve().parent.parent / "rust" / "crates" / "sirio_icons"
ASSET_DIR = CRATE / "assets"
ASSET_PREFIX = "icons/material"

# src/theme.ts's own keyMapping, applied to values as well as to keys.
KEY_MAPPING = {
    "git": "vcs",
    "code": "json",
    "coffeescript": "coffee",
    "default": "file",
    "storage": "database",
    "template": "templ",
}

DEFAULT_FILE = "file"
DEFAULT_FOLDER = "folder"
DEFAULT_FOLDER_OPEN = "folder-open"


def die(message):
    print(f"vendor-material-icons: {message}", file=sys.stderr)
    sys.exit(1)


def fetch(sha):
    url = f"https://codeload.github.com/{REPO}/tar.gz/{sha}"
    print(f"fetching {url}")
    with urllib.request.urlopen(url, timeout=120) as response:
        payload = response.read()
    archive = tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz")
    root = archive.getnames()[0].split("/")[0]
    members = {}
    for member in archive.getmembers():
        if not member.isfile():
            continue
        name = member.name[len(root) + 1 :]
        members[name] = archive.extractfile(member).read()
    return members


def artwork_version(members):
    """The artwork release the pinned commit packaged, from its own lockfile."""
    lock = json.loads(members["package-lock.json"])
    return lock["packages"]["node_modules/material-icon-theme"]["version"]


def fetch_artwork_licence(version):
    url = f"https://raw.githubusercontent.com/{ARTWORK_REPO}/v{version}/LICENSE"
    print(f"fetching {url}")
    with urllib.request.urlopen(url, timeout=60) as response:
        text = response.read()
    if b"MIT License" not in text:
        die(f"{url} is not the MIT licence the artwork is published under")
    return text


def stem_of(path):
    """'./icons/folder-src-open.svg' -> 'folder-src-open'."""
    return path.rsplit("/", 1)[-1][: -len(".svg")]


def collapse(table, label, resolve):
    """Lowercase the keys, refusing to merge two that disagree."""
    out = {}
    for key, value in table.items():
        resolved = resolve(value)
        if resolved is None:
            die(f"{label}: {key!r} names {value!r}, which is not a defined icon")
        lower = key.lower()
        if lower in out and out[lower] != resolved:
            die(
                f"{label}: {lower!r} means both {out[lower]!r} and {resolved!r} "
                "-- case-folding would silently pick one; fix this before vendoring"
            )
        out[lower] = resolved
    return out


def lit(text):
    """A Rust string literal.

    json.dumps escapes exactly what Rust needs (`"` and `\\`) and nothing it
    cannot read: ensure_ascii=False keeps UTF-8 literal, because Rust spells
    escapes `\\u{XXXX}` and would reject JSON's `\\uXXXX`. A repr()-and-replace
    would corrupt any key containing a quote.
    """
    return json.dumps(text, ensure_ascii=False)


def rust_pairs(name, doc, rows):
    lines = [f"/// {doc}", f"pub(crate) static {name}: &[(&str, &str)] = &["]
    lines += [f"    ({lit(key)}, {lit(value)})," for key, value in rows]
    lines.append("];\n")
    return "\n".join(lines)


def main():
    if len(sys.argv) != 2 or not re.fullmatch(r"[0-9a-f]{40}", sys.argv[1]):
        die("usage: vendor-material-icons.py <40-character commit sha>")
    sha = sys.argv[1]

    members = fetch(sha)
    theme = json.loads(members["icon_themes/material-icon-theme.json"])["themes"][0]
    definitions = {key: stem_of(value["path"]) for key, value in theme["file_icons"].items()}

    def resolve(value):
        if value in definitions:
            return definitions[value]
        return definitions.get(KEY_MAPPING.get(value, value))

    stems = collapse(theme["file_stems"], "file_stems", resolve)
    suffixes = collapse(theme["file_suffixes"], "file_suffixes", resolve)

    directories = {}
    for name, pair in theme["named_directory_icons"].items():
        entry = (stem_of(pair["collapsed"]), stem_of(pair["expanded"]))
        lower = name.lower()
        if lower in directories and directories[lower] != entry:
            die(f"named_directory_icons: {lower!r} means two different pairs")
        directories[lower] = entry

    referenced = set(stems.values()) | set(suffixes.values())
    referenced |= {DEFAULT_FILE, DEFAULT_FOLDER, DEFAULT_FOLDER_OPEN}
    for collapsed, expanded in directories.values():
        referenced |= {collapsed, expanded}

    available = {stem_of(name) for name in members if name.startswith("icons/")}
    absent = sorted(referenced - available)
    if absent:
        die(f"{len(absent)} referenced icons are not in the tree: {absent[:5]}")

    light = {stem: f"{stem}_light" for stem in sorted(referenced) if f"{stem}_light" in available}
    vendored = sorted(referenced | set(light.values()))

    if ASSET_DIR.exists():
        shutil.rmtree(ASSET_DIR)
    ASSET_DIR.mkdir(parents=True)
    for stem in vendored:
        (ASSET_DIR / f"{stem}.svg").write_bytes(members[f"icons/{stem}.svg"])

    generated = [
        "// @generated by Scripts/vendor-material-icons.py -- do not edit by hand.",
        f"// Upstream: {REPO} @ {sha}",
        "//",
        "// Every table is sorted by its key: `lib.rs` binary-searches them, and",
        "// `tables_are_sorted_and_deduplicated` is what keeps that true.",
        "",
        rust_pairs("STEMS", "Whole lowercased file names -> asset stem.", sorted(stems.items())),
        rust_pairs("SUFFIXES", "Lowercased dotted tails -> asset stem.", sorted(suffixes.items())),
        "/// Lowercased directory names -> (collapsed stem, expanded stem).",
        "pub(crate) static DIRECTORIES: &[(&str, &str, &str)] = &[",
    ]
    for name, (collapsed, expanded) in sorted(directories.items()):
        generated.append(f"    ({lit(name)}, {lit(collapsed)}, {lit(expanded)}),")
    generated += ["];", "", "/// Asset stem -> (asset path, embedded bytes).",
                  "pub(crate) static ASSETS: &[(&str, &str, &[u8])] = &["]
    for stem in vendored:
        generated.append(
            f"    ({lit(stem)}, {lit(f'{ASSET_PREFIX}/{stem}.svg')}, "
            f"include_bytes!({lit(f'../assets/{stem}.svg')})),"
        )
    generated += ["];", "",
                  rust_pairs("LIGHT_VARIANTS",
                             "Stems that ship a light-appearance companion.",
                             sorted(light.items()))]
    (CRATE / "src").mkdir(parents=True, exist_ok=True)
    (CRATE / "src" / "generated.rs").write_text("\n".join(generated) + "\n")

    artwork = artwork_version(members)
    (CRATE / "LICENSE").unlink(missing_ok=True)
    (CRATE / "LICENSE-APACHE").write_bytes(members["LICENSE"])
    (CRATE / "LICENSE-MIT").write_bytes(fetch_artwork_licence(artwork))
    (CRATE / "UPSTREAM.md").write_text(
        f"""# Upstream

Vendored from [{REPO}](https://github.com/{REPO}) at commit `{sha}`
on {date.today().isoformat()} by `Scripts/vendor-material-icons.py`, which is
the only writer of `assets/` and `src/generated.rs`. Re-run it with a newer
commit to update; never hand-edit either.

- artwork: [{ARTWORK_REPO}](https://github.com/{ARTWORK_REPO}) v{artwork}
- mapping entries: {len(stems)} names, {len(suffixes)} suffixes, {len(directories)} directories
- assets: {len(vendored)} ({len(referenced)} referenced, {len(light)} light companions)
"""
    )
    (CRATE / "ATTRIBUTION.md").write_text(
        f"""# Attribution

Two projects, two licences:

- **The SVGs in `assets/`** are the artwork of
  [{ARTWORK_REPO}](https://github.com/{ARTWORK_REPO}) (formerly
  PKief/vscode-material-icon-theme), release v{artwork}, vendored unchanged.
  MIT licensed; its notice is `LICENSE-MIT`.
- **The mapping in `src/generated.rs`** is generated from
  [{REPO}](https://github.com/{REPO})'s `icon_themes/material-icon-theme.json`,
  which that project derives from the artwork's manifest. Apache-2.0
  licensed; its licence is `LICENSE-APACHE`.

`UPSTREAM.md` records the pinned commit of both.
"""
    )

    print(f"stems:       {len(stems)}")
    print(f"suffixes:    {len(suffixes)}")
    print(f"directories: {len(directories)}")
    print(f"assets:      {len(vendored)} ({len(light)} light companions)")


if __name__ == "__main__":
    main()
