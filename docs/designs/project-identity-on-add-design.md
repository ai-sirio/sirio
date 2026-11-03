# Name and icon when a project is added: design

*   **Author:** epalmisano
*   **Owner and decider:** Enzo
*   **Status:** Proposed
*   **Date:** 2026-10-30

## Overview

Sirio names a new project after the last component of its folder path (its
root name, defined below), so `~/work/client/app` and `~/work/server/app` both
appear as `app` in the sidebar. This design adds one step to every project
add made from the UI. Right after the project enters the catalog, a modal
dialog titled "Project added" shows the project's root path, a field for the
label (prefilled with the root name), and the icon picker from the Project
Settings tab. Save writes the choice once. Skip or Esc leaves the project with
its root name and default icon. The project is added either way.

Agents that add projects through the control socket never see the dialog.
`project.add` gains an optional `displayName` parameter, and
`sirioctl project add` gains a matching `--display-name` flag.

The document uses these terms:

*   **Root name.** The default name of a project: the last component of its
    catalog root (`base_name`). The catalog root is the primary checkout of the
    folder the user chose. For a linked worktree, the root name is not the
    chosen folder's name.
*   **Root path.** The path of the catalog root, as the dialog shows it.
*   **Label.** The name the user sets for the sidebar: `display_name` in the
    catalog, `displayName` on the socket. It is stored only when it differs
    from the root name (§5).
*   **Shown name.** The label, or the root name when there is none.
*   **Identity.** The label and the icon.
*   **Caption.** The visible name of a field, such as "Name in the sidebar".
*   **Workspace.** `SirioWorkspace`, the app's root view, in
    `rust/crates/sirio/src/main.rs`. It is not `sirio_host`, the separate host
    process.
*   **The Settings screen and the Project Settings tab.** The first is the
    app's Settings. The second is the settings tab of one project.
*   **Add path.** One of the five ways the UI adds a project, one for each
    `AddOrigin` (§2): Open folder, Add without Git, Initialize Git, Clone and
    Create.
    *   Open folder on a folder that is not a git repository shows the
        *non-git prompt* (F-PRJ-03). It has three buttons: Initialize Git,
        Add without Git and Cancel.
    *   The centre's empty state ("Add Project") is a second entry point to
        Open folder. It is not a sixth add path.
*   **Root modal.** One of the workspace's three existing modals: the
    pane-close banner (`pending_pane_close`), the End All confirmation
    (`pending_end_all`) and the title prompt (`pending_title_prompt`). They are
    drawn with `sirio_ui::modal::render_modal`, not with Ely.

The dialog is an Ely entity hosted by the workspace. This follows from three
facts:

*   Ely is the RFC's choice of components.
*   `render_modal`'s `ModalSpec` carries only a `String` body, a field and
    buttons (`rust/crates/sirio_ui/src/modal.rs`), so it cannot hold the
    picker.
*   A dialog hosted by the sidebar fails Goal 2.

