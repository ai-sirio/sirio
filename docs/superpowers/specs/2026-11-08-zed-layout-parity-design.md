# Divide the main window like Zed

**Date:** 2026-11-08
**Status:** draft, awaiting Enzo's review
**Builds on:** `2026-09-22-center-panes-design.md` (the centre split),
`2026-10-15-secondary-on-demand-design.md` (the Secondary exists only while it
holds tabs), `2026-11-01-primary-rail-design.md` (the rail and its peek).
**Mockups:** `2026-11-08-zed-layout-parity-mockups.html` beside this file. The
mockups are schematic: they show proportions and heights, not Sirio's final
look. Where they differ from this document, the document wins.

## §0 Intent

Enzo put a screenshot of Zed next to Sirio. Zed's window felt better divided,
especially in how wide each part is. He wants Sirio to have the same distances
and sizes: the same column widths, bar heights, row heights and row padding.
There is one exception. Sirio's interface text stays at 13.

The reference is Zed **as it ships** (its factory defaults), not the panel
widths in Enzo's own Zed. Sirio keeps its own look, meaning the panels drawn
as separate cards. Only the numbers move.

## §1 Decided with Enzo

In the order he decided them:

1. **Zed's factory widths.** The left sidebar starts at **300**, the right
   panel at **240**, and the two centre halves split what is left equally.
   The 50/50 split already exists (`center_split_ratio` 500). Only the side
   widths change.
2. **Keep Sirio's cards** (mockup variant B). The 4 px outer inset, the 4 px
   gap between panels, the 1 px panel border and the rounded corners all stay.
   Zed's flush panels with 1 px rules were shown as variant C and rejected.
3. **Fix the centre divider's persistence.** Today a drag of the divider
   between the two centre halves is saved only as a side effect of a later
   side-panel drag (§4.3).
4. **Take Zed's vertical measures too**: title bar, tab strip, status bar,
   sidebar rows and headers, and row padding (the table in §3).
5. **All interface text at 13.** That includes the sidebar and the right
   panel, which today draw their labels at 12.5 (§3.3).

Enzo reviewed the playback of §2–§3, including the calls marked *(my call)*,
and changed nothing.

## §2 Widths

### §2.1 Reference: what Zed does

These were read from `zed-industries/zed` main at commit `d2a4fb6d`
(2026-11-08). They agree with Enzo's screenshot once it is measured at its real
size: 3420 px on a Retina display, so a 1710 pt window.

- **Threads sidebar:** fixed width, default 300 (`assets/settings/default.json:1201`),
  clamped 200..800 (`crates/agent_settings/src/agent_settings.rs:37-38`).
- **Project panel:** fixed width, default 240 (`default.json:888`). It has no
  minimum.
- **Agent panel:** `flexible: true` (`default.json:1191`). It shares the width
  left over after the fixed docks with the centre pane group, at equal weight
  (`crates/workspace/src/workspace.rs:2739-2757`). `agent.default_width: 640`
  is unused while it is flexible.
- **Rules:** 1 px between docks and between panes (`crates/workspace/src/dock.rs:1356`,
  `crates/workspace/src/pane_group.rs:1161`). Resize handles are 6 px.

Enzo's own Zed in the screenshot has 300 / ~533 / ~533 / ~344. The 300 is the
default. The ~344 is his drag of the project panel. He chose the factory 240.

### §2.2 Sirio after the change

| | Today | New |
|---|---|---|
| Sidebar default | 280 | **300** |
| Right panel default | 320 | **240** |
| Centre halves | equal | equal (unchanged) |
| Cards: inset / gap / border / radius | 4 / 4 / 1 / 7 | unchanged |

Worked numbers, with the Secondary open and the rail not drawn:

