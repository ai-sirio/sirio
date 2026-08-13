# manifest — pass 14 display session (pireview)

Frames captured from live Tiller instances on :1 via import -window, PID-matched,
GPUI_X11_SCALE_FACTOR=1. Colour counts pass MIN_COLORS on every frame.

| file | evidence for | how produced |
|---|---|---|
| m2-01-picker-open.png | F-TAB-07 picker gating live: exactly two chat rows (Claude Code, Codex) — pi excluded by the acp_program filter | + menu → New Chat → capture |
| p1-02-codex-tab.png | New Chat → Codex opens a chat tab (content bg leaves #151515) | picker row 2 click → capture |
| s1-01-chat-open.png / s1-02-typed-noclick.png | NEW FINDING: composer not auto-focused — typed NOFOCUS-PROBE-7777 before any click renders nothing | type before click → capture |
| s1-06-after55.png | claude chat: user card + reply rendered in the transcript | composer click → type nonce → Return → +55s |
| q1-ps.txt | Codex per-adapter command live: codex-acp → codex.js app-server → codex app-server (the fix, verified by process identity) | ps during a Codex chat |
| n2-ps.txt | GAP 1 live: 4× claude-agent-acp at one launch — one per restored chat tab incl. a Codex tab (restore = Claude default) | ps during restore-heavy launch |
| n1-ps.txt | claude chat spawn: npm exec @agentclientprotocol/claude-agent-acp | ps during a Claude chat |

Machine transcripts (not pixels):
- ~/.codex/sessions/2026/08/13/rollout-2026-08-13T22-13-58-019ffcc2….jsonl — prompt
  "Reply with exactly CRIT14-CODEX-DONE-3317…" in, reply "CRIT14-CODEX-DONE-3317" out.
- ~/.claude/projects/-tmp-critic14-cwd/514189c4….jsonl — prompt
  "Reply with exactly CRIT14-DONE-8219…" in, reply "CRIT14-DONE-8219" out.
- /tmp/critic14.sqlite — 5 tabs; tab.agent_id column (P70) present, all NULL.
