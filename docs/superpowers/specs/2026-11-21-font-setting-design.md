# Settings: the app's font, chosen by pair; default Atkinson Hyperlegible

**Date:** 2026-11-21
**Status:** asked for by Enzo, 2026-11-20 21:56 ("Aggiungiamo un'opzione nelle
impostazioni per far scegliere all'utente il font. Come default Atkinson
Hyperlegible Next + Atkinson Hyperlegible Mono."). The calls marked **(my
call)** were made while writing this, each with the recommended option and its
cost; **(coordinator)** marks the ones the coordinator settled.
**Inputs:** the font page Enzo looked at (six pairs plus today's Geist, with
their reasoning), and the coordinator's brief.

## §0 Intent

Sirio draws its interface in Geist and its code in Geist Mono, fixed. Enzo
wants the person to choose, and wants the most legible pair as the default:
Atkinson Hyperlegible, designed so that I, l and 1, and O and 0, never look
alike, which matters in long sessions reading agents' output. The six pairs
were drawn up as pairs (an interface face and the code face that suits it), so
the choice is a pair, not two free fields.

## §1 What

**One "Font" row** in Settings → Appearance → Interface, above "Font size". It
opens a list of seven pairs:

| id | Title | Interface | Code |
|---|---|---|---|
| `atkinson` | Atkinson Hyperlegible (**default**) | Atkinson Hyperlegible Next | Atkinson Hyperlegible Mono |
| `inter` | Inter + JetBrains Mono | Inter | JetBrains Mono |
| `plex` | IBM Plex | IBM Plex Sans | IBM Plex Mono |
| `redhat` | Red Hat | Red Hat Text | Red Hat Mono |
| `recursive` | Recursive | Recursive Sans Linear Static | Recursive Mono Linear Static |
| `figtree` | Figtree + Fira Code | Figtree | Fira Code |
| `geist` | Geist | Geist | Geist Mono |

(The families are the typographic names in the bundled files, checked by test.)

- **Applies live**, with no restart, to everything drawn in the interface or
  code font: sidebar, tabs, chat, markdown, editor, diffs, settings, dialogs,
  Ely components.
- **The terminal does not change.** It keeps JetBrainsMono Nerd Font Mono: the
  agents' status lines need Nerd Font glyphs. The row sits in the Interface
  section, which already says what it governs, so no note is drawn under it.
  **(my call)** Cost: someone might expect the terminal to follow; the
  Terminal section has its own size row and no font row.
- **Persisted** as one key, `appearance.fontPair`, the pair's id. Absent,
  empty or unknown reads as `atkinson`. Nothing else is persisted.
- **Every pair is bundled** in the binary, as Geist is today: offline, the same
  faces on every platform.

## §2 Decisions (each with its cost)

1. **Existing users move to Atkinson.** A person who never chose has no
   `appearance.fontPair` row, and absent means the default, as the brief asks.
   Settings are saved whole (a panel resize saves them too), so the row is
   written as `atkinson` at the first save. Cost: everyone's Sirio looks
   different after the update (Geist is one click away, last in the list);
   and if the default ever changes again, people who never chose keep
   `atkinson`. **(my call)**