| Window width | Sidebar | Primary | Secondary | Right panel |
|---|---|---|---|---|
| 1470 (Sirio's launch size, `main.rs:32076`) | 300 | ≈456 | ≈456 | 240 |
| 1710 (Enzo's screen) | 300 | ≈576 | ≈576 | 240 |

The centre is the window width minus both side panels, 16 px of inset and
gaps, and the 1 px divider. Each half takes half of that.

### §2.3 What stays as it is *(my calls)*

- **Drag ranges are unchanged:** sidebar 220..480, right panel 220..640
  (`panel_layout.rs:26-31`, `model.rs:869-871`). Both contain the new
  defaults.
- **The narrow-window rule is unchanged.** Each centre half keeps at least
  320 (`MIN_CENTER_PANE_WIDTH`), and the side panels yield first. Zed instead
  lets its editor shrink to nothing. The new defaults only move the
  thresholds where the side panels start to yield:

  | Case | Today | New |
  |---|---|---|
  | Secondary open | below 1257 | below 1197 |
  | Secondary closed | below 936 | below 876 |
  | Rail drawn | below 976 | below 916 |

- **The window still opens at 1470 × 833.** The window size is not persisted,
  and that does not change here.

### §2.4 Existing installations *(my call)*

`AppDatabase::save_settings` (`sirio_persistence/src/db.rs`, around 1633)
writes every setting, the panel widths included, whenever any setting is
saved. So a stored `appearance.sidebarWidth` of 280 does not mean the user
dragged it. Most installs that ever changed a setting hold the old defaults
verbatim.

Rule: a **one-shot migration** (the next schema version, `migrate_v27`, which
no local or `origin` branch claims as of 2026-11-08) rewrites a stored
`appearance.sidebarWidth` of exactly `280` to `300`, and a stored
`appearance.rightPanelWidth` of exactly `320` to `240`. It leaves every other
value alone. It runs once, so a user who later drags the sidebar back to
exactly 280 keeps 280. A rule applied at load time would undo that drag on
every launch, and is rejected for that reason. This includes Enzo's own
install.

Check again for a v27 on in-flight branches before taking the number.
`primary_rail` once had to renumber after such a collision.

## §3 Vertical measures and type

### §3.1 Reference: Zed's values

Read from the same commit, at Zed's defaults (`ui_font_size` 16, macOS). They
agree with the screenshot measured at 1710 pt.

| Element | Zed | Source |
|---|---|---|
| Title bar | 34 | `crates/ui/src/utils/constants.rs:17-18`, `max(1.75 rem, 34)` |
| Tab bar / panel toolbar | 32 | `crates/ui/src/components/tab.rs:83-84` (`Base32`) |
| Status bar | ≈30 | `crates/workspace/src/status_bar.rs:152-153`: 4 px padding around 22 px buttons |
| Sidebar group header | 31 | `crates/sidebar/src/sidebar.rs:2410` (`Tab::content_height`) |
| Sidebar one-line row | 34 | `crates/ui/src/components/ai/thread_item.rs:438-453` |
| Sidebar two-line row | ≈53 | measured on the screenshot |
| Row horizontal padding | 6 | `list_item.rs:364`, `thread_item.rs:439` |
| File tree row | ≈26 | measured on the screenshot |
| File tree indent per level | 20 | `default.json:907` |

### §3.2 Sirio after the change

The values are logical px, the same as Zed's.

| Sirio element | Today | New | Where today |
|---|---|---|---|
| Title bar | 38 | **34** | `sirio_theme` `browser_chrome.bar_height` (`lib.rs:683`); wrapper `main.rs:28856` |
| Tab strip of each centre half | 34 + 1 hairline | **32 including the hairline** *(my call: Zed's toolbar is 32 with its border inside)* | `main.rs:669` `TAB_BAR_HEIGHT`, duplicated at `tab_bar.rs:19` |
| Status bar | 40 | **30** | `main.rs:668`, duplicated at `status_bar.rs:28` |
| Sidebar section header | 26 | **31** | `sidebar/section.rs:10` |
| Sidebar one-line row (tab rows) | 32 | **34** | `sidebar/mod.rs:195` |
| Sidebar two-line row (worktree cards) | 51 | **53** | `sidebar/mod.rs:212` |
| Horizontal padding of sidebar rows and headers | 12 | **6** | `sidebar/row.rs:218-222`, `sidebar/section.rs:36-46` |
| Right panel header | 40, 28 px icons | **32, 24 px icons** *(my call, see below)* | `right_panel/mod.rs:37`, `:611-612` |
| File tree row | 26 | 26 (already Zed's) | `right_panel/files.rs:54` |
| File tree indent / left padding | 16 / 8 | **20 / 6** | `right_panel/files.rs:584`, `:603` |

Calls marked *(my call)*:

- **Right panel header.** Zed's project panel has no header, so there is no
  direct equivalent. The header is aligned to the tab strip (32), so the line
  under it continues the line under the centre tabs. The icons must shrink as
  well: at 240, seven views × 28 px + 6 × 4 px gaps + 20 px padding = 240,
  more than the 238 px inside the panel's borders. At 24 px they take 212.
- **Anything aligned to the tab strip or to the status bar follows it**, so
  the lines that run across the window stay aligned. Known cases:
  - The rail's top expand row is 34 today (`primary_rail.rs:49`), equal to the
    tab strip, so it becomes 32.
  - The rail's bottom bar is 40 (`primary_rail.rs:204`), equal to the status
    bar, so it becomes 30.
  - The Secondary's close button (26 × 26, top 6, `main.rs:25423-25431`) is
    re-centred in the 32 strip.
  - Anything else the builder finds pinned to the old 34, 35 or 40 for the
    same reason follows too.
- **Surfaces with no Zed equivalent keep their sizes:** the sidebar's view
  switch (30), its filter field (34) and header actions, the peek, the
  drawer, the chat composer and the chat transcript.

### §3.3 Text at 13

- **Every primary label in the sidebar and the right panel is drawn at 13.**
  That covers row titles, section headers, file tree names, the History
  subject and the References rows. 13 is the size the tab titles and
  `Typography::ui_size` already use.
- **Do not get there by deleting `Theme::with_sidebar_typography`**
  (`sirio_theme/src/lib.rs:1521`). It subtracts 1 from sizes that start at
  13.5 (`sidebar/section.rs`, `scaled(13.5)` in `sidebar/mod.rs:202` and
  `row.rs:695`), so deleting it gives 13.5, not 13. The target is the size
  per label. How it is reached is the builder's choice. Its callers are listed
  by `grep -rn with_sidebar_typography rust/crates`: sidebar, right panel
  views, `changes.rs`, `project_forms.rs`.
- **Secondary text stays a step smaller than the label beside it.** That
  covers timestamps (`1w`, `1mo`), counts, badges and captions. *(My call:
  "everything at 13" read literally would make the captions as loud as the
  titles they annotate. Zed keeps its metadata a step smaller too.)*
- **The rule that settles every size** *(my call)*. `with_sidebar_typography`
  lowers `base_size` by 1, so inside these panels `typography.scaled(x)`
  draws at x − 1 and every named step (`footnote`, `ui_size`, …) draws 1
  smaller. The rule has two parts:
  1. Every text in the sidebar and the right panel draws at the size the same
     call draws anywhere else in Sirio. The −1 shift goes.
  2. Nothing in these two panels draws above 13. A size that would land
     above it is drawn at 13.

  At the default interface size this gives:

  | Text | Call | Today | New |
  |---|---|---|---|
  | Row titles, section headers | `scaled(13.5)` / `ROW_TITLE_FONT_SIZE` | 12.5 | 13 (cap) |
  | Worktree card subline (`row.rs:748`) | `scaled(12.5)` | 11.5 | 12.5 |
  | Filter field text (`mod.rs:4303`) | `scaled(12.5)` | 11.5 | 12.5 |
  | `footnote` captions and counts | `footnote` | 11 | 12 |
  | Session time, armed delete confirm (`sessions.rs:702`, `:725`) | `scaled(11.5)` | 10.5 | 11.5 |
  | Unread pill (`section.rs:183`), primary star (`row.rs:727`) | `scaled(10.0)` | 9 | 10 |
  | Headings, e.g. `headline`, `scaled(14..16)` | various | 13–15 | 13 (cap) |

  Fixed boxes keep their size. The unread pill's 14 px box holds 10 px text,
  as it already does outside the sidebar. The captures in §6.1 are where a
  clipped glyph would show.
- **Content text is out of scope:** the chat transcript body (14.5), the
  terminal and the code editor keep their sizes.
- **The interface font setting stays** (12..18, `model.rs:859`), with 13 as
  its default. At other settings every size above moves by the same delta,
  as today (`Typography::for_interface_size`).

### §3.4 Titlebar controls

The traffic lights are placed for a 38 px bar today (`traffic_light_inset`
14, position (12, 12), `main.rs:32241`). At 34 they must be re-centred
vertically. The titlebar's own controls (`titlebar_control_frame` 26 × 26)
must still fit, and on Linux and Windows the client-side window controls too.
This cannot be checked from Linux, so it is an owed macOS hand check (§6).

## §4 The centre divider

### §4.1 Today

`update_center_split_ratio` (`main.rs:27208-27241`) updates
`center_split_ratio` in memory but never calls `schedule_panel_width_save`. The
divider's drop handler (`main.rs:27031-27041`) does not save either. The ratio
reaches `appearance.centerSplitRatio` only when a later side-panel drag
schedules a save, which writes every setting. A divider drag followed by a
quit is lost.

### §4.2 New

A centre-divider drag is saved exactly like a side-panel drag: the same
500 ms debounce (`main.rs:5459`) and the same settings write. On relaunch the
halves come back at the dragged ratio, within the existing 100..900 clamp.

### §4.3 Scope

Only the divider between the two centre halves. In-tab terminal splits
(`PaneNode`, `panes.rs`) persist with their tab and are not touched.

## §5 Goals, non-goals, anti-goals

**Goals**

- At Sirio's launch size and at Enzo's 1710 pt window, the columns measure
  as §2.2 says.
- The bars and rows measure as §3.2 says, with every interface label at 13.
- A centre-divider drag survives a quit.

**Non-goals**

- Zed's flush look, Zed's colours, or Zed's icons. Variant B keeps Sirio's
  cards.
- Zed's flexible-dock model, where the editor can shrink to nothing. The 320
  floor per centre half stays.
- New user settings for any of these sizes.
- Persisting the window size or the panels' visibility. Both reset on every
  launch today (`main.rs:7160-7161`), and both stay that way here.

**Anti-goals**

- **A user who dragged a panel loses the width.** Only the exact old defaults
  are rewritten, and only once (§2.4).
- **Lines that run across the window no longer line up.** For example, the
  rail's top row and the tab strip end at different heights, or the right
  panel's header rule sits 8 px off the centre's tab rule. §3.2 lists the
  known pairs.
- **Text clipped by a shorter bar.** The status bar loses 10 px and the tab
  strip 3 px. Every control in them must still fit at 13, and at the largest
  interface font setting (18).
- **A view of the right panel that is unusable at 240.** History loses its
  date column at 240, and Enzo accepted that (§7).

## §6 Verification

### §6.1 The E2E

Testing is end to end (CLAUDE.md). The new E2E runs a real, isolated Sirio
(lavapipe, the usual isolation per `docs/agents/e2e-tests.md`) and ends in an
artifact anyone can re-run: a socket transcript plus PID-matched window
captures under `--out-dir`.

- **Extend `surface.layout.read`** (debug builds only, like the rail's
  `railRects`) to report the drawn rects of the three shell columns and the
  two centre halves, and the drawn heights of the title bar, of each tab
  strip, of the status bar and of the right panel header.
- **Assert at two window sizes, 1470 × 833 and 1710 × 987.** The column
  widths must match §2.2 within 1 px. The heights must match §3.2 exactly.
- **Migration cases**, each launched on a seeded database:
  - 280 / 320 stored → 300 / 240 drawn.
  - 350 / 400 stored → 350 / 400 drawn.
  - No width stored → 300 / 240 drawn.
  - After the migration ran, a stored 280 (as if the user dragged back to
    it) → 280 drawn on the next launch.
- **Divider case.** Drag the centre divider with real X input to a ratio
  other than 500, quit, relaunch. The same ratio is drawn. Today this case
  fails, which is the real gap in behaviour coverage that earns it a place.
- **Captures**, dark and light, at both sizes.

### §6.2 Tests that go red by design

Existing tests that pin the old numbers are **updated, not deleted**. Known
ones:

- the `panel_layout` thresholds (1257 / 936 / 976);
- `ranges_match_the_persisted_settings_ranges`, if a range constant moves;
- any fixture built on `DEFAULT_SIDEBAR_WIDTH` 280 (`sidebar/mod.rs:187`) or
  the right panel's 320 fallback (`right_panel/mod.rs:387`,
  `right_panel/history.rs:178`);
- E2E scripts that click fixed coordinates under the old chrome. Some,
  including the rail E2E, already click at the rects `surface.layout.read`
  reports and should pass unchanged.

The bezel-off capture diff (`Scripts/bezel-off-captures.sh` with
`Scripts/capture-diff.py`) will flag every frame, because every coordinate
moves. That is expected. The PR says so and attaches both capture sets.

### §6.3 Owed hand checks

- **macOS:** the traffic lights centred in the 34 px title bar, and the
  status bar at 30 with the `NSStatusItem` tray unaffected.
- **Windows:** the client-side window controls in the 34 px title bar.

## §7 Questions put to Enzo

1. **History at 240.** The History view decides its columns from the panel
   width (`right_panel/history.rs:1039`, `row_columns`; panel width minus
   18 px of chrome, author 78, date 80, 6 px gaps, subject at least 130).
   The unfiltered view always draws the branch graph, at least one 12 px
   lane:
   - **Today, at 320:** a row shows the graph, the subject and the date (up
     to about six lanes). The author is already dropped. The toolbar shows
     its filter chips on a second row.
   - **At 240:** a row shows only the graph and the subject. The date is
     dropped from one lane up. The toolbar collapses the four chips behind
     one `Filters (n)` button (`history_toolbar.rs:31-34`, `CHIP_ROW_MIN`
     260).
   - A filtered view draws no graph. There the date stays at 240.

   **Decided by Enzo, 2026-11-08: accepted as it is.** History keeps its
   current width rules. Its toolbar and row tests only follow the new
   default width.

## §8 Left to the builder

- The mechanism for §3.3: per-call sizes, or a reworked
  `with_sidebar_typography` that applies the cap instead of the shift.
- Removing the duplicated constants listed in §3.2 (34 in two places, 40 in
  two places, 280 and 320 fallbacks), or keeping them in step.
- The exact traffic-light offsets for the 34 px bar.
- Stale references touched along the way: `panel_layout.rs:35` cites
  `main.rs:3339`, and `main.rs:667` cites a `docs/MEASURED.md` that does not
  exist.

## §9 Seen on the way, not in scope

- `center_available_width_for_shell` (`main.rs:694-706`) may not subtract the
  centre card's two 1 px borders, which could clip the Secondary's rightmost
  2 px. This is unverified. Check it while measuring for §6.1, and file an
  issue if it is real.
- `SidebarRow.depth` is stored, but no drawing code reads it.
