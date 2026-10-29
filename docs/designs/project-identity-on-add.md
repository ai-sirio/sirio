# Name and icon when a project is added

* **Author:** epalmisano
* **Date:** 2026-10-28

## Overview

**The problem.** Two projects often end up with the same name in the sidebar. The default name is the last component of the folder path: `~/work/client/app` and `~/work/server/app` both become `app`.

**The proposal.** Every project added from the UI goes through one of four paths: Open folder, Clone, Create, and the center's empty state, which reuses Open folder. After each one:
- the project enters the catalog right away, as it does today;
- then a modal Ely `Dialog` opens with the label field (`display_name`), prefilled with the folder name, and the full `ProjectIconPicker` from the Project Settings tab;
- Confirm saves; Esc or closing leaves the default values.

The socket method `project.add`, used by agents through `sirioctl`, never opens the dialog. It receives an optional parameter for the label, and `sirioctl` a matching flag; the icon stays the default one. The work starts on Ely after the "bezel-off" stack is merged.

**What the review found.** Six lenses, all completed. The code already has a single point where the four UI paths converge (`SidebarEvent::AddProject` → `add_project`) and a separate path for the socket (`control_add_project`). So the hook point exists, and excluding the socket costs no code. The concept, however, rests on three premises that the code contradicts:
- **"Saves like the Project Settings tab".** The tab saves on every keystroke and has no Confirm. Only the host function `update_project_settings` can be shared, not the view.
- **"The label field the tab already contains".** It is a hand-written field, with no paste, cursor or selection. It cannot be reused.
- **"The app has no HTTP client".** This is false: `ureq` is a dependency of four crates. What is true is that no avatar download is wired up. The decision on the globe stays valid; only the reasoning needs correcting.

Three blockers remain to be resolved in the draft:
1. Who opens the dialog and for which project.
2. Where the dialog lives.
3. How it is tested end to end.

On top of these there are a number of keyboard and focus problems inside Ely `Dialog`.

## Analysis

