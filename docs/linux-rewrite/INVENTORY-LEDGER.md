# INVENTORY-LEDGER — per-entry status of all 388 entries

Written by pireview (the critic). Pass 9 built it; pass 10 converted the bulk of
`NOT EXERCISED` into real verdicts (see `CRITIC-baseline.md` PASS 10). Pass 10 snapshot:
whole tree copied to `/tmp/critic-pass10` at **2026-08-13T12:40:06Z**; build clean in
14.14 s; headless launch served `TILLER_SOCKET=/tmp/critic10.sock`; live exercises: real
claude usage fetch (Success 4%/30%), `sleep` pane surviving select-workspace away/back.
Pass 9 snapshot: whole tree copied to
`/tmp/critic-pass9` at 2026-08-13T12:27:17Z (14:27:17 CEST); `cargo build -p tiller
-p tiller_control` clean in 4.87 s; headless launch served `TILLER_SOCKET=/tmp/critic9.sock`
(ping pongs, 42 methods advertised, `browser.*` answers its specific unsupported error,
the P37 mutation doors `surface.changes.stage/unstage/discard/stage_all/discard_all` exist).

One row per entry from both inventory files, in inventory order. This ledger consolidates
critic passes 1–8 from `CRITIC-baseline.md`; pass 9 exercised nothing new beyond the
snapshot health check and the environment fact that `opencode`/`omp` are absent from PATH.
**The ledger wins over `INVENTORY-STATUS.md` wherever they disagree.**

Verdict vocabulary: `PASSED` · `FAILED — absent` · `FAILED — defective` ·
`UNREACHABLE` (with reason) · `N/A — platform` · `NOT EXERCISED — blocked on display`
(appearance only) · `NOT EXERCISED` · `half-proven` (with which half).
`judged` names the critic pass that established the verdict, or
`builder-claimed, unverified` / `never claimed` where no independent critic pass has
touched the entry — those rows do **not** count toward done.

## App target — `01-inventory-app.md` (217)

### Window and application shell (12)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-WIN-01` | PASSED | pass 1 display: gear→Settings, Back; pass 8 socket surface.settings.open/select/read | pass 8 |
| `F-WIN-02` | NOT EXERCISED | P48 builder claim: Linux `Ctrl+T` dispatches `NewTerminalTab` → `NewTabAction::NewTerminal`; focused chord fixture green; independent critic verification required | builder-claimed, unverified |
| `F-WIN-03` | NOT EXERCISED | P48 builder claim: Linux `Ctrl+O` opens the file picker and `Ctrl+S` calls `FileView::save` with typed `NoActiveFile` gating; focused chord and file-view save tests green; independent critic verification required | builder-claimed, unverified |
| `F-WIN-04` | NOT EXERCISED | P48 builder claim: Linux `Ctrl+Shift+S` dispatches `ToggleSidebar`; title-strip selector emits the same typed event; focused chord fixture green; independent critic verification required | builder-claimed, unverified |
| `F-WIN-05` | NOT EXERCISED | P48 builder claim: Linux `Ctrl+Shift+I` dispatches `ToggleRightPanel`; title-strip selector emits the same typed event; focused chord fixture green; independent critic verification required | builder-claimed, unverified |
| `F-WIN-06` | N/A — platform | no browser; NewBrowser is a typed no-op | pass 8 |
| `F-WIN-07` | PASSED | session.restore (pass 3) + restore_* tests green (pass 8); History-menu route absent | pass 8 |
| `F-WIN-08` | N/A — platform | macOS hide-on-close delegate | pass 8 |
| `F-WIN-09` | N/A — platform | macOS title-bar preference | pass 8 |
| `F-WIN-10` | FAILED — absent | no toast implementation, only a theme radius token | pass 8 |
| `F-WIN-11` | N/A — platform | Sparkle updater; no Linux update code | pass 8 |
| `F-WIN-12` | N/A — platform | TCC onboarding sheet | pass 8 |

### Projects and sidebar (19)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-SID-01` | PASSED | Projects header, + control, project row render | pass 1 |
| `F-SID-02` | PASSED | filter narrows and restores rows | pass 1 |
| `F-SID-03` | PASSED | drawn: + click → dir-picker → AddProject → shell add_project | pass 8 |
| `F-SID-04` | PASSED | chevron hides/restores children (measured) | pass 1 |
| `F-SID-05` | PASSED | drawn selection event + live select-workspace mounts tabs | pass 8 |
| `F-SID-06` | half-proven — state half: status dot + activity model real; collapsed-project badge pixels display-blocked | pass 8 | pass 8 |
| `F-SID-07` | FAILED — absent | no gear/project-settings sheet | pass 8 |
| `F-SID-08` | FAILED — absent | no init-git action | pass 8 |
| `F-SID-09` | FAILED — absent | no context menu; Finder external | pass 8 |
| `F-SID-10` | half-proven — remove flow + confirmation + shell remove_project real; prompt flow not drawn-tested | pass 8 | pass 8 |
| `F-SID-11` | half-proven — path + agent-status dot render; branch/comment/primary text absent | pass 8 | pass 8 |
| `F-SID-12` | FAILED — absent | no Set Primary action | pass 8 |
| `F-SID-13` | PASSED | drawn test against real git: worktree created | pass 8 |
| `F-SID-14` | FAILED — absent | no context menu | pass 8 |
| `F-SID-15` | PASSED | drawn test against real git: worktree removed | pass 8 |
| `F-SID-16` | FAILED — absent | drag reorder removed by design; order untouched | pass 8 |
| `F-SID-17` | FAILED — absent | drag reorder removed by design | pass 8 |
| `F-SID-18` | FAILED — absent | no "No Terminals" empty state | pass 8 |
| `F-SID-19` | FAILED — absent | no ⌘T binding | pass 8 |

### Project creation and project settings (18)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-PRJ-01` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-02` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-03` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-04` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-05` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-06` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-07` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-08` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-09` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-10` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-11` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-12` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-13` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-14` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-15` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-16` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-17` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |
| `F-PRJ-18` | NOT EXERCISED | Add-Project sheet flows never per-entry judged; F-SID-03 proves only the + gate | never claimed |

