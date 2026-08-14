# Triage group T4-tab — 14 outstanding rows

Families: F-TAB.

For each row decide what it actually needs and which files a fix would touch.
**Read only — change no code and set no verdict.**

| row | line | verdict | evidence recorded so far |
|---|---|---|---|
| `F-TAB-01` | 117 | FAILED — defective | Live-driven: every click on an expanded folder's child row (file or subfolder), 3 separate isolated attempts, collapses the parent instead of acting on the child. No document tab ever opened — both the document-icon and dirty-route conjuncts unreachable as a direct consequence. shots/135,142. |
| `F-TAB-02` | 118 | FAILED — defective | P104 §Group 1: overflow chevron rendered before real pixel overflow; clicking it twice showed no all-tabs list or selected-tab marker. |
| `F-TAB-08` | 124 | half-proven | Critic verified live click 2026-08-14: pre/post-click menu content is byte-identical (same fallback text, no Settings surface) -- corroborates no-op. Chased an apparent frame-resolution mismatch (04@1715x972 vs 05@1400x900) to wayland-drive.sh's shot() deliberately toggling output size to force a repaint (script lines 285-312); confirmed the click was dispatched at 1715x972, matching the frame the row's bounds were read from, so it genuinely landed inside the fallback row, not a miss. Source-… |
| `F-TAB-11` | 127 | FAILED — absent | Live-driven, sole-tab state: right-click menu shows only a plain 'Copy' entry — no Split Left/Right/Above/Down items appear at all, disabled or otherwise, no reason text. Clause's premise (a disabled split item with a reason) does not exist. shots/146. |
| `F-TAB-12` | 128 | FAILED — defective | P104 §Group 1: tab context menu never appeared after repeated right-clicks, so Move Existing Tab could not be invoked. |
| `F-TAB-13` | 129 | FAILED — defective | P104 §Group 1: the tab menu never appeared, so the required no-eligible-tab explanation was absent; a terminal Split Right is a different surface. |
| `F-TAB-14` | 130 | FAILED — defective | P104 §Group 1: tab context menu never appeared, so Rename was unavailable; the separately required double-click path was not reached. |
| `F-TAB-15` | 131 | FAILED — defective | P104 §Group 1: a tab context menu never appeared, so context Close Tab could not be used; the close-control trial remains unperformed. |
| `F-TAB-16` | 132 | FAILED — defective | P104 §Group 2: closing the dirty file-hosting tab silently discarded the unsaved y with no confirm/discard prompt. The two-dirty-tabs precondition was also unreachable. |
| `F-TAB-17` | 133 | FAILED — defective | P104 §Group 1: with four terminal tabs present, no tab menu appeared; Close Others and Close Tabs to Right were unreachable. |
| `F-TAB-21` | 137 | FAILED — defective | P104 §Group 1: repeated tab-strip right-clicks produced no Tab menu despite a verified working terminal-body right-click; no move action was available. |
| `F-TAB-23` | 139 | FAILED — defective | P106 fable §F-TAB-23: live pane.split created all directions, but direction=left placed the new terminal on the right exactly like right; the required corresponding left placement is wrong. Menu-route exercise remains owed. |
| `F-TAB-25` | 141 | FAILED — absent | still no attach-to-terminal code; P65 built Move-to-Pane (a different feature) and explicitly refused this one (assigned-but-absent recheck, pass 14) |
| `F-TAB-28` | 144 | FAILED — defective | Live-driven: ctrl-w on clean tab (Chat) — no change across 3 isolated tries incl. explicit refocus. ctrl-w on dirty tab (Terminal, tab_is_dirty) — byte-identical capture, no confirm dialog, no close. Zero observable effect on either path. shots/164,165,168,169. |
