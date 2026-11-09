# Divide the main window like Zed

**Date:** 2026-11-08
**Status:** approved by Enzo, 2026-11-08
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

After his approval, the plan's code survey corrected three playback numbers.
They are reported to him with the plan:
- The tab strip is 34 today, hairline included, not 35.
- The rail's bottom bar stays 40 (§3.2).
- `footnote` texts go from 12 to 13, not from 11 to 12 (§3.3).

What he decided does not change.

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
| 1470 (Sirio's launch size, `main.rs:32076`) | 300 | 456 | 455 | 240 |
| 1710 (Enzo's screen) | 300 | 576 | 575 | 240 |

The centre is the window width minus both side panels, 16 px of inset and
gaps, the centre card's 2 px of border, and the 1 px divider. The Primary
takes half of that, rounded to a whole pixel, and the Secondary the rest
(#723; first written without the border, as ≈456 / ≈576 each).

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
  | Secondary open | below 1257 | below 1199 |
  | Secondary closed | below 936 | below 878 |
  | Rail drawn | below 976 | below 918 |

  The new column counts the centre card's 2 px of border, which #723 added
  to every floor (first written as 1197 / 876 / 916).

  *Found while building this:* the rule did not always hold. Without a drag,
  `resolve_panel_widths` (`panel_layout.rs`) scaled both side panels by the
  same factor and then lifted each to its floor. When one panel was lifted,
  the other kept its scaled width, so the pair overran the budget and the
  centre went under its floor: at 1100 wide each half drew 308. The old
  defaults overran too, by less. Now the other panel gives back what the
  floored one took, down to its own floor. At 1100 the panels draw 223 and
  220, and each half is laid out 320 wide (with #723 the panels draw 221
  and 220, and both halves fit inside the centre card's border, see §9). The
  layout E2E's case 1c pins this.

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

There is a precedent. `migrate_v21` (`migrations.rs:502-509`) moved the
previous defaults the same way, 325 → 280 and 405 → 320, with one `UPDATE
setting … WHERE key = … AND value = …` per panel.

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
| Tab strip of each centre half | 34, hairline inside | **32 including the hairline** *(my call: Zed's toolbar is 32 with its border inside)* | `main.rs:669` `TAB_BAR_HEIGHT`. The `TabBar` inside draws `tab_bar.rs:19` `HEIGHT` 34, clipped to the 33 px content box today, so it becomes 31. |
| Status bar | 40 | **30** | `main.rs:668`, duplicated at `status_bar.rs:28` |
| Sidebar section header | 26 | **31** | `sidebar/section.rs:10` |
| Sidebar one-line row (tab rows) | 31 drawn (constant 32) | **34 drawn** | `sidebar/mod.rs:195` |
| Sidebar two-line row (worktree and session cards) | 50 drawn (constant 51) | **53 drawn** | `sidebar/mod.rs:212` |
| Horizontal padding of sidebar rows and section headers | 12 | **6** | `sidebar/row.rs:568`, `sidebar/sessions.rs:607`, `sidebar/section.rs:38`, `sidebar/mod.rs:4003` |
| Right panel header | 40, 28 px icons | **32, 24 px icons** *(my call, see below)* | `right_panel/mod.rs:37`, `:611-612` |
| File tree row | 26 | 26 (already Zed's) | `right_panel/files.rs:54` |
| File tree indent / left padding | 16 / 8 | **20 / 6** | `right_panel/files.rs:584`, `:603` |

Sidebar rows draw 1 px shorter than their constants today. `row_drawn_height`
(`row.rs:238-240`) adds the title line height minus 18, and under the −1
shift the title line is `scaled(18)` = 17. When §3.3 removes the shift, the
title line is 18 and the drawn heights equal the constants.

*Found while building this (2026-11-08):* no surface draws a one-line row.
Project rows render as section headers and Worktree rows are always two-line,
so the "one-line row (tab rows)" line of the table has nothing to measure.
`ROW_HEIGHT` stays 34 as the constant, but no probe or E2E asserts it, and
§6.1 no longer asks for a first one-line row probe.

Calls marked *(my call)*:

- **Right panel header.** Zed's project panel has no header, so there is no
  direct equivalent. The header is aligned to the tab strip (32), so the line
  under it continues the line under the centre tabs. The icons must shrink as
  well: at 240, seven views × 28 px + 6 × 4 px gaps + 20 px padding = 240,
  more than the 238 px inside the panel's borders. At 24 px they take 212.
- **Anything aligned to the tab strip follows it**, so the lines that run
  across the window stay aligned. Known cases:
  - The rail's top expand row is drawn at `TAB_BAR_HEIGHT`
    (`primary_rail.rs:49`), so it follows with no edit.
  - The Secondary's close button (`main.rs:25423-25431`) and the tab overflow
    button (`main.rs:26574-26581`) are 26 × 26 at top 6. In the 31 px
    content box they are re-centred, at top (31 − 26) / 2 = 2.5.
- **The rail's bottom bar stays 40** (`primary_rail.rs:204`). It is 5 + 30 + 5
  around the rail's 30 px `+` (`tab_bar.rs:24`, `:27`), and it is not tied to
  the status bar. The playback said it would become 30. Checking the code
  showed that premise was wrong.
- **Equal numbers with no link stay as they are**, for example the Changes
  toolbar (`changes.rs:97`, 34), the settings header (`settings.rs:53`, 40),
  the drawer header (`drawer/view.rs:70`, 30) and the outline rows
  (`outline.rs:23`, 31).
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
  (`sirio_theme/src/lib.rs:1521-1542`) subtracts 1 from every field of the
  typography, `base_size` included. So inside these panels
  `typography.scaled(x)` draws at x − 1 and every named step draws 1
  smaller. The rule has three parts:
  1. Every text in the sidebar and the right panel draws at the size the same
     call draws anywhere else in Sirio. The −1 shift goes, line heights
     included.
  2. Nothing in these panels draws text above `ui_size`, which is 13 at the
     default interface size and moves with it. A named size above it
     (`caption2`, `headline`, `title*`, `callout`) is capped at it. A
     `scaled(x)` text size above 13 at the call site becomes 13.
     `scaled(x)` used as a line height is not text and is not capped.
  3. Glyph marks keep the size they are drawn at today: the project mark
     (`row.rs:538`, `headline`), the emoji mark (`row.rs:542`, `scaled(15)`)
     and the tab close × (`row.rs:830`, `scaled(14)`). They are icons set in
     a font, not text to read.

  At the default interface size this gives:

  | Text | Call | Today | New |
  |---|---|---|---|
  | Row titles, section headers, file names | `ROW_TITLE_FONT_SIZE` / `scaled(13.5)` | 12.5 | 13 |
  | Panel roots' inherited size (`sidebar/mod.rs:4131`, `right_panel/mod.rs:945`) | `scaled(16)` | 15 | 13 |
  | `ui_size`, `footnote`, `caption2` texts | named | 12 / 12 / 13 | 13 |
  | Worktree card subline (`row.rs:748`), filter field text (`mod.rs:4303`) | `scaled(12.5)` | 11.5 | 12.5 |
  | Session time, armed delete confirm (`sessions.rs:702`, `:725`) | `scaled(11.5)` | 10.5 | 11.5 |
  | Small labels | `scaled(11.0)` | 10 | 11 |
  | Unread pill (`section.rs:183`), primary star (`row.rs:727`) | `scaled(10.0)` | 9 | 10 |
  | Headings (`headline` in `mod.rs:4558`, `:4572`) | named | 14 | 13 |

  Secondary text that is drawn smaller today stays smaller than the labels
  next to it (subline, time, pill). `footnote` texts reach 13 under part 1.
  The playback said 12 because it assumed they draw at 11 today. They draw
  at 12.

  `sidebar/project_settings.rs` is not in these panels: it draws a centre
  card on the global theme (`*Theme::get(cx)`), so its `headline` stays at
  the global size. The right panel's empty state ("No worktree selected")
  draws its title (`headline`) and subtitle (`footnote`) both at 13. The
  subtitle stays secondary by colour (`fg_subtle` against `fg`). Its icon is
  a glyph and keeps its 20 px as `scaled(20.0)` (part 3); it was sized with
  `large_title`, which the cap would have brought to 13. The prompt card's
  title and preview (`right_panel/prompts.rs`) also both reach 13; the
  preview stays secondary by weight and colour.

  Fixed boxes keep their size. The unread pill's 14 px box holds 10 px text,
  as it already does outside the sidebar. The captures in §6.1 are where a
  clipped glyph would show.
- **Content text is out of scope:** the chat transcript body (14.5), the
  terminal and the code editor keep their sizes.
- **The interface font setting stays** (12..18, `model.rs:859`), with 13 as
  its default. At other settings every size above moves by the same delta,
  as today (`Typography::for_interface_size`).

### §3.4 Titlebar controls

- **macOS.** AppKit draws the traffic lights at `traffic_light_position`
  (12, 12) (`main.rs:32241`). That centres AppKit's 14 pt frame in a 38 bar
  (`sirio_theme/src/lib.rs:630-634`). At 34 it becomes (12, 10). The same
  (12, 12) literal sits in five `sirio_ui` / `sirio_terminal` examples, which
  follow.
- **Linux client-side decorations** draw three 12 px dots, centred by the
  row's flex (`titlebar.rs:740-766`, inset `BrowserChrome::traffic_light_inset`
  10 at `lib.rs:686`). With a window manager (`OsDrawnAbove`) the WM draws its
  own bar, and Sirio's row holds only its own controls.
- **Windows** caption buttons take the bar's height (`titlebar.rs:668`), so
  they follow.
- **The row's own buttons** are `compact_action` 24 px (`titlebar.rs:707`).
  They fit in 34.

The macOS position cannot be checked from Linux, so it is an owed hand check
(§6.3).

**Status bar at the largest interface size.** Its text is `caption2` with
gpui's default 1.618 line height. At interface 18 that line box is 30.7 px,
0.7 over the 30 px bar. The glyphs themselves fit. This is accepted, and the
§6.1 captures include one at interface 18.

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

**Found while building this (2026-11-08).** That shared write,
`schedule_panel_width_save`, built its row from `app_settings_from_snapshot`.
That function hard-codes `updates_enabled: true` and the default
`forge_hosts`. So every panel-width save reset the user's update opt-out and
wiped their custom forge hosts. The bug dates from #578. Saving divider drags
would have made it fire far more often. So the save now starts from the
stored row, through `app_settings_for_settings_save`, and overrides only the
three drag keys and the LSP list. A regression test covers the opt-out and
the hosts.

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
  `railRects`) to report the drawn rects of:
  - the title bar, the three shell columns, the two centre halves, each tab
    strip, the status bar and the right panel header;
  - the sidebar's first section header and first two-line card (no surface
    draws a one-line row, see §3.2);
  - the centre divider's handle, which the divider case drags.
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

Existing tests that pin the old numbers are **updated, not deleted**. Checked
against the code:

- `panel_widths_round_trip_and_clamp_into_their_ranges`
  (`sirio_persistence/tests/persistence_integration.rs:1951`) asserts the
  fresh defaults 280 / 320. They become 300 / 240.
- `compact_panel_migration_updates_only_the_previous_defaults`
  (`migrations.rs:692`) runs every migration, so v27 would rewrite its
  expected 280 / 320. It is pinned to `migrate_up_to(&mut conn, 21)`, so it
  keeps proving v21 alone.
- The `panel_layout` thresholds have no literal test. They are arithmetic,
  and the tests pass explicit widths.
- `DEFAULT_SIDEBAR_WIDTH` (`sidebar/mod.rs:187`) and the right panel's 320
  fallbacks (`right_panel/mod.rs:387`, `right_panel/history.rs:178`) move with
  the defaults. `rows_follow_the_panel_width` derives from the constant.
- Fixed-pixel regions in `Scripts/Tests/test-ely-chat-ui-e2e.py`
  (`SIDEBAR_BLOOM`, `FIELD_BAND`, `BAND_BESIDE_FIELD`, `POPUP_REGION`,
  `COMPOSER_CLICK`) and the rows of `Scripts/capture-masks.tsv` for the status
  bar, the transcript clocks, the sidebar dots, the identity lane and the
  settings toolbar move with the chrome. A drifted mask fails nothing; it
  just stops covering its clock. So they are re-measured from a run, not left
  to a red test.
- The rail E2E already clicks at the rects `surface.layout.read` reports and
  should pass unchanged.

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
- Stale references touched along the way: `panel_layout.rs:35` cites
  `main.rs:3339`, and `main.rs:667` cites a `docs/MEASURED.md` that does not
  exist.

## §9 Seen on the way, not in scope

- `center_available_width_for_shell` (`main.rs:694-706`) may not subtract the
  centre card's two 1 px borders, which could clip the Secondary's rightmost
  2 px. This is unverified. Check it while measuring for §6.1, and file an
  issue if it is real.

  *Found while building this (2026-11-08):* it is real, and the layout E2E
  verified it: with the Secondary open at the floor (1100 wide), its laid-out
  right edge (873) passes the centre card's inner edge, inside the border
  (871), by 2 px, and the card's outer edge (872) by 1. The visible Secondary
  there is 318, not 320. The fix couples with `min_center` (641 to 643). Out of scope
  here; filed as #723.

  *Fixed by #723:* the panes now share the card's content box and every
  centre floor counts the border (643 open, 322 closed, 362 with the rail).
  The last pane (the Secondary, or the Primary alone) fills what is left by
  layout, not by its computed width, so it meets the border whatever the
  arithmetic says: at a fractional scale the 1 px border is one device pixel,
  not one logical pixel. That is exact at 1x and 2x only; at 125 % or 150 %
  gpui's own clip still insets by the logical pixel, so the column next to the
  border can be clipped there (gpui's behaviour, not measured here). The
  Primary is rounded to a whole pixel so that at 1x and 2x the computed widths
  (the peek, the tab strips, the drop ghost) are the drawn ones. The
  E2E's row (now `secondaryEndsAtCenterContentEdge`, cases 1a, 1b, 1c, 7b)
  fails unless the Secondary ends exactly at the card's content edge.
- `SidebarRow.depth` is stored, but no drawing code reads it.
