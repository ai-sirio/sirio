# Wave E slice E-P3 — report

## `F-PRJ-06` — Clone form URL field char-drop — recorded diagnosis did NOT hold; behaviour is already correct

**Verdict: already-correct.** The wave-D critic's diagnosis ("the Clone form's own per-keystroke
Destination-recompute" drops characters) was tested directly with instrumentation and disproven.

Method: temporarily added `eprintln!` timestamps inside `CloneForm::on_url_key` and
`CloneForm::render` (removed before commit — `git diff` on `project_forms.rs` is empty), then drove
the real popover with `Scripts/wayland-drive.sh` (`ctl project.add` → click `+` → click "Clone
Repository…" → click the URL field → `type` a 37-char URL).

- With the lane's normal fixed ~1s post-`type` settle, the log showed only the first 10 characters
  ever reached `on_url_key` (`https://ex`) before the script's cleanup killed the process — this
  reproduces the critic's "massive char drop" exactly.
- With `TILLER_WL_KEEP=1` (process not killed) or with several extra `shot` calls inserted after
  `type` (each adds ~1s of real wall-clock time via its resolution-flip + sleep), **every single
  keystroke arrived and was appended correctly** — 35–36 of 36 `on_url_key` calls logged, final
  `self.state.url()` exactly equal to the typed string, `Destination` correctly derived
  (`/home/…/repository`), `can_submit()`/status behaving exactly as the state-machine unit tests
  already assert.
- The measured cause is a **uniform ~120–130ms gap between successive keystroke deliveries**,
  present regardless of which characters were typed — consistent with this headless
  pixman/lavapipe-backed Wayland lane pacing input dispatch to its own slow software-composited
  frame rate (~8 fps), not with any per-keystroke cost inside `project_forms.rs` (the handler and
  the following render both complete in 1–2ms per the same timestamps). A 36-44 char field simply
  needs ~4.5s of real time to finish typing in this lane; the script's fixed ~1s settle window after
  `type` does not scale with string length, so longer fields falsely read as "dropping" characters
  while shorter ones (e.g. the 18-char Projects filter the critic used as a control) happen to fit.

No code change was made in `project_forms.rs` (confirmed empty diff) — the field, its focus
handling, and `CloneFormState` are correct as they stand.

**howToExercise:** `Scripts/wayland-drive.sh <dir> 'ctl project.add path=<abs>; click 293 51; shot
a; click 188 112; shot b; click 160 411; shot c; type https://example.test/repository.git; shot d;
shot e; shot f; shot g'` (the trailing `shot` calls are load-bearing — each buys ~1s more real
delivery time before the process is torn down) — the final frame shows "Repository URL" reading the
full typed string verbatim and "Destination" deriving `/…/repository` from it. A single `shot`
immediately after `type` will *look* like a regression; it is measuring the lane's frame rate, not
the app.

## `F-PRJ-14` — GitHub/PNG avatar never rendered as an image — partially closed

**Verdict: implemented (PNG arm only); GitHub/Favicon confirmed to need real new infrastructure.**

Confirmed the critic's evidence and extended it: it isn't only `render_avatar_mode`'s "Current:"
caption — `Sidebar`'s own row-icon `match` (`sidebar.rs`, was line ~2419) mapped **every**
`ProjectIconValue::Avatar(_)` variant, PNG included, to a generic `Icon::Globe` glyph. `grep`
confirmed `gpui::img()` had zero call sites anywhere in `tiller_ui` before this change, and no HTTP
client exists anywhere in the app (`main.rs`/`tiller_ui`) — so `AvatarSource::GitHub`/`Favicon`
genuinely cannot render a real image without adding real network+async-image infrastructure
(fetch, decode, cache, loading/error states). That is out of this slice's size and is the
"architectural gap" case the brief calls out — left `blocked`-in-spirit, not faked.

`AvatarSource::LocalPng(path)` is different: the bytes are already on disk (the user picked the
file), so `gpui::img(path.clone())` renders them directly with the crate's existing (already
vendored, just previously unused) image asset loader — no new infrastructure. Wired it in:

- `sidebar.rs` row icon: `Avatar(LocalPng(path))` now renders `img(path)` sized to the row's glyph
  box instead of falling through to `Icon::Globe`.
- `project_identity.rs` `render_avatar_mode`: a 48×48 `img(path)` preview now renders above the
  existing "Current: <filename>" caption when the committed value is a local PNG.

GitHub/Favicon still render the caption-only fallback exactly as before — unchanged behaviour there,
now honestly a narrower true gap than "no avatar source ever shows a picture."

`cargo test -p tiller_ui` — all 54 project_forms/project_identity/sidebar tests still pass
(`avatar_mode_local_png_validates_signature_and_size_before_committing`,
`project_settings_mounts_the_icon_picker`, etc.), no new test added because the file-picker path
(`choose_local_png`) opens a native OS dialog (`PathPromptOptions`) that this headless lane cannot
drive — the rendering change itself was verified by code inspection + the existing PNG-signature
unit test proving a `LocalPng(path)` value reaches `self.value.value` the same way the new render
arm reads it.

**howToExercise:** open a project's icon picker (`Project Settings` → icon → Icon mode "Avatar" tab
→ "Choose PNG…"), pick any PNG (real file dialog, not drivable headlessly in this lane) — the
48×48 preview above "Current: <filename>" and the project's sidebar row glyph should both show the
actual picture instead of a globe outline. `cargo test -p tiller_ui avatar_mode_local_png` covers
the state machine that feeds this render path.