Reference conventions:
- `gr:` is the target base, branch `refactor/bezel-off-e` at commit `6fa07d19` (top of the bezel-off PR stack #670, #671, #672, #674 and PR E).
- `main:` is `main` at commit `d5e29172`.
- Paths are relative to `rust/crates/`, except those starting with `rust/vendor/`, `docs/` or `Scripts/`.
- `ely:` abbreviates `gr:rust/vendor/ely-gpui-component/src/`.
- The `main.rs` line numbers differ between the two bases: on `main:` `add_project` is at line 10186 and `update_project_settings` at 10207; on `gr:` at 10223 and 10244. The draft must cite by symbol, because base E can still be rebased.
- Each point lists the lenses that raised it. "[synthesis]" means the synthesizer re-read the cited lines.

### 🔴 Blocker

**B1. The dialog does not know which project to edit, and opening it in the wrong place overwrites an existing project.**
*Lenses: design, structural, codebase-details, implementation-readiness, comprehension, zoom-out. [synthesis]*

What the code says:
- **The event carries only a path.** The sidebar emits `SidebarEvent::AddProject(PathBuf)` before knowing how it went (gr:sirio_ui/src/sidebar/mod.rs:1793, 1809, 1828, 1881, 1899).
- **Nobody returns the id.** `add_project` returns nothing (gr:sirio/src/main.rs:10223-10242). `ProjectCatalog::add` returns `Result<bool, String>`, with no id (gr:sirio/src/session.rs:830-852).
- **`Ok(false)` covers three different cases:** project already tracked, nested in an existing project, or containing one (session.rs:839-845).
- **Saving replaces the whole record.** It includes `default_worktree_base` and `worktree_location_override` (gr:sirio/src/main.rs:10257-10264; session.rs:771-781, `self.settings.insert`). A dialog opened on a duplicate would therefore erase the label, the icon and the worktree defaults of an existing project. The same applies to the socket parameter applied with `added=false`.
- **The prefill would be wrong** if the chosen folder were used. The catalog root is the primary checkout (`catalog_root`, session.rs:1065-1072), and the name comes from that root (session.rs:974-981). A user who picks a linked worktree would see a name different from the one that gets saved.

Requirement:
- `ProjectCatalog::add`, or a shared wrapper, returns the outcome with the id, for example `Added { id }` / `AlreadyTracked`.
- The dialog opens only in the host, only on a real add.
- The starting data comes from `Sidebar::project_settings_seed(id)` (gr:sirio_ui/src/sidebar/mod.rs:1590-1648), read after `refresh_sidebar`, which is synchronous (main.rs:7636-7665).
- On `Ok(false)` and `Err` the current toast stays and no dialog opens.

**B2. A dialog hosted by the Sidebar is not drawn when the sidebar is hidden.**
*Lenses: structural (Blocker), design, codebase-details. Zoom-out proposed the opposite. [synthesis]*

- The sidebar enters the tree only with `sidebar_visible` (gr:sirio/src/main.rs:23524-23544).
- The "Add Project" button in the center's empty state still calls `sidebar.start_open_project` (main.rs:23204-23210).
- With the sidebar hidden, the project would be added and the dialog would stay pending but invisible.
- The same holds for the app's Settings screen, which replaces the columns. It can be opened from the socket with `surface.settings.open` (a release method, gr:sirio_control/src/served.rs:160) while a clone is finishing.
- The code already has the rule for this case: "Root-level transient surfaces must outlive whichever main content branch is mounted" (main.rs:24793-24807). The precedent is the update card: component in `sirio_ui`, state and routing in `main.rs`, drawn by `render_update_card` (main.rs:24811-24822).
- Zoom-out recommended the Sidebar because the Ely backdrop covers the whole window (ely:primitives/backdrop.rs). That holds only if the owning view is drawn, so the proposal does not stand.

Requirement:
- The dialog is its own entity in `sirio_ui`, next to `project_identity.rs`. It owns its own `TextInput` and the picker, calls `crate::ely::sync_theme_if_changed` in render and emits `Confirmed { .. }` / `Dismissed`. `sync_theme_if_changed` is `pub(crate)` (gr:sirio_ui/src/ely.rs:53), so the dialog cannot live in `main.rs`.
- `main.rs` keeps only an `Option<Entity<..>>`, a subscription, an entry in `render_global_overlays` and the opening in the `add_project` branch.
- `main.rs` is about 49,900 lines: add the wiring, not the logic.

**B3. No path today reaches the dialog from a script, so the feature cannot be tested end to end.**
*Lenses: codebase-details (Blocker), implementation-readiness, structural, zoom-out. [synthesis]*

Why none of the existing paths works:
- `project.add` by decision does not open the dialog.
- Open folder goes through the native picker `cx.prompt_for_paths` (gr:sirio_ui/src/sidebar/mod.rs:1755-1760). On Linux this is an XDG portal call, which has already ended up on an operator's desktop (gr:Scripts/wayland-drive.sh:350-366, reported by readiness).
- Clone and Create have no socket methods (gr:sirio_control/src/served.rs:111-194).

Requirement:
- **Debug-only verbs in `DEBUG_METHODS`** (served.rs:68-107), never in the release list. One feeds a path into `confirm_add_project`, skipping only the picker and so also covering the non-git prompt. One reads the dialog state: open or closed, id, name draft, icon. One confirms or closes, distinguishing Esc from the scrim.
- **Precedents:** `surface.change_request.act` merge-open/merge-confirm (gr:sirio_ui/src/change_request_tab/merge.rs:418-439) and `surface.changes.confirm`.
- **Every new verb also has** a branch in `request_for_control_method`, otherwise the `cfg(windows)` test panics (main.rs:44038-44187, panic at 44181-44183), plus a usage line marked "(debug builds)" in `sirioctl` and a mention in `docs/agents/architecture/control-socket.md`.
- **The script:** git cases, non-git ("Add without Git" with the fallback prompt in a gpui window on Linux), duplicate (toast and no dialog), Create, Clone from a local repository (not verified that a local URL works), relaunch with a persisted label, real typing with xdotool and a check of where the keys go after the dialog closes.
- **Checking the result** goes through `sirioctl project list --json` (requires M2), a DB read and PID-matched captures. The script ends with a success line.
- **The socket path** goes in `Scripts/Tests/test-sirioctl-e2e.sh`, which already covers "the flags sirioctl offers" (docs/agents/e2e-tests.md:41-48). It must also check that the dialog is closed after `project add`.
- **Known limit:** on macOS the scripts run `--state-only`, so interaction with the dialog on the reference platform remains a manual check.

### 🟡 Major

**M1. Copying the behavior of the Project Settings tab breaks "Esc = default values".**
*Lenses: comprehension, structural, codebase-details, design, zoom-out.*

- The tab emits `Changed` on every keystroke and every icon choice (gr:sirio_ui/src/sidebar/project_settings.rs:174-182, 201-211), and the host saves every event (main.rs:16195-16196).
- The picker notifies on every click right away (gr:sirio_ui/src/project_identity.rs:414-423).
- Wiring `on_change` to the save would write before Confirm.

Requirement:
- The dialog keeps local drafts and writes once, on Confirm.
- It reads `ProjectIconPicker::value()` (project_identity.rs:400-402).
- It builds the update starting from `project_catalog.project_settings(id)` read at that moment, so the worktree defaults pass through intact. The comment at main.rs:10246-10256 assumes the sender has already loaded every draft.
- Esc, scrim and close write nothing.
- "Same path as the tab" must be written in the draft as "same host function", not "same view".

**M2. After a save, `project.list` still reports the old `displayName`. The defect already exists today.**
*Lenses: design, zoom-out, structural, codebase-details, implementation-readiness. [synthesis]*

- `update_project_settings` calls `schedule_catalog`, `publish_worktree_lookup` and `refresh_sidebar`, but not `sync_control_state` (gr:sirio/src/main.rs:10244-10278).
- `project.list` reads a snapshot built by `ControlState::from_catalog` (main.rs:1801-1811, 1880-1889), which is rebuilt only in `sync_control_state` (main.rs:7667-7684).
- The most natural E2E check would therefore read stale data.
- The same applies to the socket parameter, if applied after the synchronization that `control_add_project` performs right after the add (main.rs:10823-10827).

Requirement:
- A call to `sync_control_state` in `update_project_settings`. It also fixes the Settings tab.
- The socket parameter is applied before the synchronization, with a single `schedule_catalog`.

**M3. Sirio cannot configure the initial focus, Enter and the scrim click of Ely `Dialog`.**
*Lenses: all. [synthesis]*

- **The hooks are crate-private.** `Dialog::on_enter`, `held` and `focus_first` are `pub(crate)` (ely:overlays/dialog.rs:72-94).
- **On first render the dialog takes focus on its own handle** (dialog.rs:105-111; ely:primitives/focus.rs:147-153). Without `focus_first` the cursor is not in the name field.
- **A click on the scrim closes the dialog** by default (dialog.rs:295-301). A careless click throws away a name that was already typed.
- **`PromptDialog`** already does half the work (focus, preselected text, Enter), but it has no slot for other content and cannot host the picker (ely:overlays/dialogs.rs:184-326).
- **Focusing the field before the first render does not help:** `take_focus` would register the field as the "previous" focus to restore.

Three ways forward:
- (a) Make `focus_first`, `on_enter` and possibly `held` public, noting it in `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`, which today has no entry on Dialog.
- (b) Give `PromptDialog` a content slot.
- (c) Workaround on the Sirio side: Enter through `InputEvent::Submit`, as `CloneForm` does (gr:sirio_ui/src/project_forms.rs:162-163), and focus moved one frame after the first render. That this works with Ely's focus-taking is not verified.

**M4. Tab does nothing inside the dialog, and the picker is mouse-only.**
*Lenses: codebase-details, comprehension. The two lenses disagreed; [synthesis] decided.*

- **Comprehension** claimed that the `FocusScope` trap would cycle between the field and the buttons.
- **The code:** `FocusScope` reacts to the `ely::FocusNext` action (ely:primitives/focus.rs:82-83). In Sirio no key is bound to that action: there is only a dispatch from code in `gr:sirio_ui/src/file_view/status_actions.rs:662, 711`. `LOCAL-CHANGES.md` (lines 27-29) says explicitly that `init_chat` "registers no global Tab binding".
- **The picker's handles** are created without `.tab_stop(true)` (project_identity.rs:341-352), and its controls are clickable divs.
- **The consequence goes beyond this feature:** it applies to every Ely dialog in Sirio.

Requirement: the draft says explicitly what the keyboard path is: name, then Enter or Esc, with the picker mouse-only. Alternatively, bind Tab to `FocusNext`, but this is a cross-cutting change (see question 7).

**M5. Esc reaches other handlers first, and the first Esc can close the wrong thing.**
*Lenses: design, structural, comprehension, codebase-details, zoom-out, implementation-readiness. [synthesis]*

- **(a) Esc in the picker's emoji search does not stop there.** It closes the grid and returns without `cx.stop_propagation()` (project_identity.rs:513-516). Bubbling up, it reaches the dialog's Esc handler (dialog.rs:253-258), which closes the dialog and discards the name and icon. The bubbling order is inferred from the code, not executed.
- **(b) The shell intercepts Esc before the dialog.** `handle_root_key_down` runs in the capture phase, so it runs before the dialog's handler; if a Clone or Create form is open, Esc closes that and stops (gr:sirio/src/main.rs:24225-24231). A clone with truncated output deliberately leaves the form open (gr:sirio_ui/src/sidebar/mod.rs:1864-1866, 1878-1881). Form and dialog are therefore open together, and the first Esc closes the hidden form.
- **(c) Shell shortcuts act behind the scrim.** Tab closing (`close_tab_fallback_fires`, main.rs:24237-24246) and the palette (main.rs:24247-24255) stay active. The project has already solved the same problem for its own `pending_pane_close` modal (main.rs:24204-24219).

Requirement:
- The picker consumes the Esc presses it handles, and the fix belongs in the picker. An E2E step is needed: Esc in the emoji search closes only the grid.
- An "a modal is open" predicate in the root handler: shell shortcuts stop, but Esc and Enter reach the dialog and Ctrl+C is never swallowed. The predicate is preferable to a fourth special branch.
- When the dialog opens, the Clone or Create form closes (see question 2 for the truncation notice).

**M6. The tab's label field cannot be reused, and the normalization rule is spread across several places.**
*Lenses: comprehension, structural, zoom-out, codebase-details, implementation-readiness.*

- **The field is hand-written.** `push_keystroke` (gr:sirio_ui/src/sidebar/project_settings.rs:184-211) appends characters or removes the last one; the render is at 641-691. It has no paste, cursor, selection or IME. With a prefilled name the user would have to delete it one character at a time.
- **Ely `TextInput::detached` already has everything.** `CreateForm` uses it (project_forms.rs:489). It does not require a `Window`, which the `add_project` branch does not have (main.rs:6608-6611, 10105). It has a public `select()` (ely:forms/text/mod.rs:356).
- **The rule "trim, empty → `None`" is written several times:** project_settings.rs:121-136, 157-160; sidebar/mod.rs:1506-1508; main.rs:5084-5087.
- **`Project::effective_name`** (gr:sirio_project/src/project.rs:57-59) does not trim and has no callers, so it must not be cited as current behavior.
- **There are no limits** on length and control characters. The sanitizer at session.rs:194 truncates to 120 characters and removes control characters.

Requirement:
- The field is an Ely `TextInput` with the name preselected.
- A single pure normalization function, used by the tab, the dialog and the socket: trim; empty → `None`; equal to the base name → `None`. The third rule follows the seed, which treats "equal" as "not set" (sidebar/mod.rs:1612-1616).
- The limits on length and control characters are to be decided (question 9).

**M7. Confirm without changes writes different data than Esc.**
*Lenses: design, codebase-details, implementation-readiness, comprehension. [synthesis]*

- **Esc** leaves `color_hex = None` and `icon_value = None` (gr:sirio/src/session.rs:693-703).
- **Confirm** with the default icon writes `color_hex = Some("coral")` (main.rs:10258) and the value `folder`, through `persisted_parts` (main.rs:10245).
- **With the prefilled name** Confirm would save `display_name = Some("app")`.
- **On screen the two cases look the same,** but `project list --json` shows different `color`, `iconValue` and `displayName`. Agents would see a label set for UI adds and empty for socket adds.

Requirement, recommended by readiness and comprehension: Confirm without changes produces the same record as Esc. Name equal to the base name → `None`; icon untouched → no icon write (question 8).

**M8. A background catalog refresh can undo an add or a Confirm that has just been made. The defect already exists today.**
*Lenses: design. [synthesis]*

The mechanism:
- `refresh_project_for_path_in_background` clones the whole catalog before starting (gr:sirio/src/main.rs:8025-8027).
- When the result arrives it replaces it wholesale: `workspace.project_catalog = catalog;` (main.rs:8042).
- Then it decides whether to save by comparing against the same old snapshot (main.rs:7942).
- An `add` or an `update_project_settings` that happened in the meantime disappears from memory. The catalog also contains the `settings` (session.rs:707-709), so the label is lost too.
- The next `schedule_catalog` writes the old memory, and `write_catalog` deletes from the DB the rows it does not find in memory (session.rs:1342-1352).

The refresh starts when the window becomes active again (main.rs:24905-24912). The feature puts writes right after three moments when this happens:
- the closing of the folder picker;
- the closing of the native non-git prompt on macOS;
- the closing of the PNG picker inside the picker (project_identity.rs:618-646).

The mechanism is verified. That the current timings make it fire is not.

Requirement: the refresh merges the result by project id, replacing only the worktrees of that project, instead of replacing the whole catalog. An E2E case must be added: PNG picker, Confirm right away, focus cycle, relaunch, check the label. Whether this work is part of the feature is question 10. Tracked as issue [#682](https://github.com/ai-sirio/sirio/issues/682).

**M9. The socket parameter: the name reads backwards and is lost silently when versions do not match.**
*Lenses: comprehension, design, codebase-details, implementation-readiness, zoom-out. [synthesis on protocol.rs, sirioctl.rs and main.rs]*

- **The name.** In `project.list`, `name` is the folder name and `displayName` the label (gr:sirio/src/main.rs:1866, 1880-1883). A `name` parameter that sets the label reads backwards. Comprehension proposes `displayName` and `--display-name`, in line with the kebab-case → camelCase conversion already in use (`maxBytes`, `agentSession`).
- **Silent loss, new `sirioctl` and old app.** The parameters are a `BTreeMap<String, String>` without `deny_unknown_fields` (gr:sirio_control/src/protocol.rs:19-27). The handler reads only `path` (main.rs:2733-2742). The old app answers `added=true` and the CLI exits with 0.
- **Silent loss, old `sirioctl` and new app.** The stable copy on the PATH is rewritten by the app at startup, in a background thread (gr:sirio/src/cli_install.rs:373-395, reported by readiness). The previous copy can reach the panes.
- **The parser.** A `--name` without a value becomes a boolean flag and is ignored (gr:sirio_control/src/bin/sirioctl.rs:256-265, reported by comprehension and codebase-details).
- **`added=false`.** Whether the name applies is undefined. Applying it would rewrite the whole record (B1).
- **Visibility.** `sirioctl project list` without `--json` prints only `name` (sirioctl.rs:532). `workspace.list` uses `project.name` (main.rs:1761). `workspace.create` selects by folder name or path, not by label (main.rs:12257-12266). An agent cannot use the label just chosen as a reference.
- **Windows.** Changing the signature of `request::project_add` breaks a `cfg(windows)` caller (main.rs:44047), which the Linux CI does not compile.

Requirement:
- The response repeats the saved label and says whether it was applied.
- The new `sirioctl` warns or exits with an error if it passed the label and the echo is missing.
- A missing or empty value gives an error.
- With `added=false` the label is ignored, and the response says so.
- The documentation says the label is not a selector.
- Update `control-socket.md`, the usage line (sirioctl.rs:130) and `test-sirioctl-e2e.sh`. `skills/sirio/SKILL.md` does not mention `project add` today.

**M10. The clone does not really run in the background, and when the dialog arrives it can steal focus from a terminal.**
*Lenses: all. [synthesis on project_forms.rs]*

- **The clone is tied to the form.** It runs in a detached `std::thread` (gr:sirio_ui/src/project_forms.rs:225-234), but the task that reads the result and emits `Cloned` belongs to the form (project_forms.rs:236-285).
- **The form closes easily:** with a click outside the card (gr:sirio_ui/src/sidebar/mod.rs:3467-3471) or with Esc even from a terminal (main.rs:24225-24231).
- **Closing the form during the clone** drops the task. The clone finishes on disk, but the project is not added and nobody is told. This is inferred from the code, not executed.
- **The reasoning for the rejection is weakened.** "Avoid an orphan clone" was the reason for discarding the deferred add, but an orphan clone already exists on this path.
- **The focus theft** happens only if the user moved the focus by keyboard while leaving the card open. In that case the dialog takes focus on the first frame (ely:primitives/focus.rs:147-153). With `focus_first` and `on_enter` active (M3), an `ls` + Enter typed straight into the terminal would become the label `ls`.
- **The same asynchronous interval** exists after "Initialize Git", which runs `git init` in the background (sidebar/mod.rs:1816-1828).

Question 1.

**M11. Several adds in quick succession: nothing today decides whether dialogs queue or replace each other.**
*Lenses: design.*

- The focus-taking state is tied to the element id (ely:primitives/focus.rs:147-153). A second dialog that reuses the same id on the next frame would inherit the state "focus already taken and returned": it would not take focus and Esc would not reach it.
- A clone or a `git init` finishing while another dialog is open can overlap.

Requirement: a FIFO queue per project id, showing only the first dialog, and one element id per project, for example `("project-identity-dialog", id)`. Never two overlapping dialogs.

**M12. The socket path is excluded only because it is a separate copy of the add code.**
*Lenses: structural, implementation-readiness.*

- `add_project` (main.rs:10224-10229) and `control_add_project` (main.rs:10823-10828) repeat the same sequence: `add`, `schedule_catalog`, `sync_control_state`, `refresh_sidebar`.
- Whoever merges them one day would open the dialog for the socket too.
- About fifteen E2E scripts add projects with `ctl project add`, for example `gr:Scripts/Tests/test-changes-e2e.sh:198` and `test-update-ui-e2e.sh:846, 942`. A modal there would take focus, and `test-update-ui-e2e.sh` (lines 969-990) proves precisely that the card does not take keys.

Requirement:
- A shared insert that returns the outcome, called by two wrappers; the dialog lives only in the UI one.
- A comment on `SidebarEvent::AddProject` (gr:sirio_ui/src/sidebar/mod.rs:446) says that every future UI path must emit that event.
- The E2E checks that `project.add` does not open the dialog.

**M13. The dialog text does not say that the project is already added and that the folder does not change.**
*Lenses: comprehension, zoom-out.*

- A normal Ely dialog has no close button (×); only the fullscreen variant draws it (dialog.rs:164-206). With only Confirm, Esc is invisible.
- A "Cancel" button would suggest that the add can be undone, which is false.
- `Dialog::detail` draws plain, non-selectable text (dialog.rs:134-140), so the path there would break the rule on text meant to be read. Today no path in this area is selectable (project_settings.rs:629; project_forms.rs:358, 641, 649).

Requirement proposed by comprehension:
- Title "Project added".
- The catalog root path, shown with `selectable_text(..)` in the body.
- The field "Name in the sidebar", with a line saying that the folder on disk keeps its name.
- The buttons **Skip** (same as Esc) and **Save** (same as Enter), never "Cancel".

Showing the full, selectable path is also the cheapest way to tell the user which `app` they are naming (zoom-out).

**M14. GitHub and favicon avatars are more misleading at add time than in Settings.**
*Lenses: comprehension, zoom-out, codebase-details. [synthesis on Cargo.toml]*

- **The picker prefills the GitHub owner from the `origin` remote** (project_identity.rs:372-381). After a clone from GitHub, "Use GitHub Avatar" is the most immediate choice, and the sidebar shows a globe (gr:sirio_ui/src/sidebar/row.rs:477-484).
- **Only the local PNG is shown as an image** (row.rs:507-516). Two projects with the same name and one avatar each therefore remain indistinguishable.
- **The reasoning "no HTTP client" is false.** `ureq` is a dependency of `sirio_update/Cargo.toml:13`, `sirio_registry/Cargo.toml:25`, `sirio_forge/Cargo.toml:23` and `sirio_diagram/Cargo.toml:17`.
- **The correct statement** is that no avatar download is wired up and gpui's `img(url)` downloads nothing. The comments at project_identity.rs:1058-1062 and row.rs:507-511 repeat the wrong statement.

Requirement:
- A caption in the Avatar tab, for example: "GitHub and favicon avatars show as a globe until Sirio can download images".
- Correct the reasoning in the draft. The decision stays.

**M15. Save errors are not visible, and the outcome of the dialog leaves no trace.**
*Lenses: codebase-details, design.*

- An error from `update_project_settings` shows only a sidebar warning, with no toast (main.rs:10274-10276). With the sidebar hidden (B2) the warning cannot be seen.
- A catalog write error goes only to stderr (session.rs:2395-2397). The label stays visible until restart, then disappears.
- Ely's `on_close` receives no reason (dialog.rs:112-119): Esc, scrim and button are indistinguishable.
- Ely's logs go through the `log` crate. No initialized logger was found in Sirio, so probably nobody reads them (not verified).

Requirement:
- A save error gives a toast.
- The host writes a content-free stderr line per outcome, confirmed or closed.
- The debug read verb (B3) reports the last outcome.

**M16. The base moves: what can enter `main` before the stack and what cannot.**
*Lenses: implementation-readiness, zoom-out, structural.*

- **Modified by the stack:**
  - `project_forms.rs`: the bezel `TextField` fields became Ely `TextInput::detached`, and PR E still touches line 395.
  - The Add Project menu in `sidebar/mod.rs` now uses Ely menus.
- **Not modified by the stack:**
  - The add functions have the same body in both bases.
  - `project_settings.rs` and `project_identity.rs` have the same line count in both bases, but were not compared byte for byte.
- **The picker still uses bezel** (`bezel_theme`, `controls::card`: project_identity.rs:10, 1236-1239; controls.rs:20). "Built with Ely" holds only for the dialog shell, until someone moves the picker.
- **Can enter `main` independently:** the socket parameter (`protocol.rs`, handler, `control_add_project`, `sirioctl.rs`), the M2 fix, the documentation and the case in `test-sirioctl-e2e.sh`.
- **Must wait for the stack:** the dialog, the debug verbs and any change to the Ely vendor. The stack is still modifying `LOCAL-CHANGES.md`.

Question 11.

### 🟢 Nit

- **Where focus goes on close** [comprehension, design]. After Create or Clone the form, with its focused field, is dropped in the same update that emits the event (sidebar/mod.rs:1879, 1898). The "previous" focus points to a dead handle (ely:primitives/focus.rs:159-171; not verified). Requirement: on close, focus goes to an element present in the frame, for example the new project's row. Both buttons call the dialog's `close` after the action, like `render_discard_dialog` (gr:sirio_ui/src/changes.rs:3803-3812).
- **Ctrl+C and paste** [comprehension]. The Ely field copies the selection and in practice does not swallow the key (ely:forms/text/actions.rs:305-308). The picker's GitHub, favicon and emoji fields ignore keys with Ctrl or Cmd, so they cannot be pasted into (project_identity.rs:549-553, 587-591). This behavior is already present in Settings.
- **Text selection and focus** [comprehension]. Dragging a selection moves the focus to the workspace's focus sink, outside the dialog (gr:sirio_ui/src/text_selection.rs:196-211). Esc, Enter and Tab might stop working until the user clicks again. Not verified; to be checked in the E2E. Any fix goes in the shared code.
- **Same-name warning** [comprehension, proposal]. A dimmed line under the field, for example "Another project is also named "app" — ~/work/server/app", with the selectable path. It does not block saving. It is different from the rejected alternative "dialog only on collision".
- **`git config` on the UI thread** [design, structural, codebase-details]. `with_value_and_repo` runs `git config --get remote.origin.url` synchronously (gr:sirio_git/src/remote.rs:17-22), on top of the synchronous `discover_project` of `add`. Fine for now; if it is moved one day, it should be done inside the picker.
- **Card inside card and width** [structural, zoom-out, codebase-details]. The picker draws its own `controls::card` (project_identity.rs:1236-1239). Inside the dialog it becomes a card in a card, with bezel spacing on an Ely surface. Proposal: a "cardless" constructor for the picker, not a second picker. That the picker fits in `theme.dialog_width()` (dialog.rs:221) is not verified; one capture per picker mode is needed.
- **Translucency** [codebase-details]. The dialog card is opaque (gr:sirio_ui/src/ely.rs:155-171). The picker's inner card uses `ely.surface`, which fades with translucency. To be checked by eye with translucency on.
- **No migration** [implementation-readiness]. `display_name`, `icon_kind`, `icon_value` and `color_hex` exist since schema v1 (main:sirio_persistence/src/migrations.rs:31-45), and an earlier Sirio version reads everything the feature writes. A new column, such as a "named at add" marker, would raise `CURRENT_SCHEMA_VERSION` and an old app would reject the DB. The draft must say "no migration".
- **Different paths per platform** [implementation-readiness]. The non-git prompt is a native sheet on macOS, the fallback in a gpui window on Linux (gr:rust/vendor/gpui_linux/src/linux/x11/window.rs:1497-1505) and native on Windows. Focus after the PNG picker and the IME in the Ely dialog are not verified on macOS and Windows; they must be listed as manual checks.
- **Performance labels** [codebase-details]. Spans and events must be named with fixed `&'static str`, for example `"ProjectCatalog.add"`, `"project_add.dialog_opened"`, `"project_add.dialog_confirmed"`, `"project_add.dialog_dismissed"`. The project id must never go in the entity column, because it is a hash of the path.

## Goals vs. Non-Goals

* **In-Scope Goals:**
  * After every successful add from the UI (Open folder, Clone, Create, the center's empty state) a modal Ely dialog opens with the label (`display_name`) and the full `ProjectIconPicker`. It opens only on a real add and is identified by the catalog id (B1).
  * The dialog belongs to the workspace and is drawn in `render_global_overlays`. The component lives in `sirio_ui` (B2).
  * Confirm makes a single write through `update_project_settings`, preserving the worktree defaults. Esc, scrim and close write nothing (M1).
  * Keyboard path defined (M3, M4, M5):
    * initial focus in the field with the name preselected for synchronous adds, on the dialog for asynchronous ones;
    * Enter confirms, Esc closes, the scrim click is ignored;
    * Esc in the picker does not close the dialog;
    * shell shortcuts do not act behind the scrim;
    * `focus_first` and `on_enter` of Ely `Dialog` made public in the vendor.
  * Label field on Ely `TextInput`, and a single normalizer for the tab, the dialog and the socket: trim, empty or equal to the folder name → `None`, at most 120 characters, control characters removed (M6).
  * Texts: title "Project added", selectable path, Skip/Save buttons, non-blocking same-name warning, Create's field renamed "Folder name" (M13).
  * With a truncated clone the form closes and the notice moves into the dialog (M5b).
  * `project.add` accepts an optional `displayName`, with the `--display-name` flag in `sirioctl` (M9, M12):
    * the response repeats the applied value;
    * with `added=false` the label is ignored and the response says so;
    * `sirioctl` exits with an error if the echo is missing;
    * the icon stays the default one and no dialog opens.
  * Fix for `sync_control_state` in `update_project_settings` (M2), needed for the E2E proof.
  * Debug-only verbs and an E2E script with a re-runnable artifact, plus the socket case in `test-sirioctl-e2e.sh` (B3).
  * Fix for Esc in the picker's emoji search (M5a).
  * Update of `docs/agents/architecture/control-socket.md`, `docs/agents/e2e-tests.md`, the `sirioctl` usage line and `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` if the vendor is touched.
* **Out-of-Scope Non-Goals:**
  * Renaming the folder on disk.
  * Opening the dialog for adds from the socket, or choosing the icon from the socket.
  * Downloading GitHub avatars or favicons. The globe stays, as in the Settings tab.
  * Replacing the native non-git prompt with an Ely component: `ConfirmDialog` has two buttons, not three (ely:overlays/dialogs.rs:101-178).
  * Using the label as a selector in socket methods (`workspace.create`, `workspace.list`) or showing it in `sirioctl project list` without `--json` (decision 6).
  * New columns or schema migrations.
  * Moving the Project Settings tab's label field to Ely, or binding Tab across the app (decision 7).
  * Fixing the orphan clone when the form is closed during the clone: it goes in a separate issue (decision 12).
  * Fixing the background refresh that replaces the catalog (M8): it is a separate fix that must be done before this feature (decision 10, issue [#682](https://github.com/ai-sirio/sirio/issues/682)).
  * Bringing any part to `main` before the bezel-off stack is merged (decision 11).
  * Changing the add of a duplicate: the toast stays (decision 4).

## Alternatives Weighed

**Rejected by the user:**

1. **Dialog only when the new name matches that of an existing project.** Rejected: the user wants the dialog on every add.
2. **No dialog; automatic disambiguation `name — parent-folder`.** Rejected: the user wants to choose a name and an icon at the moment of adding.
3. **Icon choice inside the Create form instead of a dialog after Create.** Rejected: the dialog also appears after Create.
4. **Ask for name and icon before adding the project, with a pending add that Esc cancels.** Rejected: first add, then ask, and Esc keeps the default values. This also avoids an orphan clone on disk when Clone is cancelled. Review note, not a reopening: an orphan clone already exists today when the form is closed during the clone (M10).
5. **Open the dialog also for adds from the socket.** Rejected: agents pass the name, and no modal appears to a user who did not ask for it.

6. **Delivery in two parts: socket parameter, M2 fix and documentation on `main` right away, before the stack; dialog, debug verbs and vendor after** (implementation-readiness, zoom-out, structural). Rejected: the user wants a single delivery after the stack merge.

**Proposed by the review, then decided:**

7. **Open the new project's Project Settings tab instead of a dialog** (zoom-out). It costs about ten lines and reuses the whole view. Rejected: it is not modal and saves on every keystroke, so Esc could not keep the default values. It also opens the secondary panel, leaves one more tab after every add and shows much more than name and icon. Zoom-out itself did not recommend it.
8. **Non-git folder: add it as it is and offer "Initialize Git" inside the dialog** (design). One modal instead of two, and the id stays stable (main.rs:10868-10893). Rejected: it redoes the F-PRJ-03 flow, out of scope.
9. **Asynchronous adds (Clone, Initialize Git)** (design, comprehension, implementation-readiness, zoom-out). The options were: no focus in the field; a toast "Cloned app — Name it…" that opens the dialog; open it only if the form is still visible and focused; ignore keys for a short interval. Chosen: no focus in the field. The others are rejected: the toast and the conditional opening contradict "dialog on every add", and a timed interval gives no guarantees.
10. **Socket parameter `displayName` and flag `--display-name` instead of `name`** (comprehension), in line with the vocabulary of `project.list`. Accepted.
11. **How to get initial focus and Enter.** The options were: make `focus_first`, `on_enter` and `held` of Ely `Dialog` public; give `PromptDialog` a content slot; a workaround on the Sirio side (M3). Chosen: make them public in the vendor, with the entry in `LOCAL-CHANGES.md`. The workaround is rejected because it is not verified to work with Ely's focus-taking. `PromptDialog` with a content slot is rejected because it is a wider change to the vendor.
12. **A write function for identity only** (label and icon, reading and rewriting the current settings) in the catalog layer, used by the dialog and the socket, instead of building a full `ProjectSettingsUpdate` (structural). To decide in the draft: it is a technical choice.
13. **Non-blocking same-name warning inside the dialog** (comprehension). Accepted.
14. **Rename the Create form's field to "Folder name"**, so the two fields ask for two different things (comprehension). Accepted.
15. **UI add of a duplicate: instead of only the toast, highlight the existing project or open its settings** (comprehension, design, implementation-readiness). Rejected: today's toast stays.

## Key Decisions & Staging for Next Steps

**Decisions made (by the user):**

* The problem to solve is identical names in the sidebar, caused by the default name being taken from the last path component.
* The dialog appears on every add from the UI, even when the name is already unique.
* The chosen name is only a label (`display_name`); the folder on disk is never renamed.
* Esc or close keeps the default values; the project stays added.
* Order: first add, then ask.
* The dialog also appears after Create; the user accepts naming twice.
* The dialog contains the full picker, including the GitHub and favicon sources that show as a globe. The reasoning must be corrected: the HTTP client exists, the avatar download is missing (M14).
* Agents via socket choose only the label; the icon stays the default one; no dialog for adds from the socket.
* Built with Ely components, after the bezel-off stack is merged.

**Requirements that emerged from the review** (they do not ask for a product choice, but the draft must write them down):

* The dialog opens only in the host, on a real add, with the catalog id and the name of the catalog root (B1).
* The dialog belongs to the workspace, not to the Sidebar (B2).
* Local drafts with a single write on Confirm (M1), and `sync_control_state` after every save (M2).
* Debug-only verbs and an E2E script with an artifact (B3).
* The picker consumes its own Esc presses, and the shell does not act behind the modal (M5).
* A queue of dialogs per project id (M11).

**Decisions made by the user after the review (2026-10-28):**

The thirteen questions opened by the review are closed as follows. Answers 1-10, 12 and 13 are the proposals presented to the user and accepted by them; 11 is their explicit decision.

1. **Asynchronous adds (Clone, Initialize Git).** The dialog opens with focus on the dialog itself, not in the field: keys already in flight are lost and Enter does not confirm until the user clicks the field. For a synchronous add (Open folder on a git repository) focus goes in the field with the name preselected. (M10)
2. **Clone with truncated output.** When the dialog opens, the Clone form closes, and the truncation notice moves into the body of the dialog. (M5b)
3. **Non-git folder.** Two modals in a row remain: the native F-PRJ-03 prompt, then the Ely dialog. That Esc cancels in the first and keeps the defaults in the second is accepted. Alternative 8 is out of scope.
4. **UI add of a duplicate or a nested folder.** Only today's toast, no dialog. Alternative 15 is rejected.
5. **Socket parameter.** It is called `displayName`, with the `--display-name` flag in `sirioctl`. With `added=false` it is ignored and the response says so. The new `sirioctl` exits with an error if it passed the label and the app does not repeat it in the response. (M9)
6. **Label visibility for agents.** It stays only in `--json` (`displayName`), as today; `sirioctl project list` without `--json` and `workspace.list` do not change.
7. **Keyboard.** The picker is mouse-only and Tab is not bound across the app. Initial focus and Enter are obtained by making `focus_first` and `on_enter` of Ely `Dialog` public in the vendor, with the entry in `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`. The scrim click is ignored (`held`), so a careless click does not discard a name already typed; it closes with Esc or Skip. (M3, M4)
8. **Confirm without changes** produces the same record as Esc: label equal to the folder name → `None`, icon untouched → no icon write. (M7)
9. **Label validation.** Reuse of the existing sanitizer (gr:sirio/src/session.rs:194): at most 120 characters, control characters removed. (M6)
10. **Background refresh that undoes writes** (M8). It is fixed separately and before the feature, because the defect already exists today for the Settings tab and for `add`. It is a prerequisite, not part of this delivery; tracked as issue [#682](https://github.com/ai-sirio/sirio/issues/682).
11. **Delivery.** Everything after the bezel-off stack is merged, in a single delivery: socket parameter, M2 fix, dialog, debug verbs, vendor change and documentation. No part goes to `main` before. (M16)
12. **Orphan clone when the form is closed during the clone** (M10). Out of scope; opened as issue [#681](https://github.com/ai-sirio/sirio/issues/681).
13. **Texts.** Title "Project added"; the catalog root path with `selectable_text(..)`; the field "Name in the sidebar" with the line saying that the folder on disk keeps its name; buttons **Skip** (= Esc) and **Save** (= Enter), never "Cancel". The Create form's field becomes "Folder name". Under the field a non-blocking warning appears when the name matches that of another project, with the path of the latter selectable.

**To decide in the draft** (technical choice, not a product one): whether the write from the dialog and the socket goes through a full `ProjectSettingsUpdate`, rebuilt by reading `project_catalog.project_settings(id)` at the time of Confirm (M1), or through a function that writes only the identity in the catalog layer (alternative 12). The required result is the same: the write rereads the current state and does not touch the worktree defaults.

## Reviewer Status

| Reviewer | Status |
| --- | --- |
| `egm-design` | completed |
| `egm-codebase-details` | completed |
| `egm-zoom-out` | completed |
| `egm-comprehension` | completed |
| `egm-implementation-readiness` | completed |
| `egm-structural` | completed |
