# Wave A slice W07-prj — 5 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-PRJ-06` — ledger line 99, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Shim git on PATH with a sleep wrapper (or otherwise slow the clone) so Running persists across a frame, then fire two real separate clicks on clone-submit and confirm only one worker starts.
- **Shared cause:** Same as F-PRJ-09: the in-flight guard (CloneFormState::begin/can_submit, project_forms.rs:70-85) is correct and unit-tested, but no-network git clone fails near-instantly so Running never survives a frame for a second click to land in.
- **Evidence on record:** Guard half still unreachable: no network + sub-frame completion + unreliable rapid-click delivery (first of a pair dropped 4/4 tries). Caution: cited captures (06-fprj06c-doubleclick.png, 07-fprj06-settle2.png) do not show the claimed Retry-clone/red-error outcome -- both still show the pre-submit Clone repository button, Ready to clone text, and a partial URL (ht/h). Empty-URL-disablement half remains proven from pr

## `F-PRJ-07` — ledger line 100, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Retry path is implemented and unit-tested (clone_failure_keeps_url_and_allows_retry). clone_repository shells to plain git clone and accepts local paths. After the proven invalid-URL failure, edit the URL to a local git repo path and click the relabeled button; confirm Failed->Running->Complete and the project appears.
- **Evidence on record:** Failure half confirmed live: invalid URL submitted, button relabeled Retry clone, verbatim red failure text shown. Correct-the-URL-and-retry half never driven — no second, valid submission attempted. shots/05a.

## `F-PRJ-09` — ledger line 102, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Slow create_project's git-init shell-out (wrapper shim) so Running persists, then fire two real clicks and confirm only one project is created.
- **Shared cause:** Same as F-PRJ-06: CreateFormState::begin/can_submit (project_forms.rs:444-458) is correct and unit-tested, but local mkdir+git init resolves in well under a frame -- evidence's own capture (second click landing on empty sidebar space after the dialog closed) confirms the form unmounted before the second click could land.
- **Evidence on record:** Guard half still unreachable, same structural reason as F-PRJ-06. New live proof: single click created exactly one real project (e09testproj row+path visible in sidebar, 06-fprj09b-click1.png); second click landed on empty sidebar space after the dialog had already closed (07-fprj09b-click2.png). Empty-name-disablement half remains proven from prior evidence.

## `F-PRJ-14` — ledger line 107, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Drive the untried PNG-upload and favicon-domain arms live, Close, confirm the sidebar row shows the Globe glyph (render_row's glyph match maps any Avatar(_) source to Icon::Globe, sidebar.rs:2066-2072) for each.
- **Shared cause:** The propagation half rides the same P97 fix as F-PRJ-13/F-PRJ-15 -- commit_favicon and local-PNG commit both funnel through the same commit()->on_change_with_context->apply_icon_change chain as the proven GitHub-avatar arm.
- **Evidence on record:** Avatar tab opened live: PNG/GitHub/favicon controls all present. GitHub-avatar arm works: typed octocat, clicked Use GitHub Avatar, real confirmation Current: GitHub avatar for octocat appeared. Clause requires PNG upload and favicon domain too -- both untried; whether the accepted avatar reaches the sidebar row is also unchecked.

## `F-PRJ-15` — ledger line 108, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** The 'grid is real, its output is discarded' reasoning looks stale against commit 28a41fa (Aug 14 12:59), which post-dates the evidence and appears to fully wire apply_icon_change -> project_identities (in-session) and -> SidebarEvent::ProjectSettingsChanged (durable). Unlike F-PRJ-13 there's no second independent defect recorded for this row, so a clean live re-drive (pick glyph, Close, confirm row updates; relaunch, confirm persisted) may simply pass it.
- **Shared cause:** Same P97 fix as F-PRJ-13; see group notes.
- **Evidence on record:** **pass 13 superseded** — a six-glyph grid renders (folder, git branch, chat, terminal, document, globe) and selection works: clicking the git-branch moved the orange selection ring off the folder onto it (`orch18-picked.png`). **The chosen glyph never reaches the project** — the sidebar row is unchanged after `Close` (`orch18-sidebar.png`). The grid is real; its output is discarded

