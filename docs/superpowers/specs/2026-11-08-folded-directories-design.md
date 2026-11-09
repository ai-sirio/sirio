# Folded directories in the Files tree

**Date:** 2026-11-08
**Status:** design
**Branch:** `files-managment-fix`. It was cut from the pre-squash history of #718
(`origin/feature/detect_claude_code`, 71 commits ahead of `origin/main`, none touching the Files
tree). It moves onto `origin/main` before any code is written.
**Reference:** Zed's project panel, `auto_fold_dirs`, read at zed-industries/zed
`d2a4fb6ded7c470be25d1541281da3882495b65b` (`crates/project_panel/src/project_panel.rs`, the
fold block in `update_visible_entries` at 4678-4692).

## §0 Intent

Enzo had a Java project open in Sirio and had to open folder after folder
(`src/main/java/it/unipolsai/aad/prodotto/v2`) to reach the code. Zed shows the same project
with every chain of single-folder directories joined into one row,
`java / it / unipolsai / aad / prodotto / v2`, and `docs / adr`, `config / pmd` the same way.
He wants that in Sirio's Files tree (the right panel's Files view).

Decided with Enzo, in order:

1. **Generic, like Zed.** Zed has no Java rule: a directory whose only entry is a directory joins
   it, for any language. Enzo asked to look at other languages too. The rule helps most where
   chains hold nothing but folders: Java, Kotlin, Scala, Android, PHP PSR-4 and `src/` trees in
   JS/TS. It helps little in Python, where `__init__.py` stops the chain, and Rust, where
   `Cargo.toml` sits beside `src/`. No language gets a special rule.
2. **The joined row is enough** ("mi basta la riga unita come in Zed"). The pieces of the row
   are not clickable one by one. The row behaves as its last folder.
3. **The playback was approved** ("va bene così, procedi"). It covered:
   - the row acts as the last folder;
   - only entries on screen count, so a hidden dotfile does not break a chain;
   - git-ignored entries count;
   - rows appear already joined, with no visible jump;
   - the row splits and rejoins by itself when the disk changes.
   §3 is that playback made precise.
4. **Correction to the playback.** I told Enzo the git dot is the same at every link of a chain.
   That is false, and §3.3 shows why. The row shows the outermost link's dot instead. He has not
   yet confirmed this correction.

## §1 How Zed does it, and where Sirio differs

- **Zed's rule.** A directory folds into its child when the directory is not the worktree root,
  is not explicitly unfolded, and its snapshot listing holds exactly one entry, which is a
  directory. Zed decides from its whole-worktree scan, so it knows every listing up front.
- **Hidden entries.** Zed counts hidden and git-ignored entries even when its own filters hide
  them, so a hidden `.DS_Store` breaks a chain on macOS. **Sirio counts what is on screen**
  (§3.1).
- **Segments.** Zed makes each segment of the row selectable, as the target of new file, rename,
  delete and drag and drop. It also has Fold and Unfold menu items, and ←/→ walk the segments.
  None of that is in scope (§4).

## §2 The Files tree today (code facts)

All paths are under `rust/crates/sirio_ui/src/right_panel/`.

- **Loading is lazy.**
  - `refresh` reads only the root listing (`files.rs:211`).
  - A directory is read when it is first expanded: `toggle_file` checks `children.is_empty()`
    (`files.rs:352`), then `start_walk` reads off-thread (`files.rs:371-401`).
  - A collapsed directory's contents are therefore unknown, and so is whether it holds a single
    folder.
  - `build_snapshot`, used by the host's per-worktree precache (`mod.rs:164`), builds the same
    root listing.
- **Nested listings never refresh.**
  - On a root refresh, `preserve_expansion` (`files.rs:1140-1156`) copies the old children of
    every expanded node and drops the children of collapsed ones.
  - Nothing re-reads a directory that is already loaded. So a file created inside an expanded
    subfolder never appears today. That is an existing bug, and this design must fix it (§3.6).
- **Hidden filtering comes after flattening.** It runs in `file_rows(show_hidden)`
  (`files.rs:521-542`), after `rebuild_file_rows` has already flattened the tree.
  `ShowHiddenFilesSetting` is a session-wide global (`mod.rs:176-190`).
- **`FileRow` deep-clones.** It holds a deep clone of its `FileNode`, children included
  (`files.rs:1158-1168`), and `file_rows` clones every row again on every render.
