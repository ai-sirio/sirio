# GAP — waku's theme source was read, not its screenshots

> **Second remedy applied 2026-08-19, NOT cleared.** Step 5 ran and the first
> remedy *failed* it: a fresh critic returned **TRANSPLANTS REMAIN**
> (`fullapp/CRITIC-theme-transplant.md`). It upheld the measurement work and
> the two findings below, and refuted the claim that the rest of the palette
> was ours — sixteen values were still exact transcriptions, ten of them
> undiscussed anywhere. Its sharpest point is one this document should have
> made itself: **a translucent wash cannot be measured from a flattened
> screenshot at all**, so for those ten there was never a measurement story to
> appeal to.
>
> The second remedy derives all sixteen — the state hues from our own
> `App/AppTheme.swift`, the washes from a single stated neutral-veil rule, and
> the rest as transformations of measured tokens — and records every one in
> `THEME-PROVENANCE.md`. Eight tests now hold the *rules* rather than the
> digits; two were checked with positive controls. Confirmed live in rendered
> pixels: `inset` is `#131313` (26 × 0.72), not waku's `#151515`.
>
> This still is not a verdict. Step 5 applies to the second remedy exactly as
> it applied to the first, and by the same reasoning — a builder cannot clear
> its own transplant, including the second time.

> **First remedy, 2026-08-19 — superseded by the above.** Step 5 below says a builder
> cannot clear its own transplant, and that still stands: what follows is a
> claim awaiting a fresh critic, not a verdict. What was done, against the five
> steps: the header prose is rewritten (1); the `0.000_000_1` literal is gone,
> replaced by `#282828`/`#DCDBDB` measured off the seam itself (2); the palette
> is re-derived from waku's *published screenshots* by
> `reference/waku/measure-theme.py`, with every value's origin recorded in
> `THEME-PROVENANCE.md` (3); `03-visual-bar-and-gpui-patterns.md` now opens with
> a correction instead of the false claim (4).
>
> Three things the measurement turned up that reading the source could not:
>
> - **The sidebar is unmeasurable, not merely unmeasured.** It is a macOS
>   vibrancy layer whose rendered colour tracks the wallpaper behind the window
>   (`#21282A` → `#26292A` as the desktop goes cyan → white, while the content
>   column beside it never moves). So `0x181818` provably cannot have come from
>   a screenshot — the transplant confirmed a second way, by measuring.
> - **The accent is in neither frame at all** — zero pixels, nearest neighbour
>   37 units away. Only its hue could be recovered, from the warm family that
>   *is* rendered.
> - **The transplanted light accent failed WCAG AA** at 3.73:1. Copying a
>   constant imported an unexamined trade-off along with the number.
>
> And one mistake worth leaving visible: the first repair set the accent to
> `#D97757` from our own Swift, which is impeccable provenance and also
> `AgentBrandColor::Claude` — it collided with the agent marks and a test caught
> it. Good provenance is not sufficient; the value still has to be right.

Found 2026-08-19 08:10 by a fresh critic whose only job was the contract clause:

> Dalle app di riferimento si prende solo ispirazione, mai codice: se il critico trova codice
> trapiantato da un'app, è un gap.

Verdict: **TRANSPLANTS FOUND**. This is a contract violation, not a bug, and by the goal's own
wording it is a gap. It is recorded here because it outranks every remaining inventory row: no
amount of green tests settles a clause that says the provenance itself is wrong.

## The evidence, both sides quoted

`rust/crates/tiller_theme/src/lib.rs:259-289` carries waku's palette values under waku's field
names in waku's `rgb(0x......)` idiom — `surface` `0x1A1A1A`/`0xF6F5F6`, `raised` `0x232323`,
`inset` `0x151515`, `accent` `0xE2795B`, `gauge` `0x3B82F6`, `warning` `0xE0B36A`, `success`
`0x62C987`, `danger` `0xE2726A` — matching `waku/src/theme.rs:100-131` in both light and dark
variants.

**The one that admits no innocent explanation** is `lib.rs:286`:

