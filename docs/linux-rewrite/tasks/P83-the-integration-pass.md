# P83 — the integration pass

**Owner: `codex12`.** Three seams in one pass, **seventeen rows**. Supersedes the mount section of
`P79`; read this instead of it.

Read `../ENVIRONMENT.md`, `../OWNERSHIP.md` and `../SEAMS.md` first.

`SEAMS.md` says to batch the mounts, because dispatching three seams as three briefs pays three
context resets and three build cycles for what is often a dozen lines. This is that batch. Each seam
stays individually judgeable — do them in the order below and commit them separately.

---

## Seam 1 — the browser (9 rows: all of `F-BRW`)

`tiller_ui/src/browser.rs` is 1283 lines and complete. `BrowserSurface` implements `Render` at
`browser.rs:1011`. It has **six references in the tree and zero in the app.**

**This is not a dozen lines, and `SEAMS.md` was wrong to imply it might be.** The shell does not
merely fail to mount the browser — **it asserts the browser cannot exist**, in three places:

| site | what it does today |
|---|---|
| `main.rs:2156` | `TabKind::Browser => unreachable!("Browser surfaces are external to the shell")` — in `tab_icon()`. **Panics on render.** |
| `main.rs:2803` | `unreachable!("browser tabs are unsupported in shell persistence")` — **panics on save.** |
| `main.rs:3904` | `TabKind::Browser => unreachable!(…)` — in `tab_width()`. **Panics on layout.** |

And two tests hold the stubs in place:

| site | what it asserts |
|---|---|
| `main.rs:9781` | for each of the 10 `BROWSER_METHODS`, `assert!(!response.ok)` and the error contains `"unsupported on Linux"` |
| `main.rs:9817` | `system.capabilities` advertises **none** of the browser methods |

**When those two tests go red, that is the seam closing, not a regression.** Invert them: the methods
must succeed and must be advertised. A red test here is the one case in this project where deleting
the assertion is right — but replace it with its opposite, never with nothing.

### The design question, already resolved — do not re-open it

`unreachable!("Browser surfaces are external to the shell")` is **half true**, and the half that is
true is why it must not simply be deleted without reading `browser.rs`.

Per `browser.rs:622`: *"all browser chrome is GPUI, while page pixels live in the proven native X11
child window below it."* So:

- **The page pixels are external** — a real WebKitGTK child window via `wry`, parented to GPUI's X11
  surface. That is the P72 constraint and it holds: **keep GPUI chrome outside the webview
  rectangle**, because a native child window sits above the GL surface and cannot be z-reordered.
- **The tab is not external.** The chrome — address bar, back/forward, error and permission UI — is
  GPUI, and the tab bar must represent it like any other tab.

So `tab_icon` and `tab_width` are simply wrong and their panics become real arms. **`Icon::Globe`
already exists** (`icons.rs:107`, backed by `assets/icons/comet/global.svg` — comet's own icon, which
is what this project uses). Width: treat it like the chat/editor family unless it looks wrong.

`tiller_project/src/workspace.rs:447` already seeds a `TabKind::Browser` tab in a test fixture and
the domain layer handles it fine. **Only the shell refuses.**

### What to build

1. Remove the three panics; give each arm a real answer.
2. Mount `BrowserSurface::new(initial_url, window, cx)` as the surface for `TabKind::Browser`, and
   drain `BrowserEvent` with `take_events`.
3. A palette entry — **"New Browser"**.
4. Persistence: `TabKind::Browser` needs its serialised string (the others are `"file"`, `"chat"`,
   `"terminal"`, `"diff"`) **and the matching restore arm**. A save path without a restore path is
   half a seam.
5. Route all ten `BROWSER_METHODS` at `main.rs:1668` to the real surface instead of
   `ControlResponse::failure`.
6. Invert the two tests.

Row-to-method mapping is in `P79` and still correct: `F-BRW-02` → `go_back`/`go_forward`/`reload`/
`stop_loading`; `03` → `submit_address`; `04` → `did_fail_navigation`/`error()`/`BrowserError`;
`05` → `set_agent_driving`; `06` → `request_permission`/`allow_permission`/`deny_permission`;
`07` → persistence v11 + `set_allowed_origins`; `09` → `open_link`/`BrowserLinkTarget`.

`F-BRW-08` is a **second seam, to `sonnet`** — the Permissions section in `settings.rs`. Do not build
it; make sure the state it needs is reachable and say so.

`browser_origin_grant` exists in the DB at `user_version` 11 and is **empty** — it has zero rows
because nothing can grant an origin until this seam closes. That is `F-BRW-07`'s other half.

---

## Seam 2 — the project forms (`F-PRJ-01`, and unblocks `F-PRJ-05/06/07/08/09/10`)

`codex11` delivered `tiller_ui/src/project_forms.rs` in P77 (`da67e8c`): `CloneForm` and `CreateForm`
with explicit states, a double-submit guard, progress, visible errors and retry. **Seven references
in the tree, zero in the app.**

Handoff, verbatim from `SEAMS.md`:

> Clone: mount `CloneForm`, listen for `CloneFormEvent::Cloned(path)`, register/load the resulting
> project. Create: mount `CreateForm`, listen for `CreateFormEvent::Created(path)`, register/load the
> resulting project.

Plus: the sidebar `+` currently calls `start_add_project` (`sidebar.rs:797`, called from
`sidebar.rs:1972`). It must offer **three choices** — open, clone, create. **`sidebar.rs` is
`codex11`'s file**: if the change lands there, it is a seam back to `codex11`, so register it rather
than editing it. If you can present the three choices from your own side, do that instead.

Behind this sits `tiller_git/src/clone.rs:57 clone_repository` — real, tested, and with zero callers.
Six rows are waiting on nothing but a call site.

---

## Seam 3 — the divider (`F-TERM-SPLIT-01`)

One constant. `main.rs:179`:

```rust
const SEAM_WIDTH: f32 = 1.;
```

Used at `main.rs:3897` (`div().w(px(SEAM_WIDTH))` — the divider itself) and `main.rs:5761`/`5764`
(width arithmetic, which is why it cannot just be changed at the call site). **The reference divider
is 6px; ours is 1.** Change the constant and check the two width subtractions still produce the
layout you expect.

The rest of `F-TERM-SPLIT-01` — recursive leaf/split hosts, cached leaf controllers, 50/50 initial
fractions — was proven live at pass 4. **Check before declaring any of it absent.** `codex11` holds
the terminal crate (P82); this row is yours only because the constant is in your file.

---

## Done means

1. Three seams, three commits, each naming the rows it moves.
2. A browser tab that **opens, navigates, and survives a restart** — not one that compiles.
3. The two browser tests inverted, not deleted.
4. `F-BRW-08` and any `sidebar.rs` change registered in `SEAMS.md` as new seams with owners named.
5. `SEAMS.md`'s Open table updated — move what you closed into Closed. **A seam is done when a row
   moves**, and you are the pane that moves them.
6. `Scripts/transplant-check.py` run, with your files' status stated. 46 candidates are pre-existing.

**The critic will open a browser tab, type a URL, press back, and restart the app.** If it panics,
every row in the cluster fails, and three `unreachable!()` calls are the most likely reason.