### Tabs, panes, navigation (28)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-TAB-01` | half-proven — strip renders icon/title/✓-done/close; dirty indicator absent, status only Done ✓ | pass 2 display + pass 8 | pass 8 |
| `F-TAB-02` | FAILED — absent | no All-Tabs overflow control | pass 8 |
| `F-TAB-03` | PASSED | drawn +-menu dispatches New Terminal; shell creates real tab | pass 8 |
| `F-TAB-04` | PASSED | drawn +-menu New Terminal action; shell open_action creates tab | pass 8 |
| `F-TAB-05` | half-proven — menu lists all 5 adapters → add_agent_tab; launch-error half (opencode/omp) unexercised | pass 8 | pass 8 |
| `F-TAB-06` | N/A — platform | no browser; NewBrowser typed no-op | pass 8 |
| `F-TAB-07` | half-proven — New Chat item → chat tab; ACP-agent submenu absent | pass 8 | pass 8 |
| `F-TAB-08` | FAILED — absent | no no-agent fallback / "Other agents…" | pass 8 |
| `F-TAB-09` | FAILED — absent | no pane menu / Open File | pass 8 |
| `F-TAB-10` | PASSED | pane.split right/down live (pane-3 created); chords bound | pass 8 |
| `F-TAB-11` | half-proven — split_disabled_reason unit-tested; never rendered, no split menu | pass 8 | pass 8 |
| `F-TAB-12` | FAILED — absent | no move-tab UI | pass 8 |
| `F-TAB-13` | FAILED — absent | no move-tab menu or empty state | pass 8 |
| `F-TAB-14` | FAILED — absent | no rename anywhere | pass 8 |
| `F-TAB-15` | PASSED | ✕ closes tab (pass 2 display); close_tab_by_id | pass 8 |
| `F-TAB-16` | FAILED — absent | close_tab has no confirmation and no dirty check | pass 8 |
| `F-TAB-17` | FAILED — absent | no close-others/right | pass 8 |
| `F-TAB-18` | FAILED — absent | no tab drag; only pane divider drags | pass 8 |
| `F-TAB-19` | PASSED | ctrl-tab/ctrl-shift-tab bound; handler = live tab.cycle; chord fixture green | pass 8 |
| `F-TAB-20` | PASSED | ctrl-1..9 bound; handler = live tab.select; chord fixture green | pass 8 |
| `F-TAB-21` | FAILED — absent | no Tab menu | pass 8 |
| `F-TAB-22` | PASSED | ctrl-alt-arrows bound; live pane.focus moved focus | pass 8 |
| `F-TAB-23` | half-proven — right/down splits real; Pane menu and left/up absent | pass 8 | pass 8 |
| `F-TAB-24` | FAILED — absent | vacuous: no tab drag to cancel | pass 8 |
| `F-TAB-25` | FAILED — absent | no attach-to-terminal code | pass 8 |
| `F-TAB-26` | FAILED — absent | no context menu | pass 8 |
| `F-TAB-27` | FAILED — absent | no resume-chat; resumeAgentSessions flag is not a surface | pass 8 |
| `F-TAB-28` | FAILED — absent | no ⌘W; ctrl-alt-w closes a pane, no-op on single tab | pass 8 |

### Chat surface over ACP (37)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CHAT-01` | PASSED | live ACP chain: user bubble, streamed reply, 3 turns | pass 1 |
| `F-CHAT-02` | PASSED | drawn authentication-required + Retry | pass 8 |
| `F-CHAT-03` | NOT EXERCISED | disconnected-agent state never exercised | never claimed |
| `F-CHAT-04` | half-proven — Return sends (pass 1); shift-return bound but no dispatching test | pass 8 | pass 8 |
| `F-CHAT-05` | NOT EXERCISED | composer-disable state never exercised | never claimed |
| `F-CHAT-06` | NOT EXERCISED | next-turn queueing never exercised | never claimed |
| `F-CHAT-07` | NOT EXERCISED | Stop/Escape never exercised | pass 1 |
| `F-CHAT-08` | PASSED | drawn connecting/send/stop states | pass 8 |
| `F-CHAT-09` | FAILED — absent | no slash commands in composer | pass 8 |
| `F-CHAT-10` | FAILED — absent | no @ file mentions | pass 8 |
| `F-CHAT-11` | FAILED — absent | no image attachment control | pass 8 |
| `F-CHAT-12` | FAILED — absent | no attachment chips | pass 8 |
| `F-CHAT-13` | NOT EXERCISED | file drop into chat never exercised | never claimed |
| `F-CHAT-14` | FAILED — absent | no overflow menu / Follow Edited Files | pass 8 |
| `F-CHAT-15` | NOT EXERCISED | permission-mode pill never exercised | never claimed |
| `F-CHAT-16` | PASSED | drawn model picker select + escape | pass 8 |
| `F-CHAT-17` | FAILED — absent | no effort levels | pass 8 |
| `F-CHAT-18` | PASSED | drawn context-ring popover + focus | pass 8 |
| `F-CHAT-19` | FAILED — absent | no warning colour above 80% | pass 8 |
| `F-CHAT-20` | NOT EXERCISED | tail-follow code exists (chat.rs), unexercised | pass 8 |
| `F-CHAT-21` | FAILED — absent | no Thinking expand/collapse | pass 8 |
| `F-CHAT-22` | FAILED — absent | no grouped-steps expansion | pass 8 |
| `F-CHAT-23` | PASSED | Write tool card Pending→Completed live (pass 1) | pass 1 |
| `F-CHAT-24` | FAILED — absent | generic Permission card only; named Plan card absent | pass 8 |
| `F-CHAT-25` | FAILED — absent | no text-answer/cancel on questions | pass 8 |
| `F-CHAT-26` | FAILED — absent | no pending-question bar | pass 8 |
| `F-CHAT-27` | FAILED — absent | no expired-question state | pass 8 |
| `F-CHAT-28` | FAILED — absent | no subagent task cards | pass 8 |
| `F-CHAT-29` | NOT EXERCISED | copy code exists (chat.rs), unexercised | pass 8 |
| `F-CHAT-30` | NOT EXERCISED | code-block copy never exercised | never claimed |
| `F-CHAT-31` | FAILED — absent | no chat diff preview | pass 8 |
| `F-CHAT-32` | FAILED — absent | no edit summary | pass 8 |
| `F-CHAT-33` | NOT EXERCISED | turn errors/MCP warnings never exercised | never claimed |
| `F-CHAT-34` | FAILED — absent | no chat history menu | pass 8 |
| `F-CHAT-35` | FAILED — absent | no no-past-chats empty state | pass 8 |
| `F-CHAT-36` | FAILED — absent | no no-models fallback | pass 8 |
| `F-CHAT-37` | PASSED | empty transcript + usable composer | pass 1 |

