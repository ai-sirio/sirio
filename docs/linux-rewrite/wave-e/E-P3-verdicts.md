# E-P3 critic verdicts

Critic pass, independent of the builder — everything in `E-P3-report.md` was treated as a claim to
verify, not evidence. Instrument: `Scripts/wayland-drive.sh` (labels `e-p3-14*`, `e-p3-06*`), fresh
frames only, none cited from `sweep-D*`. Both rows' real acceptance clauses were re-read from
`docs/linux-rewrite/01-inventory-app.md` rather than taken from the brief's paraphrase.

## `F-PRJ-14` — PASSED (LocalPng arm; GitHub/Favicon gap confirmed real and unchanged)

Bypassed the native file-picker (unreachable headlessly, confirmed) by seeding the persisted DB
directly: `project.add`'d a real repo, then wrote `icon_kind='avatar'`,
`icon_value='png:<32x32 solid-red test PNG>'` into the `project` table's row via `sqlite3`-free
python, then relaunched the same instance against the same DB (`TILLER_DB` reused via a fixed
`TILLER_WL_LABEL`). This reaches exactly the state `choose_local_png` would commit, without needing
the dialog.

Discriminating evidence, both render call sites, before/after the same project:
- **Sidebar row glyph**: default (Symbol::Folder) renders a coral folder-outline icon
  (`e-p3-14-shots1/02-added.png`). After the DB edit, the same row renders a solid filled red
  circle — the actual PNG content, not a glyph (`e-p3-14-shots2/02-loaded.png`, confirmed by pixel
  read: default renders a stroked outline shape, the avatar renders a uniform-red filled disc).
- **Icon picker preview**: opened the real Project Settings sheet live (gear icon, found by pixel
  diff against a non-hover frame, not guessed) → Avatar tab already selected, showing a 48×48 solid
  red square above "Current: e-p3-avatar-test.png" (`e-p3-14-shots5/04-post2.png`) — the exact
  render path added in `project_identity.rs:1005-1016`.
- **GitHub/Favicon unchanged**: same screenshot shows "Use GitHub Avatar" and "Use Favicon" as
  text-field+button rows with no image preview slot at all — confirms the builder's claim that this
  half is a genuine, not-yet-buildable gap (no HTTP client anywhere in the app), not a quiet
  regression.

Both sidebar-row and picker-preview render sites were driven live to a state the app cannot reach by
default, with a real image (not a placeholder) visible in both. GitHub/Favicon remain caption-only,
matching the builder's own scoped claim.

## `F-PRJ-06` — PASSED (both guard halves proven live; overturns the ledger's char-drop finding)

The ledger's current `FAILED — defective` text is entirely about URL-field character loss, not
about the row's actual VERIFY clause (`01-inventory-app.md:45`: "confirm Clone project is disabled
[on empty URL]; start a clone and confirm it cannot be started twice"). Verified both, independently
of the builder.

**Char-drop is a lane-pacing artifact, independently reproduced both ways.** Same exact action
sequence (`click`+`+`, Clone Repository…, click URL field, `type` a 36-char URL), only the settle
time after `type` varied:
- Single `shot` immediately after `type`: field reads `https://example.` — 16/36 chars
  (`e-p3-06-short/05-d.png`), reproducing the ledger's symptom exactly.
- Same sequence with 3 extra trailing `shot` calls (no code involved, only wall-clock): field reads
  the full `https://example.test/repository.git`, Destination correctly derives
  `/home/enzopalmisano/repository` (`e-p3-06-long/08-g.png`). `project_forms.rs` diff is empty
  (confirmed via `git show` on the builder's claimed commit — there is none; no code changed this
  row).

**Empty-URL disables the button** — visible contrast confirmed: empty field shows dimmed "Clone
repository" text (`e-p3-06-long/04-c.png`); full valid URL shows bright/enabled text
(`e-p3-06-long/08-g.png`).

**Double-submit-while-Running guard, proven live for the first time** — this half was previously
unreachable because `git clone` fails in well under a frame with no network. Put a `sleep 4`
wrapper in front of the real `git` on `$PATH` (confirmed bare `"git"` is resolved via `PATH`,
`tiller_git/src/git.rs:48`) so `Running` survives across two real, separately-dispatched clicks at
the button's actual (resolution-corrected — button y differs between the lane's two alternating
output sizes, verified by pixel-scanning both) coordinates. Result: the shim's log shows exactly
**one** `clone-start` line despite two clicks; the popover shows "Cloning… 0%" in accent color with
a real progress bar, and the button itself is now in its disabled/muted state
(`e-p3-06-g2/09-g.png`). This is the live version of the already-unit-tested
`clone_state_disables_empty_url_and_double_submission` — now demonstrated end-to-end with a real
subprocess, not just the state machine in isolation. No leftover `~/repository` directory (checked
and removed).

All test artifacts (temp DB, temp PNG references, shim, cloned-dir) were scratch-only or cleaned up;
no changes made to the app tree.
