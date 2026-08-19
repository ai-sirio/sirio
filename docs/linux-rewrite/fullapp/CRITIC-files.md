# Critic pass — commit c3b9e7ac, "file-01-08-natsort-icons" (F-CORE-FILE-01, F-CORE-FILE-08)

**Verdicts: F-CORE-FILE-01 FAILED — defective (regressed to absent). F-CORE-FILE-08 PASSED.**

Fresh critic, no relation to the two commits under judgement. Worktree
`/var/tmp/tt-f01f08-1787151932-b18`, branch `file-01-08-natsort-icons-b18`, HEAD
`c3b9e7acb013bb492cda9ae1b9869b354c4bb959`. Built with my own `CARGO_TARGET_DIR=/var/tmp/tt-critic-files`
(`cargo build -p tiller_ui -p tiller_project --lib`, then `cargo build -p tiller --bin tiller`; both
finished clean, `dev` profile, no warnings beyond a pre-existing `gpui_linux` `nightly_coverage`
`cfg` warning unrelated to this diff). Driven live under my own `Xvfb :3` (booted with `-displayfd`,
never touched `:1`/`wayland-0`/`wayland-1`), binary `/var/tmp/tt-critic-files/debug/tiller`, isolated
`TILLER_DB=/var/tmp/tt-critic-files-db.sqlite`, driving script adapted from `Scripts/linux-drive.sh`
(same click/key/shot helpers, pointed at my own binary and display) saved at
`/tmp/claude-1000/.../scratchpad/critic-drive.sh`. Fixture: a throwaway git repo at
`/var/tmp/tt-critic-files-fixture` with the awkward natural-sort cases named in the brief
(`file1/2/9/10.txt`, `a1b2/a1b10/a2b1/a10b1`, `007`/`7`) plus one file per `FileIconKey.swift`
extension family (`main.swift .py .rs .go .html .css .json .yaml .toml .xml .md .txt .pdf .png .mp4
.mp3 .ttf .zip .sh .sql .sqlite .log .env .lock .conf`, `Dockerfile`, `Makefile`, `.gitignore`,
`LICENSE`) and one directory per named-folder convention (`.github .remember(app-created) a-dir docs
node_modules src tests Z-dir`). Screenshots under
`docs/linux-rewrite/fullapp/critic-files-shots/` (committed, not `/dev/shm`).

I read both commits' full diffs (`ec7e84eb` "sort the file tree in natural-numeric order" and
`c3b9e7ac` "port FileIconKey's file-icon table into tiller_ui") against the Swift originals
(`Packages/TillerCore/Sources/TillerCore/FileTree.swift`, `FileIconKey.swift`) before driving
anything live.

## F-CORE-FILE-01 — natural sort: the fix exists, was never wired to the UI, and is now deleted

**What `ec7e84eb` actually did.** It added `tiller_project::file_sort` (`compare_file_tree_names`,
`natural_case_insensitive_compare` — digit runs compare numerically, leading zeros ignored then
tie-broken by raw bytes, a bounded Latin-1 Supplement accent fold, 9 unit tests) and wired it into
`tiller_project::file::read_directory`'s comparator, replacing the naive
`(!is_directory, name.to_lowercase(), name)` tuple sort. This part was a correct, well-tested port of
the natural-numeric half of Swift's `localizedStandardCompare` (the diacritic half was honestly
documented as bounded, per the brief).

**The defect the brief names was never in that function to begin with.** The Files panel a user
actually sees is rendered by `tiller_ui::right_panel.rs`'s `read_tree` (called from lines 323 and
425, feeding the row list at line 596's `file_glyph` and the tree flattening below it). `read_tree`
does its own `std::fs::read_dir` and has always had — before, during, and after both commits under
judgement — its own independent inline comparator:

```rust
nodes.sort_by(|left, right| {
    right.is_dir.cmp(&left.is_dir)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
});
```