### Files, changes, activity (22)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CHG-01` | FAILED — absent | right panel Files+Activity only; Changes moved to a Diff tab | pass 5 |
| `F-CHG-02` | NOT EXERCISED — blocked on display | no-worktree state pixels | pass 5 |
| `F-CHG-03` | half-proven — expansion/error state (handler-direct tests); loading/error/Retry absent; drawn click builder-claimed | pass 7 | pass 7 |
| `F-CHG-04` | half-proven — expansion state handler-direct; drawn click builder-claimed, unverified | pass 7 | pass 7 |
| `F-CHG-05` | FAILED — absent | no keyboard handling in right_panel.rs | pass 7 |
| `F-CHG-06` | NOT EXERCISED — blocked on display | status symbols/colours pixels | pass 5 |
| `F-CHG-07` | PASSED | clean/empty tree 0/0/0 ready=true matches porcelain | pass 5 |
| `F-CHG-08` | PASSED | sections+counts match porcelain through MM/RM/UU/binary | pass 5 |
| `F-CHG-09` | PASSED | drawn error/Retry click recovers from removed .git; green ×3 | pass 7 |
| `F-CHG-10` | PASSED | drawn stage/unstage mutates real checkout; green ×3 (pass 8) | pass 8 |
| `F-CHG-11` | half-proven — stage-all drawn green; no Unstage all / section-level actions | pass 8 | pass 8 |
| `F-CHG-12` | PASSED | drawn section/row/context-band clicks + diff data tier | pass 7 |
| `F-CHG-13` | FAILED — absent | ↗ opens read-only File tab; no Open-diff action | pass 5 |
| `F-CHG-14` | PASSED | drawn discard + confirmation; green ×3 (pass 8) | pass 8 |
| `F-CHG-15` | half-proven — binary=true matches numstat; unavailable/retry states owed | pass 5 | pass 5 |
| `F-CHG-16` | FAILED — absent | no resolve-in-terminal action | pass 5 |
| `F-CHG-17` | PASSED | numbered lines + hunk headers match real file (data tier) | pass 5 |
| `F-CHG-18` | FAILED — absent | no drag handlers in changes.rs | pass 7 |
| `F-CHG-19` | half-proven — rows render, row-click switches tab; close-X click unverified | pass 1 | pass 1 |
| `F-CHG-20` | NOT EXERCISED — blocked on display | running-count pixels; builder claims no such element (unverified) | pass 5 |
| `F-CHG-21` | half-proven — rows + hover close-X render; close click did not close (unverified) | pass 1 | pass 1 |
| `F-CHG-22` | half-proven — status glyphs render; status variety unverified | pass 1 | pass 1 |

### Documents and editors (13)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-EDIT-01` | FAILED — absent | no Code/Preview switch (pass 3/7); P39 builder claim of drawn switch unverified | pass 7 |
| `F-EDIT-02` | FAILED — absent | no formatting toolbar | pass 3 |
| `F-EDIT-03` | FAILED — absent | no manual-preview state; hardcoded 1 MiB notice instead; P39 claim unverified | pass 3 |
| `F-EDIT-04` | FAILED — absent | no ⌘S binding | pass 3 |
| `F-EDIT-05` | FAILED — absent | no Reload/Keep banner | pass 3 |
| `F-EDIT-06` | FAILED — absent | no save path | pass 3 |
| `F-EDIT-07` | FAILED — absent | code renders as plain numbered lines, no language detection | pass 3 |
| `F-EDIT-08` | FAILED — absent | add_file_tab pushes unconditionally — no dedupe | pass 3 |
| `F-EDIT-09` | half-proven — drawn double-click emits OpenFile event; end-to-end tab creation unexercised | pass 8 | pass 8 |
| `F-EDIT-10` | FAILED — absent | no context menu on file rows | pass 7 |
| `F-EDIT-11` | FAILED — absent | no copy-path code | pass 7 |
| `F-EDIT-12` | FAILED — absent | no product drag; payload-drag fixture is harness-only | pass 7 |
| `F-EDIT-13` | half-proven — missing-file message model-level in drawn window, no input driven | pass 7 | pass 7 |

### Persistence and lifecycle (8)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-PER-01` | half-proven — projects/worktrees/tabs restore; chats restore without transcript; scrollback P35 fix builder-claimed, replay test green pass 8 | pass 4 | pass 8 |
| `F-PER-02` | PASSED | select-workspace → DB write → relaunch → selection restored | pass 3 |
| `F-PER-03` | PASSED | 2 splits persist across quit/relaunch (pane-1/2/3 present) | pass 4 |
| `F-PER-04` | half-proven — pass 2 FAILED by design → P35 fix + restore_* scrollback-replay test green; end-to-end capture unverified | pass 8 | pass 8 |
| `F-PER-05` | PASSED | session.restore restoredCount 2 + worktree re-selected | pass 3 |
| `F-PER-06` | FAILED — defective | compound-command panes orphan process groups on quit (pass 6); simple panes flush (pass 4) | pass 6 |
| `F-PER-07` | NOT EXERCISED — blocked on display | icon appearance; settings persistence P11/P38 builder-claimed | pass 1 |
| `F-PER-08` | N/A — platform | no browser on Linux | pass 2 |