The product decisions come from the RFC
[`project-identity-on-add.md`](./project-identity-on-add.md) (PR
[#683](https://github.com/ai-sirio/sirio/pull/683)). Six reviews examined it,
and Enzo decided its open questions on 2026-10-28. This document turns those
decisions into a plan against `main` at `d945ca2f`. Enzo took further
decisions on 2026-10-31, after a review of this document. They are dated where
they apply.

Code is cited by symbol, because line numbers move. Paths are relative to the
repository root. The first mention of a file gives its full path.

Codes point into the RFC:

*   M# and B# are its review findings (Major and Blocker).
*   "Decision N" is its Nth post-review decision.
*   "RFC N" in brackets is its Nth weighed alternative.
*   F-PRJ-## and F-SID-## are feature ids of the app's feature inventory.

### Changes since the RFC

*   **Base.** The RFC was written against the top of the bezel-off stack (the
    pull requests that removed bezel's UI crates), commit `6fa07d19`, before
    it was squash-merged. The stack merged on 2026-10-29 (#668, #670–#674,
    #680). The base is now `main` at `d945ca2f`, in the 0.37.1 cycle, and the
    RFC's line numbers no longer match.
*   **The background refresh no longer replaces the whole catalog (RFC M8,
    decision 10).** Decision 10 made this fix a prerequisite of the feature,
    and it is now on `main`. PR
    [#684](https://github.com/ai-sirio/sirio/pull/684) (`74a92579`) changed
    `refresh_project_for_path_in_background`. It now merges its result through
    `ProjectCatalog::apply_refresh(&project_id, …)` and
    `apply_catalog_refresh`. This design keeps one E2E step that guards it.
*   **The picker still uses bezel's theme.** On `main`,
    `rust/crates/sirio_ui/src/project_identity.rs` still imports `bezel_theme`
    and draws `controls::card`. The bezel-off stack removed bezel's UI crates
    but kept its theme crate.
*   **This narrows RFC decision 8.** A save that changes only the label also
    writes `color: "coral"`, `icon_kind: "icon"` and `icon_value: "folder"`.
    So `project list --json` shows `color` and `iconValue` changing from `""`
    (§4).
*   **Each add path is classified.** Open folder and Add without Git are
    synchronous. Initialize Git and Clone are asynchronous. Create is treated
    as synchronous (§2).
*   **Shortcuts bypass the root key handler,** and Esc over the Settings
    screen goes to the Settings screen first (§7).
*   **Ely's Enter confirms on release,** which needs the extension of
    decision 7 (§6).
*   **The write path is chosen** (§4), and **catalog write errors never reach
    the caller** today (§4).
*   **Three RFC open questions are answered:** the write path (§4), "icon
    untouched" (§3) and the local-path clone (§10).

## The Problem

`ProjectCatalog::add` names a project after its catalog root, which is the
primary checkout of the chosen folder. That name is the last path component.
Two checkouts called `app` under different parents therefore get the same row
title. It happens, for example, with client/server pairs, forks cloned into
different parents, and several copies of one repository.

Today the user can fix this only afterwards. They have to:

1.  Notice the collision.
2.  Work out which `app` is which.
3.  Open the Project Settings tab of one of them.
4.  Type the new name into a hand-written field that has no paste, cursor or
    selection.

Every keystroke saves, so Esc there cannot mean "leave it as it was".

Agents cannot do this. `project.add` reads only `path`, and no socket method
writes a project's settings. An agent that adds two checkouts of `app` leaves
two identical rows behind.

Two existing defects affect this feature:

*   After a save, `project.list` keeps reporting the old `displayName` until
    something else rebuilds the socket's snapshot. This already affects the
    Project Settings tab (RFC M2).
*   Esc in the picker's emoji search does not stop at the search. It bubbles
    up to whatever handles Esc next (RFC M5a). Inside a dialog, that handler
    is the dialog's close.

Three premises of the RFC did not hold when this design was written:

*   **There is no sanitizer to reuse (decision 9).** The code the RFC cites at
    `session.rs:194` (`rust/crates/sirio/src/session.rs`) is inline in
    `PersistedCiLog::restorable`, and it cleans CI job names. The rule of
    decision 9 stays the same, but the label normalizer is new code (§5).
*   **`LOCAL-CHANGES.md` already covers `dialog.rs`.**
    `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` has an entry for
    `rust/vendor/ely-gpui-component/src/overlays/dialog.rs` (the card's
    `text_color`). RFC M3 said it had none. The new entry goes next to the
    existing one.
*   **The update card is not a precedent for a stateful dialog.** The RFC (B2)
    pointed to the update card. `UpdateCard` is a stateless `RenderOnce` that
    is rebuilt from workspace fields on every frame. It is not a subscribed
    entity, and `render_update_card` returns `None` while the Settings screen
    is open. The dialog needs state (a text field and the picker), so it is an
    entity.

## Goals

Each goal is phrased as something the end-to-end proof can show.

1.  **One dialog per successful UI add.** Every successful add from the UI
    opens exactly one "Project added" dialog for the project that was added,
    with its root name prefilled, unless the project is removed before its
    turn in the queue (§8). The add paths are:
    *   Open folder, on a git repository;
    *   Add without Git;
    *   Initialize Git;
    *   Clone;
    *   Create.

    The centre's empty state is an entry point to Open folder.

    An add that adds nothing opens no dialog and shows today's toast. That
    covers a project that is already tracked, a folder nested in a tracked
    project, a folder that contains one, and an add that fails.
2.  **Visible over the shell.** Once open, the dialog is drawn above the
    shell, with the sidebar shown or hidden, the Settings screen, and a Clone
    or Create form. While another root modal is open, it waits in the queue
    (§8). A root modal that opens while the dialog is open waits behind it
    (§7).
3.  **Save changes only the identity, and only on Save.**
    *   Save changes only the label and the icon of the project that was
        added.
    *   Skip, Esc and a click on the scrim (the dimmed backdrop around the
        dialog) change nothing.
    *   Save with no edits leaves the stored record identical to Skip.
    *   The project's worktree defaults (`default_worktree_base`,
        `worktree_location_override`) never change.
4.  **Keyboard behaviour.**
    *   After Open folder, Add without Git and Create, the dialog is in field
        mode (§2): the cursor is in the label field with the name selected.
        Typing replaces it, and Enter saves.
    *   After Clone and Initialize Git, the dialog is in dialog mode (§2):
        focus is on the dialog rather than the field. Keys typed while the
        clone or `git init` ran never reach the field. Enter does not save
        until the user clicks into the field.
    *   Esc skips. Esc in the picker's emoji search closes only the emoji
        grid.
    *   While the dialog is open, shell shortcuts such as closing a tab or
        opening the palette (Ctrl+Shift+P) do nothing. Ctrl+C still copies.
    *   After the dialog closes, focus returns to where it was. If that
        element has gone, as a form's field has after Create, focus goes to
        the workspace root (§7).
    *   The picker is mouse-only, and Tab is not bound (decision 7).
5.  **One label rule everywhere.** The dialog, the Project Settings tab and
    the socket store the same label for the same input. §5 states the rule.
6.  **Labels are visible to agents at once.** A label saved from the dialog or
    from the Project Settings tab appears in `sirioctl project list --json`
    (`displayName`) without a restart. It is still there after a relaunch.
7.  **One dialog at a time.** When several adds happen in quick succession,
    their dialogs appear one after another, in the order of the adds. Two
    dialogs are never open at once.
8.  **Agents name a project in one call.**
    `sirioctl project add <path> --display-name <label>` adds the project with
    that label, with no dialog and the default icon.
    *   The response states the label that was applied.
    *   If the project was not added (already tracked, inside a tracked
        project, or containing one), the label is ignored and the response
        says so.
    *   A missing value, or a value that is empty after removing control
        characters and trimming, is an error.
    *   A `sirioctl` talking to an app that does not know the parameter exits
        with an error instead of reporting success.
9.  **Wording shown to the user.**
    *   The dialog:
        *   title "Project added";
        *   the project's root path as selectable text;
        *   the field with the caption "Name in the sidebar", and a line
            saying that the folder on disk keeps its name;
        *   the buttons Skip and Save, never "Cancel";
        *   a warning under the field that does not block Save, shown when
            the shown name matches another project's, with that project's
            path selectable.
    *   After a truncated clone, the Clone form closes and the truncation
        notice appears in the dialog's body. A *truncated clone* is a
        successful clone whose progress output `sirio_git` capped. The
        *truncation notice* is the text that `clone_status_line` shows for it
        today.
    *   The Create form's field caption reads "Folder name".
    *   The picker's Avatar tab says that GitHub and favicon avatars show as a
        globe until Sirio can download images.
10. **Failures are visible.**
    *   A save error shows a toast, even with the sidebar hidden. The next
        queued dialog opens only after that toast expires or is dismissed, so
        the toast is never covered.
    *   Each dialog outcome (saved, unchanged, skipped or error) is recorded
        without content, as the last outcome and on one stderr line. The
        record contains no label, path or project id (Non-Functional
        Requirements, Privacy).
11. **Proven end to end.**
    *   On Linux, a script drives the real binary through the flow and ends
        with a re-runnable artifact and a success line.
    *   The socket path is covered in `Scripts/Tests/test-sirioctl-e2e.sh`,
        including a check that `project add` opens no dialog.
    *   macOS interaction with the dialog stays a listed manual check, because
        the scripts run there with `--state-only`.

## Non-Goals

*   Renaming the folder on disk.
*   Undoing the add from the dialog. There is no "Cancel".
*   Opening the dialog for adds from the socket, or choosing the icon from the
    socket.
*   Downloading GitHub avatars or favicons. The globe stays, as in the Project
    Settings tab.
*   Replacing the non-git prompt (F-PRJ-03) with an Ely component. Ely's
    `ConfirmDialog` has two buttons, and the prompt needs three.
*   Using the label as a selector in socket methods (`workspace.create`,
    `workspace.list`), or printing it in `sirioctl project list` without
    `--json` (decision 6).
*   New columns or a schema migration. `display_name`, `icon_kind`,
    `icon_value` and `color_hex` exist since schema version 1, so an older
    Sirio reads everything this feature writes.
*   Moving the Project Settings tab's label field to Ely, binding Tab across
    the app, or making the picker reachable by keyboard (decision 7).
*   Porting the icon picker off `bezel_theme` and `controls::card`.
*   Fixing the orphan clone left when the Clone form closes during a clone
    (issue [#681](https://github.com/ai-sirio/sirio/issues/681)).
*   Changing what a UI add of a duplicate does. The toast stays (decision 4).
*   Fixing the pre-existing bug that the palette opens over the pane-close
    banner. A separate pull request fixed it
    ([#693](https://github.com/ai-sirio/sirio/pull/693), merged on
    2026-10-31), and this delivery depends on it (Detailed Implementation
    Plan).

## Open Questions

*   **The picker's width in the dialog.** The dialog is 440 px wide
    (`theme.dialog_width()`). The picker's Avatar sources are each a 220 px
    field next to a button, in a row that does not wrap. The plan lets those
    rows wrap in the dialog (§3). The E2E captures one picker mode at a time.
    *   **Resolved by:** Enzo, who judges the captures in the E2E artifact.
*   **Unverified on macOS and Windows.** It is not verified where focus goes
    after the PNG file chooser closes, whether IME input works in the Ely
    dialog, or where focus returns after the non-git prompt closes.
    *   **Resolved by:** Enzo's hand check on macOS before release. The
        scripts run there with `--state-only`, so they do not cover it.
*   **Key release across windows (macOS and Windows).** It is not verified
    whether a key released after a native window closes (the folder chooser,
    the native non-git prompt) reaches Sirio's window. The armed confirm (§6)
    makes the answer harmless for Enter. On Linux the non-git prompt is drawn
    inside Sirio's window, so the question does not arise there.
    *   **Resolved by:** the same macOS hand check.
*   **The deferred card's place in the dispatch tree at run time.** The source
    of `bezel-gpui` 0.3.8 (`window.rs`, `defer_draw` and the replay in
    prepaint) shows that a deferred draw keeps the active dispatch node of the
    moment it was deferred as its parent, so the card stays under the dialog
    entity's node. This is unverified at run time.
    *   **Resolved by:** <!-- TODO(F1): D1 does not say how or by whom this is
        checked. -->

## Critical User Journeys (CUJs)

### CUJ 1: Name a second `app` while adding it

*   **Given**: the catalog holds `~/work/client/app`, shown as `app`.
*   **When**: the user chooses Open folder, picks `~/work/server/app`, sees
    the same-name warning, types `server` and presses Enter.
*   **Then**:
    *   the sidebar shows `server` next to `app`;
    *   `sirioctl project list --json` reports `displayName: "server"` for the
        new project;
    *   after a relaunch, both still hold.

### CUJ 2: Clone while typing in a terminal

*   **Given**: the Clone form is open and a clone of a slow repository is
    running.
*   **When**: the user moves focus to a terminal by keyboard and runs `ls`
    before the clone finishes.
*   **Then**:
    *   the dialog opens with focus on the dialog itself;
    *   the name field still holds the root name, not `ls`;
    *   nothing is saved until the user clicks the field and presses Save or
        Enter;
    *   after Skip, focus is back in the terminal.

### CUJ 3: Esc, or Save with no edits, keeps the default

*   **Given**: the dialog is open after Create.
*   **When**: the user presses Esc, or Save without editing anything.
*   **Then**:
    *   the project stays added under its root name, with the default icon;
    *   its stored record is identical in both cases, and identical to a
        socket add without a label.

### CUJ 4: An agent adds and names a project

*   **Given**: Sirio is running, and an agent's pane has `sirioctl` on its
    PATH.
*   **When**: the agent runs
    `sirioctl project add ~/work/server/app --display-name server`.
*   **Then**:
    *   no dialog opens;
    *   the response repeats `server` as applied;
    *   `project list --json` reports it at once;
    *   running the same command again reports the project as not added
        (already tracked) and the label as ignored.

### CUJ 5: Adding with the sidebar hidden

*   **Given**: the sidebar is hidden and the catalog is empty.
*   **When**: the user clicks "Add Project" in the centre's empty state and
    picks a git repository.
*   **Then**:
    *   the dialog is visible and usable;
    *   the sidebar stays hidden.

### CUJ 6: A duplicate add

*   **Given**: `~/work/server/app` is tracked with the label `server`.
*   **When**: the user adds `~/work/server/app` again from Open folder.
*   **Then**:
    *   today's toast appears;
    *   no dialog opens;
    *   the label `server` is unchanged.

## Technical Plan (Architecture)

### Shape

```text
 Sidebar (sirio_ui)
 ------------------
 Open folder ------+
 Add without Git --+
 Initialize Git ---+-- SidebarEvent::AddProject { path, origin } -->
 Clone ------------+
 Create -----------+

 Workspace (sirio/src/main.rs)
 -----------------------------
 add_project(path, origin)                  <- UI only
   +- insert_project(path, None) -----------> ProjectCatalog::insert
   |    one write, one sync                   Ok(Some(id)) | Ok(None) | Err
   +- Ok(Some(id)): push on identity_queue
   +- Ok(None) / Err: today's toast, no dialog

 sirioctl project add <p> --display-name <l>
   project.add { path, displayName } -->
 control_add_project(path, label)           <- socket only
   +- insert_project(path, Some(label))

 render, every frame
   +- drain conditions hold: cx.defer_in(open_next_identity_dialog)
   +- dialog open, focus lost: dialog.reclaim_focus

 ProjectIdentityDialog (sirio_ui entity), created by open_next_identity_dialog
   name field (Ely TextInput), ProjectIconPicker (bare), Skip, Save
   emits Closed(Saved | Unchanged | Skipped) --> on_identity_dialog_closed
     +- Saved: save_project_identity
     |           -> update_project_settings
     |           (on error: toast, identity_error_toast)
     +- identity_last_outcome and stderr line

 render_global_overlays, called by both shell branches, draws the open dialog
```

The workspace keeps the wiring: a queue, one optional dialog entity with its
subscription, an entry in `render_global_overlays`, the drain and the focus
rule in `render`, and the opening in the UI add path. `main.rs` is about
50,000 lines, so it gets wiring and no dialog logic. The dialog, the label rule
and the picker changes live in `sirio_ui`.

### 1. Knowing which project was added (B1, M12)

*   **`ProjectCatalog::insert(path) -> Result<Option<String>, String>`.**
    *   Same body as `add`. It returns the new project's id, or `None` in the
        three "nothing added" cases.
    *   `add` becomes `#[cfg(test)]`, a thin wrapper over `insert`. Its 15 test
        call sites (14 in `session.rs` and one in `main.rs`) stay unchanged.
        `#[cfg(test)]` keeps it out of release builds, so there is no
        `dead_code` warning.
*   **One workspace insert, two wrappers.**
    `insert_project(path, display_name: Option<&str>)` does the following in
    order:
    1.  `insert`;
    2.  if a label was given, it applies the label to the catalog (§4);
    3.  exactly one `schedule_catalog`, one `sync_control_state` and one
        `refresh_sidebar`.

    `add_project` (UI) and `control_add_project` (socket) both call it. Only
    `add_project` queues a dialog.
*   **A guard comment.** A comment on `SidebarEvent::AddProject` says that
    every UI path that adds a project must emit this event, and that the
    socket path must never reach `add_project`.
*   **The seed.** The dialog's starting data is
    `Sidebar::project_settings_seed(id)`, read after `insert_project`
    returns. `refresh_sidebar` is synchronous, so the seed is already current.
    The seed carries:
    *   the root name, which is not the chosen folder's name when the user
        picked a linked worktree;
    *   the root path;
    *   the current icon.

### 2. Where an add came from, and where focus goes

*   **The event carries an origin.**
    `SidebarEvent::AddProject { path, origin: AddOrigin }`, with
    `AddOrigin = OpenedFolder | AddedWithoutGit | InitializedGit | Cloned { truncated } | Created`.
    There is one variant for each add path, and each of the five emit sites
    sets its own.
*   **Which origins are synchronous.** Decision 1 named Open folder as
    synchronous and Clone and Initialize Git as asynchronous. The other two
    are classified here:
    *   Add without Git is synchronous. The sidebar emits the add as soon as
        the non-git prompt's button is clicked.
    *   Create is treated as synchronous (Enzo, 2026-10-29).
        `CreateForm::submit` runs `create_project` on `background_spawn`, but
        that work is two directory creations, and the user has just pressed
        Enter in the form's own field.
*   **Focus mode, by origin.** The dialog has two modes:
    *   *Field mode*: the field takes focus and the root name is selected, so
        typing replaces it. It is used after `OpenedFolder`, `AddedWithoutGit`
        and `Created`.
    *   *Dialog mode*: the dialog's own handle takes focus, so keys that were
        in flight never reach the field. It is used after `InitializedGit` and
        `Cloned`.
*   **A truncated clone (decision 2).**
    *   `Cloned { truncated: true }` puts the truncation notice into the
        dialog's body. One function builds the text, and `clone_status_line`
        and the dialog both call it.
    *   After a successful Clone or Create, the sidebar closes the form
        itself, a truncated clone included. The `Cloned` subscriber always
        closes the form, and `clone_form_stays_open_after` goes away.
    *   The workspace never closes a form. An open form can hold a clone that
        is still running, and closing it drops the clone (issue
        [#681](https://github.com/ai-sirio/sirio/issues/681)).

### 3. The dialog

*   **What it owns.** `ProjectIdentityDialog` is an entity in `sirio_ui`, in
    `rust/crates/sirio_ui/src/project_identity_dialog.rs`, next to
    `project_identity.rs`. It owns:
    *   the project id, the root name and the root path;
    *   an Ely `TextInput::detached`, single-line, prefilled with the root
        name;
    *   a `ProjectIconPicker`, built with `with_value_and_repo(seed icon, root path)`;
    *   the initial icon and the focus mode;
    *   the truncation notice, if any;
    *   the other projects' shown names and paths, used for the same-name
        warning;
    *   a `body_focus` handle, tracked on a div that wraps the whole dialog
        body, field included, inside the Ely content.
*   **How it renders.**
    *   It calls `crate::ely::sync_theme_if_changed`.
    *   It builds
        `Dialog::new(("project-identity", seq), "Project added", skip)`
        with `.held()`, so a press on the scrim does nothing. `seq` is a
        per-workspace counter (§8).
    *   In field mode it adds `.focus_first(field)`, and the name is selected
        on the entity's first render, as `PromptDialog` does. In dialog mode
        it adds `.focus_first(body_focus)`.
    *   In both modes it adds `.on_enter(field, save)`, the extended API of
        §6.
    *   The body, in order:
        1.  the root path through `selectable_text(..)`;
        2.  the caption "Name in the sidebar", the field, and the line saying
            that the folder on disk keeps its name;
        3.  the same-name warning, recomputed as the user types, which does
            not block Save. It compares shown names exactly. It shows the path
            of the first other project with the same shown name, in catalog
            order;
        4.  the truncation notice;
        5.  the picker.
    *   The actions are Skip and Save.
*   **Its focus methods.** The workspace's focus rule (§7) uses three of them:
    *   `contains_focus(window, cx)` is `body_focus.contains_focused(window, cx)`;
    *   `reclaim_focus(window, cx)` focuses the field in field mode and
        `body_focus` in dialog mode;
    *   `rendered_once()` is `true` once the entity has rendered. The rule
        waits for it (§7).

    `body_focus` sits inside the Ely layer, so the layer's Esc listener stays
    on the dispatch path.
*   **What Save does.**
    1.  It normalizes the field's text (§5).
    2.  It compares the picker's value with the initial icon *by value*.
        Choosing another icon and then the original one again counts as
        untouched.
    3.  If both the label and the icon are unchanged, it emits
        `Closed(Unchanged)` and nothing is written. Otherwise it emits
        `Closed(Saved { display_name, icon })`.

    Skip and Esc emit `Closed(Skipped)`. Ely runs `on_close` on every way out:
    Esc, the scrim and each action. The entity's own `emitted` guard makes the
    event fire at most once.
*   **The picker in bare mode.** A new builder on `ProjectIconPicker`:
    *   It drops the picker's own `controls::card`, so the dialog's card is the
        only surface. The card is opaque (`colors.overlay`). The picker's own
        surfaces (`ely.surface`) change tint with translucency but show
        nothing through them. The manual check covers tint and contrast.
    *   It lets the source rows wrap, so they fit the 440 px dialog.

    The Project Settings tab keeps the carded picker.
*   **The Avatar caption (M14).** The Avatar mode of the picker gets the
    caption "GitHub and favicon avatars show as a globe until Sirio can
    download images", in the Project Settings tab and in the dialog. The two
    code comments that blame a missing HTTP client are corrected: what is
    missing is the avatar download.
*   **Create's field caption.** Create's field caption becomes "Folder name".

### 4. Writing the identity (decides RFC alternative 12: full `ProjectSettingsUpdate` chosen over the identity-only write)

*   **Split the workspace's `update_project_settings`.**
    *   `apply_project_settings(&ProjectSettingsUpdate) -> Result<(), String>`
        builds the `CatalogProjectSettings` exactly as today and stores it in
        the catalog.
    *   `update_project_settings` becomes `apply_project_settings`, then the
        write, then `sync_control_state` (new, fixes M2; it already calls
        `publish_worktree_lookup`), then `refresh_sidebar`. If the first or
        second step fails, it sets the sidebar notice as today, skips the
        rest and returns the error.
*   **Dialog Save.** `save_project_identity(id, display_name, icon) -> Result<(), String>`
    reads `project_settings(id)` at the moment of the save. It builds a
    `ProjectSettingsUpdate` with:
    *   the dialog's label and icon;
    *   the current `default_worktree_base` and `worktree_location_override`;
    *   `is_git` filled from the seed. The write ignores it.

    It then calls `update_project_settings`. On an error it shows a toast,
    because the sidebar may be hidden, and it remembers the toast's id in
    `identity_error_toast` (§8). The toast reads "Couldn't save the name of
    <shown name> to disk: <error>. Sirio keeps it and retries with its next
    save." The in-memory catalog is not rolled back after a failed write. The
    Project Settings tab behaves the same way today.
*   **Socket label.** Inside `insert_project`, between `insert` and the single
    write, it builds the same update. The icon is
    `ProjectIcon::from_persisted_parts` of the current settings. It calls
    `apply_project_settings` only, so the add is written and synchronized
    once. A label that normalizes to no label writes no settings.
*   **What this choice writes.** Under this choice, a save that sets only the
    label also writes the default icon explicitly: `color_hex = "coral"`,
    `icon_kind = "icon"`, `icon_value = "folder"`.
    *   `project list --json` then shows `color: "coral"` for that project,
        and `color: ""` for a project that was skipped or added without a
        label.
    *   A socket add with `displayName` behaves the same.
    *   On screen the icons are identical.
    *   Save with no edits writes nothing, so it stays identical to Skip.
*   **Write errors reach the caller.** Today `schedule_catalog` writes
    synchronously, but it only prints a failure to stderr and returns `()`.
    *   A new `SessionStore::try_schedule_catalog(&catalog) -> Result<(), String>`
        does the write. It returns `Ok(())` when there is no database, which
        is the in-memory mode that tests run in. `schedule_catalog` becomes a
        wrapper that logs as it does today.
    *   `update_project_settings` uses the new function, so the dialog's
        toast covers both a project removed while the dialog was open and a
        failed database write (Goal 10).
    *   The Project Settings tab now also shows a failed write in its sidebar
        notice.

### 5. One label rule (decision 9)

This section is the only normative statement of the rule.

*   **The function.** `normalize_display_name(input, base_name) -> Option<String>`,
    in `sirio_ui::project_identity`, where `base_name` is the root name:
    1.  trim;
    2.  remove `char::is_control` characters;
    3.  keep the first 120 characters;
    4.  trim again;
    5.  return `None` if the result is empty or equals `base_name`. The
        comparison is exact.
*   **The empty check.** A value that is empty after removing control
    characters and trimming is an error where an input is required (the
    socket, §9). So `"\u{7}"` is an error. A value equal to the root name is
    not an error. It normalizes to no label.
*   **Who uses it.** The Project Settings tab's `to_update` (in place of its
    own trim), the dialog and the socket handler.
*   **What it does not replace.** The read-side checks (`title_name`,
    `set_project_identity`, `publish_worktree_lookup`) stay, because they
    handle values that older versions stored.
*   **One visible effect.** Typing the root name itself in the Project
    Settings tab now stores no label. On screen it looks the same.

### 6. Ely `Dialog` (decision 7, extended by Enzo on 2026-10-30)

*   **Why decision 7 needs an extension.** In
    `rust/vendor/ely-gpui-component/src/overlays/dialog.rs`, `on_enter`
    confirms when Enter is *released* in the field, and it discards the press.
    It also requires `focus_first`, which always moves focus into the field on
    the first frame. That causes two problems:
    *   **A release can arrive without its press.** The user presses Enter in
        the Create form. The press submits the form, the folder is created in
        milliseconds, and the dialog opens with focus in the field. The
        release of that same Enter then lands in the dialog, saves the
        defaults, and the dialog closes. The same can happen after Add
        without Git chosen with the keyboard in the native non-git prompt on
        macOS and Windows. On Linux the prompt is gpui's fallback, drawn
        inside the window, and its buttons take clicks only, so the case does
        not arise there. This is inferred from the code, not reproduced in the
        running app.
    *   **Asynchronous adds cannot have Enter.** Decision 1 wants Clone and
        Initialize Git to keep focus off the field, with Enter saving once
        the user clicks into it. `on_enter` cannot do that, because it brings
        `focus_first` with it.
*   **Public builders.** `held`, `focus_first` and `on_enter` become `pub`.
*   **`on_enter` names its own field.** It becomes
    `on_enter(field: FocusHandle, build)` and no longer requires
    `focus_first`. `focus_first` keeps a single meaning: where focus goes
    when the dialog opens. `PromptDialog` passes its own field to both.
*   **An armed confirm.**
    *   The existing `capture_action` for Enter on the dialog's layer sees the
        press. It now also records that Enter went down while the field had
        focus. The record lives in the dialog's keyed state.
    *   The release confirms only when that record is set, and every release
        clears it.
    *   So an Enter pressed before the dialog existed can no longer confirm
        it.
*   **What the armed confirm does not stop.** It stops a release with no
    press. It does not stop a press that repeats. A held Enter that passes
    the repeat delay (about 660 ms) repeats the press on every platform,
    because gpui's X11 client drops the synthetic release. The dialog then
    closes as `Unchanged` and writes nothing. The label can still be set later
    in the Project Settings tab. Accepted (Risks & Mitigations, item 1).
*   **Enter in dialog mode.** Enter does nothing until the field has focus,
    because Ely binds `enter` only in the `ElyInput` context.
*   **The vendor note.** A new entry in
    `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` goes next to the
    existing `overlays/dialog.rs` one.

### 7. Keys and the shell while the dialog is open (M4, M5)

*   **Shell actions are off while the dialog is open.** gpui hands a key that
    matches a binding to its action before the key listeners run. A check in
    `handle_root_key_down` alone would miss action-bound shortcuts, so the
    shell's actions are gated too.
    *   The main shell branch registers its 38 `on_action` listeners only
        while `identity_dialog.is_none()`. One helper,
        `shell_actions(&self, frame, cx)`, takes no flag and covers all 38,
        including the ToggleSidebar, ToggleRightPanel and ToggleSecondaryPane
        closures.
    *   The Settings screen branch has one listener,
        `handle_close_settings_surface`. It gets the same condition.
    *   `text_selection::on_copy` stays registered in both branches, so Ctrl+C
        is never swallowed.
    *   With the listeners absent, a shortcut's action finds no listener and
        the key falls through to the key listeners.
    *   Esc is bound globally to `CloseSettingsSurface`. While the Settings
        screen is open, that handler closes it and stops the key. With the
        listener absent, Esc reaches the dialog even over the Settings screen.
*   **The palette is gated at its function.** The palette fix pull request
    makes `open_command_palette` return early while `root_modal_open()` holds.
    This delivery adds the identity dialog to that condition. That covers
    every caller:
    *   the `intercept_keystrokes` interceptor for Ctrl+Shift+P, which still
        stops the key, so the chord is eaten;
    *   the palette branches of `handle_root_key_down`;
    *   the new-tab palette action.

    The queue waits for the palette to close. It never closes it.
*   **A new first branch in `handle_root_key_down`.** While `identity_dialog`
    is `Some`, this branch always returns.
    *   If the key is Esc and focus is on `root_focus`, it first skips the
        dialog and stops the key. `root_focus` is where text selection parks
        focus (`text_selection::focus_sink`) after someone selects the path,
        and from there the dialog's own Esc handler is out of the key's path.
    *   Any other key it returns without stopping, so the key reaches the
        dialog.

    This branch comes before the branch that closes a Clone or Create form on
    Esc. So the first Esc never closes a form hidden behind the scrim, and
    the close-tab and palette branches never run. The case that matters is a
    different form, still cloning, that an Open folder left open behind the
    dialog. It is why the order of the branches matters, and why A20 lost.
*   **The focus rule.** Each frame, while the dialog is open and no prompt is
    up, `render` checks where focus is. If focus is neither inside the dialog
    (`contains_focus`) nor on `root_focus`, `render` calls `reclaim_focus`. It
    starts on the frame after the dialog's first render, so Ely records the
    focus the dialog must return to first. The rule is a method on the dialog
    entity, and it yields only to a prompt, such as the non-git prompt. It is
    the one place that brings focus back, whichever way it left, including a
    control-socket action that moves it.
    *   **What "a prompt is up" means.** It is
        `window.has_active_prompt() || self.sidebar.read(cx).prompt_open()`.
        `has_active_prompt()` is true only for prompts that gpui draws inside
        the window (bezel-gpui 0.3.8, `window.rs:6215–6219`). On macOS and
        Windows the non-git prompt is native, so gpui does not see it. The
        sidebar therefore tracks its own prompt: `Sidebar::prompt_open()` is
        true from the moment `confirm_add_project` shows the prompt until the
        answer arrives, whatever the answer is. The drain (§8) waits on the
        same condition.
*   **Other root modals.**
    *   A root modal that is open when a dialog is due makes the dialog wait in
        the queue (§8).
    *   A root modal that opens while the dialog is open waits behind it. An
        example is End All from the tray menu, which raises the End All
        confirmation. No socket verb raises a root modal. The `needs_focus`
        blocks in `render` (pane close, End All, title prompt) take focus only
        while `identity_dialog.is_none()`. Until then the modal is drawn under
        the scrim, visible but dimmed, without focus or keys.
    *   When the dialog closes, the modal claims focus, and its branch in
        `handle_root_key_down` works as it does today.
*   **Popups and in-tab dialogs.**
    *   A popup or menu that is open when an asynchronous add lands is drawn
        above the card. This is accepted, because there is no popup registry
        (Risks & Mitigations, item 2).
    *   An in-tab Ely dialog (discard, merge, hand-off, bypass) keeps its own
        focus. The identity dialog takes focus and `give_back` returns it.
        Which of the two draws on top, when both are raised at priority 0, is
        unverified.
*   **The picker's emoji search.** It stops Esc after closing the grid
    (M5a).
*   **After an asynchronous add.**
    *   Focus sits on `body_focus`, so keys that were in flight never reach the
        field.
    *   Enter needs the field focused and armed.
*   **Tab and the picker.** Tab is not bound, and the picker stays
    mouse-only (decision 7).
*   **Focus after close (Enzo, 2026-10-31).** The workspace adds no focus code
    of its own. Ely's `give_back` restores the focus held before the dialog
    opened.
    *   For `Created` and `Cloned`, that handle belonged to a form field that
        is no longer drawn. The workspace's existing rule F-SID-19 then moves
        focus to `root_focus`.
    *   So after a Create made from the keyboard, focus is on the workspace
        root, not on the new project's row.
    *   For the other origins, focus returns to where it was, for example the
        terminal.

### 8. The queue (M11)

*   **The state.** Five fields of the workspace:
    *   `identity_queue`, the requests in the order of the adds, each with a
        project id and an origin;
    *   `identity_dialog`, the open dialog entity and its subscription;
    *   `identity_open_scheduled: bool`;
    *   `identity_error_toast: Option<u64>`;
    *   `identity_seq: u64`, the counter behind each dialog's element id.
*   **The drain, in `render`.** The queue is not pumped from close sites.
    Each frame, `render` schedules `open_next_identity_dialog` through
    `cx.defer_in(window, ..)` when all of these hold:
    *   no identity dialog is open;
    *   no root modal is open, by one predicate, `root_modal_open()`, that
        covers the pane-close banner, End All and the title prompt;
    *   the palette is closed;
    *   no prompt is up: `!window.has_active_prompt()` and
        `!self.sidebar.read(cx).prompt_open()` (§7, the focus rule, says why
        both);
    *   the queue is not empty;
    *   `identity_open_scheduled` is `false`;
    *   no save-error toast is showing: `self.toast` is not `Some` with the id
        in `identity_error_toast`.

    `render` sets `identity_open_scheduled` when it defers the open, and the
    deferred call clears it. So two renders before the deferred open runs
    cannot pop two entries. The dialog opens on the next frame, which keeps the
    synchronous `git config` (Non-Functional Requirements, Performance) out of
    `render`. `schedule_restored_scrollback` already defers work from `render`
    this way, through `cx.defer_in`.
*   **`open_next_identity_dialog(&mut self, window, cx)`.**
    *   It clears `identity_open_scheduled`.
    *   It drops requests whose project no longer exists. If none is left, it
        does nothing.
    *   It builds the dialog from the seed and subscribes to its `Closed`
        event with a plain `cx.subscribe`.
    *   It records the trace event (§11).
*   **No close-site calls.** Closing a root modal, cancelling the non-git
    prompt or closing the dialog calls nothing to open the next dialog. The next
    frame's drain finds the queue and the conditions, so a close site never has
    to know the queue exists. The handler of `Closed` takes the dialog, acts
    on the outcome and records it.
*   **After a save error.** `save_project_identity` shows the toast and
    remembers its id in `identity_error_toast`. The drain then does not open
    the next dialog until that toast expires (4 s) or the user dismisses it.
    Ordinary toasts delay nothing.
*   **Element ids.** Each dialog's element id is `("project-identity", seq)`,
    where `seq` is a per-workspace counter. Ely's focus takeover lives in keyed
    state, so a second dialog never inherits the first one's "focus already
    taken and returned". The id holds no project id, because Ely logs
    `dialog {id:?}: closed` (`rust/vendor/ely-gpui-component/src/overlays/dialog.rs`),
    and a project id would reach the log.

### 9. The socket parameter (M9, decisions 5 and 6)

*   **The request.** `request::project_add(path, display_name: Option<&str>)`
    adds `displayName` only when it is given. Its callers are `sirioctl` and
    the `cfg(windows)` test in `main.rs`.
*   **The handler.** With `displayName` present, it first checks the value.
    *   A value that is empty after removing control characters and trimming
        fails with an error before anything is added.
    *   Otherwise it normalizes the value (§5). A value equal to the root name
        is not an error. It stores no label.
*   **The response.**
    *   It keeps `added`, `projectId` and `worktreeCount`.
    *   When the request carried a label, it adds `displayName` (the stored
        value, empty if none) and `displayNameApplied`.
    *   `displayNameApplied` is `false` when `added` is `false`: the project
        was not added (already tracked, inside a tracked project, or
        containing one), the label is ignored and the record is untouched.
    *   Otherwise it is `true`, including when the label normalizes to no
        label. That case writes no settings.
*   **`sirioctl --display-name <label>`.**
    *   A flag with no value, or an empty value, is an error before the
        socket is contacted.
    *   If the response lacks `displayNameApplied`, the running app predates
        the parameter. `sirioctl` exits 1 and says that the project was added
        without a label.
    *   `displayNameApplied = false` prints a note that the project was not
        added and the label was ignored, and exits 0, as an add of an
        already-tracked path does today.
*   **Unchanged by this design.** No dialog opens and the icon stays the
    default. The documentation says that the label is not a selector.

### 10. The proof (B3)

*   **Debug-only verbs.** They go in `DEBUG_METHODS`, never in the release
    list:
    *   `surface.project_add { kind: path|clone|create, value, submit, answer }`
        starts an add without the native folder chooser:
        *   `path` runs the Open folder flow on the folder `value`. A git
            folder is added.
        *   A non-git folder gets the non-git prompt, unless `answer` is
            given. `answer` is `0` (Initialize Git), `1` (Add without Git) or
            `2` (Cancel), the order of the prompt's buttons. It exists only in
            debug builds. When it is given, `confirm_add_project` skips
            `window.prompt` and uses the value as the user's choice. On Linux
            the prompt is gpui's fallback, drawn inside the window, and its
            buttons take clicks only, so a script could not find or answer
            it.
        *   `clone` and `create` open the Clone or Create form with its field
            filled from `value` (a URL or a folder name). Without `submit`,
            the script types and presses Enter itself. That is how the
            press-and-release step of §6 is driven.
        *   Clone accepts a local path. `sirio_git::clone` adds `--no-local`
            when the URL is an existing path, so the E2E script can clone
            without the network.
    *   `surface.project_identity.read` reports:
        *   whether the dialog is open;
        *   `projectId` and `origin`;
        *   `focus`, one of `field`, `dialog`, `terminal`, `root`, `none` or
            `other`. The dialog entity fills `field` or `dialog`, and the
            workspace fills the rest;
        *   the name draft;
        *   the picker's value;
        *   the queue length;
        *   the last outcome;
        *   keys the workspace fills in: `projectForm` (`none`, `clone` or
            `create`), `sidebarVisible`, `paletteOpen`, `pendingPaneClose`,
            `toast` (the current text, or empty) and
            `projectSettingsLabelBounds`.
    *   `surface.project_identity.act { action, value }` runs the button's
        handler for `save` and `skip`. It also drives the picker, with `mode`,
        `open-emoji-grid` and `pick-glyph`.

    Each verb gets an arm in `request_for_control_method`, without which the
    `cfg(windows)` test panics. Each also gets a "(debug builds)" usage line
    in `sirioctl` and a mention in
    `docs/agents/architecture/control-socket.md`.
*   **A debug-only switch.** The environment variable
    `SIRIO_DEBUG_FAIL_CATALOG_WRITES=1` makes
    `SessionStore::try_schedule_catalog` return `Err`. It is compiled only
    under `cfg(debug_assertions)`. The E2E uses it to force the save-error
    path. A read-only `SIRIO_DB` would not do, because the database connection
    stays open.
*   **A new script, `Scripts/Tests/test-project-identity-e2e.sh`.** It takes
    the same flags as its siblings: `--xvfb` / `--display`, `--state-only`,
    `--out-dir` and `--appearance`. It runs isolated, with its own database,
    a private `HOME` and stub agent CLIs. It types with xdotool.
    *   It checks results through `sirioctl project list --json`, a dump of
        the `project` table and PID-matched captures.
    *   It ends with `PROJECT IDENTITY E2E OK (...)`.
    *   The Detailed Implementation Plan, file 18, lists its scenarios.
*   **`Scripts/Tests/test-sirioctl-e2e.sh`** gets the socket cases, including
    a check that the dialog is closed after `project add`.
*   **What the proof does not cover.** The real non-git prompt, and the way it
    restores focus, are never exercised on Linux, because the scripts answer
    it through `answer`. The macOS hand check covers the native sheet
    (Goal 11).

### 11. Trace and logs (M15)

*   **The trace.** `sirio_perf` is opt-in performance instrumentation and may
    lose events. It carries only `"project_add.dialog_opened"`. The entity
    column never holds the project id, which is a hash of the path.
*   **The outcomes.** `save_project_identity` returns `Result<(), String>`.
    Each dialog outcome is `saved`, `unchanged`, `skipped` or `error`. It is
    recorded in `identity_last_outcome` and on one content-free stderr line,
    `project identity: <outcome>`. The RFC (M15) asked for a toast, a stderr
    line and a last outcome. The read verb reports the last outcome.

## Alternatives Considered

A bracket after a title names where the RFC treats the alternative. A number is
its entry under "Alternatives Weighed". M# or B# is one of its review
findings. Alternatives the RFC accepted (10, 13 and 14) are part of the plan
and are not repeated here.

### Product shape (rejected by Enzo in the RFC brainstorm)

<!-- TODO(F31): the heading and A1 and A2 need the date of the RFC
brainstorm. Line 26 of the earlier draft gives 2026-10-28 only for the RFC's
open questions. -->

#### A1. A dialog only when the new name collides [RFC 1]

*   **Pros:** No extra step for the common case of a unique name.
*   **Cons:** The user cannot pick a name or an icon for a project whose
    name happens to be unique.
*   **Reason for rejection:** Rejected because the user could not pick a name
    or an icon for a project whose name is unique; decided by Enzo, in the RFC
    brainstorm. Enzo wants the dialog on every add from the UI.

#### A2. No dialog: automatic `name — parent-folder` titles [RFC 2]

*   **Pros:** No interaction at all, and every collision is resolved.
*   **Cons:** The user does not choose. Titles get long, and the parent
    folder is often no better a name.
*   **Reason for rejection:** Rejected because the user does not choose a name
    or an icon; decided by Enzo, in the RFC brainstorm.

#### A3. The icon choice inside the Create form [RFC 3]

*   **Pros:** One surface for Create instead of two in a row.
*   **Cons:** Open folder and Clone would still need something else.
*   **Reason for rejection:** The dialog also appears after Create, so it
    serves every add path, and Enzo accepts naming twice after Create.

#### A4. Ask before adding, with an add that Esc cancels [RFC 4]

*   **Pros:** Esc could undo the whole add.
*   **Cons:**
    *   A pending add needs its own state.
    *   A cancelled Clone leaves a clone on disk with no project.
*   **Reason for rejection:** The order is add first, then ask, and Esc keeps
    the defaults. An orphan clone already exists today when the Clone form is
    closed during a clone (issue #681), but that does not reopen this choice.

#### A5. The dialog for adds from the socket too [RFC 5]

*   **Pros:** Every add would behave the same way.
*   **Cons:**
    *   A modal would appear in front of a user who did not ask for an add.
    *   About 15 E2E scripts add projects with `sirioctl project add`, and a
        modal would take their focus.
*   **Reason for rejection:** Agents pass the label as a parameter, and no
    modal appears for them.

#### A6. Delivery in two parts [RFC 6]

*   **Pros:** The socket parameter, the M2 fix and the documentation would
    have reached `main` before the bezel-off stack.
*   **Cons:** Two reviews, and two release-note entries for one feature.
*   **Reason for rejection:** Enzo wanted a single delivery after the stack.
    The stack has since merged, so the question no longer arises.

### Raised by the review

#### A7. Open the new project's Project Settings tab instead of a dialog [RFC 7]

*   **Pros:** About 10 lines, and the whole view is reused.
*   **Cons:**
    *   It is not modal.
    *   It saves on every keystroke, so Esc cannot keep the defaults.
    *   It opens the secondary panel and leaves one more tab after every add.
    *   It also shows the project's other settings, such as the worktree
        defaults.
*   **Reason for rejection:** It cannot meet Goal 3. The review that proposed
    it did not recommend it either.

#### A8. A non-git folder is added as it is, with "Initialize Git" inside the dialog [RFC 8]

*   **Pros:** One modal instead of two, and the project id stays stable.
*   **Cons:** It redoes the F-PRJ-03 flow.
*   **Reason for rejection:** Out of scope. The non-git prompt stays, followed
    by the dialog (decision 3).

#### A9. Other ways to protect the field after an asynchronous add [RFC 9]

These were the options:

1.  A toast "Cloned app — Name it…" that opens the dialog.
2.  Opening the dialog only if the form is still visible and focused.
3.  Ignoring keys for a short interval.

*   **Pros:** Each avoids the dialog taking focus while the user types
    elsewhere.
*   **Cons:**
    *   The toast and the conditional opening break "a dialog on every add".
    *   No interval length separates keys meant for the field from keys typed
        elsewhere.
*   **Reason for rejection:** Focus on the dialog instead of the field
    (decision 1) avoids the same problem without these costs.

#### A10. Sirio-side focus, or `PromptDialog` with a content slot [RFC 11]

*   **Pros:**
    *   Moving focus one frame after the first render needs no vendor
        change.
    *   A content slot on `PromptDialog` would reuse its focus and Enter
        handling.
*   **Cons:**
    *   It is not verified that moving focus a frame later works with Ely's
        focus takeover.
    *   A content slot is a wider vendor change than exposing `Dialog`'s own
        builders.
*   **Reason for rejection:** Decision 7 exposes `Dialog`'s builders, and §6
    of this plan extends them.

#### A11. Highlight the existing project, or open its settings, on a duplicate add [RFC 15]

*   **Pros:** It tells the user where the project they tried to add already
    is.
*   **Cons:** It is new behaviour for a path the feature does not otherwise
    touch.
*   **Reason for rejection:** Today's toast stays (decision 4).

### Decided while drafting

#### A12. After Create, focus on the dialog, or on the field only if the form still had focus

*   **Pros:** Neither risks a stalled `create_dir` turning keys typed
    elsewhere into the label.
*   **Cons:**
    *   With focus on the dialog, the Create path needs a click right after
        the user typed the folder name.
    *   The conditional variant needs the add's origin plus a focus probe,
        and it behaves differently depending on where focus was, which the
        user cannot see.
*   **Reason for rejection:** Enzo chose to treat Create as synchronous, with
    focus in the field (2026-10-29). §6 removes the hazard of that choice: an
    Enter release that arrives without its press.

RFC alternative 12 weighed three ways to write the identity. Option A is the
full `ProjectSettingsUpdate`, the write that the Project Settings tab already
uses, now also used by the dialog. Option B is a write function for the
identity only. Option C is patch semantics for every settings write. Enzo chose
option A on 2026-10-29 (§4). A13 and A14 are options B and C.

#### A13. A write function for the identity only [RFC 12, option B]

`ProjectCatalog::set_identity(id, display_name, icon: Option<..>)` would
replace the name, and the colour and icon only when given.

*   **Pros:**
    *   An untouched icon stays unset, so a label-only save matches a
        skipped add in `project list --json`.
    *   The worktree defaults cannot be touched by construction.
*   **Cons:** Two functions write the same columns: the tab's full update
    and this one. A future identity field must be added to both.
*   **Reason for rejection:** Enzo chose the full `ProjectSettingsUpdate`
    (option A, 2026-10-29). That keeps one workspace write function for the
    tab and the dialog, at the cost of the explicit default icon described in
    §4.

#### A14. Patch semantics for every settings write [RFC 12, option C]

`ProjectSettingsUpdate` would carry "unchanged" or "set" per field, and the
workspace would merge it into the current settings.

*   **Pros:**
    *   There is a single write function.
    *   It drops the assumption, flagged in RFC M1, that the sender has
        loaded every draft.
*   **Cons:**
    *   It changes the Project Settings tab, its `to_update` and its tests.
    *   `display_name` becomes `Option<Option<String>>`.
    *   A caller that forgets a field leaves it unchanged without any error.
*   **Reason for rejection:** Enzo chose option A; decided by Enzo, 2026-10-29.
    <!-- TODO(F29): the deciding con of A14 is not recorded. -->

#### A15. Enter through the field's `InputEvent::Submit`

On the press, as `CloneForm` and `CreateForm` already do. `on_enter` stays
crate-private.

*   **Pros:**
    *   A smaller vendor change: only `focus_first` and `held` become public.
    *   It works in both focus modes.
    *   The forms already prove the mechanism.
*   **Cons:**
    *   It changes the mechanism that decision 7 chose.
    *   Like §6, a held Enter past the repeat delay confirms the dialog. The
        exposure is the same, so it does not decide the choice.
*   **Reason for rejection:** Rejected because it changes decision 7's
    mechanism; decided by Enzo, 2026-10-30, confirmed 2026-10-31. Enzo kept
    `on_enter` and extended it with the armed confirm and its own field.

#### A16. Decision 7 to the letter, with Create made asynchronous

*   **Pros:** No extension of `on_enter`.
*   **Cons:**
    *   Clone, Initialize Git and Create would have no Enter, only the Save
        button.
    *   The release hazard would remain for Open folder and Add without Git.
    *   It reverses the Create decision.
*   **Reason for rejection:** Enzo chose the extended `on_enter`; decided by
    Enzo, 2026-10-30.
    <!-- TODO(F29): the deciding con of A16 is not recorded. -->

#### A17. A "modal is open" check in `handle_root_key_down` only [RFC M5c]

*   **Pros:** A single branch, next to the existing `pending_pane_close`
    one.
*   **Cons:** gpui delivers a key that matches a binding to the action first,
    so this check misses every action-bound shortcut. It also misses Esc
    over the Settings screen, which `CloseSettingsSurface` consumes. The
    palette has a third entry point, the `intercept_keystrokes` interceptor for
    Ctrl+Shift+P, which also bypasses the check.
*   **Reason for rejection:** It does not meet Goal 4. §7 keeps a branch in
    `handle_root_key_down` for what only that function sees: Esc while focus
    is parked on `root_focus`, and the project-form Esc. The shell's
    `on_action` listeners are left unregistered while the dialog is open, and
    `open_command_palette` returns early, which covers the interceptor.

#### A18. A guard inside each of the shell's action handlers

*   **Pros:** The registration code stays as it is.
*   **Cons:**
    *   38 handlers change.
    *   Every future shell action has to remember the guard.
*   **Reason for rejection:** Not registering the listeners covers every
    action that `shell-frame` registers, now and later, in one place.

#### A19. The picker embedded with its own card

*   **Pros:** No new builder on `ProjectIconPicker`.
*   **Cons:**
    *   It draws a card within a card.
    *   The inner card paints `ely.surface`, which changes tint with
        translucency, inside a modal whose card is opaque.
*   **Reason for rejection:** Enzo's rule, set with PR
    [#652](https://github.com/ai-sirio/sirio/pull/652), is that translucency
    never reaches a foreground surface.

#### A20. Closing any open Clone or Create form when a dialog opens [RFC M5 requirement]

*   **Pros:** The form behind the scrim can never take the first Esc.
*   **Cons:** A Clone form can hold a clone that is still running, and
    closing it drops the clone (issue #681).
*   **Reason for rejection:** §7 already stops the form's Esc branch from
    running while the dialog is open, and the workspace never closes a form
    (§2).

#### A21. Changing `ProjectCatalog::add` to return the outcome [RFC B1 example]

*   **Pros:** One function instead of two.
*   **Cons:** 15 test call sites (14 in `session.rs` and one in `main.rs`)
    change for no change in behaviour.
*   **Reason for rejection:** A new `insert` returns the id, and `add`
    becomes a one-line `#[cfg(test)]` wrapper around it.

## Risks & Mitigations

Each item names a risk and says how it is handled, or that it is accepted.

1.  **A held Enter closes the dialog as `Unchanged`.** Accepted by Enzo,
    2026-10-31 (§6). The label can still be set in the Project Settings tab.
2.  **A popup or menu that is open when an asynchronous add lands is drawn
    above the card.** Accepted, because there is no popup registry (§7).
3.  **A root modal that opens during the dialog waits, dimmed.** It is drawn
    under the scrim without focus or keys, and it claims focus when the dialog
    closes (§7).
4.  **The dialog opens one frame after the add, and a focus rule runs on every
    frame.** Accepted by Enzo, 2026-10-31 (§8, §7).
5.  **The real non-git prompt is not exercised on Linux.** The scripts answer
    it through the debug `answer` parameter. Enzo's macOS hand check covers
    the native sheet (§10).
6.  **`cfg(windows)` code is compiled only by the release job.** Two pieces of
    code are compiled only on Windows, and no pre-merge job compiles them,
    because `.github/workflows/windows-check.yml` builds only the host crates:
    *   `request_for_control_method`, and its test
        `every_control_response_hides_verbatim_paths`;
    *   `respond_to_control_action`, an exhaustive match over
        `ControlAction`.

    Every new verb and every new `ControlAction` variant needs an arm in both.
    The diff review checks them by hand, and the release job is the first
    compile.
7.  **Shell shortcuts act behind the other root modals.** This exists today
    and is out of this delivery, which gates only the identity dialog (Enzo,
    2026-10-31). The palette opening over the pane-close banner is a separate
    bug, fixed by its own pull request, which this delivery reuses.
8.  **A socket client can set any label.** Normalization (§5) is the
    mitigation. Any local client can already call `project.add`.
9.  **After a save error, the next queued dialog waits up to 4 s.** It waits
    for the error toast to expire or be dismissed (§8). Accepted by Enzo,
    2026-10-31.

## Non-Functional Requirements

*   **Privacy of labels and paths.**
    *   No label, path or project id goes into a trace name, a trace entity or
        the stderr outcome line (§11).
    *   The project id is a hash of the path, so it stays out of the trace's
        entity column.
    *   The dialog's element id holds a counter, not a project id, because
        Ely logs the id when a dialog closes (§8).
*   **Compatibility.** There is no migration and no new column (see
    Non-Goals), so an older Sirio reads everything this feature writes. The
    socket parameter behaves as follows across versions:
    *   **New `sirioctl`, old app.** The app ignores the unknown parameter
        and adds the project. `sirioctl` sees no `displayNameApplied` and
        exits 1, saying that the project was added without a label.
    *   **Old `sirioctl`, new app.** An old `sirioctl` accepts `--display-name`
        and ignores it, so the project is added without a label, with exit
        code 0.
    *   **The stable copy of `sirioctl`.** The app rewrites the stable copy on
        `PATH` in a background thread at startup
        (`cli_install::ensure_at_startup`). An old copy can be called until
        that thread finishes. In that window it behaves as in the "Old
        `sirioctl`, new app" case above.
*   **Performance.** The dialog adds one synchronous
    `git config --get remote.origin.url` when it is created, in the deferred
    open (§8), through `ProjectIconPicker::with_value_and_repo`. That happens
    on top of the synchronous discovery that `insert` already runs. Both run
    on the UI thread, as the Project Settings tab does today.

## Future Improvements

*   **The other root modals.** `pending_pane_close`, `pending_end_all` and
    `pending_title_prompt` could join the shell-action gate of §7. Today the
    shell's shortcuts still act behind them (Risks & Mitigations, item 7).
*   **The Project Settings tab's label field.** It could move to an Ely
    `TextInput`, with paste, a cursor and selection.
*   **Tab traversal and keyboard access to the picker** (decision 7).
*   **Downloading GitHub and favicon avatars,** so they stop showing as a
    globe.
*   **The orphan clone** left when the Clone form closes during a clone
    (issue #681).
*   **The picker on Ely,** off `bezel_theme` and `controls::card`.

--------------------------------------------------------------------------------

## Detailed Implementation Plan

One branch, one pull request, based on `main` after the palette fix pull
request has merged. The pull request is squash-merged, as Sirio's pull
requests are.

This delivery depends on a separate pull request:
[#693](https://github.com/ai-sirio/sirio/pull/693), merged on 2026-10-31 as
`7ffeb935`. That pull request adds
`fn root_modal_open(&self) -> bool` to `SirioWorkspace`, next to
`open_command_palette`. The predicate is true while the pane-close banner, the
End All confirmation or the title prompt is open. It also makes
`open_command_palette` return early when `self.palette_open` or
`self.root_modal_open()` holds. This plan reuses `root_modal_open()` and does
not create it. It only adds `|| self.identity_dialog.is_some()` to the
palette's early return.

### Terms of the plan

The spec terms apply. These are the plan's own terms:

*   **The drain.** The check in `render` that schedules
    `open_next_identity_dialog` for the next queued request (§8, file 12).
    **The control-action drain.** The code in `SirioWorkspace::new`, in
    `rust/crates/sirio/src/main.rs`, that runs the queued `ControlAction`s: its
    `match` has one arm for each variant.
*   **The add verb.** The debug socket method `surface.project_add`. **The path
    verb** is the add verb with `kind=path`.
*   **The read verb.** `surface.project_identity.read`. **The act verb** is
    `surface.project_identity.act`.
*   **The integrator.** The header of `rust/crates/sirio_ui/src/lib.rs` says
    that file declares modules only, and that "the integrator owns it". The
    integrator is whoever merges pieces built in separate worktrees. This
    delivery is one branch, so its author is the integrator.
*   **Return.** The name xdotool gives the Enter key. Scenarios say "Return"
    where they send the key, and "Enter" everywhere else.
*   **`key=value`.** In a verb call, a parameter (`kind=create`). In a check,
    a key of a `sirioctl … --json` reply, or a column of the stored `project`
    row, has that value (`open=false`, `icon_kind=icon`). No label is
    `displayName=""` in a reply and `display_name IS NULL` in the row.
*   **Shows.** "The read verb shows X" means the script polls with `wait_for`
    until X holds, and fails after 10 s. The same holds for any other verb.
    "At once" means the first read after the action, with no polling.
*   **The warning and the notice.** Two different lines of the dialog. The
    same-name *warning* is drawn dimmed, like the other helper text. The
    *truncation notice* is the one coloured `theme.ely.warning`, as the Clone
    form colours it.
*   **The sidebar notice.** The text the sidebar shows under its rows
    (`Sidebar::set_notice`). It is not the truncation notice.

The commits follow the order below. Each commit compiles and passes the tests
of the crates it touches, and its Check names them. Every `cargo nextest run`
below runs under `TMPDIR=~/.cache/sirio-sdd/tmp` (see the Verification Plan).
Every bullet of the files below carries the number of its commit in square
brackets. Where a file's bullets carry different numbers, the bullet's number
decides the commit. A number after "Proposed Changes" covers the whole block.

1.  **`fix(ely): confirm a dialog only on an Enter it saw pressed`.** The Ely
    `Dialog` vendor change (files 1, 2 and 3). Check:
    `cd rust && cargo build -p ely-gpui-component -p sirio_ui`, then
    `cargo nextest run -p ely-gpui-component`.
2.  **`fix: refresh the socket's project snapshot after a settings save`.**
    The catalog insert and the write path, including the M2 fix and the
    `SIRIO_DEBUG_FAIL_CATALOG_WRITES` switch (file 4 and the bullets of file
    12 marked [2]). Check: `cargo build -p sirio`, then
    `cargo nextest run -p sirio`.
3.  **`fix: apply one label rule in the Project Settings tab`.**
    `normalize_display_name` and its unit test, and the Project Settings tab
    (the bullets of files 5 and 9 marked [3]). Check:
    `cargo build -p sirio_ui`, then `cargo nextest run -p sirio_ui`.
4.  **`feat: add --display-name to project add`.** The socket parameter,
    `sirioctl`, `SKILL.md` and the notes in `docs/agents` (files 16 and 17,
    and the bullets of files 12, 13, 15 and 21 marked [4]). Check:
    `cargo build -p sirio_control --bins && cargo build -p sirio`, then
    `cargo nextest run -p sirio_control` and `cargo nextest run -p sirio`.
5.  **`feat: add the Project added dialog`.** The dialog, the picker, the
    forms and the module list (files 6, 7 and 10, and the bullets of files 5
    and 11 marked [5]). The dialog is not used yet. Check:
    `cargo build -p sirio_ui`, then `cargo nextest run -p sirio_ui`.
6.  **`feat: show the dialog after every add from the UI`.** The add origin,
    the queue, the drain, the keys and the shell (the bullets of files 8, 11
    and 12 marked [6]). `origin` and the queue land together, because the
    origin has no reader without the queue. The commit also adds the gpui
    test of the `needs_focus` gating (file 12, "the other root modals").
    Check: `cargo check --workspace --all-targets`, then
    `cargo nextest run -p sirio_ui` and `cargo nextest run -p sirio`.
7.  **`feat: add the debug verbs for the identity dialog`.** `DEBUG_METHODS`,
    the request builders, the `sirioctl` subcommands and the arms of
    `request_for_control_method` and `respond_to_control_action` land
    together (file 14, and the bullets of files 8, 9, 12, 13, 15 and 21 marked
    [7]). Check: `cargo build -p sirio_control --bins && cargo build -p sirio`,
    then `cargo nextest run -p sirio_control`, `cargo nextest run -p sirio_ui`
    and `cargo nextest run -p sirio`.
8.  **`test: prove the project identity dialog end to end`.** The E2E scripts,
    the CI step and their documentation (files 18, 19, 20, 23 and 24). Check:
    the scripts of the Verification Plan.
9.  **`chore: move the version to 0.39.0`.** The version, only when file 22's
    condition holds. Check: `cargo build -p sirio`, which rewrites
    `Cargo.lock`.

Code is cited by symbol, and line numbers refer to `d945ca2f`. Paths are
relative to the repository root. The first mention of a file in a file block
gives its full path.

### Target Files to Create/Modify

#### 1. `rust/vendor/ely-gpui-component/src/overlays/dialog.rs` (Modify)

*   **Type / Scope:** the vendored Ely dialog primitive.
*   **Proposed Changes:**
    *   **Public builders [1].** `held` and `focus_first` go from `pub(crate)`
        to `pub`, and their doc comments stay.
    *   **`on_enter` names its field [1].**
        *   New signature: `pub fn on_enter(self, field: FocusHandle, build: impl FnOnce(Close) -> Close + 'static) -> Self`.
            The builder still receives the dialog's close, so the action can
            close the dialog after it runs.
        *   The crate-private `Run` in `forms/options.rs` is the same type as
            the public `Close` (`Rc<dyn Fn(&mut Window, &mut App)>`). Naming
            `Close` in the public signature therefore leaves `Run` private
            and changes nothing for `PromptDialog`.
        *   The field is stored in a new `enter_field: Option<FocusHandle>`.
            The `expect("a dialog that takes Enter names its field with focus_first")`
            in `render` goes away.
    *   **The armed confirm [1].**
        *   `render` gets keyed state `(self.id.clone(), "enter-armed")` that
            holds a `bool`, through the same keyed-state mechanism that
            `take_focus` uses.
        *   The existing `capture_action(|_: &Enter, _, cx| cx.stop_propagation())`
            also sets the flag to `true` when `enter_field.is_focused(window)`.
        *   The `on_key_up` handler confirms only when the flag is `true`,
            the key is an unmodified Enter and the field is focused. Every
            Enter release sets the flag back to `false`, whatever the
            outcome.
        *   The armed confirm stops a release that has no press. It does not
            stop a press that repeats (§6).
    *   **The doc comment [1].** The line at the top of `Dialog`, "Escape
        closes it, Tab stays inside, and focus returns on every close", gets a
        sentence about the armed confirm. The comment at line 40 ("or the
        close button") is left as it is, because fullscreen dialogs do draw a
        close button.

#### 2. `rust/vendor/ely-gpui-component/src/overlays/dialogs.rs` (Modify)

*   **Type / Scope:** Ely's dialog wrappers.
*   **Proposed Changes [1]:** `PromptDialog`'s render passes its field's handle
    to both `focus_first` and the new `on_enter(field, ..)`. Its only change
    in behaviour is the armed confirm.

#### 3. `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` (Modify)

*   **Type / Scope:** the vendor change log.
*   **Proposed Changes [1]:** A new bullet under "Local source changes", right
    after the existing `overlays/dialog.rs` bullet. It records:
    *   `held`, `focus_first` and `on_enter` are public;
    *   `on_enter` names its own field;
    *   a release confirms only after a press that the dialog saw, and why:
        without it, the release of an Enter pressed before the dialog opened
        confirms the dialog;
    *   what it does not stop: a press that repeats;
    *   `PromptDialog` passes its field to `focus_first` and `on_enter`.

    The bullet has the same format as its neighbours, with continuation lines
    indented by two spaces.

#### 4. `rust/crates/sirio/src/session.rs` (Modify)

*   **Type / Scope:** the project catalog and the session store.
*   **Proposed Changes:**
    *   **`ProjectCatalog::insert(&mut self, path: &Path) -> Result<Option<String>, String>` [2].**
        It has `add`'s current body. It returns `Ok(Some(project.id.clone()))`
        after the push, and `Ok(None)` where `add` returns `Ok(false)`.
        `add` becomes `#[cfg(test)]`, a one-line wrapper:
        `self.insert(path).map(|id| id.is_some())`. Its 15 test call sites (14
        in `session.rs` and one in `main.rs`) stay unchanged, and
        `#[cfg(test)]` keeps it out of release builds, so there is no
        `dead_code` warning. The workspace calls `insert` through
        `insert_project` (file 12) and no longer calls `add`.
    *   **`SessionStore::try_schedule_catalog(&self, catalog: &ProjectCatalog) -> Result<(), String>` [2].**
        It has `schedule_catalog`'s current body, but it returns the
        `write_catalog` error as a string, and `Ok(())` when there is no
        database.
        *   Treating "no database" as success is deliberate. It is the
            in-memory mode tests run in, and `schedule_catalog` treats it the
            same way today.
        *   `schedule_catalog` keeps its signature. It calls
            `try_schedule_catalog` and prints the same
            `[session] failed to persist project catalog` line on error.
    *   **A debug-only switch [2].** Under `cfg(debug_assertions)`,
        `try_schedule_catalog` first reads the environment variable
        `SIRIO_DEBUG_FAIL_CATALOG_WRITES`. When it is `1`, the function
        returns `Err` before it writes. The variable is compiled out of
        release builds. The E2E scenario `save_error` uses it. A read-only
        `SIRIO_DB` would not do, because the connection stays open and a
        `chmod` does not make its writes fail. The switch has no unit test.
        The scenario `save_error` is its proof.

#### 5. `rust/crates/sirio_ui/src/project_identity.rs` (Modify)

*   **Type / Scope:** the project icon types and the picker.
*   **Proposed Changes:**
    *   **`normalize_display_name(input: &str, base_name: &str) -> Option<String>` [3].**
        A public free function that applies the rule of §5, in that order.
        `base_name` is the root name. Its `#[test]` is written before the
        function, listing first every way it can fail:
        *   surrounding spaces;
        *   only spaces;
        *   control characters in the middle, such as `\t`, `\n` and
            `\u{7}`;
        *   a value made only of control characters, which gives `None`;
        *   exactly 120 characters;
        *   121 characters;
        *   multi-byte characters around the cut, which must cut on
            characters, not bytes;
        *   a cut that leaves a trailing space;
        *   a label equal to the root name;
        *   one equal to the root name after trimming;
        *   one equal to the root name except for case, which is a label and
            is kept;
        *   an empty root name.

        It is a pure unit with no other proof, which is the case the project
        rules allow a unit test for.
    *   **`ProjectIconPicker::bare(mut self) -> Self` [5].** A builder that
        sets a private `bare: bool`. With `bare`:
        1.  `render` skips `controls::card(theme)` and returns the mode row,
            the separator and the body inside a plain `div()`;
        2.  the source rows that `render_source_field` builds for Avatar mode
            get `flex_wrap()`, and so does the row of `render_emoji_mode` that
            holds the emoji field and its Set Emoji and Open Emoji Picker
            buttons.

        Without `bare`, the layout is unchanged. The caption below applies in
        both.
    *   **Esc in the emoji grid [5].** In `on_emoji_grid_query_key`, the Esc
        branch calls `cx.stop_propagation()` after `close_emoji_grid`.
    *   **The Avatar caption [5].** `render_avatar_mode` gets a dimmed caption,
        "GitHub and favicon avatars show as a globe until Sirio can download
        images", under its source rows. It shows in both the Project Settings
        tab and the dialog.
    *   **Comments [5].**
        *   The comment in the preview code, "rendering those needs a network
            fetch this app has no HTTP client for yet (F-PRJ-14)", becomes
            "needs an avatar download, which Sirio does not do yet
            (F-PRJ-14)".
        *   The module doc at the top names `render_project_settings`, which
            no longer exists. It is pointed at
            `rust/crates/sirio_ui/src/sidebar/project_settings.rs`.
    *   **Hooks for the dialog's act verb [5].** These are `pub(crate)`, for
        `project_identity_dialog.rs` only:
        *   `control_set_mode(&mut self, mode: &str, cx)`, accepting `icon`,
            `emoji` or `avatar`. It ignores any other value, and the dialog's
            `control_act` rejects an unknown mode before it calls this;
        *   `control_open_emoji_grid(&mut self, window, cx)`;
        *   `control_pick_glyph(&mut self, id: &str, cx) -> Result<(), String>`,
            which accepts a `ProjectGlyph::id()` and commits through the same
            `commit` path a click uses. An unknown id is an error.

#### 6. `rust/crates/sirio_ui/src/project_identity_dialog.rs` (Create)

*   **Type / Scope:** a new `sirio_ui` component, `ProjectIdentityDialog`, an
    entity.
*   **Proposed Changes [5]:**
    *   **Public types.**
        *   `FocusMode { Field, Dialog }`.
        *   `IdentityOutcome { Saved { display_name: Option<String>, icon: ProjectIcon }, Unchanged, Skipped }`.
        *   `ProjectIdentityEvent::Closed { project_id: String, outcome: IdentityOutcome }`,
            with `impl EventEmitter<ProjectIdentityEvent>`.
        *   `OtherProject { shown_name: String, path: PathBuf }`. Its shown
            name is the project's label, or its root name when it has none.
        *   `ProjectIdentityDialogInput`: `seq: u64`, `project_id`,
            `root_name`, `root_path`, `initial_icon: ProjectIcon`,
            `focus_mode`, `notice: Option<String>` (the truncation notice) and
            `others: Vec<OtherProject>`.
    *   **The constructor.**
        `ProjectIdentityDialog::new(input, cx: &mut Context<Self>) -> Self`
        needs no `Window`.
        *   It calls `crate::ely::init(cx)`, as the forms do through
            `ensure_theme`, so the field's keys are bound.
        *   It creates the field, `TextInput::detached(cx)`, with the root
            name, through `set_text`. `field_focus` is the field's
            `FocusHandle`, taken with `Focusable::focus_handle(cx)`, because
            `TextInput::focus()` is `pub(crate)`. The two names are used
            below: `field` is the `TextInput` entity, and `field_focus` is its
            handle.
        *   It creates a second handle, `body_focus`.
        *   It creates
            `ProjectIconPicker::with_value_and_repo(initial_icon, &root_path, cx).bare()`.
        *   It sets `emitted` to `Rc::new(Cell::new(false))`, `stored_close`
            to `Rc::new(RefCell::new(None))`, `selected_once` to `false` and
            `rendered_once` to `false`. The first two are shared with the
            closures below, see "The handlers".
    *   **Render.**
        *   It calls `crate::ely::sync_theme_if_changed(cx)`.
        *   In field mode, on the entity's first render, it calls
            `select(0..len)` on the field once (`selected_once`), as
            `PromptDialog` selects its text on its first render
            (`rust/vendor/ely-gpui-component/src/overlays/dialogs.rs`).
        *   It sets `rendered_once` to `true`. The workspace's focus rule
            reads it through `rendered_once()` (file 12).
        *   It builds the `Dialog`:
            *   `Dialog::new(("project-identity", seq), "Project added", on_close)`,
                where `on_close` is the closure described under "The
                handlers". `seq` is the input's per-workspace counter (file
                12). The id holds no project id, because Ely logs
                `dialog {id:?}: closed` and the log must stay free of ids;
            *   `.held()`;
            *   `.focus_first(field_focus)` in field mode, and
                `.focus_first(body_focus)` in dialog mode;
            *   `.on_enter(field_focus, |close| ..)` in both modes. The
                builder returns a `Close` that calls `save` and then the
                `Close` it was given.
        *   The body is one `flex_col().gap_5()` div, as in Ely's
            `dialog.rs:209–215`. It wraps every child below and is tracked
            with `body_focus`, inside the Ely content. So `body_focus` is
            inside the Ely layer, and the layer's Esc listener stays on the
            dispatch path. The children, in order:
            1.  `selectable_text(root_path.display())`;
            2.  the caption "Name in the sidebar";
            3.  `Input::new(&field)`;
            4.  the dimmed line "The folder on disk keeps its name.";
            5.  the same-name warning. It shows when
                `normalize_display_name(draft, root_name).unwrap_or(root_name)`
                equals an `OtherProject`'s `shown_name` exactly. The text is
                "Another project is also named "<name>"", in the same dimmed
                style, followed by the path of the first such project in
                catalog order, through `selectable_text`;
            6.  the truncation notice, if any, coloured `theme.ely.warning`;
            7.  the picker.
        *   The actions are Skip (ghost) and Save (primary). Each calls its
            handler and then the dialog's `Close`, the order
            `render_discard_dialog` uses. Ely's builder is
            `FnOnce(Close) -> E`, and it runs inside the element's render, so
            a plain `Option<Close>` field of the entity cannot be filled. The
            `.action(..)` builders therefore clone `stored_close` (an
            `Rc<RefCell<Option<Close>>>`) and store the `Close` they receive
            in it.
    *   **The handlers.**
        *   `save` reads the field's text, calls
            `normalize_display_name(text, root_name)`, reads
            `picker.value()`, and compares it with `initial_icon` by `==`. It
            emits `Unchanged` when the name is `None` and the icon is equal,
            and `Saved { .. }` otherwise.
        *   `skip` emits `Skipped`.
        *   `save` and `skip` are `pub`. Each returns at once when `emitted`
            is set, and sets `emitted` before it emits `Closed`. `emitted` is
            an `Rc<Cell<bool>>`.
        *   The `on_close` of the `Dialog` is a closure that captures
            `emitted` and a `WeakEntity` of the dialog. Ely runs `on_close`
            on every way out, and also after a stored `Close` runs
            `give_back`. The closure does nothing when `emitted` is already
            true. Otherwise it updates the entity and runs `skip`. So a close
            that began in `save`, `skip` or `control_act` never leases the
            entity a second time.
    *   **Focus methods.** The workspace's focus rule (§7) calls three of
        them:
        *   `contains_focus(window, cx)` is
            `body_focus.contains_focused(window, cx)`;
        *   `reclaim_focus(window, cx)` focuses the field in field mode and
            `body_focus` in dialog mode;
        *   `rendered_once(&self) -> bool` returns `rendered_once`. The rule
            runs only after it is `true`. On the first frame after the open,
            the workspace renders before the entity, and Ely's `take_focus`
            runs later in that same frame, inside the dialog's element. It
            records the focus held before the dialog opened as the handle to
            give back (`rust/vendor/ely-gpui-component/src/primitives/focus.rs`,
            lines 146–153). A rule that ran earlier would reclaim focus first,
            and Ely would record the dialog's own field as that handle.
    *   **The debug report.** `report(&self, window, cx) -> Vec<(String, String)>`
        returns:
        *   `projectId`;
        *   `focusMode`: `field` or `dialog`;
        *   `nameDraft`;
        *   the picker value's persisted parts, as `iconKind`, `iconValue`
            and `color`;
        *   `focus`: `field` when `field_focus` is focused, `dialog` when
            `body_focus` holds focus without the field, otherwise empty. The
            workspace fills in an empty value (file 12);
        *   `warning` and `notice`: `on` or `off`;
        *   `fieldBounds` and `pathBounds`, as `x,y,w,h` in window pixels.

        The bounds are recorded at prepaint into the entity, with the
        `canvas(..)` pattern that `rust/crates/sirio_ui/src/tab_bar.rs` uses
        for `anchor_bounds`. The script needs them to click the field and
        the path, and it cannot know where they are, because the path above
        the field can wrap to more lines.
    *   **`control_act(&mut self, action: &str, value: Option<&str>, window, cx) -> Result<(), String>`.**
        It accepts `save`, `skip`, `mode <icon|emoji|avatar>`,
        `open-emoji-grid` and `pick-glyph <id>`. An unknown action, mode or id
        is an error.
        *   `save` and `skip` run their handler, which sets `emitted`. Then
            they call the stored `Close` from `stored_close`. That `Close`
            runs `give_back` and then `on_close`, which returns at once
            because `emitted` is set. So the entity is never leased twice,
            even though `control_act` itself runs inside `dialog.update(..)`.
        *   `mode`, `open-emoji-grid` and `pick-glyph` close nothing.
        *   The stored `Close` is empty until the dialog's first render.
            Until then, `save` and `skip` return `Err("the dialog has not
            been drawn yet")` and change nothing. The read verb reports
            `open=true` only once `rendered_once` is true, so a script that
            waits for `open=true` before an `act` never meets this error.

#### 7. `rust/crates/sirio_ui/src/lib.rs` (Modify)

*   **Type / Scope:** the module list.
*   **Proposed Changes [5]:** `pub mod project_identity_dialog;` right after
    `pub mod project_identity;`. The file's header says that the integrator
    owns it (see Terms of the plan). This single-branch delivery is its own
    integrator.

#### 8. `rust/crates/sirio_ui/src/sidebar/mod.rs` (Modify)

*   **Type / Scope:** the sidebar view and its events.
*   **Proposed Changes:**
    *   **The origin [6].** `pub enum AddOrigin { OpenedFolder, AddedWithoutGit, InitializedGit, Cloned { truncated: bool }, Created }`,
        which derives `Clone, Copy, Debug, PartialEq, Eq`.
        `SidebarEvent::AddProject(PathBuf)` becomes
        `SidebarEvent::AddProject { path: PathBuf, origin: AddOrigin }`.
    *   **The comment on the variant [6].** Every UI path that adds a project
        emits this event. The socket's `project.add` must not, because only
        this path opens the "Project added" dialog.
    *   **The five emit sites set their origin [6]:**
        *   `confirm_add_project`, `.git` exists: `OpenedFolder`;
        *   the non-git prompt's "Add without Git" button (index 1):
            `AddedWithoutGit`;
        *   after `git init` (the prompt's "Initialize Git", index 0):
            `InitializedGit`;
        *   `CloneFormEvent::Cloned`: `Cloned { truncated }`;
        *   `CreateFormEvent::Created`: `Created`.
    *   **The Clone form always closes [6].** The `Cloned` subscriber sets
        `sidebar.project_form = None`, a truncated clone included.
        `clone_form_stays_open_after` and its doc comment are deleted. The
        truncation notice moves into the dialog (file 12), and the workspace
        never closes a form.
    *   **The sidebar tracks its own prompt [6].** `window.has_active_prompt()`
        is true only for prompts that gpui draws inside the window
        (bezel-gpui 0.3.8, `window.rs:6215–6219`). On macOS and Windows the
        non-git prompt is native, so the drain and the focus rule (file 12)
        could not see it.
        *   `Sidebar` gets a private `prompt_open: bool`, `false` at creation.
            `confirm_add_project` sets it just before `window.prompt`.
        *   The async block that awaits the answer clears it when the answer
            arrives, whatever the answer is. That includes the
            `unwrap_or(2)` path, taken when the receiver is dropped. The block
            clears it before it emits an event or returns, so no branch
            leaves it set. A flag left set would stall the queue.
        *   `pub fn prompt_open(&self) -> bool` returns it.
        *   When `debug_prompt_answer` (below) is set, there is no prompt, and
            the flag is never set.
        *   **A new `SidebarEvent::PromptClosed`.** The async block emits it
            right after it clears `prompt_open`, whatever the answer. The
            workspace's existing subscription to the sidebar (`main.rs:6608`)
            calls `cx.notify()` on it. Without it, a Cancel emits no event,
            the workspace does not re-render, and a request queued behind the
            prompt waits for an unrelated frame. `SidebarEvent` is public, so
            every exhaustive match on it gains an arm. The only one outside
            `sirio_ui` is `main.rs:10118`, and it gets one `PromptClosed => {}`
            arm. Because a new variant can break matches in other crates'
            test targets, this commit's check builds with
            `cargo build --workspace --all-targets`.

    *   **`project_form_kind(&self) -> &'static str` [7].** It returns `none`,
        `clone` or `create`, from `project_form`. The read verb reports it as
        `projectForm`.
    *   **Debug entry points [7],** `pub` and present in every build, as the
        entry points of the other debug verbs are, and used only by the add
        verb in `main.rs`. A release build never reaches them, because the
        verb's arm in `main.rs` is guarded (file 12):
        *   `control_add_path(&mut self, path: PathBuf, answer: Option<u8>, window, cx)`
            sets a one-shot field `debug_prompt_answer: Option<u8>` to
            `answer` and calls `confirm_add_project`. When that field is set,
            `confirm_add_project` skips `window.prompt` and uses the value in
            place of the prompt's reply. The values are those of the prompt's buttons in
            code order: `0` Initialize Git, `1` Add without Git and `2`
            Cancel (the prompt's own `unwrap_or(2)` default).
        *   `control_project_form(&mut self, kind: &str, value: &str, submit: bool, window, cx) -> Result<(), String>`:
            1.  it calls `start_clone_project` or `start_create_project`;
            2.  it fills the URL or the name with the form's `set_url` or
                `set_name`;
            3.  it focuses the form's field;
            4.  it calls the form's `submit` when `submit` is set.

            An unknown `kind` is an error. The forms use their default
            parent, `sirio_project::default_project_base()`, which honours
            `SIRIO_PROJECTS_DIR`. The scripts set it.
    *   **Tests in this file [6].** The two `SidebarEvent::AddProject(path) if *path == picked`
        patterns in the folder-picker tests become
        `SidebarEvent::AddProject { path, .. }`. They keep what they assert.

#### 9. `rust/crates/sirio_ui/src/sidebar/project_settings.rs` (Modify)

*   **Type / Scope:** the Project Settings tab.
*   **Proposed Changes:**
    *   **The label rule [3].** `to_update` computes `display_name` as
        `normalize_display_name(&self.display_name, &self.base_name)` in place
        of its own trim. Its field stays hand-written (a Non-Goal).
    *   **The label field's bounds [7].** Scenario 18 of file 18 needs to
        click the tab's label field, and the tab reports no bounds today. The
        tab records `labelBounds`, as `x,y,w,h` in window pixels, at prepaint,
        with the `canvas(..)` pattern that file 6 uses for `fieldBounds`. A
        debug read reports it as `projectSettingsLabelBounds`. Nothing else in
        the tab changes.
    *   **Which read carries it.** `surface settings read` cannot, because
        it answers "Settings surface is not open" unless the Settings screen
        is open (`control_read_settings`). So the read verb
        (`surface.project_identity.read`) carries it as one more key that the
        workspace fills in. The key holds the bounds of the Project Settings
        tab that is drawn, or is empty when none is. The read verb answers
        whether or not a dialog is open, so scenario 18 can use it.

#### 10. `rust/crates/sirio_ui/src/sidebar/row.rs` (Modify)

*   **Type / Scope:** the sidebar row.
*   **Proposed Changes [5]:** The comment on the `project_mark` binding (a
    local `let` over a `match` in the row's render code) says GitHub and
    favicon avatars "need an HTTP client this app doesn't have yet". It now
    says that they need an avatar download, which Sirio does not do yet.

#### 11. `rust/crates/sirio_ui/src/project_forms.rs` (Modify)

*   **Type / Scope:** the Clone and Create forms.
*   **Proposed Changes:**
    *   **Create's field caption [5].** `form_label("Project name", ..)` in
        Create becomes `form_label("Folder name", ..)`. The placeholder
        `project-folder-name` stays.
    *   **One notice text [5].** A new
        `pub fn clone_truncation_notice(destination: &Path) -> String`
        returns the text `clone_status_line` builds today: "Cloned to {} —
        progress output was truncated (repository is large)".
        `clone_status_line` and `open_next_identity_dialog` (file 12) both
        call it.
    *   **Two doc comments that say the form stays open [6].** The sidebar now
        closes the Clone form on every `Cloned`, so both comments change
        together with file 8's subscriber:
        *   On `CloneStatus::Complete`, the comment keeps its account of a
            truncated clone as a notice, not a failure. It adds that the
            notice is shown in the identity dialog, with the text
            `clone_truncation_notice` builds.
        *   On `CloneFormEvent::Cloned`, the sentence that tells a subscriber
            to keep the form open when `truncated` is set is replaced by: the
            subscriber closes the form on `Cloned`, `Sidebar` does so, and a
            truncated clone's notice is shown in the identity dialog.

#### 12. `rust/crates/sirio/src/main.rs` (Modify)

*   **Type / Scope:** the workspace (`SirioWorkspace`), the control handler
    and the Windows tests.
*   **Proposed Changes**, grouped by area:
    *   **Adding.**
        *   `insert_project(&mut self, path: &Path, display_name: Option<&str>) -> Result<Option<String>, String>`
            follows §1. Commit 2 [2] adds it as `insert_project(&path)`,
            without the `display_name` parameter, and commit 4 [4] adds the
            parameter. With `display_name`, after `insert` returns an id, it:
            1.  normalizes the label against the catalog project's root name.
                When the result is `None`, it skips steps 2 and 3, so no
                settings are written (§4, §9);
            2.  builds a `ProjectSettingsUpdate` from `project_settings(id)`
                (the icon through `ProjectIcon::from_persisted_parts`);
            3.  calls `apply_project_settings`.

            Then it calls `schedule_catalog`, `sync_control_state` and
            `refresh_sidebar` once each. When `insert` returns `Ok(None)` it
            skips all three, as today.
        *   **The callers move to `insert_project` [2].** `add_project` and
            `control_add_project` call `insert_project(&path)` in place of
            `ProjectCatalog::add`. Commit 4 adds the `display_name` argument,
            and commit 6 adds `origin`.
        *   `add_project(&mut self, path: PathBuf, origin: AddOrigin, cx)`
            [6] calls `insert_project(&path, None)`.
            *   On `Ok(Some(id))`, it calls
                `identity_queue.push_back(IdentityRequest { id, origin })` and
                `cx.notify()`. It does not open the dialog; the drain
                schedules the open.
            *   `Ok(None)` and `Err` keep today's sidebar notice and toast
                ("already tracked or nested: <path>" for `Ok(None)`).
        *   `control_add_project(&mut self, path: &Path, display_name: Option<String>, cx) -> Result<Vec<(String, String)>, String>`
            [4] calls `insert_project`, then builds the reply. The reply keeps
            today's keys, plus `displayName` and `displayNameApplied` when a
            label was sent (§9).
        *   **Call sites to update.**
            *   Commit 4 [4]: the test that calls
                `control_add_project(&working_directory, cx)` (about line
                46929), and the control-action drain's `AddProject` arm, which
                calls `workspace.control_add_project(&path, cx)`.
            *   Commit 6 [6]: the test that calls
                `workspace.add_project(path.clone(), cx)` (about line 39574),
                and the `SidebarEvent::AddProject` arm of
                `handle_sidebar_event`, which passes `path` and `origin`.
            *   The one test call of `ProjectCatalog::add` in `main.rs` stays
                as it is (file 4).
    *   **Writing [2].**
        *   `apply_project_settings(&mut self, update: &ProjectSettingsUpdate) -> Result<(), String>`
            holds today's `CatalogProjectSettings` construction and the call
            to `ProjectCatalog::update_project_settings`.
        *   `update_project_settings` becomes `-> Result<(), String>`. It
            runs, in order:
            1.  `apply_project_settings`;
            2.  `self.session.try_schedule_catalog`;
            3.  `sync_control_state`, which already calls
                `publish_worktree_lookup`;
            4.  `refresh_sidebar`.

            If step 1 or step 2 fails, it sets the sidebar notice as today,
            skips steps 3 and 4 and returns the error.
        *   The comment that starts "F-PRJ-17/F-PRJ-18: `update.default_worktree_base`/"
            stays above the settings construction, which moves into
            `apply_project_settings`.
        *   `subscribe_project_settings_tab` ignores the returned `Result`,
            because the sidebar notice is already shown. Every other caller of
            `update_project_settings` is checked in the diff.
        *   `save_project_identity(&mut self, id: &str, display_name: Option<String>, icon: ProjectIcon, cx) -> Result<(), String>`
            [6] follows §4. On `Err` it calls `show_toast` with the text
            "Couldn't save the name of <shown name> to disk: <error>. Sirio
            keeps it and retries with its next save." After `show_toast`
            returns, it stores the id of `self.toast` in
            `identity_error_toast`. `show_toast` keeps its signature. It
            returns the error.
    *   **The dialog's state [6].** Two new structs in `main.rs`:
        *   `struct IdentityRequest { id: String, origin: AddOrigin }`;
        *   `struct OpenIdentityDialog { entity: Entity<ProjectIdentityDialog>, subscription: Subscription, origin: AddOrigin }`.

        These new fields of `SirioWorkspace` start empty:
        *   `identity_queue: VecDeque<IdentityRequest>`;
        *   `identity_dialog: Option<OpenIdentityDialog>`;
        *   `identity_open_scheduled: bool`. `render` sets it when it defers
            the open, `open_next_identity_dialog` clears it, and the drain
            checks it;
        *   `identity_error_toast: Option<u64>`;
        *   `identity_last_outcome: Option<&'static str>`;
        *   `identity_seq: u64`, the counter behind the dialog's element id.
            `open_next_identity_dialog` increments it for each dialog.
    *   **`render`: where the drain and the focus rule sit [6].** They sit with
        the `needs_focus` blocks, before the `if self.show_settings` early
        return, so the dialog also opens over the Settings screen (scenario
        9).
    *   **`render`: the drain [6].** `render` schedules
        `open_next_identity_dialog` through `cx.defer_in(window, ..)` when all
        of these hold:
        *   `identity_dialog` is `None`;
        *   `!self.root_modal_open()` (the predicate of the palette fix PR);
        *   `!self.palette_open`;
        *   no prompt is up: `!window.has_active_prompt()` and
            `!self.sidebar.read(cx).prompt_open()` (file 8). The first term
            sees only prompts that gpui draws in the window, and the
            non-git prompt is native on macOS and Windows;
        *   `identity_queue` is not empty;
        *   `!identity_open_scheduled`;
        *   `self.toast` is not `Some` with the id in `identity_error_toast`.

        It sets `identity_open_scheduled` when it defers. The dialog opens on
        the next frame, which keeps the synchronous `git config` out of
        `render`. `schedule_restored_scrollback` already defers work from
        `render` this way, through `cx.defer_in`.
    *   **`render`: the focus rule [6].** It runs only when
        `dialog.read(cx).rendered_once()` is `true`. On the first frame after
        the open, the workspace renders before the entity, and Ely's
        `take_focus` runs later in that same frame, inside the dialog's
        element. It records the focus held before the dialog opened. From the
        next frame on, the rule can reclaim focus without hiding that handle.
        While the dialog is open and no prompt is up
        (`!window.has_active_prompt() && !self.sidebar.read(cx).prompt_open()`),
        `render` calls the dialog's `reclaim_focus` when focus is neither
        inside the dialog (`contains_focus`) nor on `root_focus`.
    *   **`render`: the other root modals [6].** The three `needs_focus` blocks
        (pane close, End All, title prompt) also require
        `self.identity_dialog.is_none()`. A modal that opens while the dialog
        is open is drawn under the scrim, visible but dimmed, without focus or
        keys, and claims focus when the dialog closes.
        *   **The test, written first [6].** No socket verb raises a root
            modal, so no E2E scenario proves this gating. A new
            `#[gpui::test]` in the test module of `main.rs` does. It is
            written in this commit's test-first step, before the
            `needs_focus` change, and it goes red without that change. It is
            built like
            `drawn_pane_close_prompt_is_held_for_every_status_including_idle_and_done`
            and does the following:
            1.  It opens the identity dialog through the workspace.
            2.  It calls `request_close_terminal_at(0, 0, None, cx)`, the
                entry point of that existing test.
            3.  It asserts that focus stays inside the dialog and that the
                pane-close banner does not claim it.
            4.  It skips the dialog.
            5.  It asserts that the banner takes focus: `pane-close-confirm`
                is drawn and focused.
            A gpui test can draw an Ely `Dialog`: the tests in
            `rust/crates/sirio_ui/src/changes.rs` around line 6588 open
            `changes-discard-dialog` and check its bounds.

    *   **`open_next_identity_dialog(&mut self, window, cx)` [6].**
        *   It clears `identity_open_scheduled`.
        *   It calls `identity_queue.pop_front()` until a request names a
            project that is still in the catalog, and reads that project's
            `Sidebar::project_settings_seed(id)`. If no request is left, it
            does nothing.
        *   It builds `others` from `project_settings_seed` for every other
            project: its shown name (the seed's `display_name`, or its
            `base_name` when that is empty) and its path.
        *   It chooses the focus mode from the origin: field mode for
            `OpenedFolder`, `AddedWithoutGit` and `Created`, dialog mode for
            `InitializedGit` and `Cloned`.
        *   For `Cloned { truncated: true }`, it sets the input's `notice` to
            `clone_truncation_notice(&seed.path)`. It does not call
            `close_project_surface`: the sidebar already closed the form.
        *   It increments `identity_seq`, creates the entity and subscribes to
            `Closed` with a plain `cx.subscribe`.
        *   It records `sirio_perf::event("project_add.dialog_opened", 0)` and
            calls `cx.notify()`.
    *   **`on_identity_dialog_closed(&mut self, event, cx)` [6].**
        1.  It takes `identity_dialog`.
        2.  It acts on the outcome. `Saved` calls `save_project_identity`.
            `Unchanged` and `Skipped` write nothing.
        3.  It sets `identity_last_outcome` to `saved`, `unchanged`,
            `skipped` or `error`.
        4.  It prints one stderr line, `project identity: <outcome>`.
        5.  It calls `cx.notify()`.

        It moves no focus and calls nothing to open the next dialog. Ely's
        `give_back` restores the focus held before the dialog opened. When
        that handle belonged to a form that has gone, the workspace's
        existing rule F-SID-19 moves focus to `root_focus`.
    *   **Drawing [6].** `render_global_overlays`, which both shell branches
        call, draws the dialog entity's element. The dialog is drawn in the
        Settings screen branch too. It draws the dialog just before the
        toast, so a toast raised while the dialog is open stays visible above
        the scrim.
    *   **Shell actions [6].**
        *   The `on_action(cx.listener(Self::handle_*))` chain of the main
            shell branch moves into one helper,
            `fn shell_actions(&self, frame: Stateful<Div>, cx) -> Stateful<Div>`.
            It takes no flag and covers the 38 listeners, including the
            ToggleSidebar, ToggleRightPanel and ToggleSecondaryPane closures.
        *   The main shell branch applies it with
            `.when(self.identity_dialog.is_none(), |frame| self.shell_actions(frame, cx))`.
        *   The Settings screen branch has one listener,
            `handle_close_settings_surface`. It gets
            `.when(self.identity_dialog.is_none(), ..)`.
        *   `on_action(sirio_ui::text_selection::on_copy)`,
            `track_focus(&self.root_focus)`, `key_context("Workspace")` and
            `capture_key_down(handle_root_key_down)` stay unconditional in
            both branches.
    *   **The palette [6].** `open_command_palette` adds
        `|| self.identity_dialog.is_some()` to the early return that the
        palette fix PR gives it. The `intercept_keystrokes` interceptor for
        Ctrl+Shift+P still calls `stop_propagation`, so the chord is eaten.
    *   **`handle_root_key_down` [6].** A new first branch, before the palette
        check. While `identity_dialog` is `Some`, this branch always returns.
        On Esc with `self.root_focus.is_focused(window)`, it first calls
        `dialog.update(cx, |d, cx| d.control_act("skip", None, window, cx))`,
        which runs `skip` and then the stored `Close`, so Ely's `give_back`
        also runs, and it stops propagation. Any other key it returns
        without stopping, so the key reaches the dialog and the Esc branch
        that closes a form never runs.
    *   **The control handler [4].**
        *   **`"project.add"`.** It reads the optional `displayName`. When the
            value is present and empty after removing control characters and
            trimming, it answers `project.add displayName must not be empty`
            and adds nothing. The check is
            `normalize_display_name(value, "").is_none()`, which is true
            exactly when nothing is left after cleaning. A value equal to the
            root name is not an error: `insert_project` stores no label for it
            and the reply carries `displayNameApplied=true`.
        *   **The action.** `ControlAction::AddProject` gains
            `display_name: Option<String>`, and the control-action drain's
            arm passes it to `control_add_project`.
    *   **The debug verbs [7].** Each verb's match arm carries the guard
        `if cfg!(debug_assertions)`, as the arms of the existing debug verbs
        do, and queues a `ControlAction`. The methods the arms call exist in
        every build, so a release build still compiles them:
        *   `"surface.project_add"`, with `kind` and `value` required and
            `submit` and `answer` optional. `answer` is `0`, `1` or `2` and
            is valid only with `kind=path`:
            `ControlAction::ProjectAddDebug { kind, value, submit, answer, reply }`.
            The name differs from `ControlAction::AddProject`, so the two
            cannot be confused. The control-action drain routes `path` to
            `sidebar.control_add_path` and `clone` or `create` to
            `sidebar.control_project_form`, and replies `ok`.
        *   `"surface.project_identity.read"`:
            `ControlAction::ProjectIdentityRead { reply }`. It replies:
            *   `open` (`true` once the dialog has rendered, that is, while
                `identity_dialog` is `Some` and its `rendered_once()` is true;
                `false` otherwise), `queue` (the length) and `lastOutcome`;
            *   while a dialog is open, the dialog's `report` and `origin`
                (`opened-folder`, `added-without-git`, `initialized-git`,
                `cloned` or `created`);
            *   `focus`, one of `field`, `dialog`, `terminal`, `root`, `none`
                or `other`. The dialog's `report` fills `field` or `dialog`.
                The workspace fills the rest: `terminal` (through
                `focused_terminal`), `root` (`root_focus`), `none`, or
                `other`;
            *   `projectForm` (`none`, `clone` or `create`, from
                `Sidebar::project_form_kind`), `sidebarVisible`,
                `paletteOpen`, `pendingPaneClose`, `toast` (the current
                text, or empty) and `projectSettingsLabelBounds` (file 9, or
                empty).
        *   `"surface.project_identity.act"`, with `action` required and
            `value` optional:
            `ControlAction::ProjectIdentityAct { action, value, reply }`. It
            calls the dialog's `control_act`, or answers
            `no project identity dialog is open`.
    *   **Windows.** Each match is compiled only on Windows.
        *   The `project.add` arm of `request_for_control_method` becomes
            `request::project_add(VERBATIM_PATH, None)` [4], because the
            builder's signature changes. The `AddProject` arm of
            `respond_to_control_action` already reads `{ reply, .. }` and does
            not change.
        *   `respond_to_control_action` gets an arm for each of the three new
            `ControlAction` variants [7].
        *   `request_for_control_method` gets arms for the three new methods
            [7], using the new `request::` builders with fixed arguments:
            *   `kind=path` and `VERBATIM_PATH` for `surface.project_add`;
            *   `skip` for `surface.project_identity.act`.

#### 13. `rust/crates/sirio_control/src/protocol.rs` (Modify)

*   **Type / Scope:** the request builders.
*   **Proposed Changes [4, 7]:**
    *   `request::project_add(path: &str, display_name: Option<&str>)` [4]
        inserts `displayName` only for `Some`, following the pattern of
        `worktree_set`.
    *   New builders [7]:
        *   `surface_project_add(kind: &str, value: &str, submit: bool, answer: Option<u8>)`;
        *   `project_identity_read()`;
        *   `project_identity_act(action: &str, value: Option<&str>)`.

        They produce the three method names of file 12.

#### 14. `rust/crates/sirio_control/src/served.rs` (Modify)

*   **Type / Scope:** the served method list.
*   **Proposed Changes [7]:**
    *   `DEBUG_METHODS` grows from 36 to 39 entries. The new entries are
        `surface.project_add`, `surface.project_identity.read` and
        `surface.project_identity.act`.
    *   The file has three per-verb tests today, each of which names one
        debug-only verb (`surface.settings.act`, `surface.chat.panel` and
        `surface.chat.effort`). A new test of the same shape covers the three
        new verbs: a debug build serves them and a release build does not. It
        is written before the list changes.
    *   `project.add` stays in the release list.

#### 15. `rust/crates/sirio_control/src/bin/sirioctl.rs` (Modify)

*   **Type / Scope:** the CLI.
*   **Proposed Changes:**
    *   **`cmd_project` `add` [4].** These checks run before any request is
        sent:
        *   `parsed.flag("display-name")`, the flag given with no value, is
            an error: "--display-name needs a value".
        *   An empty `parsed.value("display-name")` is the same error.

        Then it sends `project_add(path, display_name)`. With a label:
        *   A response without `displayNameApplied` is an error that exits 1.
            It says "this Sirio predates --display-name; the project was
            added without a label" when `added` is `true`, and "this Sirio
            predates --display-name; the project was not added (already
            tracked, inside a tracked project, or containing one)" when
            `added` is `false`.
        *   `displayNameApplied=false` prints "not added: already tracked,
            inside a tracked project, or containing one; label ignored" to
            stderr and exits 0.

        The printed columns become
        `["added", "projectId", "worktreeCount", "displayName"]` when a label
        was sent. Without a label the output is unchanged, so the existing
        test still holds.
    *   **Usage [4].** The usage line reads
        `project add <path> [--display-name <label>] [--json]`. A label that
        starts with `--` is passed as `--display-name=--x`.
    *   **New subcommands, for debug builds only [7].** Under `cmd_surface`:
        *   `surface project-add --kind path|clone|create --value <path|url|name> [--submit] [--answer 0|1|2]`;
        *   `surface project-identity read`;
        *   `surface project-identity act <action> [<value>]`.

        Each usage line carries "(debug builds)" after its arguments and
        before its description, as the existing lines do.

#### 16. `rust/crates/sirio_control/tests/control_integration.rs` (Modify)

*   **Type / Scope:** integration tests of `sirioctl` against the fake
    socket server this file already runs.
*   **Proposed Changes [4]:** The fake `TestHandler` answers `project.add`
    today with `ControlResponse::success` over a `BTreeMap<String, String>`
    that holds `added=true` and `projectId=project-1`. It now reads the
    request's params and answers from them:
    *   with no `displayName`, it gives the current reply, byte for byte;
    *   with `displayName`, it gives `displayNameApplied=true`;
    *   for the path `/tracked`, it gives `added=false` and
        `displayNameApplied=false`;
    *   for the path `/old-app`, it gives the reply without the key.

    These tests are written before the `sirioctl` change, with each way the
    client can fail listed first:
    1.  `--display-name` with no value exits 1 and sends nothing.
    2.  `--display-name=` exits 1 and sends nothing.
    3.  A label is sent as `displayName`, exactly as given; the app
        normalizes it, not the client.
    4.  A response without `displayNameApplied` (path `/old-app`) exits 1 with
        the "predates" message. This is the old-app case, which no E2E can
        reach without an old binary.
    5.  `displayNameApplied=false` (path `/tracked`) exits 0 and prints the
        note.

    The existing `sirioctl_project_add_sends_the_path_and_reports_identity`
    is not changed.

#### 17. `skills/sirio/SKILL.md` (Modify)

*   **Type / Scope:** the agent skill that Sirio installs.
*   **Proposed Changes [4]:** A short section, "Add a project", before "Report
    durable and user-visible status". It contains:
    *   one code block: `sirioctl project add "$REPO" --display-name 'client app'`;
    *   one sentence: the label is what the sidebar shows; the folder keeps
        its name; the label cannot be used to select the project in other
        commands;
    *   one sentence: a path that is not added (already tracked, inside a
        tracked project, or containing one) ignores the label, and an agent
        checks `displayNameApplied` in the `--json` reply, because an older
        `sirioctl` accepts `--display-name` and ignores it.

    `skill_is_served.rs` parses this code block and finds `project.add` in
    the release list.

#### 18. `Scripts/Tests/test-project-identity-e2e.sh` (Create)

*   **Type / Scope:** an E2E script that drives a real debug build.
*   **Proposed Changes [8]:**
    *   **The frame**, copied from `Scripts/Tests/test-changes-e2e.sh` and
        `Scripts/Tests/test-editor-e2e.sh`:
        *   `$WORK` is the script's work directory;
        *   it builds `sirio` and `sirioctl`;
        *   **each scenario starts a fresh app on a fresh `SIRIO_DB`, a fresh
            private `HOME` and a fresh `SIRIO_PROJECTS_DIR`, and creates its
            own fixtures under `$WORK/<scenario>/`.** No scenario depends on
            another, so `--only` and `--state-only` select any of them. Every
            add in a scenario uses the absolute `$WORK/<scenario>/…` path of
            its fixture;
        *   stub agent CLIs are on `PATH`, and the socket path is short;
        *   its flags are `--xvfb` / `--display :N`, `--state-only`,
            `--out-dir DIR`, `--appearance light|dark` (default: light) and
            `--only NAME[,..]`. `--only` prints
            `PROJECT IDENTITY E2E PARTIAL (<n> scenarios)` instead of the
            success line;
        *   on Linux, `--state-only` still runs under `--xvfb` or
            `--display`, as `test-editor-e2e.sh` does. It skips only captures
            and real X input. The dialog opens only in a window that renders;
        *   it launches the app with `GPUI_X11_SCALE_FACTOR=1`, as
            `test-changes-e2e.sh` does;
        *   its helpers come from the sibling scripts:
            *   `wait_for` from `Scripts/Tests/test-changes-e2e.sh` (line 93);
            *   `click_at`, the helper with window-relative coordinates, from
                `Scripts/Tests/test-editor-e2e.sh` (lines 857–864). The
                coordinates come from the read verb's `fieldBounds` and
                `pathBounds`;
            *   `ctl`, `capture` and `expect_clipboard` from
                `Scripts/Tests/test-chat-session-panel-e2e.sh`;
        *   it reads the stored rows with a read-only Python `sqlite3`
            query on the `project` table, as
            `Scripts/Tests/test-sirioctl-e2e.sh` does;
        *   a display step is skipped under `--state-only`, which is how the
            script runs on macOS. A display step needs real keys, a window or
            a capture.
    *   **The git wrapper.** Before each launch, the script:
        1.  exports an absolute `XDG_DATA_HOME=$WORK/<scenario>/xdg`;
        2.  creates `$XDG_DATA_HOME/Sirio/bin-debug` with mode 755, and fails
            with a message unless `stat` shows a real directory, owned by the
            user and not writable by group or others (`safe_path_prefix`);
        3.  resolves the real git by absolute path, and writes into that
            directory a wrapper named `git` that calls the real git by its
            absolute path.

        This works because `bin-debug` is the debug build's own copy
        directory (`sirioctl_install_dir` in
        `rust/crates/sirio/src/cli_install.rs`), and `sirio_env::command` puts
        it first on the `PATH` that Sirio's git calls use. The script does not
        set `SIRIO_CLI_AS_RELEASE`, because with it a debug build keeps its
        copy in `bin` instead.

        The wrapper looks for its flag files in `$WORK/<scenario>/`. It logs
        every call to `$WORK/<scenario>/git-calls.log`, then:
        *   when the file `flood-clone` exists and `$1` is `clone`, writes 11
            MiB of progress lines to stderr, more than the 10 MiB cap
            (`DEFAULT_OUTPUT_LIMIT_BYTES` in `rust/crates/sirio_git/src/git.rs`),
            and runs `exec` on the real git;
        *   when the file `slow-clone` exists and `$1` is `clone`, sleeps 20 s
            and runs `exec` on the real git;
        *   when the file `slow-discovery` exists and its arguments begin with
            `worktree list`, it writes `SLOW-DISCOVERY-START` to the log,
            sleeps 3 s, writes `SLOW-DISCOVERY-END` and runs `exec` on the
            real git (discovery runs `git worktree list --porcelain`, within
            a 10 s budget);
        *   otherwise runs `exec` on the real git.

        The flag files are created by the scenario that needs them.
    *   **Shortcuts.** The display steps run on Linux. The script sends
        Ctrl+Shift+S for the sidebar, Ctrl+T for a new tab, Ctrl+Shift+W for
        close tab and Ctrl+Shift+P for the palette. Windows binds the
        sidebar to Ctrl+Shift+D, and macOS uses Cmd for the others.
    *   **Scenarios.** Each scenario checks some of: the dialog through the
        read verb, the result through `project list --json`, and the stored
        row. Checks are written as `key=value`, and "shows" and "at once" mean
        what Terms of the plan say they mean.
        Scenarios marked [display] are skipped with `--state-only`:
        1.  **`open_git` [display].** Fixtures: git repositories `client/app`
            and `server/app`.
            1.  Add `client/app` with the path verb.
            2.  Skip with Esc, using a real key.
            3.  Add `server/app` with the path verb. The read verb shows
                `focus=field`, `warning=on` and `nameDraft=app`.
            4.  Capture it.
            5.  Type `server` with xdotool. The name is preselected, so the
                typing replaces it.
            6.  Press Return.
            7.  The read verb shows `open=false` and `lastOutcome=saved`.
            8.  `project list --json` shows `displayName=server` at once (M2)
                and `color=coral` (§4).
            9.  The stored row of `server/app` has `default_worktree_base IS
                NULL` and `worktree_location_override IS NULL` (Goal 3).
        2.  **`skip_matches_socket`.** Fixtures: git repositories `skip/a`,
            `skip/b` and `skip/c`.
            1.  Add `skip/a` with the path verb, then `act skip`.
            2.  Add `skip/b` through `sirioctl project add`, without a label.
            3.  The two rows' `color_hex`, `display_name`, `icon_kind` and
                `icon_value` are equal.
            4.  [display] Open a terminal tab with `surface terminal open`,
                which takes focus. Add a third repository, `skip/c`, with the
                path verb, then `act skip`. The read verb shows
                `focus=terminal`: Ely's `give_back` returned focus to it.
        3.  **`save_unchanged` [display].** Fixtures: git repositories
            `unchanged/a` and `unchanged/b`.
            1.  Add `unchanged/a` through `sirioctl project add`, without a
                label.
            2.  Add `unchanged/b` with the path verb and press Return without
                edits.
            3.  The read verb shows `lastOutcome=unchanged`, and the two rows'
                `color_hex`, `display_name`, `icon_kind` and `icon_value` are
                equal.
        4.  **`create_enter_release` [display].** No fixtures: Create makes
            its own folder under `$SIRIO_PROJECTS_DIR`.
            1.  Run `xset r off`, so that the held key does not repeat. The
                script's trap runs `xset r on`.
            2.  Run the add verb with `kind=create` and `value=made`, without
                `submit`.
            3.  `xdotool keydown Return`.
            4.  The read verb shows `open=true`.
            5.  `xdotool keyup Return`.
            6.  The read verb shows `open=true`, `focus=field` and
                `nameDraft=made`, and the stored row has
                `display_name IS NULL`.
            7.  Type `made here` and press Return. The read verb shows
                `lastOutcome=saved`, and `project list --json` shows
                `displayName=made here`.
        5.  **`clone_async` [display].** Fixture: a bare repository
            `origin/lib`.
            1.  Run the add verb with `kind=clone`, the local path of
                `origin/lib` as `value`, and `submit`.
            2.  The read verb shows `open=true`, `origin=cloned` and
                `focus=dialog`.
            3.  Type `ls` and press Return. `nameDraft` is unchanged and
                `open` is still `true`.
            4.  `click_at` the field, press Ctrl+A, type `lib copy` and press
                Return. The read verb shows `lastOutcome=saved`, and
                `project list --json` shows `displayName=lib copy`.
            5.  The read verb shows `focus=root`: the form's field is gone, so
                F-SID-19 moved focus to `root_focus`. Capture the sidebar.
        6.  **`clone_truncated` [display].** Fixture: a bare repository
            `origin/lib`.
            1.  Create the file `flood-clone`, so that the git wrapper floods
                stderr.
            2.  Run the add verb with `kind=clone`, `submit` and the local
                path of `origin/lib`. The read verb shows `notice=on`.
            3.  The read verb shows `projectForm=none`.
            4.  Capture.
        7.  **`non_git` [display].** Fixtures: plain folders `plain/cancel`,
            `plain/notes` and `plain/init`, none of them a git repository.
            1.  Run the path verb on `plain/cancel` with `answer=2`
                (Cancel). The read verb shows `open=false`, and
                `project list --json` has no new project.
            2.  Run the path verb on `plain/notes` with `answer=1` (Add
                without Git). The read verb shows `origin=added-without-git`
                and `focus=field`.
            3.  `act skip`.
            4.  Run the path verb on `plain/init` with `answer=0`
                (Initialize Git). The read verb shows `open=true`,
                `origin=initialized-git` and `focus=dialog`, and
                `plain/init/.git` exists.
            5.  `act skip`.
        8.  **`sidebar_hidden` [display].** No fixtures.
            1.  Toggle the sidebar off with its real shortcut. The read verb
                shows `sidebarVisible=false`.
            2.  Run the add verb with `kind=create`, `value=hidden` and
                `submit`. The read verb shows `open=true`. Capture the dialog
                visible. Type a name and press Return.
            3.  After the save, `sidebarVisible=false` and `focus=root`: the
                form's field is gone, so F-SID-19 moved focus to
                `root_focus`.
        9.  **`over_settings` [display].** Fixture: a git repository
            `settings/app`.
            1.  Run `surface settings open`, then add `settings/app` with the
                path verb.
            2.  Capture the dialog over the Settings screen.
            3.  Press Esc. The read verb shows `open=false`, and
                `surface settings read` shows the Settings screen still open.
        10. **`shortcuts_blocked` [display].** Fixture: a git repository
            `keys/app`. Precondition: at least one tab is open, so the script
            opens one with `surface terminal open --cmd 'sleep 600'`.
            1.  Add `keys/app` with the path verb.
            2.  Press Ctrl+T, Ctrl+Shift+P and Ctrl+Shift+W.
            3.  `surface tabs read` shows the same tab count, and the read
                verb shows `paletteOpen=false` and `open=true`.

            This scenario has no banner steps. No socket verb raises a root
            modal, so the gating of the root modals is proved by the gpui test
            of file 12, "the other root modals".
        11. **`emoji_esc` [display].** Fixture: a git repository `emoji/app`.
            1.  Add it with the path verb, then run `act open-emoji-grid`.
            2.  Press Esc. The read verb shows `open=true`.
            3.  A second Esc closes the dialog. The read verb shows
                `lastOutcome=skipped`.
        12. **`path_selection_esc` [display].** Fixture: a git repository
            `path/app`.
            1.  Add it with the path verb. Double-click inside `pathBounds`
                (`xdotool click --repeat 2`). The read verb shows
                `focus=root`.
            2.  Press Esc. The read verb shows `open=false` and
                `lastOutcome=skipped`.
        13. **`queue`.** Fixtures: git repositories `queue/one` and
            `queue/two`.
            1.  Run the path verb on both, back to back. The read verb shows
                `open=true`, `queue=1` and the first project's id.
            2.  `act skip`. The read verb shows the second project's id.
        14. **`duplicate`.** Fixture: a git repository `dup/app`.
            1.  Run `sirioctl project add dup/app --display-name server`. The
                read verb shows `open=false`.
            2.  Run the path verb on `dup/app` again. The read verb shows
                `open=false` and `toast` equal to
                "already tracked or nested: <the absolute path of dup/app>".
            3.  `project list --json` still shows `displayName=server`.
        15. **`refresh_then_relaunch` [display].** Fixture: a git repository
            `refresh/app`.
            The focus refresh covers only the project that holds the current
            worktree's `working_directory`
            (`refresh_project_for_path_in_background`). So the scenario first
            makes `refresh/app` the current project.
            1.  Add `refresh/app` with the path verb.
            2.  Run `sirioctl select-workspace --workspace <refresh/app root>`.
                Wait until its own rescan has finished: its `worktree list`
                line appears in `git-calls.log`.
            3.  While Sirio still has focus, type `refreshed` in the field.
                The draft must differ from the root name, or `act save` would
                be `Unchanged` and write nothing.
            4.  Create the file `slow-discovery`.
            5.  Build the `surfaces_probe` example of `sirio_ui` and start
                it, so that it takes focus from Sirio.
            6.  Refocus Sirio's window. That starts the focus refresh, whose
                discovery the wrapper holds for 3 s. This is the window in
                which #684 keeps catalog writes.
            7.  While discovery sleeps, run `act save`. When it replies, the
                script appends a marker line (`SAVE-REPLIED`) to
                `git-calls.log`.
            8.  `git-calls.log` shows `SLOW-DISCOVERY-START`, then
                `SAVE-REPLIED`, then `SLOW-DISCOVERY-END`, in that order. Once
                the refresh lands, `project list --json` still shows
                `displayName=refreshed`.
            9.  Remove `slow-discovery`. Quit and relaunch on the same
                database. The label is in `project list --json` and in the
                sidebar capture.

            The in-process proof that the merge keeps a write made during a
            refresh is the unit test
            `catalog_writes_made_while_a_focus_refresh_runs_survive_its_landing`
            in `main.rs`. This scenario proves it in the running app.
        16. **`picker_modes` [display].** Fixture: a git repository
            `picker/app`.
            1.  Add it with the path verb. Run `act mode icon`, `act mode
                emoji` and `act mode avatar`, with one capture each. The
                script is run twice, with `--appearance light` and
                `--appearance dark`.
            2.  The Avatar capture shows the caption, and its source rows stay
                inside the card. Enzo judges this by eye from the captures.
        17. **`over_open_form` [display].** Fixtures: a bare repository
            `origin/lib` and a git repository `fresh/two`.
            1.  Create the file `slow-clone`. Run the add verb with
                `kind=clone`, `submit` and the local path of `origin/lib`, so
                that a clone is running.
            2.  Add `fresh/two` with the path verb.
            3.  Press Esc once.
            4.  The read verb shows `open=false` and `projectForm=clone`: the
                first Esc skipped the dialog, and the form behind the scrim
                stayed open.
        18. **`settings_tab` [display].** Fixture: a git repository
            `tab/app`.
            1.  Add it through `sirioctl project add`. Run
                `sirioctl select-workspace --workspace <tab/app root>`,
                because `surface open-list open` needs a selected worktree
                (`control_open_list` answers "no worktree is selected"
                otherwise).
            2.  Open its Project Settings tab with
                `surface open-list open --entry project-settings`. Read
                `projectSettingsLabelBounds` (file 9), click the label field
                with `click_at`, type a label and check `project list --json`
                at once.
            3.  Clear the field with as many BackSpace presses as the step-2
                label has characters, then type the root name. The stored
                row has `display_name IS NULL`.
        19. **`copy_field` [display].** Fixture: a git repository `copy/app`.
            1.  Add it with the path verb. Press Ctrl+A in the field, then
                Ctrl+C.
            2.  `expect_clipboard` finds the field's text, and the read verb
                shows `open=true`.
        20. **`save_error` [display].** Fixtures: git repositories `err/one`
            and `err/two`. The app starts with
            `SIRIO_DEBUG_FAIL_CATALOG_WRITES=1`.
            1.  Add both with the path verb, back to back.
            2.  Type a label in the first dialog and run `act save`.
            3.  The read verb shows a non-empty `toast`, `open=false`,
                `queue=1` and `lastOutcome=error`.
            4.  After the toast clears, the read verb shows `open=true` for
                the second project.
        21. **`trace_privacy` [display].** Fixtures: git repositories `trace/one` and
            `trace/two`. The app starts with `SIRIO_PERF_TRACE=<file>`.
            1.  Add `trace/one` with the path verb, type a label and run
                `act save`. Add `trace/two` and run `act skip`.
            2.  A grep of the trace file and of this scenario's `app.log`
                finds no label, no path and no project id.
        22. **`icon_save` [display].** Fixtures: git repositories `icon/app`
            and `icon/two`.
            1.  Add it with the path verb. Run `act pick-glyph terminal`, then
                `act save`. The stored row has `icon_kind=icon` and
                `icon_value=terminal`, and `default_worktree_base IS NULL`
                and `worktree_location_override IS NULL` (Goal 3).
            2.  Add a second repository, `icon/two`. Run
                `act pick-glyph terminal`, then `act pick-glyph folder` (the
                original icon), then `act save`. The read verb shows
                `lastOutcome=unchanged`.
    *   **The artifact.** Under `--out-dir`:
        *   `transcript.log`, every `sirioctl` call and its reply, prefixed by
            the scenario name;
        *   `<scenario>/app.log`;
        *   `<scenario>/projects.tsv`, the final `project` table of that
            scenario;
        *   `<scenario>/git-calls.log`;
        *   `frames/<scenario>-*.png`, the PID-matched captures.

        The last line is
        `PROJECT IDENTITY E2E OK (<n> scenarios, appearance <a>, state-only <0|1>)`.

#### 19. `Scripts/Tests/test-sirioctl-e2e.sh` (Modify)

*   **Type / Scope:** the existing `sirioctl` E2E, which also runs on macOS
    with `--state-only`.
*   **Proposed Changes [8]:** A new `case_14` after `case_13`, registered with
    `run_case 14 "project add --display-name: the label, no dialog, a duplicate" case_14`.
    It opens its own world, `world_open w8`, which makes the repository
    `$W_REPO`, and makes a second one, `<repo2>`, with
    `repo_init "$W/repo2"`. Every step works with `--state-only`:
    1.  `project add $W_REPO --display-name 'client app' --json` replies
        `added=true`, `displayNameApplied=true` and
        `displayName=client app`.
    2.  `project list --json` shows that `displayName` at once.
    3.  `surface project-identity read` shows `open=false` and `queue=0`.
        That is the proof that a socket add opens no dialog.
    4.  The same command again replies `added=false` and
        `displayNameApplied=false`, and the label is unchanged.
    5.  `--display-name` with no value, and `--display-name=`, exit 1.
    6.  On `<repo2>`, a label equal to its folder name (`repo2`) replies
        `added=true`, `displayNameApplied=true` and `displayName=""`. The
        stored row has `display_name IS NULL`.

#### 20. `docs/agents/e2e-tests.md` (Modify)

*   **Type / Scope:** the E2E catalogue.
*   **Proposed Changes [8]:**
    *   A new entry for `test-project-identity-e2e.sh`: what it proves, its
        flags, its artifact and its success line.
    *   The `test-sirioctl-e2e.sh` entry names the `--display-name` case.

#### 21. `docs/agents/architecture/control-socket.md` (Modify)

*   **Type / Scope:** the control-socket notes.
*   **Proposed Changes:**
    *   **The debug methods [7].** The sentence that lists them names the
        three new verbs: `surface.project_add`, `surface.project_identity.read` and
        `surface.project_identity.act`.
    *   **`displayName` [4].** One sentence covers four points:
        *   `project.add` takes an optional `displayName`;
        *   the app answers with `displayNameApplied`;
        *   a path that is not added (already tracked, inside a tracked
            project, or containing one) ignores the label;
        *   the label is not a selector.

#### 22. `rust/Cargo.toml` and `rust/Cargo.lock` (Modify, conditional)

*   **Type / Scope:** the workspace version.
*   **Proposed Changes [9]:**
    *   **Version.** The feature is a `feat:`. If it is the first `feat:` of
        the 0.38.1 cycle on `main` when this pull request merges, it moves
        `[workspace.package] version` to 0.39.0 through
        `Scripts/set-workspace-version.sh`. If another `feat:` landed first,
        the version stays where it is.
    *   **How to check.** Just before merging, run
        `git fetch origin && git log --oneline v0.38.0..origin/main | grep -E '^[0-9a-f]+ feat'`.
        That range leaves out this branch, so its own `feat:` commits do not
        count. If the list is empty, this delivery is the cycle's first
        `feat:`: keep commit 9. Otherwise, drop it. The pull request is
        squash-merged, so `main` gets one `feat:` commit.
    *   **`Cargo.lock`.** `Scripts/set-workspace-version.sh` is the only
        writer of the version. The first `cargo build` after it rewrites the
        workspace crates' entries in `Cargo.lock`.

#### 23. `.github/workflows/pr.yml` (Modify)

*   **Type / Scope:** the pull request workflow.
*   **Proposed Changes [8]:**
    *   A step "Project identity E2E" after the sirioctl E2E step, in the
        same job and in the same shape, with its own Xvfb:
        `run: Scripts/Tests/test-project-identity-e2e.sh --xvfb --out-dir project-identity-e2e`,
        followed by an `actions/upload-artifact@v4` step for
        `project-identity-e2e`, with `if: success() || failure()` on both.
    *   The job's graphics environment is already there:
        `.github/workflows/pr.yml` (line 182) installs
        `mesa-vulkan-drivers` (lavapipe) in the step "Install the display
        tools for the host sessions E2E".
    *   That apt step also gains the packages the script needs and the job
        does not install: `x11-xserver-utils` (for `xset`, scenario 4),
        `python3-gi` and `gir1.2-gtk-3.0` (for `expect_clipboard`, scenario
        19).

#### 24. `Scripts/Tests/test-pr-workflow.sh` (Modify)

*   **Type / Scope:** the guard of the workflow's shape.
*   **Proposed Changes [8]:**
    *   A `grep -qE` on the `run:` line of the new step, in the form of its
        neighbours, failing with "pr.yml must run the project identity E2E
        under Xvfb".
    *   Assertions that the display-tools apt step installs
        `x11-xserver-utils`, `python3-gi` and `gir1.2-gtk-3.0`.

### Verification Plan

*   **Compilation and build.**
    *   `cd rust && cargo build -p sirio_ui && cargo build -p sirio && cargo build -p sirio_control --bins`.
    *   `cargo check --workspace --all-targets`, because a changed enum
        variant (`SidebarEvent::AddProject`) is matched in `sirio_ui`'s tests
        and in `main.rs`, which other crates' test targets compile.
    *   `cargo clippy -p sirio` on its non-test target shows no `dead_code`
        warning for `ProjectCatalog::add`, which is `#[cfg(test)]`.
    *   Zig 0.15.2 must be on `PATH`.
*   **Unit tests.** Three sets are written first: `normalize_display_name`'s
    `#[test]`, the five `control_integration.rs` tests and the `#[gpui::test]`
    of the `needs_focus` gating (file 12, "the other root modals"), which
    proves that a root modal raised while the dialog is open does not take
    focus until the dialog closes. Then:
    *   `cargo nextest run -p sirio_ui project_identity`;
    *   `cargo nextest run -p sirio_control`;
    *   `cargo nextest run -p sirio` and `cargo nextest run -p sirio_ui`,
        to show that the existing suites still pass. These run with
        `TMPDIR=~/.cache/sirio-sdd/tmp`.
        <!-- TODO(F14): the reason for this TMPDIR is not recorded. -->

    Count the selected tests. A filter that selects nothing prints a green
    "0 passed". The first command must select at least one test, and the
    second must list the five new tests and
    `sirioctl_project_add_sends_the_path_and_reports_identity`. The run of
    `cargo nextest run -p sirio` must list the new `#[gpui::test]`.
*   **Static analysis.** `cargo clippy -p sirio_ui -p sirio -p sirio_control --all-targets`
    with no new warnings. The workspace is not fmt-clean, so `cargo fmt` is
    not run on a crate, and `rustfmt` is not run on a file that declares
    modules, because it recurses into them. Formatting is kept by hand in the
    changed hunks.
*   **E2E.**
    *   `Scripts/Tests/test-project-identity-e2e.sh --xvfb --out-dir <dir>`
        prints `PROJECT IDENTITY E2E OK`, once with `--appearance light` and
        once with `--appearance dark`.
    *   `Scripts/Tests/test-sirioctl-e2e.sh --xvfb --out-dir <dir>` prints
        `SIRIOCTL E2E OK`.
    *   `Scripts/Tests/test-update-ui-e2e.sh --state-only` and
        `Scripts/Tests/test-changes-e2e.sh --state-only` still pass. They add
        projects through the socket and must not meet a dialog.
    *   `Scripts/Tests/test-pr-workflow.sh` passes with the new step.
*   **The full gate.** `Scripts/ci.sh` prints `CI OK`. It runs only when
    Enzo asks for it, before the pull request.
*   **Manual checks before release.**
    *   **macOS, the reference platform:**
        *   the dialog after each of the five add paths;
        *   the native non-git prompt followed by the dialog;
        *   focus after the native non-git prompt closes;
        *   Enter held through the folder chooser or the non-git prompt, then
            released over the dialog: the dialog stays open;
        *   focus after the PNG file chooser closes;
        *   IME input in the name field;
        *   Esc over the Settings screen.
    *   **Windows:** the same, plus the first compile of the `cfg(windows)`
        arms in the release job.
    *   **Translucency:** with translucency on, check by eye the tint and
        contrast of the picker's surfaces (`ely.surface`) inside the opaque
        card, and that the dialog stays opaque.

--------------------------------------------------------------------------------

<!-- Provenance: Indicates initial structural drafting via EGM (Elephant-Goldfish Model). -->

*Drafted with EGM (Elephant-Goldfish Model)*
