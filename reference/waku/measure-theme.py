#!/usr/bin/env python3
"""Measures Tiller's theme tokens off waku's published product screenshots.

Why this exists: the goal freezes waku as Tiller's *visual* bar while
forbidding its code. Those two are only compatible if our palette comes from
looking at rendered frames. This script is that looking, written down so the
numbers in `tiller_theme` are auditable instead of asserted — re-run it and you
get the table in `docs/linux-rewrite/THEME-PROVENANCE.md` back.

Frames: `app-screenshot-{dark,light}.png` in this directory, waku's own
published product shots (the ones on waku.sh), copied verbatim.

Method, and its limits:

- Flat regions are sampled as a *patch*, not a pixel, and reported with the
  share of the patch the winning colour occupies. A single pixel can land on a
  glyph or an antialiased edge and lie; a patch that is 100% one colour cannot.
  Coverage well under 100% is the signal that a region is not flat, and that is
  a finding rather than a nuisance — see the sidebar.
- Glyph colours take the histogram entry furthest from the patch's dominant
  colour, which is the text core. Accent hunting instead takes maximum chroma,
  because "furthest from the background" picks white text over a warm accent
  every time.
- Both frames are 8-bit palettized PNGs (255 colours). Large flat areas get
  their own palette entry and survive intact; thin antialiased detail is an
  approximation. Every conclusion below is drawn from areas large enough that
  quantization cannot account for the result.

Requires ImageMagick's `convert` on PATH. No network, no other dependencies.

    ./measure-theme.py                 # both frames, full report
"""

import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
FRAMES = {
    "dark": os.path.join(HERE, "app-screenshot-dark.png"),
    "light": os.path.join(HERE, "app-screenshot-light.png"),
}

# Regions in frame pixels. The frames are 2266x1752 at 2x device pixel ratio,
# so every rect here is twice the logical size it covers.
FLAT = [
    ("surface", 887, 351, 80, 80, "transcript background, above the first bubble"),
    ("composer", 1608, 1360, 80, 40, "composer field, right of its placeholder"),
    ("raised", 1814, 290, 80, 10, "user message bubble, above the cap height"),
    ("titlebar", 1402, 182, 80, 20, "header strip, right of the title"),
    ("footer", 1402, 1539, 80, 20, "bottom bar, right of the branch chips"),
    ("sidebar", 372, 1196, 80, 80, "empty sidebar below the session list"),
]

GLYPH = [
    ("text", 700, 548, 90, 26, "bold body text 'Waku'"),
    ("text_tertiary", 1330, 462, 180, 26, "'Worked for 10 seconds' meta line"),
]

# Warm accent candidates, picked by chroma rather than distance.
CHROMA = [
    ("code span 0.0.3", 1690, 545, 95, 30),
    ("code span src/driver/", 720, 855, 200, 30),
    ("code span activity/support", 1830, 855, 180, 30),
    ("send button", 1990, 1440, 60, 60),
]

# The token each frame's palette is claimed to carry but may not.
ABSENT_PROBE = {"dark": "#E2795B", "light": "#C85F44"}


def histogram(frame, x, y, w, h):
    """Colour histogram of one patch, as [(count, '#RRGGBB'), ...]."""
    out = subprocess.run(
        ["convert", frame, "-crop", f"{w}x{h}+{x}+{y}", "+repage",
         "-format", "%c", "histogram:info:-"],
        capture_output=True, text=True, check=True).stdout
    rows = []
    for line in out.splitlines():
        match = re.match(r"\s*(\d+):.*?(#[0-9A-Fa-f]{6})", line)
        if match:
            rows.append((int(match.group(1)), match.group(2).upper()))
    return rows


def rgb(hexcolor):
    return tuple(int(hexcolor[i:i + 2], 16) for i in (1, 3, 5))


def chroma(hexcolor):
    red, green, blue = rgb(hexcolor)
    return max(red, green, blue) - min(red, green, blue)


def dominant(rows):
    return max(rows, key=lambda row: row[0])


def report_flat(frame, label):
    print(f"\n## Flat regions — {label}")
    print(f"{'token':<12} {'rect':<20} {'colour':<9} {'coverage':>8}  region")
    for name, x, y, w, h, note in FLAT:
        rows = histogram(frame, x, y, w, h)
        count, colour = dominant(rows)
        share = count * 100 // (w * h)
        print(f"{name:<12} {f'{w}x{h}+{x}+{y}':<20} {colour:<9} {share:>7}%  {note}")


def report_glyph(frame, label):
    print(f"\n## Glyph colours — {label}")
    print(f"{'token':<15} {'rect':<20} {'behind':<9} {'glyph':<9}  region")
    for name, x, y, w, h, note in GLYPH:
        rows = histogram(frame, x, y, w, h)
        _, back = dominant(rows)
        base = rgb(back)
        best = None
        for count, colour in rows:
            if count < 15:
                continue
            distance = sum((a - b) ** 2 for a, b in zip(rgb(colour), base))
            if best is None or distance > best[0]:
                best = (distance, colour)
        glyph = best[1] if best else "-"
        print(f"{name:<15} {f'{w}x{h}+{x}+{y}':<20} {back:<9} {glyph:<9}  {note}")