- **The row.** Each row is 26 px tall in a `uniform_list` (`files.rs:592-719`).
  - The name ellipsises at its end (`files.rs:683-685`).
  - Indent is `8 + depth*16` px, and there are no indent guides.
  - The folder icon is the open/closed control (`file_row_glyph`).
  - The git dot comes from `DirectoryGitStatus`, an aggregate over the directory's subtree.
- **What is not a directory.** `.git` is skipped (`files.rs:1103`). A symlinked directory is
  classified as a file, because `file_type()` does not follow links (`files.rs:1106`).
- **Refresh triggers.**
  - The host's recursive watcher (`main.rs:8305-8387`) calls `mark_files_dirty` (`mod.rs:474`),
    which calls `request_refresh` (300 ms debounce, `files.rs:305`). The changed paths are
    dropped along the way.
  - Several comments still describe a 1 s refresh loop that no longer exists (for example
    `files.rs:234-243` and `:489-491`).
- **No E2E or socket verb.** No E2E drives the tree, and no control-socket verb exposes its rows.

## §3 Behaviour

### §3.1 The fold rule

A directory **D** joins its child **C** into one row when all of the following hold:

- D's **visible listing** holds exactly one entry, and that entry is C, a directory. The visible
  listing is D's entries after the hidden filter at the current toggle state. With hidden files
  off, every name starting with `.` drops out. With hidden files on, nothing drops. `.git` is
  never listed. Git-ignored entries stay visible (dimmed), so they count.
- D's listing is known and readable. An unreadable D, or a D whose listing has not arrived,
  ends the chain where it stands.

The chain then continues from C under the same rule. It ends at the first directory that does
not satisfy it: the **leaf**.

- The worktree root is not a row, so a top-level directory can fold like any other.
- A symlinked directory stays a file row, as today, so no chain passes through it.
- A file never joins a chain. A directory holding exactly one file does not fold.
- An empty leaf is fine: `a/b/c` with `c` empty is one row, `a / b / c`, with nothing under it.

The fold depends on the hidden toggle by design.
- `it/` holding `.DS_Store` and `unipolsai/` folds through `it` while hidden files are off.
- The same `it/` splits at `it` while they are on.
- A directory whose only entry is a dotfile or a dot-directory has an empty visible listing
  with hidden files off. It is a plain row and does not fold.

### §3.2 The label

The row's single name slot shows the link names joined with ` / `, a space, a slash and a
space, outermost first: `java / it / unipolsai / aad / prodotto / v2`. The separator is the
same on every platform, Windows included.

When the row is too narrow, **the leaf's name stays readable and the start of the chain is
shortened** (my call). Example: `… / aad / prodotto / v2`. Today's end-ellipsis would cut away
`v2`, the part that matters.

### §3.3 The row acts as its leaf

- Selection, open/close, the context menu (Reveal in File Manager, Copy Path, Refresh) and
  keyboard navigation all use the **leaf's path**.
- The icon is the leaf's folder glyph, open or closed (`folder_glyph(leaf, expanded)`).
- The name dims when the leaf is git-ignored. Ancestry already makes the leaf ignored whenever
  any link above it is.
- The leaf's read-error warning shows on the row.
- The leaf's children appear one level deeper than the row.
- **The git dot is the outermost link's status.**
  - A link's subtree holds every link below it. The only things in the outermost link's subtree
    and not in the leaf's are paths that exist only in git: a deleted or renamed-from tracked
    file in a middle link.
  - Example: `java/it/Old.java` was deleted. On disk `it` still holds only `unipolsai`, so the
    chain still folds, and git marks `it` and `java` but not `v2`.
  - The outermost link's aggregate is therefore the superset. It is the dot that never hides a
    change.

### §3.4 Opening and closing

- **Opening a folded row marks every link expanded. Closing marks every link collapsed** (my
  call).
- **A row is drawn open only when every link is expanded**, and opening or closing acts on that
  drawn state. The links can disagree: a refresh that started before a close reopens the leaf
  it re-read, and turning hidden files off can join a closed folder with an open one. Taking
  the state from the leaf alone would then draw a closed row as open. This was revised after the
  final review; the first version took the row's state from the leaf.
- When a chain later splits, each piece keeps the state the user saw. If `v2` was open and a
  file appears in `it`, the result is an open `java / it` with an open
  `unipolsai / aad / prodotto / v2` under it, and the content stays on screen.
- `collect_expanded`, `restore_expanded` and the worktree-switch snapshot keep working on plain
  paths.

### §3.5 No visible jump: lookahead in the same task

**Lookahead is one level plus chains** (my call, after the builder check). When the tree shows a
directory's children, that is when the root loads or a directory is opened, the **same
background task** reads that directory's listing and, for each child directory, the chain that
starts there. This applies to the root refresh, the walk started by opening a directory, and
`build_snapshot`.

