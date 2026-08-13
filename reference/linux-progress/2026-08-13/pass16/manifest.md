# manifest — pass 16 (pireview)

| file | evidence for | how produced |
|---|---|---|
| p73-ps-live.txt | P73 new-chat half: codex-acp spawned for the Codex tab, claude-agent-acp only for the NULL-agent Chat tab | ps during a live Codex chat |
| p73-ps-relaunch.txt | **P73 restore half, the proof**: after quit+relaunch with the same DB, exactly one codex-acp (the Codex tab) and one claude-agent-acp (the NULL-agent Chat tab) — no wrong-agent processes | ps after relaunch |
| p73-02-codex-tab.png | Codex chat tab live | picker → Codex → capture |
| chg6-01/02-before-after-refresh.png | F-CHG-06: Files tree rows with status symbols; explicit Refresh click under load | git fixture + Refresh click |
| set-01-appearance.png | F-SET-19/20: Appearance page reachable via socket (theme-segment clicks unresponsive under load 30) | socket settings open/select |

DB transcript: /tmp/critic16-p73.sqlite — tab-2 `agent_id='codex'` (the pass-14 NULLs are populated).
Harness: right-click cap reached (2 attempts, focus-first included) — chronicled in
ENVIRONMENT.md and findings; not an app defect.
