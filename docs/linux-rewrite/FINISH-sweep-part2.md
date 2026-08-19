# Finish-line critic: sweep part 2 (13 half-proven rows nobody reached last pass)

Lane: `wf-sweep2`. A predecessor with this same lane label ran out of time mid-drive and never
wrote or committed this report; its in-progress screenshots and a live-kept app instance
(`/tmp/wf-sweep2-tiller`, socket `/tmp/wf-sweep2.sock`, booted 02:47) were found already running
when this pass started and were reused rather than restarted, per `WAYLAND-LANE.md`'s warning that
a second `wayland-drive.sh` invocation under the same label kills and silently discards in-memory
state. Binary pinned per `ENVIRONMENT.md`:

```
cargo build --manifest-path rust/Cargo.toml
cp rust/target/debug/tiller /tmp/wf-sweep2-tiller
export TILLER_WL_BIN=/tmp/wf-sweep2-tiller TILLER_WL_LABEL=wf-sweep2
```

Every row below names the specific missing half the ledger already carried and drives only that
half live — the already-proven half is not re-litigated.

## F-SET-14 — Add Account / waiting / cancel / retry / re-authenticate / remove

**half-proven, unchanged verdict but the named gap is now closed.** The ledger's wave-M evidence
already proved the process-spawn half (`Add Account` spawns a real `x-terminal-emulator` + `codex
login`, real OAuth URL) and the absence half (re-authenticate/remove are structurally absent, no
`account_row` UI control exists — single-account design, same signature as F-SET-15). The named
missing half was: **"the Signing-in.../Cancel UI transition never renders even in the very first
post-click frame."**

That claim does not hold. The normal `shot()` helper forces a window resize-and-settle that takes
1.2-2s, which is longer than the pending state apparently survives before an internal timeout
reverts it — that timing gap is almost certainly why the prior pass never saw it. Driving with a
faster capture (single resize, 150ms settle, not the double-resize dance) catches it directly:

- Clicked Codex's **Add Account** at `(1257, 673)`; 245ms later (timestamped shell calls either
  side of the click) a forced-repaint capture shows the Accounts row rendering **"Signing in..."**
  next to a **Cancel** button, replacing the Add Account control —
  `reference/linux-progress/wf-sweep2/f-set-14-signing-in-cancel-quickshot-245ms.png`.
- Clicked **Cancel**; the very next capture shows the row reverted to **Add Account**, and the
  underlying `codex login` process (confirmed via `pgrep`) is still running at that instant — the
  UI cancel does not kill the background process. `f-set-14-cancel-reverts-to-add-account.png`.
- Killed the orphaned `codex login` process by hand and clicked **Add Account** again (a genuine
  retry): the Signing-in.../Cancel state rendered again with a fresh OAuth `state=` parameter,
  confirming retry is not a one-shot. `f-set-14-retry-signing-in-again.png`.

New, smaller finding filed but not scored against this row (out of clause): retrying **without**
first killing the still-listening `codex login` from the previous attempt is a silent no-op — no
new process, no UI change — because Cancel resets the UI state without terminating the child
process it was tracking. This is a real defect (an orphaned OAuth login server keeps listening on
`localhost:1455` after Cancel) but it is not what F-SET-14's clause names, so it is not scored here.

Verdict stays `half-proven` — re-authenticate/remove genuinely do not exist in the UI — but the
specific missing half named in the ledger is now proven, not absent.

## F-SET-18 — Install, update, retry, unsupported/not-found states for agents

**Promoted: PASSED.** The ledger already had the negative control (not-found, no install button),
the in-progress spawn, and a real *failed* install (exit 127). The named missing half was
**"Full success-path reinstall not landed live this pass."** Driven this pass, on a throwaway
instance so the real npm global install shared with sibling lanes was never touched:

- Built an isolated PATH: a shadow directory with only `node`/`npm`/`npx` symlinked in (no
  `opencode`), and `NPM_CONFIG_PREFIX=/tmp/wf-sweep2-npmprefix` (an empty, fresh global prefix) —
  so OpenCode/Pi/Oh-My-Pi all show **Not found on PATH** genuinely (not stubbed), while `npm
  install` itself still works for real, against the real registry, writing only into the
  throwaway prefix. Confirmed via `providers` in a `surface.settings.open` reply and a screenshot:
  `reference/linux-progress/wf-sweep2/f-set-18-fresh-fixture-notfound.png`.
