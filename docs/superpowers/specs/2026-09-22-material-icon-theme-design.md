# The Material icon set, whole — design

**Date:** 2026-09-22
**Status:** approved — implementation not started
**Parent work:** F-CORE-FILE-08 (`FileIconKey`, ported from the Swift app's
`FileIconKey.swift`), `9ce7abd2 feat: restore polychrome file type icons`
**Scope of this document:** replacing the 54-asset Material *subset* and the
hand-written lookup in front of it with the complete upstream theme — files
and folders — resolved from a table generated at vendoring time.

## Where the subset stops

Sirio already draws Material icons. It draws 54 of them.

`rust/assets/icons/file-types/` holds SVGs taken from
PKief/vscode-material-icon-theme by way of the retired Swift app, and
`FileIconKey` (`rust/crates/sirio_project/src/file_icon.rs`) decides which
one a name gets: seven exact file names, 82 extensions, twenty-seven
directory names, collapsing onto a fifty-seven-variant enum. Everything else —
every name the tables do not list — draws the generic mark.

| | Sirio today | upstream |
|---|---|---|
| file names matched | 7 | 2020 |
| extensions matched | 82 | 1158 |
| directory names matched | 27 | 3499 |
| distinct file icons | 34 | 563 |
| distinct folder icons | 0 | 498 |

The last row is the one to read twice. Sixteen `folder-*.svg` assets are
vendored in that directory and **not one of them is used**:
`FileIconKey::material_asset` returns `None` for every folder variant and
`file_glyph` (`rust/crates/sirio_ui/src/icons.rs`) only consults it when
`!is_dir`, so every directory in the Files tree draws the monochrome
`zed/folder.svg` ↔ `zed/folder_open.svg` pair regardless of its name.

The failure mode is the one this repo keeps running into and keeps writing
down: **silence**. A `.kt` file and a `.zig` file look the same, and nothing
anywhere says one of them was recognised and the other was not. Widening a
hand-written table one entry at a time never ends, because the table is the
wrong shape for the problem: the answer is a thousand-row mapping that
somebody else already maintains.

## §1 The source — one pinned commit

`zed-extensions/material-icon-theme` carries 1192 SVGs (~1.0 MB) and
`icon_themes/material-icon-theme.json` (929 KB). Its `src/theme.ts` builds
that JSON from PKief's npm manifest, and reading it is what settles the
lookup semantics rather than guessing them:

- `file_stems` are **whole file names**, not stems in the `Path::file_stem`
  sense — `.adonisrc.json` is a key. They are blown up into case variants
  (lower, upper, mixed, title) for one stated reason: "ZED's API is
  case-sensitive but the manifest is not."
- `file_suffixes` are extensions, and 195 of them carry a dot of their own
  (`blade.php`, `bench.ts`, `xml.dist.sample`; never more than two).
- `named_directory_icons` is `folderNames` joined with `folderNamesExpanded`
  into a collapsed/expanded pair per name.
- `directory_icons` is the default pair, `folder.svg` / `folder-open.svg`.
- PKief's `default` key is renamed to `file`, so **the theme's own fallback
  for an unrecognised file is `file.svg`** — it is part of the set, not an
  invention of ours.

Sirio lowercases a name before it looks it up and always has
(`FileIconKey::for_file_name`), so Zed's case expansion is dead weight here:
8863 stems collapse to **2020** with **zero conflicts** — no two keys
differing only in case point at different icons. The vendoring script
re-checks that and fails if it ever stops being true (§5); collapsing a
genuine conflict would silently pick a winner.

Total mapping after normalisation: **6677 entries** (1158 + 2020 + 3499).
Assets actually referenced: **1061** (563 file, 498 folder) of the 1192,
plus **52 `_light` variants**.

Taking the Zed extension rather than PKief's npm package is deliberate: one
pinned commit gives the artwork and the already-flattened mapping together,
so there is no second source to keep in step.

## §2 `sirio_icons` — a leaf crate, and no gpui

A new workspace member, `rust/crates/sirio_icons`, with no local
dependencies and **no gpui**:

```
assets/           ~1113 SVGs, byte-identical to upstream
src/generated.rs  STEMS, SUFFIXES, DIRECTORIES, ASSETS, LIGHT_VARIANTS
src/lib.rs        the resolver
UPSTREAM.md       the pinned commit, the date, the script that produced this
LICENSE           upstream MIT
```