### Browser (9)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-BRW-01` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-02` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-03` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-04` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-05` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-06` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-07` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-08` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |
| `F-BRW-09` | FAILED — absent | no browser surface; browser.* answers specific unsupported errors (pass 8) | pass 8 |

### Status bar / usage (6)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-USE-01` | PASSED | status bar: gear/refresh/providers/worktree info render | pass 1 |
| `F-USE-02` | PASSED | provider segments + tooltips render | pass 1 |
| `F-USE-03` | PASSED | real Codex 67% parsed from ~/.codex/auth.json | pass 1 |
| `F-USE-04` | N/A — platform | menu-bar-only AgentRosterView; no Linux counterpart | pass 10 |
| `F-USE-05` | N/A — platform | depends on the absent roster | pass 10 |
| `F-USE-06` | FAILED — absent | NotificationPolicy exists in crate; zero app callers of should_notify/build_payload — no delivery path | pass 10 |

### Control socket automation (9)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-AUTO-01` | half-proven — drawn toggle + resolved socket path; socket disable effect untested | pass 7 | pass 7 |
| `F-AUTO-02` | PASSED | panel.create live | pass 3 |
| `F-AUTO-03` | PASSED | panel split/list/write/key/read/wait/focus/close live | pass 3 |
| `F-AUTO-04` | PASSED | notify + worktree.set + session.ref live | pass 3 |
| `F-AUTO-05` | PASSED | workspace list/current/select/create/close live | pass 3 |
| `F-AUTO-06` | PASSED | notification create/list/clear round trip | pass 3 |
| `F-AUTO-07` | PASSED | ping/identify/capabilities live | pass 3 |
| `F-AUTO-08` | PASSED | session.restore restoredCount 2 | pass 3 |
| `F-AUTO-09` | N/A — platform | no browser; explicit unsupported error exercised | pass 3 |

### Settings (25)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-SET-01` | PASSED | categories render; live surface.settings.select | pass 8 |
| `F-SET-02` | half-proven — Back wired in shell; Escape still unbound | pass 8 | pass 8 |
| `F-SET-03` | FAILED — defective | Check for Updates is a dead no-op; no update states | pass 8 |
| `F-SET-04` | half-proven — toggle flips (pass 1); persistence P11-fixed, unverified | pass 1 | pass 1 |
| `F-SET-05` | FAILED — defective | auto_naming report-only (not persisted); summarizer button dead no-op |_,_,_|{} | pass 10 |
| `F-SET-06` | FAILED — defective | retention report-only; SettingsSnapshot/persistence has no such keys | pass 10 |
| `F-SET-07` | FAILED — defective | mount cap report-only; eviction has zero callers (see ACT-26) | pass 10 |
| `F-SET-08` | half-proven — drawn toggle + resolved path; copy-install half unverified | pass 8 | pass 8 |
| `F-SET-09` | FAILED — absent | no skill provisioner in port (pass 6 SAFE-01) | pass 6 |
| `F-SET-10` | half-proven — provider card + real status render; config effects unverified | pass 1 | pass 1 |
| `F-SET-11` | half-proven — Active state real; other 6 states unverified | pass 1 | pass 1 |
| `F-SET-12` | FAILED — absent | no cookie UI or state | pass 10 |
| `F-SET-13` | FAILED — absent | no cookie UI or state | pass 10 |
| `F-SET-14` | FAILED — absent | status-only provider cards; no add/re-auth/remove | pass 10 |
| `F-SET-15` | FAILED — absent | no multi-account model | pass 10 |
| `F-SET-16` | PASSED | registry rows render; drawn discovery rows | pass 8 |
| `F-SET-17` | FAILED — absent | no agent registry | pass 10 |
| `F-SET-18` | FAILED — absent | availability badges only; no install/update/retry actions | pass 10 |
| `F-SET-19` | NOT EXERCISED — blocked on display | theme choice pixels | pass 8 |
| `F-SET-20` | NOT EXERCISED — blocked on display | translucency/font pixels | pass 8 |
| `F-SET-21` | PASSED | drawn file-icon-set selection changes snapshot | pass 7 |
| `F-SET-22` | NOT EXERCISED — blocked on display | agent colour pixels | pass 8 |
| `F-SET-23` | N/A — platform | TCC permissions | pass 8 |
| `F-SET-24` | N/A — platform | browser-origin permissions | pass 8 |
| `F-SET-25` | N/A — platform | TCC refresh on activate | pass 8 |

### Terminals and agents — app (11)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-TERM-01` | PASSED | real login shell on pts, breadcrumb | pass 1 |
| `F-TERM-02` | FAILED — absent | no empty-pane prompt | pass 7 |
| `F-TERM-03` | half-proven — exit-status state core (0/3/137 via panel.state); seeing half display | pass 4 | pass 4 |
| `F-TERM-04` | FAILED — absent | no terminal context menu | pass 7 |
| `F-TERM-05` | FAILED — absent | no context menu | pass 7 |
| `F-TERM-06` | FAILED — absent | no context menu | pass 7 |
| `F-TERM-07` | half-proven — agent launch/identity state core; opencode/omp launch unexercised | pass 4 | pass 4 |
| `F-TERM-08` | FAILED — defective | process-group leak on close/quit; no confirmation prompt | pass 6 |
| `F-TERM-09` | half-proven — indicator-condition state core; pixels owed | pass 4 | pass 4 |
| `F-TERM-10` | PASSED | LIVE pass10: sleep 300 survived select-workspace away+back via socket; scrollback intact | pass 10 |
| `F-TERM-11` | FAILED — absent | no no-worktree empty state | pass 7 |

## Package tier — `02-inventory-packages.md` (171)