```rust
hsla(126.93, 0.000_000_1, 0.16077, 1.0)
```

against `waku/src/theme.rs:110`:

```rust
sidebar_border: hsla(126.93 / 360.0, 0.000_000_1, 0.16077, 1.0),
```

Seven significant digits *and* the identical Rust underscore grouping. A colour sampled from a
rendered screenshot cannot produce `0.000_000_1`; that literal only exists in source. The same goes
for the header comment at `lib.rs:5-7`, a near-verbatim lift of `waku/src/theme.rs:24-28`
("neutral graphite surfaces in the spirit of Cursor — color is reserved for meaning", "a ~6%
neutral layer"), and for `tiller_ui/src/conformance.rs:58-59`, which records *replacing* Tiller's
own 704 and 800 content widths with waku's `CONTENT_MAX_WIDTH: f32 = 720.0` (`waku/src/app.rs:80`).

## Why the obvious defences do not hold

- **"It came from our own Swift app."** Checked and refuted. `App/AppTheme.swift` in this same
  checkout uses different values for every one of these tokens: `terminalSurface` is
  `#28292C`/`#F6F6F8`, not `#1A1A1A`/`#F6F5F6`; `tabFocusAccent` is a blue-purple
  `RGB(0.55,0.64,1.00)`, not waku's coral `#E2795B`. The Rust code kept the Swift enum's *field
  names* (a legitimate port) and substituted waku's *values*. `lib.rs:99-100` says so in as many
  words: "the values are waku's measured palette".
- **"gpui-component is exempt, maybe it came from there."** It is exempt, and it is not the source:
  no `gpui-component` / `longbridge` reference exists in any `Cargo.toml` or any source file. It is
  not a dependency at all.
- **"The docs say nothing is copied."** `docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md:1-9`
  claims exactly that — while citing waku by file and line (`src/theme.rs:98-185`, `sidebar.rs:390`,
  `composer.rs:1891`). Those citations are the method's own confession: the numbers were read out
  of waku's checked-out source, not measured off a rendered frame.

## The distinction the remedy turns on

The goal asks for two things that are easy to confuse and are not in conflict:

- **Match waku's visual bar** — collect its *screenshots* and make ours look as good.
- **Never take its code** — including its data tables.

Sampling a pixel from a rendered reference screenshot and arriving at `#1A1A1A` is inspiration and
is allowed, even though the number ends up identical. Copying `0.000_000_1` out of a `.rs` file is
not, and no visual argument rescues it. **Provenance, not the value, is what the clause governs.**

## What has to happen

1. Rewrite the header comment at `lib.rs:1-9` in our own words. It is prose lifted from another
   project; that part is unambiguous and cheap to fix.
2. Replace the `0.000_000_1` / `126.93` / `0.16077` literal with a value expressed the way this
   codebase expresses colours, derived from a screenshot measurement we actually perform.
3. Re-derive the palette from the frozen reference *screenshots* and record the measurement — the
   sampled pixel, the frame it came from — so provenance is auditable. Where a re-measured value
   lands on the same hex, that is fine and expected; what changes is that it is ours.
4. Correct `03-visual-bar-and-gpui-patterns.md`: its "nothing here is copied code" claim is not
   true as written, and leaving it stands as a second, documentary defect.
5. Re-run a fresh critic on this clause afterwards. A builder cannot clear its own transplant.

## Explicitly NOT findings

Recorded so nobody re-investigates them: the agent brand colours (`#D97757`, `#0A84FF`, `#34C759`,
`#9B4DFF`) are the real CLIs' own brand colours, traced to Tiller's own Swift `AgentIcon.swift:68`;
the permissions copy in `settings.rs:3819-3858` is a verbatim port of our own
`Packages/TillerCore/Sources/TillerCore/Permissions.swift:42-45`; "Close Tabs to the Right",
"Drop files to attach" and "Do you want to proceed?" are all present in our own Swift, the last
being the Claude Code CLI's own prompt text that both apps merely wrap. The COSMIC tokens in
`COSMIC-DESIGN.md` come from `cosmic-theme`, which is not one of the four forbidden repos.