- A child that can start a chain is followed link by link until the chain ends. The leaf's
  listing is read, because that is how the walk learns the chain has ended, and it is kept. Only
  the fold decision uses it.
- **Children appear already folded.**
  - A directory is *resolved* when every child directory in its listing has its own listing
    loaded, or failed to read.
  - Every link of a chain is resolved. The walk loads all of a link's child directories,
    dot-directories included, so the chain is right in both toggle states.
  - Opening a resolved directory draws its children at once. They are already folded, because
    every listing the fold needs is held.
  - Opening an unresolved directory starts a walk, as today. An unresolved directory is a leaf,
    or a plain folder with subfolders. The walk reads the directory's listing plus its
    children's chains.
  - The children of an unresolved directory are drawn only when that walk lands, never first
    from the held listing and then folded. That open is asynchronous, as today.
  - This bullet was revised after the plan review. The first version said that every open walks.
- **macOS privacy.** Lookahead never reads a child that is itself one of the folders macOS
  guards with a privacy prompt: `~/Desktop`, `~/Documents`, `~/Downloads`, `~/Pictures`,
  `~/Movies`, `~/Music` and `~/Library`.
  - Such a folder is read only when the user opens it, as today, so browsing a project rooted at
    `~` raises no prompt by itself.
  - A project that lives inside one of them is unaffected: Sirio already reads it.
  - This is my call, after the plan review.
- **What the tree holds:**
  - the root's listing;
  - for every directory whose children are on screen (the root, and every expanded directory):
    that directory's listing, and the listings of each child chain's links and leaf.
  - Nothing deeper is held. When a directory closes, its deeper listings are dropped at the next
    refresh, as `preserve_expansion` does today for collapsed nodes.
- **"Can start a chain" covers both toggle states.** The walk continues while the listing holds
  exactly one non-dot entry and that entry is a directory (the hidden-off rule), or holds exactly
  one entry that is a directory (the hidden-on rule). Flipping the hidden toggle therefore
  re-folds without another read.
- Lookahead listings are kept as real child nodes, holding the full listing with dotfiles
  included. The fold is decided when rows are built, with `show_hidden` known.
  - Today the hidden filter runs after flattening, so a fold decided in `rebuild_file_rows`
    alone would be wrong in one of the two toggle states.
  - `toggle_file`'s `children.is_empty()` test can no longer mean "never read", because
    lookahead fills collapsed directories.
- **The UI thread never reads the disk.**
  - An ordinary expand, under about 200 child directories, must not feel slower than today.
  - For a huge listing (`node_modules`), the builder may cap the lookahead (my call). One
    option: above a threshold of child directories, show that listing unfolded. This is the
    only case where rows may appear unfolded.
  - **A huge child listing is not read ahead** (revised after the final review).
    - Lookahead stops reading a child directory once that child is past
      `LOOKAHEAD_ENTRY_CAP = 256` entries.
    - Such a child cannot fold in either toggle state, so its listing is not held: it stays
      unknown, and opening it walks, as today.
    - Without this, opening `target/debug` read all of `deps` (36,000 entries) before drawing
      anything, and every refresh read it again.
    - One exception gives way: a listing that is all dotfiles except one folder would fold with
      hidden files off, but past the cap it shows unfolded.

### §3.6 It stays true by itself

- Every refresh re-reads the listings the tree holds (§3.5): the root, the expanded directories,
  and each visible child chain's links and leaf. So the cost of a refresh is bounded by the rows
  on screen times the chain lengths, never by the size of the worktree. That covers the watcher,
  the Refresh menu item and the failure retry.
- **Git-ignored paths stay filtered at the watcher** (my call, after the builder check).
  - `files_watch_path_is_relevant` (`main.rs:147-168`) drops events under ignored paths before
    the tree hears of them, so a build writing to `target/` or `node_modules/` starts no walks.
  - A change under an ignored path therefore shows at the *next* refresh, whatever triggers it:
    any relevant event, Refresh, coming back to Files, or a retry. This is how it works today.
  - The screen stays consistent, because rows and folds come from the same listings. An ignored
    `debug.log` created inside a chain link neither shows nor splits the row until then, and
    after that refresh it does both.
- Fresh listings replace the old ones, and the `expanded` flags carry over by path.
  `preserve_expansion`'s copying of stale children goes away.