2. **Faces bundled per pair:** interface 400, 500, 600, 700 and 400 italic;
   code 400 and 700, all static TTF (gpui's Linux path rasterises a variable
   font at its default instance only), about 8.8 MB in all. The italic is
   there because markdown draws emphasis in the interface face; without it an
   italic run paints upright (bezel's comment on Geist). The code face gets
   no italic: nothing draws code in italic except a CI log's ANSI italic,
   which already paints upright today. **(my call)** Exceptions:
   - Geist stays exactly as bundled today (its 400 and its mono are variable
     files at their default instance, no italic).
   - Red Hat Text ships its Medium at weight 565 and its SemiBold at 632. The
     SemiBold is bundled (a 600 request matches it); the Medium is not (a 500
     request matches the 400 under CSS weight matching, so the 565 file would
     never be drawn).
   - Red Hat Text's italic is not bundled: the file is italic only by its
     slant angle, not by its style bits, so Windows and macOS could take it
     for a second upright 400. Red Hat's emphasis then behaves like Geist's.
   Costs: binary size (measured in the PR); in Red Hat, medium text draws at
   400 and emphasis is not its own italic; bold italic and italic headings
   draw the 400 italic (one italic weight per pair), where Geist's system
   fallback has a bold oblique.
3. **Glyph coverage, recorded, not patched.** Atkinson Hyperlegible Mono maps
   about 360 code points and lacks box drawing (`├ │ ─ └`), arrows, `✓`, `●`
   and `λ`; Red Hat Mono lacks the same and `≥`; Recursive Mono lacks box
   drawing; Atkinson Hyperlegible Next lacks `→ ← ↑ ✓`. Those glyphs are then
   drawn from the system's fallback, which differs per machine, and in
   Atkinson Mono a fallback glyph is 0.600 em wide against its own 0.632, so a
   file tree in an agent's reply drifts by a pixel or two per column. A fixed
   bundled fallback was considered and left out: the surfaces that matter
   (bezel's code blocks, Ely's editor, direct measurements) build their fonts
   explicitly, without fallbacks, so a fallback on the root text style would
   reach only plain text. The contact sheet shows these glyphs in every pair.
   **(my call)** Cost: as described; a fallback chain per surface is a
   follow-up if Enzo wants it.
4. **Registration at startup, one batch per pair.** Geist and the terminal
   font keep their own calls; each new pair is one more `add_fonts` call,
   before the first window, and a pair whose call fails is logged and left
   out (one bad face aborts its whole batch, so one batch per pair keeps
   Geist safe; a Geist failure is logged too and the pairs still register).
   Which bundled families the text system lists is computed once, after
   registration. **(my call)** Costs: on Windows a batch is not atomic
   (DirectWrite keeps the faces added before the bad one), so a failing pair
   can stay partly registered there, with some weights drawn by fallback; about
   forty more faces parsed before the first window (bytes are `'static`; the
   time is logged at startup and reported in the PR). A family also installed
   on the system (Inter, say) may be drawn from the system's copy, as Geist
   already could. gpui's last-resort stack names "IBM Plex Sans", which is now
   always present, so a family that fails to resolve anywhere draws in the
   bundled Plex instead of whatever came next.
5. **The control is Ely's `Select`**, as wide as its widest title laid out in
   the current face and size plus the trigger's chrome (a fixed 220 px cut
   "Atkinson Hyperlegible" at interface size 18), with each pair's title as a
   plain label. It opens anchored under the row (its list is raised
   through `defer_draw`, so the card's `overflow_hidden` does not clip it),
   walks with the arrow keys and marks the chosen pair. Escape on the open
   list closes the list only; Escape again closes Settings (Settings binds
   Escape globally, so the select sits in its own key context that holds the
   global binding back while the list is open, as the summarizer picker
   already does). The other candidate was a menu built like the summarizer
   picker, which could draw each row in its own face but floats at the
   surface's bottom right; the coordinator preferred the anchored select, as
   the brief allows ("otherwise plain"). **(coordinator)** Cost: the list does
   not show each pair in its own face (the live switch and the contact sheet
   do); this is the first Ely `Select` on the Settings page; the control's width
   moves by a few pixels with the pair and the interface size.
6. **Where the families live.** `Typography.ui_family` and `code_family`
   stay `&'static str` (so `Theme` stays `Copy`), now taken from the pair
   table. `Theme::set_font_pair` sets them; `set_interface_font_size` keeps
   them (it used to rebuild `Typography` from the families resolved once at
   startup, which would undo a switch at the next size step). A pair whose
   families the text system does not list keeps, for that role, the family
   resolved at startup. **(my call)** Cost: none visible while registration
   works.
7. **bezel follows the theme.** bezel's `font_sans` and `font_mono` were the
   literals "Geist" and "Geist Mono"; they now come from the theme's
   typography, and bezel's registry is re-installed on a switch. Its
   `font_sans_fallback` (what bezel markdown uses for italic) becomes the
   pair's interface family when the pair bundles an italic, and stays the
   system face otherwise (Geist, Red Hat). This covers chat markdown and tool
   output on today's main; once Ely step 2 lands, markdown follows Ely's
   theme families, and bezel still carries the chat tool output. Cost: none;
   Geist's bezel theme comes out exactly as today.
8. **Ely follows by the path it already has:** the theme observer copies
   `typography.ui_family` and `code_family` into Ely's theme whenever the
   typography changes.
9. **Measurements tied to Geist.**
   - The Changes tab caches its longest-line width and re-measured only on
     content changes (already stale after a size change). The cache now
     remembers the family and size it measured with.
   - The Changes tab's line number column was 40 px, sized for five 12 px
     Geist Mono digits, but its digits are drawn at the interface-scaled code
     size: at interface size 18 five of them need 51 px in Geist Mono (cut on
     main already) and 53.7 in Atkinson Mono (0.632 em, the widest here; Fira
     Code 0.615, the others 0.600). The column is now five digits measured in
     the current code face and size, rounded up, plus 4 px, never under 40:
     Geist at the default size keeps exactly 40. **(coordinator)**
   - The cut on main is a wrap: gpui's line wrapper breaks a word that does
     not fit at a character, so a five-digit number drops its last digit to a
     second line below its 20 px row, and nothing shows left of the cell. The
     diff number cells (Changes, and the chat's preview for consistency) now
     never wrap, so a number that still does not fit overflows to the left,
     where it shows and where the end-to-end test looks for it. **(my call)**
     Cost: none while the measurement holds; if it is ever short, a number
     spills toward the column on its left instead of losing a digit.
   - The chat's diff preview has the same column at 34 px for four digits
     (with 4 and 8 px of padding, 28.8 px of Geist Mono digits already
     overflow the new-number side). It is measured the same way: four digits
     plus its padding, never under 34.
   - The tab title width estimate (7.5 px a character) stays.
   - The chat list keeps the heights it measured, off screen, in the previous
     face until those rows are laid out again (gpui resets the list on a width
     change only), so the scrollbar's extent can shift for a moment after a
     switch, as it already does after a size step. Left as it is; a follow-up.
   Cost: the diff's code starts a few pixels further right with the wider
   faces and at larger sizes; a tab title can ellipsise a few pixels early
   with a wider interface face.
10. **Ligatures** draw as each font ships them, under gpui's default features
    (Fira Code and JetBrains Mono join `->` and `!=`). No ligature switch.
    **(my call)** Cost: someone who dislikes ligatures picks another pair.

## §3 Licences

Every family is SIL OFL 1.1. Each licence text goes under `rust/assets/fonts`
next to its faces, and `THIRD_PARTY_NOTICES.md` gets one entry per pair
(source URL, tag or commit, version, the files), in the shape of the Geist
entry. SHA-256 sums go in `rust/assets/fonts/README.md`, as for Geist and the
terminal font.

## §4 Anti-goals

The terminal font, the terminal size, the interface size steps and their
range do not change. Nothing new is persisted beyond the one key. No vendored
change. No font is downloaded at runtime.

## §5 Testing

End to end, framed, light and dark, under a private Xvfb, with a re-runnable
artifact (transcript, measurements, frames, contact sheets). A new
`Scripts/Tests/test-font-setting-e2e.sh`:

- A fresh database: the pair is `atkinson`.
- Through the real Settings UI (a click on the Font select, then the arrow
  keys, or for one pair a click on its row), each of the seven pairs in turn.
  After each, the debug read names the pair's families for Sirio's
  typography, Ely's theme and bezel's theme; for each role and each bundled
  weight, the text system's width of a fixed string equals the width the
  bundled file's own advances give (so the bundled face draws, at that
  weight); a crop of fixed text differs from the previous pair's. Choosing
  the same pair twice gives the same crop, and a terminal's crop never
  changes.
- Escape on the open list closes the list and leaves Settings open; Escape
  again closes Settings.
- One interface size step keeps the pair's families.
- Quit and relaunch: the chosen pair is back from its first read. A seeded
  unknown id reads as `atkinson`.
- At interface size 18 in Atkinson, a diff whose line numbers have five
  digits: no ink of a number crosses its cell's left edge (pixels), and the
  frame is kept.
- A contact sheet per appearance: the same window (sidebar, a chat whose
  reply has prose, emphasis, inline code, a code block and a line of box
  drawing, arrows and checks, and a source file in the editor) in each pair.
- The startup log reports how long registering the faces took.
- The debug reads are debug-only, like the sidebar's.

The existing test that the bundled faces name their family keeps its intent
for every face: each file names its pair's family for its role, has its
declared weight and italic style (style bits and slant both), and is static
(Geist's two variable files excepted, as today).
