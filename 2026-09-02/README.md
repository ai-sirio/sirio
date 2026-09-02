
## scroll-*
Mouse wheel over the terminal pane does not move the scrollback (3 attempts, 2 panes, line and pixel deltas). Shift+PageUp reaches the shell as `;2~`.

## usage-*
Status bar usage badges: "Codex logged out" shown all session while codex is authenticated with an API key (auth.json has auth_mode + OPENAI_API_KEY, no tokens). Claude badge cycled through "logged out" (20:13) and "timed out" (20:23-20:27) under heavy CPU load before returning to the percentage.

## freeze-fix-*
Retest of #338 on main 9e21dc7d: no freeze (tab click accepted during output), but the pane stays on the frame captured at ~2 s (299958-299995) while the shell had already printed the total and the prompt (sirioctl scrollback). Stale final frame after a burst.

## tcc-*
Privacy prompt cascade at startup (Documents, Apple Music, Downloads; also Desktop, network volumes, Photos in other launches).