### F-CORE-ACT — agent activity model (26)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CORE-ACT-01` | PASSED | 70 tests green; status.rs 4 states+labels+priority; status_for_panes priority test | pass 10 |
| `F-CORE-ACT-02` | PASSED | notify returns transition, timestamps recorded; out-of-order/stale/equal-ts tests; socket notify wired | pass 10 |
| `F-CORE-ACT-03` | PASSED | agent_spawned sets running+identity, returns () by construction; test asserts no transition | pass 10 |
| `F-CORE-ACT-04` | PASSED | from_exit_code 0->done else error; apply_exit_result tests incl untracked/closed->None | pass 10 |
| `F-CORE-ACT-05` | PASSED | unregistered title path tests: glyph->claude, bare "opencode"->Running fallback, title_owned set | pass 10 |
| `F-CORE-ACT-06` | PASSED | ownership test: title-owned cleared by unmatched title; spawn/process-owned survive it | pass 10 |
| `F-CORE-ACT-07` | PASSED | TITLE_DEBOUNCE=1500ms; inside/outside/boundary tests green | pass 10 |
| `F-CORE-ACT-08` | half-proven | model half proven (content not debounce-gated, test); settle->content wiring ABSENT (no caller of detect_content_status in app) | pass 10 |
| `F-CORE-ACT-09` | PASSED | real /proc walk test: spawned sh+codex alias -> Running+process-owned; kill -> cleared | pass 10 |
| `F-CORE-ACT-10` | PASSED | MAX_DEPTH=5/MAX_PROCESSES=50 in source; walk exercised nested fixture + real procs; node non-match tested | pass 10 |
| `F-CORE-ACT-11` | PASSED | three ownership kinds isolated in one test; pane_closed clears all, idempotent | pass 10 |
| `F-CORE-ACT-12` | PASSED | overturn pass9 UNREACHABLE: pure classification, every clause case unit-tested (glyphs/pi/pi:/names/bare spinner) | pass 10 |
| `F-CORE-ACT-13` | PASSED | overturn pass9 UNREACHABLE: detect_claude/detect_pi_family; every clause mapping tested | pass 10 |
| `F-CORE-ACT-14` | PASSED | WAITING/IDLE/WORKING keyword lists + boundary negatives ("already","reworking",codex-notes) tested | pass 10 |
| `F-CORE-ACT-15` | PASSED | overturn pass9 UNREACHABLE: content.rs matches Swift ScreenManifest; proceed/esc/y-n/confirm/nonmatch tested | pass 10 |
| `F-CORE-ACT-16` | PASSED | strip_ansi CSI+OSC(BEL+ST) test; content detector uses stripped text | pass 10 |
| `F-CORE-ACT-17` | PASSED | status priority tested; identity picks status-priority pane == Swift agentIdForWorktree (clause wording imprecise) | pass 10 |
| `F-CORE-ACT-18` | PASSED | running_agent_ids dedup + catalog-order test | pass 10 |
| `F-CORE-ACT-19` | PASSED | payload test; title=displayName—status matches Swift reference; clause "worktree label" inaccurate, port faithful | pass 10 |
| `F-CORE-ACT-20` | PASSED | NotificationPolicy tests cover all three suppression rules | pass 10 |
| `F-CORE-ACT-21` | PASSED | rows test: terminal+chat kept, doc/diff/browser omitted; None->Idle code-verified (test gap: no unrecognized-pane row) | pass 10 |
| `F-CORE-ACT-22` | PASSED | sorted+urgent_first tests; sort_by_key stable for ties | pass 10 |
| `F-CORE-ACT-23` | PASSED | requires_close_confirmation tested for all five states | pass 10 |
| `F-CORE-ACT-24` | half-proven | planner half tested (resumable/prunable split); end-to-end close/relaunch unexercised | pass 10 |
| `F-CORE-ACT-25` | half-proven | partition half tested (selected/open/deferred order); real launch remount unexercised | pass 10 |
| `F-CORE-ACT-26` | half-proven | ids_to_evict tested (cap/running/unsaved/selected); ZERO app callers — no eviction side effect exists | pass 10 |