def report_chroma(frame, label):
    print(f"\n## Warmest colour per region (chroma-ranked) — {label}")
    for name, x, y, w, h in CHROMA:
        rows = histogram(frame, x, y, w, h)
        ranked = sorted(((chroma(c), n, c) for n, c in rows if n >= 8), reverse=True)
        top = "  ".join(f"{c} (chroma {ch}, {n}px)" for ch, n, c in ranked[:2])
        print(f"  {name:<28} {top}")


def report_translucency(frame, label):
    """Is a column opaque, or does the wallpaper behind the window show through?

    An opaque region holds one colour whatever is behind it. A translucent one
    drifts as the wallpaper does, and picks up its hue.
    """
    print(f"\n## Opacity probe — {label}  (sidebar x=400 vs content x=1430 vs desktop x=40)")
    print(f"{'y':>6}  {'sidebar':<9} {'content':<9} {'desktop behind window':<9}")
    for y in (1000, 1100, 1200, 1300, 1400, 1500, 1560):
        cells = []
        for x in (400, 1430, 40):
            _, colour = dominant(histogram(frame, x, y, 24, 8))
            cells.append(colour)
        print(f"{y:>6}  {cells[0]:<9} {cells[1]:<9} {cells[2]:<9}")


def report_divider(frame, label):
    print(f"\n## Sidebar/content seam — {label}  (1px column, y 1000..1200)")
    for x in range(659, 668):
        rows = histogram(frame, x, 1000, 1, 200)
        count, colour = dominant(rows)
        flat = "flat" if count == 200 else f"{count}/200"
        print(f"  x={x}  {colour}  {flat}")


def report_absent(frame, label):
    """Does the colour the source code claims is the accent appear at all?"""
    target = ABSENT_PROBE[label]
    base = rgb(target)
    out = subprocess.run(["convert", frame, "-format", "%c", "histogram:info:-"],
                         capture_output=True, text=True, check=True).stdout
    rows = []
    for line in out.splitlines():
        match = re.match(r"\s*(\d+):.*?(#[0-9A-Fa-f]{6})", line)
        if match:
            rows.append((int(match.group(1)), match.group(2).upper()))
    exact = sum(n for n, c in rows if c == target)
    near = sorted(((sum((a - b) ** 2 for a, b in zip(rgb(c), base)) ** 0.5, n, c)
                   for n, c in rows if n >= 20))[:4]
    print(f"\n## Is {target} in the frame at all? — {label}")
    print(f"  distinct colours in frame: {len(rows)}")
    print(f"  exact occurrences of {target}: {exact} px")
    for distance, count, colour in near:
        print(f"    nearest {colour}  distance={distance:6.1f}  {count} px")


def report_geometry(frame, label):
    """Spans that paint a visible edge, and are therefore measurable.

    Frames are 2x, so every span here halves to logical pixels. Most of the
    layout is *not* here, and that absence is the point: bar heights sit on the
    same fill as the content with no divider between them, so a picture of one
    contains no evidence of where it ends.
    """
    print(f"\n## Measurable spans — {label}  (frame px; halve for logical)")

    def run(x, y, axis, span, width):
        """Walks one axis reporting every colour change."""
        previous, changes = None, []
        for step in span:
            at = (x, step) if axis == "y" else (step, y)
            box = (width, 1) if axis == "y" else (1, width)
            _, colour = dominant(histogram(frame, at[0], at[1], box[0], box[1]))
            if colour != previous:
                changes.append((step, colour))
                previous = colour
        return changes

    fill = run(610, 0, "y", range(430, 570), 6)
    print("  selected sidebar card, column x=610 (no glyphs):")
    for at, colour in fill:
        print(f"    y={at}  {colour}")

    print("  window edges, for the spans above to be relative to something:")
    for name, x, y, axis, span, width in [
        ("left", 0, 800, "x", range(150, 168), 40),
        ("top", 1400, 0, "y", range(138, 152), 40),
        ("bottom", 1400, 0, "y", range(1582, 1596), 40),
    ]:
        edges = run(x, y, axis, span, width)
        print(f"    {name:<7} " + "  ".join(f"{a}:{c}" for a, c in edges))


def main():
    if subprocess.run(["which", "convert"], capture_output=True).returncode != 0:
        sys.exit("ImageMagick's `convert` is not on PATH")
    for label, frame in FRAMES.items():
        if not os.path.exists(frame):
            sys.exit(f"missing frame: {frame}")
        print(f"\n{'=' * 72}\n{os.path.basename(frame)}\n{'=' * 72}")
        report_flat(frame, label)
        report_glyph(frame, label)
        report_chroma(frame, label)
        report_translucency(frame, label)
        report_divider(frame, label)
        report_absent(frame, label)
        report_geometry(frame, label)


if __name__ == "__main__":
    main()
