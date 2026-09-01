# Bezel identity patterns adoption — design (sub-project 3)

Date: 2026-09-01
Status: approved
Umbrella: `docs/superpowers/specs/2026-08-31-bezel-gallery-adoption-design.md`
(sub-project 3 of 3). Requires sub-projects 1 (bezel `=0.1.4`, gpui `=0.3.8`,
`fe8d0989`) and 2 (generic widgets, merged as `2db80b54`).

## Goal

Adopt bezel's identity surfaces in `sirio_ui`: the chat transcript, the
changes/diff view, the document editor, and the sidebar tree. The gallery
(`crabtalk/bezel`, tag `v0.1.4`, `apps/gallery/src/patterns/`) is the visual
source of truth; where Sirio differs, Sirio changes.

## Two adoption modes (decided)

The gallery patterns are not all the same kind of thing, and the sub-project
uses two distinct mechanics:

- **Pattern copy** — `transcript.rs` and `diff.rs` in the gallery are built
  from `motion` + `theme` + `ui` only; there is no component to import. The
  pattern's structure and styling are transcribed into the Sirio surface, and
  the hand-rolled styling it replaces is deleted.
- **Crate adoption** — bezel publishes real crates outside the facade, all at
  `0.1.4` on crates.io (verified 2026-09-01): `bezel-markdown` (document
  model, parse, paint), `bezel-editor` (Notion-style block editor over that
  model: focus, keys, mouse, undo, menus), `bezel-syntax` (highlighter).
  These are added as direct dependencies pinned `=0.1.4`, same policy as the
  facade pin. `ui::tree` is already inside the facade.

**Excluded: `bezel-terminal`.** Sirio's terminal is libghostty-vt by
architecture (CLAUDE.md); the alacritty-backed bezel terminal is not a
candidate and must not enter the dependency tree.

**Excluded: orbit and project_identity.** `orbit.rs` documents its own
decision — bezel orbs mean "wait", Sirio's orbit means "empty surface" — and
stays. `project_identity.rs` renders project glyphs/emoji/PNG avatars; bezel's
avatar/mascot is bezel's own brand character and does not map.

## The families, in execution order

One branch; one commit per family; each commit builds and passes per-crate
tests before the next family starts. A single user visual review gates the
end of the whole sub-project.

1. **chat** — layout and styling from the gallery `transcript.rs` pattern
   (message cards, roles, spacing, streaming affordances). Message bodies
   move to `bezel-markdown`: parse to `Doc`, paint with bezel's renderer,
   deleting the hand-rolled `render_markdown`, `render_markdown_block`,
   `render_markdown_list`, `render_markdown_table` family in `chat.rs`
   (~500 lines). Block coverage verified: `BlockKind` carries Paragraph,
   Heading 1–6, Bullet, Ordered, Task, Quote, Code (with language), Image,
   Bookmark, Table — everything the hand renderer draws today.
   - Risk (the largest in the sub-project): streaming turns re-render on
     every delta, and `render_markdown_document_with_link_override` carries
     app-specific link behavior. The contract is behavior parity: existing
     chat tests are the net, link overrides survive, and per-delta parse
     cost is measured before merge (a coarse timing assertion is enough —
     parse of a typical turn under a frame budget).
2. **changes** — the gallery `diff.rs` pattern (`theme::ink` + `ui` widgets)
   restyles the changes/diff view, including the scroll handling explicitly
   deferred from sub-project 2.
3. **editor/file_view** — hybrid, split by file kind:
   - **Markdown files** adopt `bezel-editor`: the custom pixel half (caret,
     selection painting, toolbar interactions in `file_view.rs`) is replaced
     by `editor::Editor` over a `bezel-markdown` document. The headless
     model in `sirio_ui/src/editor.rs` (load, save, conflict detection,
     dirty state, registry — F-EDIT-04/05/06/08) stays: `bezel-editor` does
     no I/O, so Sirio's model feeds it content and receives edits.
   - **Code files** keep the custom no-wrap editor (F-EDIT-07: language
     detection, four-space indent, horizontal scroll) and gain
     `bezel-syntax` highlighting.
   - `editor::init(cx)` joins app bootstrap and every `TestAppContext`
     setup that exercises the editor, mirroring how `input::init` landed in
     sub-project 2.
4. **sidebar** — the project/worktree rows adopt `ui::tree`.
   **Gated:** `sirio_ui/src/sidebar.rs` carries the user's uncommitted WIP;
   this family starts only after the user commits or shelves it. If the WIP
   is still pending when families 1–3 are done, the sub-project merges
   without the sidebar family and the family lands as a follow-up on the
   same spec.

## Constraints

- No user-visible behavior change beyond the bezel look. Focus order,
  keyboard shortcuts, submit/dismiss semantics, editor conflict flows stay
  identical.
- New dependencies pinned `=0.1.4`: `bezel-markdown`, `bezel-editor`,
  `bezel-syntax`. Nothing else enters the tree; `bezel-terminal` is
  explicitly refused.
- The gallery reference is tag `v0.1.4` of `crabtalk/bezel` — not `main`,
  whose patterns already use unreleased APIs.
- Existing tests update only where they asserted rendering details of the
  old implementation; never where they assert behavior.
- Test failures compare as **lists** against the known-red baseline pool in
  `sirio` (oscillating; measure on main before judging).
- Deletion per family: each commit removes exactly the hand-rolled code
  that family obsoletes, nothing else.
- Workspace gates (`Scripts/ci.sh`, `Scripts/ci-linux.sh`) only on the
  user's explicit request.

## Delivery

As sub-projects 1 and 2: Herdr worker execution (orchestrator/reviewer in
the main session, one worktree per writer), one commit per family, single
visual review at the end gating the merge to main.

## Out of scope

- Any bezel/gpui version change.
- `bezel-terminal`, orbit, project_identity (reasons above).
- New editor or chat features not present in the current UI.