### F-CORE — domain, files, usage, workspace (45)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CORE-DOM-01` | PASSED | project + both worktrees round-trip DB across restart | pass 2 |
| `F-CORE-DOM-02` | PASSED | defaults_prefer_explicit_values_then_primary_and_sibling test | pass 10 |
| `F-CORE-DOM-03` | half-proven | TILLER_PROJECTS_DIR->XDG->HOME deterministic code-verified; no test, no app caller | pass 10 |
| `F-CORE-DOM-04` | PASSED | 5 filter tests: branch match, case-insensitive, empty query, nested tab title | pass 10 |
| `F-CORE-DOM-05` | PASSED | ordering_ignores_unknown_and_noop_moves | pass 10 |
| `F-CORE-DOM-06` | PASSED | tab_order_wraps_and_numeric_selection_validates | pass 10 |
| `F-CORE-DOM-07` | PASSED | auto_naming_requires_first_run_or_both_throttles | pass 10 |
| `F-CORE-DOM-08` | PASSED | once_gate_runs_only_the_first_callback | pass 10 |
| `F-CORE-WSP-01` | PASSED | legacy_content_exposes_only_terminal_panes_and_chat_tab_activity | pass 10 |
| `F-CORE-WSP-02` | PASSED | ids tests + document_identity_is_worktree_scoped_and_resolves_symlinks | pass 10 |
| `F-CORE-WSP-03` | PASSED | empty layout + InvalidFraction(1001) tested; binary split code-verified | pass 10 |
| `F-CORE-WSP-04` | half-proven | all 8 commands + classify tested; ZERO app callers apply LayoutCommand (dead enum) | pass 10 |
| `F-CORE-WSP-05` | PASSED | command_classes_distinguish_structural_and_nonstructural_changes | pass 10 |
| `F-CORE-WSP-06` | PASSED | validate() covers full reject list; malformed snapshot falls back empty | pass 10 |
| `F-CORE-WSP-07` | PASSED | snapshots_are_versioned_canonical_and_malformed_data_falls_back_empty | pass 10 |
| `F-CORE-WSP-08` | half-proven | WorkspaceTabViewState has all fields; session store never persists view_state | pass 10 |
| `F-CORE-FILE-01` | PASSED | tree tests: path hazards, .git excluded, dirs-first localized sort | pass 10 |
| `F-CORE-FILE-02` | PASSED | classify tests: supported/unsupported/oversize/relative/absolute | pass 10 |
| `F-CORE-FILE-03` | half-proven | quoted string (spaces/quotes/non-ASCII) tested; ZERO app callers — never written to a pane | pass 10 |
| `F-CORE-FILE-03A` | N/A — platform | NSItemProvider loader Apple-only; no Linux multi-file ordered resolver (single-path classification covers Linux) | pass 10 |
| `F-CORE-FILE-04` | PASSED | link tests: relative/absolute/file URLs/line:col/scheme rejection/traversal | pass 10 |
| `F-CORE-FILE-05` | PASSED | wrap/prefix selection-preserving tests | pass 10 |
| `F-CORE-FILE-06` | half-proven | atomic save/reload/conflict/deletion tested; watcher->auto-reload wiring + rename event unexercised | pass 10 |
| `F-CORE-FILE-07` | PASSED | inotify monitor + real create/modify/remove events test | pass 10 |
| `F-CORE-FILE-08` | FAILED — absent | no per-file icon key lookup (only generic File/FolderFill icons) | pass 10 |
| `F-CORE-TERM-01` | PASSED | xterm byte table tested for all 9 keys | pass 10 |
| `F-CORE-TERM-02` | FAILED — absent | no terminal context menu at snapshot; context_menu.rs appeared in live worktree AFTER snapshot | pass 10 |
| `F-CORE-TERM-03` | PASSED | SplitTree split/remove-with-collapse/leaf enumeration tests | pass 10 |
| `F-CORE-SET-01` | half-proven | refresh clamp 60-3600 + TILLER_SOCKET_ENABLE tested; resume/autoname/translucency/retention/mount-cap/widths settings absent | pass 10 |
| `F-CORE-SET-02` | N/A — platform | TCC model macOS-only (matches F-SET-23 precedent) | pass 10 |
| `F-CORE-USG-01` | PASSED | from_percent clamps 0-100, non-finite->0; has_any; reducer tests | pass 10 |
| `F-CORE-USG-02` | PASSED | real wham capture + secondary window + labels + malformed + ANSI tests | pass 10 |
| `F-CORE-USG-03` | PASSED | accepts_only_the_expected_usage_percent_field | pass 10 |
| `F-CORE-USG-04` | PASSED | cookie normalization + workspace id + real react-flight capture test | pass 10 |
| `F-CORE-USG-05` | PASSED | CODEX_HOME+~/.codex, token merge-save, 8-day refresh tests | pass 10 |
| `F-CORE-USG-06` | half-proven | refresh_token + 401 reused/revoked/expired classification tested; no controlled HTTP success/other-error tests (live refresh would rotate real tokens) | pass 10 |
| `F-CORE-USG-07` | half-proven | fetch bounded + outcome mapping tested; no live backend exercise | pass 10 |
| `F-CORE-USG-08` | PASSED | LIVE pass10: real claude PTY fetch -> Success (session 4%, weekly 30%) in 11.6s; bounded/not-installed paths in code | pass 10 |
| `F-CORE-USG-09` | PASSED | reducer stale-on-timeout + provider catalog/preference tests | pass 10 |
| `F-CORE-AUTH-01` | PASSED | claude json + codex first-nonempty-line identity tests | pass 10 |
| `F-CORE-AUTH-03` | N/A — platform | Keychain macOS-only; no keyring store built; absence handled explicitly (unknown state test) | pass 10 |
| `F-CORE-UI-01` | PASSED | appearance_follows_system_only_in_system_mode | pass 10 |
| `F-CORE-UI-02` | PASSED | updater_reaches_every_user_visible_state_and_clamps_progress | pass 10 |
| `F-CORE-AUTH-02` | PASSED | exact non-GUI install command exposed + tested | pass 10 |
| `F-CORE-PLAT-01` | N/A — platform | macOS-15 manifest does not exist in the rewrite; VERIFY itself says reference gap | pass 10 |

