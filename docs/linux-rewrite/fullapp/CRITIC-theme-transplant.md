# CRITIC — the theme-transplant clause, judged against cf97e585

Fresh, independent pass. I did not write the remedy at cf97e585 and assumed nothing in it on
trust; every claim below was re-derived from the actual files and re-run commands, not from
reading the remedy's own prose.

## Verdict: TRANSPLANTS REMAIN

The remedy repaired the five tokens the original gap named as smoking guns (`accent`,
`sidebar`, `sidebar_border`, `text_tertiary`, light `raised`) but left roughly thirty other
`ThemeColors` fields — including several the *original gap report itself named as transplanted*
(`inset`, `gauge`, `warning`, `success`, `danger`) — byte-identical or hsla-parameter-identical
to `waku/src/theme.rs`, with no measurement behind them and, for most, no mention anywhere in
`THEME-PROVENANCE.md` at all.

## What I verified myself

### 1. Re-ran the measurement script — it reproduces the doc's numbers

```
$ ./reference/waku/measure-theme.py
```

Output (full transcript kept at `/tmp/claude-1000/.../scratchpad/measure-out.txt` during this
session; key lines reproduced here):

```
## Flat regions — dark
surface      80x80+887+351        #1A1A1A       100%
composer     80x40+1608+1360      #212121       100%
raised       80x10+1814+290       #232323       100%

## Glyph colours — dark
text_tertiary   180x26+1330+462      #1A1A1A   #7C7D7D

## Sidebar/content seam — dark
  x=662  #282828  flat
  x=663  #282828  flat

## Is #E2795B in the frame at all? — dark
  exact occurrences of #E2795B: 0 px
    nearest #FF5C5F  distance=  41.2  516 px

## Flat regions — light
raised       80x10+1814+290       #EBEBEB        75%

## Glyph colours — light
text_tertiary   180x26+1330+462      #F6F5F6   #868686

## Is #C85F44 in the frame at all? — light
  exact occurrences of #C85F44: 0 px
    nearest #A8704B  distance=  36.9  1135 px
```

This matches `THEME-PROVENANCE.md`'s tables exactly: `surface`/`composer`/`raised` coverage
(100/100/100/75), the corrected `text_tertiary` (`#7C7D7D`/`#868686`, one step off the
transplanted `#7D7D7D`/`#858585`), the seam at `#282828`/`#DCDBDB`, and the accent absent with
nearest-neighbour distances in the 37–41 range. **Claim 1 (script reproduces the doc) holds.**

### 2. Diffed `tiller_theme/src/lib.rs` against `_tiller-refs/waku/src/theme.rs`, value by value

I read both files in full (`rust/crates/tiller_theme/src/lib.rs:272-374`,
`_tiller-refs/waku/src/theme.rs:109-193`) and compared every field. Five things changed since
the transplant; everything else did not:

**Changed** (measured or newly derived): `accent` (`0xE08B52`/`0xAD581F` vs waku's
`0xE2795B`/`0xC85F44`), `sidebar` (now `scaled(surface, 0.92/0.98)` vs waku's
`transparent_black()`/native vibrancy), `sidebar_border` (`0x282828`/`0xDCDBDB` vs waku's
`hsla(126.93/360, 0.0000001, 0.16077, 1.0)`/`hsla(0,0,0.078,0.12)`), `text_tertiary`
(`0x7C7D7D`/`0x868686` vs waku's `0x7D7D7D`/`0x858585`), light `raised` (`0xEBEBEB` vs waku's
`0xECECEC`).

**Unchanged — byte-identical to waku, undisclosed as such:**