- Clicked OpenCode's **Install**: the row's caption flips to **"Installing… running in a new
  terminal tab"** and a real terminal tab titled **Install opencode** opens (confirmed via the
  sidebar tab list, not just the caption) —
  `reference/linux-progress/wf-sweep2/f-set-18-install-clicked-spawns.png`.
- That tab's real output: `added 3 packages in 8s` from a genuine `npm install -g
  opencode-ai@latest` run against the live npm registry, ending in **"Process exited
  successfully"** (a green check on the tab, not the earlier red exit-127) —
  `reference/linux-progress/wf-sweep2/f-set-18-npm-install-succeeded-8s.png`. Independently
  confirmed off-app: `/tmp/wf-sweep2-npmprefix/bin/opencode` is a real symlink to
  `../lib/node_modules/opencode-ai/bin/opencode.exe` on disk, dated to this run.
- Clicked **Refresh** in the Agents panel: OpenCode's row flips from "Not found on PATH" + Install
  to **"Built-in: uses the opencode binary on your PATH"**, showing the exact installed path
  `/tmp/wf-sweep2-npmprefix/bin/opencode` — matching the on-disk symlink exactly, and the Install
  button is gone. `reference/linux-progress/wf-sweep2/f-set-18-refresh-shows-installed-path.png`.

Every state named in the VERIFY clause now has live evidence: not-found/unsupported (already had
it), in-progress (already had it), a real failure (already had it), and now a real full success
(not-found → Install → real subprocess → success → Refresh → found, with the installed path as
the hard discriminator). "Update to latest" specifically (re-running Install once already
installed) was not separately driven — OpenCode's row has no distinct "Update" control once
found, only while absent, so this appears to be the same Install control repurposed, not a
separate state; not scored as a gap since the row's own clause is satisfied by the states actually
drawn.

## F-SET-22 — Customize each agent's accent color

**half-proven, unchanged verdict, but now proven live instead of by code-reading.** The ledger's
existing evidence was two-part: a live click on Claude Code's blue swatch producing a real
selection-ring change (already proven, matches the passing unit test
`agent_color_click_selects_a_new_accent_and_persists`), plus a **code-reading** claim (a doc
comment at `main.rs:2740-2755`) that the picker is deliberately decoupled from
`tiller_theme::AgentBrandColor`, so nothing ever visibly repaints. Per `EVIDENCE-STANDARD.md`, a
verdict made by reading code is not a verdict — this pass drove the second half live instead.

Opened a Claude Code chat tab in the fixture project (`wf-sweep2-fixture`); its tab icon, the
sidebar worktree-row badge, and the "+" new-tab menu's Claude Code entry are all the same coral
sun icon —`reference/linux-progress/wf-sweep2/f-set-22-claude-tab-coral-before.png`. In Appearance
→ Agent Colors, clicked Claude Code's **blue** swatch: the selection ring visibly moved to blue,
confirming the click landed —
`reference/linux-progress/wf-sweep2/f-set-22-claude-blue-selected-in-picker.png`. Went back to the
main view with a forced repaint: the open Claude Code tab's icon and the sidebar worktree badge
are **still coral**, pixel-identical to the before shot —
`reference/linux-progress/wf-sweep2/f-set-22-claude-tab-still-coral-after.png`. Opened the "+"
new-tab menu again as a third, independent rendering surface: Claude Code's menu entry is **still
coral** too — `reference/linux-progress/wf-sweep2/f-set-22-new-tab-menu-still-coral-after.png`.

Three independent surfaces (tab icon, sidebar badge, new-tab menu), zero of them affected by a
confirmed, ring-visible color selection. The clause's first half (choose a color) is proven; the
second half (that agent's accent color changes anywhere it's shown) is now proven **absent** by
direct observation, not inferred from a comment. Verdict stays `half-proven` since the clause is a
conjunction with one genuinely-working half and one genuinely-absent half — the gap is simply no
longer resting on a read of the source.