### F-CTRL — control socket (34)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-CTRL-WIRE-01` | PASSED | NDJSON framing, id echo, sorted keys on the wire | pass 6 |
| `F-CTRL-WIRE-02` | half-proven — 1 MiB cap fuzzy by one 64 KiB chunk; 0600/SO_PEERCRED/recovery/concurrency proven | pass 6 | pass 6 |
| `F-CTRL-WIRE-03` | PASSED | fresh connection per round trip; canonical error | pass 6 |
| `F-CTRL-WIRE-04` | PASSED | TILLER_SOCKET wins; XDG default created 0600 | pass 6 |
| `F-CTRL-PANEL-01` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-02` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-03` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-04` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-05` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-06` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-07` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-08` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-PANEL-09` | PASSED | create/split/list/write/key/read/wait/focus/close live on the wire | pass 6 |
| `F-CTRL-NOTIFY-01` | PASSED | agent statuses accepted, unknown rejected; user mode listable | pass 6 |
| `F-CTRL-NOTIFY-02` | half-proven — extraction proven via real claude hook; ambiguity resolved, not rejected | pass 6 | pass 6 |
| `F-CTRL-SESSION-01` | PASSED | session.ref survived relaunch; resumed real claude session | pass 6 |
| `F-CTRL-WORK-01` | FAILED — defective | worktree.set comment gone after relaunch; no comment column | pass 6 |
| `F-CTRL-SYS-01` | half-proven — ping answers {status:ok}, not documented {pong:true}; CLI prints pong either way | pass 6 | pass 6 |
| `F-CTRL-SYS-02` | PASSED | explicit workspace / TILLER_PANE_ID env / fallback / no-context live | pass 6 |
| `F-CTRL-WORK-02` | PASSED | rows sorted, selected flag live | pass 6 |
| `F-CTRL-WORK-03` | PASSED | non-git project rejected with distinct error | pass 6 |
| `F-CTRL-WORK-04` | PASSED | select by id and exact path, current, no-selection error | pass 6 |
| `F-CTRL-WORK-05` | half-proven — row retained/selected false on close; PTY not terminated (sleep alive, process-group leak) | pass 6 | pass 6 |
| `F-CTRL-NOTIFY-03` | PASSED | create/list/clear in-memory; system posting platform N/A | pass 6 |
| `F-CTRL-SESSION-02` | PASSED | restore-session returns success, no row duplication | pass 6 |
| `F-CTRL-BROWSER-01` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-02` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-03` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-04` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-05` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-BROWSER-06` | N/A — platform | documented unsupported responses, all 10 methods exercised | pass 6 |
| `F-CTRL-CLI-01` | half-proven — all 25 subcommands map correctly; documented --socket placement broken | pass 6 | pass 6 |
| `F-CTRL-CLI-02` | FAILED — absent | no shim/install mechanism, bare tillerctl relies on PATH | pass 6 |
| `F-CTRL-PLAT-01` | PASSED | platform-neutral serde/libc manifest; same NDJSON on Linux | pass 6 |

### F-AGENT — agent adapters (20)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-AGENT-API-01` | half-proven — catalog + hook flags exact; trait has no summarizer method | pass 6 | pass 6 |
| `F-AGENT-CLAUDE-01` | PASSED | real claude TUI in pane; both hooks fired; ref resumed | pass 6 |
| `F-AGENT-CLAUDE-02` | PASSED | merge semantics: only 5 hook events replaced, rest byte-identical | pass 6 |
| `F-AGENT-CLAUDE-03` | PASSED | claude -p ran live | pass 6 |
| `F-AGENT-CODEX-01` | PASSED | no user-global writes; -c notify override loads in real codex (no TOML trap) | pass 6 |
| `F-AGENT-CODEX-02` | PASSED | codex exec --output-last-message ran live | pass 6 |
| `F-AGENT-OPENCODE-01` | UNREACHABLE — opencode not installed | command -v empty; env verified 14:27 | pass 9 |
| `F-AGENT-OPENCODE-02` | UNREACHABLE — opencode not installed | command -v empty; env verified 14:27 | pass 9 |
| `F-AGENT-OPENCODE-03` | UNREACHABLE — opencode not installed | command -v empty; env verified 14:27 | pass 9 |
| `F-AGENT-PI-01` | PASSED | bare launch + resume shape; pi --print ran live | pass 6 |
| `F-AGENT-PI-02` | PASSED | bare launch + resume shape; pi --print ran live | pass 6 |
| `F-AGENT-OMP-01` | UNREACHABLE — omp not installed | command -v empty; env verified 14:27 | pass 9 |
| `F-AGENT-OMP-02` | UNREACHABLE — omp not installed | command -v empty; env verified 14:27 | pass 9 |
| `F-AGENT-OMP-03` | UNREACHABLE — omp not installed | command -v empty; env verified 14:27 | pass 9 |
| `F-AGENT-SAFE-01` | half-proven — worktree-local half measured (fake HOME unchanged); skill-provisioning half absent | pass 6 | pass 6 |
| `F-AGENT-SAFE-02` | NOT EXERCISED | P47 builder claim: `ClaudeHookMigrator` rewrites only stale quoted leading tillerctl paths, preserves other hook/JSON data, and no-ops for current/unrelated/malformed files; focused `session_sources` tests green. App launch/session wiring still needs to invoke the migrator; independent critic verification required | builder-claimed, unverified |
| `F-AGENT-SESSION-01` | NOT EXERCISED | P47 builder claim: `AgentSessionValidator` checks Claude’s sanitized project path and Codex’s recursive reference-containing rollout files while trusting uncheckable agents; focused `session_sources` tests green. App resume flow still needs to call the validator; independent critic verification required | builder-claimed, unverified |
| `F-AGENT-SESSION-02` | NOT EXERCISED | P47 builder claim: Claude JSONL and recursive Codex rollout readers join string/text blocks, including nested Codex payloads; focused `session_sources` tests green. Transcript/history surface wiring still needs to consume these sources; independent critic verification required | builder-claimed, unverified |
| `F-AGENT-SESSION-03` | PASSED | shell_quote/json_string_literal round-trips, no slash-escaping | pass 6 |
| `F-AGENT-PLAT-01` | PASSED | platform-neutral manifest, plain CLI processes, worktree-local outputs | pass 6 |

### F-GIT — git tier (16)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-GIT-RUN-01` | PASSED | success/failure/timeout/launch-failure; deadline enforced; no output limits/cancellation | pass 5 |
| `F-GIT-RUN-02` | FAILED — absent | no streaming runner (synchronous whole-output) | pass 5 |
| `F-GIT-REPO-01` | PASSED | .git dir/file true, non-repo false, has_head 0/1 | pass 5 |
| `F-GIT-BRANCH-01` | FAILED — absent | no git branch --list (only --show-current) | pass 5 |
| `F-GIT-WT-01` | PASSED | create with/without base, duplicate refusal, dirty-removal refusal, clean removal | pass 5 |
| `F-GIT-CLONE-01` | FAILED — absent | no clone | pass 5 |
| `F-GIT-REMOTE-01` | FAILED — absent | no GitHub-remote parsing | pass 5 |
| `F-GIT-STATUS-01` | PASSED | porcelain-v2 -z parse matches v1: MM/rename/UU/spaces | pass 5 |
| `F-GIT-STATUS-02` | FAILED — absent | no directory-status aggregation/precedence | pass 5 |
| `F-GIT-ACT-01` | PASSED | stage/unstage/stage_all/discard/discard_all verified in git after each call | pass 5 |
| `F-GIT-ACT-02` | half-proven — invalid paths rejected by git; no pre-validation: conflicted path stages markers | pass 5 | pass 5 |
| `F-GIT-DIFF-01` | PASSED | tracked/untracked/binary/added/deleted/renamed/no-HEAD hunks correct | pass 5 |
| `F-GIT-DIFF-02` | PASSED | tracked/untracked/binary/added/deleted/renamed/no-HEAD hunks correct | pass 5 |
| `F-GIT-DIFF-03` | FAILED — absent | no side-by-side pairing code | pass 5 |
| `F-GIT-DIFF-04` | half-proven — numstat matches git; no-HEAD staged files show 0/0; untracked cap is 500,000 bytes not lines | pass 5 | pass 5 |
| `F-GIT-PLAT-01` | PASSED | git via PATH; whole flow on Linux; process-group kill on timeout | pass 5 |

