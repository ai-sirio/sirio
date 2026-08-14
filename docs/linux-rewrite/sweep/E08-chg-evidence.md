# E08-chg evidence — Wayland lane (drive-E08-chg)

Fixture: `/tmp/e08chg-fixture` — fresh git repo, `staged.txt` (staged M), `changed.txt`
(unstaged M), `untracked.txt` (untracked). All drives on instance
`TILLER_WL_LABEL=drive-E08-chg`, socket `/tmp/drive-E08-chg.sock`.

## F-CHG-06 (ledger 197, half-proven)

Missing half per manifest: symbol-per-status mapping and an explicit Files refresh.

- `project.add path=/tmp/e08chg-fixture` then `surface.changes.open` — first read after open
  returned `loading:true` and stale 0/0/0 (open just arms the tab; it does not itself return
  fresh data). `surface.changes.read` on the same tab then returned the correct per-section
  counts: Staged=1 (`staged.txt`), Changed=1 (`changed.txt`), Untracked=1 (`untracked.txt`).
- Drove a real state transition from outside the app: `git add untracked.txt` in the fixture
  repo, waited 2s, called `surface.changes.read` again with **no UI action** — the app's own
  1s periodic refresh (`CHANGES_REFRESH_INTERVAL`, `changes.rs:63`) picked it up on its own:
  Staged became `[staged.txt, untracked.txt]` (count 2), Untracked dropped to `[]` (count 0),
  Changed unchanged. This is the "explicit Files refresh" owed by the manifest — there is no
  manual refresh button in `changes.rs` (only the periodic timer armed by `ensure_refresh`);
  the refresh is automatic and this drive exercised it end-to-end from a real git mutation to
  a re-read reply that reclassified the file.
- Pixel evidence: captured `chg06-before-1715.png` (untracked state) and `chg06-after-1715.png`
  (staged state) at identical 1715x972 resolution after a forced repaint each side, both after
  `tab.select index=2` brought the Changes surface forward. `convert -compose difference`
  between them: stddev 5.07, mean 0.80 — nonzero, i.e. the frame visibly changed in response to
  the git-add + auto-refresh with zero manual UI action, confirming the pixel side moves with
  the data side, not just the socket reply.
- **Not closed**: I cannot see the captures (text-only agent) and can only measure per-region
  stddev, not identify which glyph (checkmark/dot/letter) is drawn next to each status. The
  symbol-to-status *identity* mapping — does Staged draw a different glyph than Changed than
  Untracked — is unverified by this drive; a sighted pass or a named drawn test is still owed
  for that specific claim. The drawn-test tier itself was already noted owed in the ledger and
  remains so.
- Captures: `reference/linux-progress/drive-E08-chg/chg06-symbols-before.png`,
  `chg06-symbols-after.png`, `chg06-before-1715.png`, `chg06-after-1715.png`.

Claim: `partially-exercised` — data half and the explicit-refresh half now fully driven and
discriminating (a genuine external git mutation reclassified via the app's own refresh, not a
default/no-op state); the glyph-identity half remains unverified because it requires sighted
comparison this drive cannot perform.