- **A user's open during a refresh is kept, and the fresh listings still win.**
  - When a refresh lands, every entry now on disk is in the tree.
  - The fresh read may not have loaded some child that sits below a directory open at landing
    time. That happens when the user opened that folder while the refresh ran. Such a child
    takes the subtree the panel already had, and the next refresh re-reads it.
  - Nothing is carried below a closed directory.
- **No refresh request is lost.** A refresh asked for while an open's walk runs is not dropped:
  the walk's landing starts it. Today `refresh` returns early while a walk is in flight and
  leaves the request pending with nothing to re-arm it.
- **The host's precache keeps a worktree's open folders.** It rebuilds a dirty snapshot through
  the same reload, with the expanded paths that snapshot already had. Switching worktrees and
  back therefore finds the same rows open. Today a rebuilt snapshot comes back with nothing
  open.
- Results:
  - A file created inside `it` splits the row after the watcher and the debounce.
  - Deleting the file rejoins the row.
  - A new file inside an expanded subfolder appears, which fixes the bug in §2.
- No refresh walks a directory the tree has not loaded.
- The builder chooses how (§8):
  - re-read every held listing; or
  - thread the watcher's changed paths through `mark_files_dirty` and re-read only the affected
    directories, plus the root on a manual Refresh.
- When chains form or split, the selection stays on the same thing.
  - A selected path that becomes a middle link shows as the folded row that contains it.
  - A folded row whose leaf was selected stays selected after a split, on the piece that now
    ends at that path.

### §3.7 Keyboard

- ↑/↓ move over a folded row as one row.
- Enter and Space open or close it, which means all of its links.
- Nothing else changes.

## §4 Non-goals and anti-goals

**Non-goals.**
- Per-segment hover, click or action target.
- Fold and Unfold menu items, and remembered per-directory unfold.
- An on/off setting: folding is always on (my call).
- A row tooltip.
- Indent guides.
- Revealing the active file.
- New file, rename, delete, and drag and drop into the tree.
- Package names with dots (IntelliJ style).
- Following symlinked directories.
- Fixing the double-click on a folder that opens and immediately closes it (`files.rs:620-631`).
  It is observed, not in scope; see §7.

**Anti-goals.** Any of these would make the feature a failure even if it folds:
- Rows that visibly appear separate and then join, in ordinary use.
- A fold that disagrees with the screen: a chain joined through an entry that is on screen, or
  broken by an entry that is hidden.
- Any `read_dir` on the UI thread.
- A git change inside a folded chain with no dot on the row.
- A file that exists on disk inside a loaded folder and is still missing after a refresh.

## §5 Constraints

- `sirio_perf` labels stay content-free `&'static str`. Never put a path or a name in a label.
- Rows stay 26 px tall for `uniform_list`.
- **`FileRow` must stop deep-cloning subtrees.** Lookahead makes collapsed directories
  non-empty, so the per-flatten and per-render clones grow. The row carries only what it
  renders: link names, leaf path, depth, expanded, git status, ignored and read error.
- The existing debug selectors keep their meaning, and the folded label sits in
  `file-directory-name`: `file-row`, `file-directory-row`, `file-directory-name`,
  `file-directory-icon-*`, `file-status-<slug>-<path>`. For a folded row, the `<path>` in the
  status selector is the leaf's path.
- Some in-crate tests in `files.rs` (module at `:1217`) have fixtures with single-folder
  chains. Those fixtures now produce folded rows: update their expectations to this contract.
  Do not switch folding off for tests.
- `build_snapshot` and the panel's refresh use one loader. A worktree restored from the host's
  snapshot cache arrives already folded.
- Testing rules (CLAUDE.md): end to end first.
  - Unit tests exist only for the two isolated units, the fold rule and the loader.
  - Each is written before its code, from a failure list. The fold rule's list is in §6; the
    loader's is in the plan.
  - The plan review added the loader.

## §6 Proof

**Fold rule, in isolation.** Write these failure cases before the code. The rule is a pure
function from the tree, `show_hidden` and the expanded flags to rows.

1. Folds through a directory that also holds a file.
2. Folds through a directory with two subdirectories.
3. With hidden files off, a dotfile in a link breaks the chain.
4. With hidden files on, a dotfile in a link fails to break the chain.
5. A directory whose only entry is a dot-directory folds into it while hidden files are off.
6. A git-ignored sibling fails to break the chain.
7. A symlink to a directory is folded through.
8. A link whose listing is unknown or unreadable folds anyway, or panics.
9. A directory holding exactly one file folds.
10. The leaf's children land at the wrong depth.
11. The row's path or icon comes from the outermost link instead of the leaf.
12. The git dot comes from the leaf: a deleted tracked file in a middle link leaves the row
    clean.