The public surface is four functions over `&'static str` slugs:

```rust
pub fn for_file(name: &str) -> &'static str;
pub fn for_directory(name: &str, expanded: bool) -> &'static str;
pub fn asset(slug: &str) -> Option<&'static [u8]>;
pub fn light_variant(slug: &str) -> Option<&'static str>;
```

The tables are sorted static slices searched by binary search: no `HashMap`,
no `OnceLock`, no allocation, nothing to build on first use. A hash map
behind a `OnceLock` would pay 3499 insertions at the moment the Files tree
draws its first frame, which is exactly the frame that must not stall.

The crate knows nothing about themes, pixels or windows — it answers slugs
and bytes. That keeps the resolver testable without `TestAppContext`, the
same reason `sirio_activity` carries no gpui dependency, and it keeps 1.4 MB
of embedded assets and 1113 `include_bytes!` out of `sirio_ui`, which is the
largest crate and the one that rebuilds most often. `sirio_project` was the
other candidate — `FileIconKey` lives there — but `sirio_terminal` depends on
`sirio_project` for entirely unrelated reasons and would inherit the weight.

## §3 The resolution contract

For a file, on the name lowercased once:

1. the whole name in `STEMS` (`dockerfile`, `makefile`, `.gitignore`,
   `justfile`, `.editorconfig`);
2. otherwise the **longest dotted tail**, tried longest first — a leading dot
   yields a tail too (`app.blade.php` → `blade.php`, then `php`;
   `cargo.toml` → `toml`);
3. otherwise `file`.

For a directory: the whole name in `DIRECTORIES`, which yields the
collapsed/expanded pair; otherwise `folder` / `folder-open`.

`FileIconKey` is **retired**, and `sirio_project` loses its `pub use`. This
is not opportunistic tidying: 563 icons do not fit in a fifty-seven-variant
enum, and keeping the enum as an intermediate layer would leave two answers
for the same name. Its only consumer is `file_glyph`.

Three consequences are visible in the app and ratified here rather than
discovered later:

- **The Foundation extension quirk stops being reachable — which changes
  some dotfiles and not others.** The Swift port copied
  `NSString.pathExtension`'s rule (a name starting with `.` and carrying no
  other dot has no extension at all) and `file_icon.rs` defends it in its
  module docs: "read `FileIconKey.swift` before 'fixing' it". Step 1 now
  answers first, on the whole name, so `.editorconfig` draws `editorconfig`
  and `.env.local` draws `tune` where both draw the generic mark today.
  `.bashrc` does **not** change: upstream lists no icon for it either. What
  decides these cases is the upstream table, not the quirk's disappearance —
  worth stating because the opposite is the intuitive guess.
- **The Swift-parity tests go with the type.** About sixty cases in
  `for_file_name_matches_the_original_exact_and_extension_tables` assert
  fidelity to a Swift file that is no longer the authority for this
  behaviour. They are replaced by §6's tables, not merely dropped.
- **Folders become polychrome and per-name, in both states.**

## §4 Rendering: colour, appearance, folders

Every file icon takes the full-colour raster path that `Icon::FileType`
already takes (`has_own_colours()` → resvg → `RenderImage` cached by
`(Icon, pixel)`), never the tinted `svg()` path. A Material icon paints its
own fills; tinting it would flatten it to a silhouette.

**Light variants.** 52 assets exist in a `_light` form, and this is not
cosmetic: `toml.svg` paints its glyph `#cfd8dc` and `toml_light.svg` paints
it `#455a64`. On a light background the first is very nearly invisible —
and `toml` means every `Cargo.toml`, in this repo's own sidebar. The Zed
extension does not solve this; it declares `appearance: "dark"` and ships no
light theme.

The swap happens inside `IconElement::render`, before the canvas closure is
built, and keys off **`Theme::get(cx).appearance`** — the resolved
`Appearance`, not `Theme::mode`. `mode` is the *preference* and includes
`System`; keying on it would send every user on "System" down the dark
branch no matter what their desktop is set to. Because the resolved icon is
itself the cache key, both variants coexist and a theme switch invalidates
nothing: the next frame finds the other entry, already warm if that
appearance has been seen.

**Folders.** `file_row_glyph` currently takes `(path, is_dir, expanded)` and,
when expanded, returns `Icon::FolderOpen` *ignoring the path*. That is
correct only while there is exactly one open folder mark. It is replaced by
two functions that cannot express the meaningless combination:

