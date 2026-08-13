# manifest — pass 15 display session (pireview)

Frames from live Tiller instances on :1 via import -window, PID-matched.
Binary: rebuilt 22:41 (post-P73 tree; workspace red earlier in the pass was
codex11's uncommitted P72 gtk/wry spike — orchestrator installed the system
libs and the build recovered).

| file | evidence for | how produced |
|---|---|---|
| g8-00..03.png | F-SID-19: ctrl-t ×3 (after a click gave the app X focus) — strip grows a chip per press; DB: 14 terminal tabs | click strip → key ctrl+t ×3 |
| h2-03/04.png | F-CHG-05: file-row click selects, Down/Up move the selection | right-panel Files, click + keys |
| h3-02-changes.png | F-CHG-11: Changes surface sections render with right-edge header controls | + menu → Changes |
| h4-02-after-header-click.png | F-CHG-11: staged-header control click = 36k px view change, git status unchanged (collapse) | click the staged header control |
| i1-02-streaming.png / i1-03-after-stop.png | F-CHAT-08: 20 transcript bands mid-stream; stop click → 9.2k px change → 19 bands stable | claude chat, long-reply prompt, stop click |
| j3-01-settings.png / j3-02-after-escape.png | F-SET-02: settings opened via socket; Escape closed it live (634k px) | tillerctl surface settings open → Escape |
| l1-01-picker-bare-path.png | F-TAB-08: bare PATH → two-line fallback in the New Chat submenu | PATH=/usr/bin:/bin relaunch |
| k3-01-running / k3-02-done.png | F-TERM-PTY-05: sleep-8 running vs done frames byte-identical — no activity signal | tillerctl panel write + captures |

Harness facts of this pass (not app findings): right-click (XTEST button 3)
does not reach the app on this display session; keyboard chords land only
after a real click gives the app X focus; the portal file picker is
Wayland-side and invisible to X captures.
