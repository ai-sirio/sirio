# Frozen reference — the bar

Captured from the running Swift Tiller (/Applications/Tiller.app) at 1470x833.
These do not change. If a shot is wrong, recapture it and say so — never adjust
a Rust render to match a shot you suspect.

| shot | state |
|---|---|
| 01-chat-empty | 3 columns, all projects collapsed, empty Chat tab, composer idle |
| 02-project-expanded | "tiller" expanded -> worktree `main` -> tab `Chat`, + New Worktree row |
| 03-new-tab-menu | the "+" menu: New Terminal / agents / New Browser / New Chat > |
| 04-terminal-pane | live zsh + powerlevel10k prompt, two tabs, second active |
| 05-changes-panel | right panel switched to Changes |
| 06,07,09,10,11-settings-* | Settings: AI Providers, General, Permissions, Appearance |
| ~~08-settings-agents~~ | **MISSING.** The capture was a duplicate of 07 — the click on the Agents category never landed, and the shot was byte-identical to AI Providers. Caught by a builder, not by me. Recapture attempts failed: after the window moved, coordinate clicks stopped switching category and the SwiftUI accessibility tree exposes nothing usable. Use `App/AgentsSettingsView.swift` as the reference for that screen instead — the source is more precise than a screenshot anyway. A wrong shot is worse than a missing one, so it is deleted rather than left in place. |
| 12-light-settings | Appearance section with Light selected |
| 13-light-main | full app in light mode |
| 14-back-dark | dark restored (sanity check against 01) |
| 15-chat-composer-focused | composer focused: orange border, send button active |
| 16-chat-streaming | assistant reply mid-stream |
| 17-chat-response | completed ACP round-trip: user bubble, reply, timestamp+copy, tab badge, worktree dot, Ask + model pickers |

Measured calibration: canvas token #131417 reads back as #141416. Deltas of 1-3 are noise.