```rust
pub fn file_glyph(path: &Path) -> Icon;                   // never a directory
pub fn folder_glyph(path: &Path, expanded: bool) -> Icon; // the pair, by name
```

The test identity in `right_panel/files.rs` stops matching on two variants
and derives from the slug (`…-open` → open, any other folder slug → closed),
preserving the property its comment defends: a mark that is not a folder
fails loudly instead of passing as shut.

**What stays.** The `zed/` catalog (chevrons, gear, close, toolbar) and the
`lobehub/` agent marks are untouched — the Material set is a file-and-folder
theme and carries no UI glyphs. The sidebar's project and worktree rows keep
their current marks (`project_identity.rs`, `Icon::FolderFill`,
`Icon::GitBranch`): a row there is a project, not a directory.

## §5 Generation and vendoring

`Scripts/vendor-material-icons.sh <commit-sha>` is the only writer of
`crates/sirio_icons/assets/` and `src/generated.rs`. It fetches that commit's
tarball, takes `icon_themes/material-icon-theme.json`, the referenced SVGs
and `LICENSE`, lowercases and de-duplicates the three dictionaries,
**fails** if de-duplication would merge two keys with different values,
writes the assets and the generated module, and records the commit and date
in `UPSTREAM.md`. Same commit in, same bytes out.

The generated file is committed. The build stays offline and reproducible,
no build script or `serde_json` enters a leaf crate's build-dependencies, and
the table is readable in review — the same reasoning `rust/vendor/README.md`
already records for `libghostty-vt-sys`.

The old subset goes: `rust/assets/icons/file-types/` and its `SOURCE.md` are
deleted, `Icon::FileType`'s thirty-seven-arm path/`svg()` match is replaced by a
lookup into the generated table, and `ATTRIBUTION.md` for the new crate is
written in the shape `zed/` and `lobehub/` already use.

## §6 The tests that keep the tables honest

In `sirio_icons`, all of them windowless:

1. `every_table_entry_names_a_vendored_asset` — a mapping that points at
   nothing renders nothing, and would look like an unrecognised file.
2. `every_vendored_asset_is_reachable` — or is the light variant of one;
   catches a drop that shipped bytes nobody can draw.
3. `generated_tables_match_the_assets_on_disk` — reads `assets/` and compares
   it with `ASSETS`; catches a hand-edit of the generated file.
4. `tables_are_sorted_and_deduplicated` — the precondition of the binary
   search. If it breaks, lookups return the wrong icon *silently*; nothing
   crashes.
5. `resolution_order_prefers_names_then_longest_suffix` — table-driven:
   `cargo.toml`, `app.blade.php`, `.bashrc`, `README`, `.env.local`,
   `Dockerfile`, unknown extensions.
6. `every_light_variant_has_a_base`.
7. `every_asset_is_utf8_and_opens_with_svg` — the existing check, kept.

In `sirio_ui`:

8. `light_appearance_picks_the_light_variant` (`TestAppContext`), which is
   the only place the `appearance`-not-`mode` decision is enforced.
9. The folder-row tests, rewritten for the pair-by-name.
10. The existing tab/tree parity test, unchanged in intent: the same name
    draws the same mark in both places.

## §7 Risks, and the numbers to confirm

- **The first diff is a vendor drop of ~1113 files.** It lands as its own
  commit (`chore(icons): vendor material-icon-theme@<sha>`), separate from
  the code, so review reads two files rather than a thousand.
- **Binary size grows by roughly 1.4 MB** (~1.06 MB of SVG, ~0.35 MB of
  tables). Estimated here; measured before the branch is finished, and the
  measurement goes in the PR.
- **Raster cache growth.** The cache is unbounded and was written for seven
  marks. Its real ceiling is *distinct types on screen × sizes in use*, and
  a 15px icon rasterises to about 3.6 KB, so 200 entries ≈ 0.7 MB. To be
  confirmed with the app open on a large tree, not assumed.
- **Compile time** for 1113 `include_bytes!` is paid in a leaf crate that
  changes only when upstream is re-vendored, so it stays out of the
  `sirio_ui` edit loop.
- **Cycle version.** This is a `feat:` and the first of the 0.23.3 cycle
  opened by #541, so it moves `rust/Cargo.toml` to `0.24.0` through
  `Scripts/set-workspace-version.sh`, per CLAUDE.md's version rule.
