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