### F-PERSIST — persistence package (13)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-PERSIST-DB-01` | PASSED | open(path)+in_memory; forward-only v1-v5 tested w/ rows intact; corrupt/empty/truncated/newer-schema refused; 64MiB cap; concurrent first-open+writers tested (22 it-tests) | pass 10 |
| `F-PERSIST-DB-02` | FAILED — absent | schema has projects/worktrees/tabs/tab-state/settings/sidebar/session-refs; absent: legacy tabs, agent accounts, browser content, chat sessions/items, context usage, permissions, quarantine | pass 10 |
| `F-PERSIST-DB-03` | half-proven | project half: all 11 fields+order round-trip (test); worktree half: comment+timestamps not persisted anywhere (runtime-only) | pass 10 |
| `F-PERSIST-DB-04` | PASSED | SessionTabState scrollback byte-exact round trip + 256KiB bound at encode AND decode (tests) | pass 10 |
| `F-PERSIST-DB-05` | FAILED — absent | no legacy terminal tab records; no chat session/item persistence anywhere | pass 10 |
| `F-PERSIST-DB-06` | half-proven | session half: v5 refs upsert/load/delete + survive reopen (tests); agent account records absent | pass 10 |
| `F-PERSIST-DB-07` | half-proven | tab-state snapshot round-trips (own schema table test); quarantine mechanism absent — corrupt state dropped, not quarantined | pass 10 |
| `F-PERSIST-DB-08` | half-proven | CRUD/cascade/reorder tested; primary exclusivity NOT enforced; exact-path lookup not in the store (runtime only) | pass 10 |
| `F-PERSIST-DB-09` | half-proven | tab-set replace + active normalization + v3 pre-invariant reconciliation tested; per-record corrupt-tab skip absent | pass 10 |
| `F-PERSIST-DB-10` | PASSED | session_references_upsert_load_and_delete + survive store reopen (tests) | pass 10 |
| `F-PERSIST-DB-11` | half-proven | Linux runner migrates old->current with rows intact, forward-only, NewerSchema refusal (tested); named v18 content absent — different schema lineage (v1-v5) | pass 10 |
| `F-PERSIST-DB-12` | UNREACHABLE — no terminalTab table in the Linux schema | no v17 rename exists; the described mismatch cannot manifest in this lineage | pass 10 |
| `F-PERSIST-PLAT-01` | PASSED | $TILLER_DB->checkout-scoped(XDG)->user-wide tested; creation/migration/concurrency exercised on Linux path (two-process tests) | pass 10 |

### F-TERM — terminal package (17)

| id | verdict | evidence | judged |
|---|---|---|---|
| `F-TERM-PTY-01` | half-proven — live shell via PTY, input→exec→output; exit-display path unexercised | pass 1 | pass 1 |
| `F-TERM-PTY-02` | half-proven — exits 0/3/137 live; signal path crate-test green, unreachable via sh | pass 4 | pass 4 |
| `F-TERM-PTY-03` | half-proven — TILLER_PANE_ID env context live (pass 6); env override/TERM unverified | pass 6 | pass 6 |
| `F-TERM-REG-01` | PASSED | panel create/split/list/write/read/wait/focus/close live | pass 6 |
| `F-TERM-REG-02` | half-proven — wait exit/timeout/unknown live; close-cancels-registration unverified | pass 6 | pass 6 |
| `F-TERM-SCR-01` | PASSED | 262144 bytes exactly; newest line kept, oldest evicted | pass 4 |
| `F-TERM-SCR-02` | FAILED — absent | no 200ms settle / 120ms resize debouncer anywhere in the terminal crate | pass 10 |
| `F-TERM-PTY-04` | half-proven — shell spawned in worktree (bash); $SHELL preference/terminfo/first-resize unverified | pass 1 | pass 1 |
| `F-TERM-PTY-05` | half-proven — input→exec→output live; content-match/session-append unverified | pass 1 | pass 1 |
| `F-TERM-PTY-06` | FAILED — absent | terminal_file_drop has zero callers; no drop-to-pane path | pass 10 |
| `F-TERM-PTY-07` | FAILED — absent | no stable-host/generation/teardown abstraction | pass 10 |
| `F-TERM-PTY-08` | FAILED — absent | no pane cache | pass 10 |
| `F-TERM-SPLIT-01` | half-proven — right/down splits + layout persistence live; 6 px divider/160 px min pixels owed | pass 4 | pass 4 |
| `F-TERM-UI-01` | FAILED — absent | no context menu in terminal view (grep) | pass 7 |
| `F-TERM-UI-02` | FAILED — absent | no URL router | pass 10 |
| `F-TERM-USG-01` | half-proven — hidden claude PTY fetch ran live; lock/timeout machinery unverified | pass 1 | pass 1 |
| `F-TERM-PLAT-01` | PASSED | gpui + alacritty_terminal only; no webview/HTML renderer | pass 10 |

## Totals

| verdict | count |
|---|---|
| PASSED | **88** |
| half-proven | **46** |
| FAILED — absent | **90** |
| FAILED — defective | **4** |
| UNREACHABLE | **9** |
| N/A — platform | **17** |
| NOT EXERCISED | **127** |
| NOT EXERCISED — blocked on display | **7** |
| **total** | **388** |

Entries never independently judged by the critic: **120**
(**80** builder-claimed, unverified; **40** never claimed by anyone).
`NOT EXERCISED — blocked on display` and `UNREACHABLE` rows *are* critic-judged
(as appearance-blocked / environment-impossible), so they are not counted in
the never-judged figure.