`tiller_project::load_file_tree` / `read_directory` (the function `ec7e84eb` fixed) has **zero**
callers anywhere outside `tiller_project` itself — confirmed by grep across `crates/tiller/src`,
`crates/tiller_ui/src`, `crates/tiller_control/src` for `load_file_tree`, `FileTreeEntry`,
`compare_file_tree_names`, `natural_case_insensitive_compare`, `file_sort::`: no matches. It is
exercised only by its own unit tests. So even at its best, `ec7e84eb` fixed a comparator the running
app never calls; `right_panel.rs`'s comparator — the one the previous critic pass actually drove live
and recorded in the ledger (`F-CORE-FILE.md`, `file.rs:216-223` cited as root cause, but the pass's
own reproduction screenshot is unambiguously of the Files panel, i.e. `right_panel.rs`'s output) —
was never touched.

**Then `c3b9e7ac` deleted the fix outright.** Its own diff (visible via
`git diff ec7e84eb c3b9e7ac -- rust/crates/tiller_project/src/file.rs`) reverts `read_directory`'s
comparator back to the exact original naive tuple sort, and deletes the
`loads_file_tree_in_natural_numeric_order` test. `file_sort.rs` (252 lines, the whole module and its
9 tests) is gone from the tree; `lib.rs` no longer has `mod file_sort` or the `pub use`. None of this
is mentioned in `c3b9e7ac`'s commit message, which describes only icon work. Confirmed on disk at
HEAD:

```
$ git show c3b9e7ac:rust/crates/tiller_project/src/file_sort.rs
fatal: il percorso 'rust/crates/tiller_project/src/file_sort.rs' non esiste in 'c3b9e7ac'

$ sed -n '217,223p' rust/crates/tiller_project/src/file.rs   # at HEAD, on disk
    entries.sort_by(|left, right| {
        (!left.is_directory, left.name.to_lowercase(), &left.name).cmp(&(
            !right.is_directory,
            right.name.to_lowercase(),
            &right.name,
        ))
    });

$ cargo test -p tiller_project --lib -- file_sort file_icon
running 2 tests
test file_icon::tests::for_directory_name_matches_the_original_table_and_falls_back_to_folder ... ok
test file_icon::tests::for_file_name_matches_the_original_exact_and_extension_tables ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 46 filtered out
```
(no `file_sort` tests ran because none exist to run — `filtered out` counts every other
`tiller_project` test, none of them natural-sort tests either.)

**Live reproduction against the HEAD binary**, fixture repo, Files panel opened with `Ctrl+Shift+I`:

- `file1.txt, file10.txt, file2.txt, file9.txt` — `file10.txt` renders **before** `file2.txt**, the
  exact case named in the brief
  (`docs/linux-rewrite/fullapp/critic-files-shots/file01-file2-file10-order.png`).
- `a10b1, a1b10, a1b2, a2b1` — the mixed letter/digit case, also wrong: natural order is
  `a1b2, a1b10, a2b1, a10b1`; what renders is plain byte-lexicographic order instead
  (`docs/linux-rewrite/fullapp/critic-files-shots/file01-a1b2-a10b1-order.png`). This is the
  strongest single frame: naive-lexicographic and natural order agree on the `file*.txt` case's
  relative pairs often enough to be ambiguous, but `a10b1` sorting before `a1b2` only happens under
  plain byte comparison.

**Net effect: F-CORE-FILE-01 is not merely unfixed, it is unfixed *and* the codebase now has one
fewer working artifact than before `ec7e84eb`** — a dead module was written, tested, then deleted by
the next commit in the same branch, and the live behaviour never moved from where the original
critic pass found it. Reading only `c3b9e7ac`'s diff (the actual HEAD state) would show a clean,
well-tested icon commit with no visible connection to sorting at all — the regression is only visible
by diffing across both commits, which is why I checked `git log --oneline --all | grep -i file-01`
before trusting the HEAD commit's own diff to be the whole story.

## F-CORE-FILE-08 — file-icon table: real, wired, and correct where it claims to be

`c3b9e7ac` added `tiller_project::file_icon::FileIconKey` (57 variants, `for_file_name` /
`for_directory_name`), ported 1:1 against `FileIconKey.swift`'s three tables (exact-name,
extension, directory-name), including the `NSString.pathExtension` vs. Rust `Path::extension()`
edge case (`.bashrc` has no "other" dot so both treat it as extension-less, correctly *not*
special-cased since the original doesn't either — documented and tested, not invented). It rewires
`tiller_ui::right_panel.rs`'s `file_glyph` (called at line 596, inside the actual row-render loop —
confirmed by reading the surrounding code, not just the function in isolation) to resolve
`FileIconKey` first and map the small subset with a real comet shape (`Shell`→terminal,
`Git`/`FolderGit`→branch, `Env`/`Settings`→gear, new `Archive`→archive-box, new `Lock`→key) while
every other key correctly falls through to the generic file/folder mark, per P76's "comet is a
replacement, not an addition" rule (no Phosphor glyph reappears).

**Live confirmation**, same fixture and binary, Files panel:

- `.env` and `service.env` both render the gear icon (exact-name and extension-suffix paths both
  reach `Settings`) — `file08-icons-env-gitignore.png`, `file08-icons-service-env.png`.
- `.gitignore` renders the branch icon — `file08-icons-env-gitignore.png`.
- `deploy.sh` renders the terminal icon, distinctly different from every neighbouring row
  (`Cargo.toml`, `clip.mp4`, `data.sqlite`, `data.yaml`, `doc.pdf`, `Dockerfile`, `face.ttf`,
  `file1.txt`/`file10.txt`/`file2.txt`/`file9.txt`, `index.html` — all the same generic file mark,
  correctly, since none of Swift/Python/Rust/Go/Video/Database/Yaml/Pdf/Dockerfile/Font/plain-text
  has a comet shape) — `file08-icons-deploy-sh.png`.
- `bundle.zip` renders the archive-box icon and `Cargo.lock` renders the key icon, immediately
  adjacent to `Cargo.toml`'s generic mark — `file08-icons-zip-lock.png`.
- Every directory (`.github`, `.remember`, `a-dir`, `docs`, `node_modules`, `src`, `tests`, `Z-dir`)
  renders the same generic folder mark, correctly — comet has one folder shape and the row's own
  code comment says so; `.git` itself is filtered out of the tree entirely before `file_glyph` ever
  sees it (`right_panel.rs`'s `read_tree`), so `FolderGit`→branch is real but not independently
  visible in a normal listing — noted, not claimed as separately proven.

`cargo test -p tiller_project --lib -- file_icon` passes both of `file_icon`'s tests (table-driven
against the Swift source, quoted above). This row does what the brief asked and I could not
reproduce anything from the prior `FAILED — defective` verdict.

## What I did not finish

A `cargo clippy -p tiller_project -p tiller_ui --lib -- -D warnings` and a full
`cargo test -p tiller_ui --lib -- right_panel` were both queued behind this same build; the host was
running at load average 28–38 on 12 cores (many other agents' concurrent builds sharing the disk —
`systemd-journal` alone was pinned at ~75% CPU in D-state during my `tiller` binary link step, which
took 18m24s for a build that took 12m01s for the lib-only target minutes earlier). I did not wait
for them past a reasonable bound once the live-drive evidence above was already conclusive for both
rows; if either surfaces a warning in `file.rs`, `file_icon.rs`, or `right_panel.rs` specifically it
is worth a follow-up, but a clippy pass cannot change either verdict above — one is a compiled
dev binary visibly reproducing the named defect, the other is the same binary visibly not
reproducing it.

## Regressions

None found outside the F-CORE-FILE-01 self-regression described above. `cargo build -p tiller_ui -p
tiller_project --lib` and `cargo build -p tiller --bin tiller` both complete with zero errors and no
new warnings; `cargo fmt --check -p tiller_project -p tiller_ui` shows pre-existing drift only in
`chat.rs`, `sidebar.rs`, `tab_bar.rs` — none of the five files this pair of commits touched
(`file.rs`, `file_icon.rs`, `right_panel.rs`, `icons.rs`, `lib.rs`) have any formatting diff.

## Biggest remaining gap

**Wire `tiller_ui::right_panel.rs`'s `read_tree` to `tiller_project`'s comparator (or re-add
`file_sort` and call it from `read_tree`'s `nodes.sort_by`), and delete `read_tree`'s own inline
`to_lowercase()` sort** — that one `nodes.sort_by` block at `right_panel.rs`'s `read_tree` (around
line 1310 at HEAD) is the entire gap. The natural-sort logic itself does not need to be re-derived:
`ec7e84eb` (`f167995d`) has a correct, tested implementation sitting in git history one commit before
HEAD — `git show ec7e84eb:rust/crates/tiller_project/src/file_sort.rs` recovers it verbatim. The
next builder's real task is placing the call where the brief's own diagnosis missed: not
`tiller_project::file::read_directory`, but `tiller_ui::right_panel::read_tree`, which is the only
file-tree implementation the running app's Files panel actually calls. A regression test belongs in
`right_panel.rs`'s own test module (`read_tree` on a fixture directory with `file2.txt`/`file10.txt`,
asserting order) precisely because a unit test on the correct-but-disconnected `tiller_project`
function already existed once and proved nothing about what the user sees.