13. Opening a folded row expands only the leaf, so a later split shows a closed head over an
    open tail.
14. The selection is lost when the selected path becomes a middle link.
15. Labels are in the wrong order or use the wrong separator.

**End to end.** Two pieces.

**The read verb.** A new debug-only read verb, `surface.files.read`, listed only in
`served::served_methods(debug)` (see `docs/agents/architecture/control-socket.md`). For each row
on screen it returns:
- the label;
- the leaf path relative to the worktree;
- depth, `is_dir`, expanded and selected;
- the git marker slug;
- the row's window bounds.

It also returns:
- the count of refreshes that have landed, so a script can wait for the refresh that sees a
  change before it holds a state;
- every row label the panel built since the last read that asked to reset that record. A row
  drawn unfolded for a single frame is therefore caught however fast the walk lands, which
  polling could not guarantee.

**The script.** A new `Scripts/Tests/test-files-fold-e2e.sh --xvfb`, shaped like
`test-prompts-e2e.sh`:
- one isolated debug Sirio, with private HOME, `SIRIO_DB`, stub CLIs and TMPDIR on disk;
- driven by real X clicks and keys at the bounds the verb reports.

The fixture is one git worktree holding:
- `source/src/main/java/it/acme/app/v2/App.java` and
  `source/src/main/java/it/acme/app/v2/domain/model/Order.java`, with
  `source/src/main/resources/` alongside;
- `source/src/test/java/it/acme/app/v2/AppTest.java`;
- `source/docs/adr/0001.md` and `source/config/pmd/rules.xml`;
- a Python package with `__init__.py` at each level;
- a tracked `source/src/main/java/it/Old.java` that the script deletes.

Its checks, each printed as `ok N`:
1. `java / it / acme / app / v2`, `docs / adr` and `config / pmd` are single rows. The Python
   package is not folded. Opening `source` and `src` never builds the unfolded form first,
   according to the verb's record of built labels.
2. Opening the Java row shows `App.java` and the folded row `domain / model` one level deeper.
   `domain / model` is never built unfolded first.
3. `touch .../java/it/New.java` splits the row into an open `java / it` with `New.java` and
   `acme / app / v2` under it.
   - Moving the selection to `java / it` with the ↑ key, then removing the file, rejoins the
     row.
   - The joined row is selected, although the selected path `it` is now a middle link.
4. Create `.../it/.DS_Store` and wait for the refresh that sees it.
   - With hidden files off, the row stays joined.
   - Turn hidden files on: it splits.
   - Turn them off again: it rejoins.
5. A new file inside an expanded ordinary subfolder appears. This is the §2 bug.
6. With `Old.java` deleted, the Java row carries the changed dot.

The artifact goes under `--out-dir`:
- `transcript.log` of every socket exchange;
- a PID-matched window capture of each state;
- the fixture's `git status` at each step.

The script ends with `FILES FOLD E2E OK`. Add an entry for it to `docs/agents/e2e-tests.md`.

## §7 Seen on the way, not in scope

Each of these comes from reading the code; none has been run.

- Double-clicking a folder opens and closes it (`files.rs:620-631`).
- Symlinked directories render as files (`files.rs:1106`).
- Comments describe a 1 s refresh loop that is gone (`files.rs:234-243`, `:278-283`,
  `:489-491`, `:1134-1139`; `mod.rs:282-284`, `:1023`). Fix any of these comments that the work
  touches anyway.
- The watcher's pending queue fills up to 4096 paths *before* the relevance filter runs
  (`main.rs:109`, `:8318`). A build's ignored writes can therefore fill it and push out a
  relevant event, against what the comment at `main.rs:104-108` assumes.

## §8 Left to the builder

- **Storage.** Where the fold function lives and how rows are represented, within §5.
- **Lookahead.** How lookahead state is stored, and how "listing known" is told apart from
  "never read" now that `children.is_empty()` no longer means the second.
- **Refresh.** Re-read every held listing, or thread the watcher's paths (§3.6).
- **Truncation.** How the start-truncated label is drawn. One option is a shrinking prefix plus
  a fixed leaf, within one 26 px row.
- **Cap.** The lookahead cap for huge listings, and whether it applies at all.
- **Verb.** The JSON shape of `surface.files.read`.

## §9 Open questions for Enzo

None left after the builder check. It raised three points:

- **Lookahead depth.** Settled as my call in §3.5.
- **Ignored paths at the watcher.** Settled as my call in §3.6.
- **The outermost-link git dot.** This is the §0.4 correction. Enzo is asked to check it while
  reading.