```
lib.rs:302   let warning = Self::adaptive(rgb_hex(0xE0B36A), rgb_hex(0xA66B20), appearance);
lib.rs:303   let success = Self::adaptive(rgb_hex(0x62C987), rgb_hex(0x2F8F52), appearance);
lib.rs:304   let danger  = Self::adaptive(rgb_hex(0xE2726A), rgb_hex(0xC64A42), appearance);
lib.rs:305   let gauge   = Self::adaptive(rgb_hex(0x3B82F6), rgb_hex(0x2563EB), appearance);
lib.rs:312   let inset   = Self::adaptive(rgb_hex(0x151515), rgb_hex(0xE6E6E6), appearance);
lib.rs:368   let favorite = Self::adaptive(rgb_hex(0xEAB308), rgb_hex(0xCA8A04), appearance);
```
vs. `waku/src/theme.rs:144-147` (dark) / `187-190` (light): `warning: 0xE0B36A`/`0xA66B20`,
`success: 0x62C987`/`0x2F8F52`, `danger: 0xE2726A`/`0xC64A42`, `gauge: 0x3B82F6`/`0x2563EB`
(also waku's `resize_handle`), `inset: 0x151515`/`0xE6E6E6`, `favorite: 0xEAB308`/`0xCA8A04`.
Ten hex values, both variants, exact match, for four non-standard semantic hues (`success`,
`warning`, `danger` do not correspond to any well-known palette I could find — I checked
Tailwind's `red-500`/`amber-500`/`green-500` and none match) plus a raw structural token
(`inset`). `favorite` and `gauge` happen to equal Tailwind's `yellow-500`/`blue-500`, which
weakens the case for those two alone, but does not explain `warning`/`success`/`danger`.

The code's own comment at `lib.rs:297-301` calls these "ours by choice," and
`THEME-PROVENANCE.md`'s "Everything else" section (lines 156-161) repeats that framing for
`inset, warning, success, danger, gauge, favorite` — but never discloses that the chosen values
are byte-identical to the ones the *original gap report explicitly named as transplant
evidence*: `GAP-transplanted-theme-tokens.md:42-46` lists `inset 0x151515`, `gauge 0x3B82F6`,
`warning 0xE0B36A`, `success 0x62C987`, `danger 0xE2726A` in its own quoted evidence of the
transplant. The remedy commit's diff (`git show --stat cf97e585`) shows these lines were not
touched.

**Unchanged — hsla-parameter-identical, never even mentioned in `THEME-PROVENANCE.md`:**

```
$ grep -n "sidebar_item_background\|border:\|border_strong:\|overlay:\|overlay_strong:\|code_wash:\|selection:\|inverse:\|on_inverse:\|danger_soft:" _tiller-refs/waku/src/theme.rs
115:  sidebar_item_background: hsla(0.0, 0.0, 0.941, 0.06),
121:  overlay: hsla(220.0 / 360.0, 0.10, 0.90, 0.05),
122:  overlay_strong: hsla(220.0 / 360.0, 0.10, 0.90, 0.09),
124:  border: hsla(220.0 / 360.0, 0.10, 0.90, 0.07),
125:  border_strong: hsla(220.0 / 360.0, 0.10, 0.90, 0.14),
137:  selection: hsla(211.0 / 360.0, 1.0, 0.50, 0.55),
139:  code_wash: hsla(220.0 / 360.0, 0.10, 0.90, 0.08),
141:  inverse: rgb(0xE7E9EC).into(),
142:  on_inverse: rgb(0x17181C).into(),
148:  danger_soft: hsla(4.0 / 360.0, 0.55, 0.63, 0.10),
```
(and the light-mode mirror at lines 158-191, also exact.) Every one of `row_hover`
(`sidebar_item_background`), `border`, `border_strong`, `overlay`, `overlay_strong`, `selection`,
`code_wash`, `inverse`, `on_inverse`, `danger_soft` in `tiller_theme/src/lib.rs` (lines 326-373)
carries the identical hue/saturation/lightness/alpha tuple, in both appearances, as waku's
source — ten tokens, twenty numbers apiece counting both variants. None of these appear in
either table of `THEME-PROVENANCE.md`, and none are described in the code's own comments as
"ours by choice" the way `warning`/`success`/`danger`/`gauge`/`favorite`/`inset` at least
nominally are. They are simply absent from the audit the remedy commit claims to be complete.

The specificity here is the same category of evidence the original gap treated as
dispositive for `0.000_000_1`: an independent author choosing `hsla(220°, 10%, 90%, 5%)` for one
wash is plausible; choosing the *exact same* hue/saturation/lightness across five different
washes (`overlay` 5%, `overlay_strong` 9%, `border` 7%, `border_strong` 14%, `code_wash` 8%) and
landing on the exact alpha waku picked for each, independently, in both light and dark, is not a
coincidence a "measured off a screenshot" or "chosen by taste" story can cover — translucent
washes are specifically *not* measurable from a flattened PNG (compositing already happened by
the time the frame was captured), so there is no measurement claim available for these at all.

**Conclusion on the diff:** claims 3 and 4 from the remedy are refuted. The frames-cannot-settle
tokens are not "stated as our own choices with reasons" across the board — `inset` and the four
non-standard semantic hues carry no derivation reasoning at all beyond a repeated assertion, and
the ten hsla-wash tokens are not discussed as either measured or chosen anywhere. "No remaining
value... is a transcription of a reference app's source constant" is false; at minimum `inset`,
`warning`, `success`, `danger`, and the ten hsla washes are exactly that.

### 3. Attacked the two strongest claims specifically

**(a) Sidebar translucency.** Verified independently via the opacity probe and seam report in
the script's own output (reproduced above and in full transcript): the sidebar column drifts
`#21282A → #26292A` (dark) / `#EAF1F3 → #F1F3F4` (light) as the desktop-behind-window column
drifts `#88DAF7 → #E4F8FF` / `#86DCF9 → #E5F8FF`, while the content column stays flat at
`#1A1A1A`/`#F6F5F6` throughout. **I could not refute this claim** — the drift is real and
tracks the wallpaper, not a rendering artefact of the sampling method.

**(b) Accent absent from both frames.** Verified via `report_absent`: 0 exact occurrences of
`#E2795B` in the dark frame and `#C85F44` in the light frame, nearest neighbours 36.9–41.2
units away (in RGB-distance space) among colours occurring ≥20 times. **I could not refute
this claim either** — I re-ran the histogram search myself via the script rather than trusting
its printed summary, and the counts are exact.

### 4. Build and tests

Disk was shared and tight: `df -h /dev/shm` started at 4.8G free (within the 4G floor stated in
my brief), but by the time I'd built `tiller_theme` and `tiller_ui`, concurrent builds from other
agents (`/dev/shm/tt` alone was 13G) had driven free space to 738M. I did **not** attempt to
build the full `tiller` binary or drive it under `wayland-drive.sh` — that would require far more
than the available headroom, and the brief says to report rather than build under 4G free. I
freed my own `/dev/shm/critic-theme-tt` (3.1G) afterward as a courtesy; it was back to only 3.1G
free after that, still under the floor. **I could not verify the running app visually — no
render check was performed.** This is a real gap in my verification, not a finding about the
remedy.

I did run both crate test suites to completion before running low on space:

```
$ cargo test -p tiller_theme
test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
```
including `accent_clears_contrast_on_its_own_surface`, `accent_is_not_any_agent_brand`,
`dark_palette_matches_recorded_provenance`, `light_palette_matches_recorded_provenance` — all
green.

```
$ cargo test -p tiller_ui --lib
test result: ok. 359 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 53.78s
```

Both suites are green. This confirms the code compiles and its own conformance/contrast tests
hold — it does **not** confirm anything about the transplant question, since a byte-identical
copy of a working value will pass a self-referential "does the code match itself" test by
construction.

### 5. Wider sweep

- `03-visual-bar-and-gpui-patterns.md:8-24` — the "nothing is copied" claim is now replaced with
  a dated correction, confirmed by reading the file. Claim 5 holds.
- `CONTENT_MAX_WIDTH = 720.0` (`waku/src/app.rs:75`) remains transplanted into
  `tiller_ui/src/conformance.rs:255-257` (`TRANSCRIPT_WIDTH`/`CONTENT_WIDTH`/
  `MARKDOWN_COLUMN_WIDTH` all pinned to `720.0`) and into `sidebar.rs`'s two-line card math. This
  matches the remedy's own account — it is declared, not hidden: the GAP doc's "What has to
  happen" step 3 and the remedy's commit message both say geometry is "the next piece,"
  deliberately unrepaired. **Known and declared, not a new finding.**
- `rust/assets/icons/comet/` — 63 SVG icon files, plus `ATTRIBUTION.md`, stating they were
  imported verbatim from `zeronsh/comet`'s `crates/ui/assets/icons/` "at the user's explicit
  request," MIT-licensed, with a note that "no comet source code was copied — assets only." SVG
  markup is arguably code by a strict reading, and comet is one of the four apps the clause
  names, so this sits close to the line — but it is openly disclosed in a checked-in
  `ATTRIBUTION.md` next to the files, distinctly not silent about its origin, and is outside
  what I was asked to judge (theme colour tokens). I flag it for the team's attention rather
  than folding it into this verdict.
- No other transplanted prose, algorithms, or unusual literals turned up in a grep sweep of
  `rust/crates/` for `waku|zeronsh|comet|t3code|pingdotgg` outside icons.rs/ATTRIBUTION.md and the
  already-known, declared `comet`-measurement comments in `conformance.rs` (top-bar 38px,
  cluster-button gap, non-macOS traffic-light inset — all cited by measurement note, not as
  silent literals).

## Claims I tried to refute and could not

1. The measurement script genuinely measures the frames and reproduces `THEME-PROVENANCE.md`'s
   numbers.
2. `surface`, `composer`, `raised`, `text`, `text_tertiary`, `sidebar_border`, `code_text` in
   `lib.rs` match the recorded measurement.
3. The sidebar is genuinely translucent in waku's frames and therefore unmeasurable as a
   constant.
4. The accent hex is genuinely absent (0 exact pixels, 37+ unit nearest neighbour) from both
   frames.
5. `03-visual-bar-and-gpui-patterns.md`'s false claim is corrected.
6. Geometry (`CONTENT_MAX_WIDTH`) remains transplanted and is honestly declared as such, not
   hidden.

## Claims I refuted

3 (partial) and 4, as stated in the remedy's own list: "Tokens the frames cannot settle
(sidebar, accent, semantic hues) are stated as our own choices with reasons" is true for
`sidebar` and `accent`, false for the semantic hues (`warning`/`success`/`danger`/`gauge`/
`favorite`) and for `inset`, which carry no stated reasoning beyond a blanket "ours by choice"
that does not acknowledge the values are unchanged from the transplant. "No remaining value in
`tiller_theme`... is a transcription of a reference app's source constant" is false — at least
sixteen tokens (`inset`, `warning`, `success`, `danger`, `gauge`, `favorite`, `row_hover`,
`border`, `border_strong`, `overlay`, `overlay_strong`, `selection`, `code_wash`, `inverse`,
`on_inverse`, `danger_soft`) remain exact transcriptions, most of them undiscussed anywhere in
the provenance document.

## The single largest remaining gap

**The ten hsla "wash" tokens** (`border`, `border_strong`, `overlay`, `overlay_strong`,
`code_wash`, `selection`, `danger_soft`, `row_hover`/`sidebar_item_background`, `inverse`,
`on_inverse`) are hue/saturation/lightness/alpha-identical to `waku/src/theme.rs` in both
appearances, and are not measurable from a flattened screenshot in principle (translucent
compositing is already baked into the pixels by capture time) — so unlike `surface`/`composer`/
`raised`, there is no legitimate "we measured and it happened to match" story available for
these at all. They are also completely absent from `THEME-PROVENANCE.md`, which audits only the
seven measured tokens and the two chosen-with-reasoning ones (`sidebar`, `accent`). This is a
bigger and more precisely-diagnosable gap than the semantic-hue duplication, because the "these
are legitimately unmeasurable, so we chose them" defense that (weakly) covers `inset`/`warning`/
`success`/`danger` does not even get attempted here — the document simply doesn't mention them.

## Explicitly NOT findings

So nobody re-investigates: the agent brand colours (`#D97757` etc. in `AgentBrandColor`), the
permissions copy in `settings.rs`, the Claude Code CLI prompt strings, and the COSMIC design
tokens — all as the original `GAP-transplanted-theme-tokens.md` already cleared them, and I found
nothing to add there. `CONTENT_MAX_WIDTH = 720.0` and the sidebar-card geometry are declared,
known-unrepaired gaps per the remedy's own account, not new findings. The Material Icon Theme
file-type icon subset is MIT-licensed and explicitly permitted per `03-visual-bar-and-gpui-
patterns.md:45`. I did not attempt to verify anything about rendering or the running app — no
build of the full binary was possible under the disk constraint, and I did not fake or infer a
visual result.
