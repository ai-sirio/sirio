#!/usr/bin/env python3
"""Side-by-side composer for reference-vs-ours comparisons.

    python3 Scripts/sbs.py REFERENCE.png OURS.png OUT.png [--label-left L] [--label-right R]

The two shots rarely have identical heights (window chrome, capture drift), so
each is pasted at its own size onto a canvas tall enough for the taller one and
left-aligned at the top. Nothing is scaled: a resized comparison hides exactly
the pixel differences this is meant to show.
"""

import sys
from PIL import Image, ImageDraw

GUTTER = 16
BAR = 24
BG = (24, 24, 27)
FG = (230, 230, 235)


def main(argv):
    if len(argv) < 4:
        print(__doc__)
        return 2
    left_path, right_path, out_path = argv[1], argv[2], argv[3]
    labels = {"--label-left": "reference", "--label-right": "ours"}
    for flag in labels:
        if flag in argv:
            labels[flag] = argv[argv.index(flag) + 1]

    left, right = Image.open(left_path), Image.open(right_path)
    width = left.width + GUTTER + right.width
    height = BAR + max(left.height, right.height)

    canvas = Image.new("RGB", (width, height), BG)
    canvas.paste(left, (0, BAR))
    canvas.paste(right, (left.width + GUTTER, BAR))

    draw = ImageDraw.Draw(canvas)
    draw.text((4, 6), labels["--label-left"], fill=FG)
    draw.text((left.width + GUTTER + 4, 6), labels["--label-right"], fill=FG)
    # The seam makes it obvious where one shot ends and the other begins,
    # which matters when both are the same dark grey.
    draw.line([(left.width + GUTTER // 2, 0), (left.width + GUTTER // 2, height)], fill=(90, 90, 100))

    canvas.save(out_path)
    print(f"{out_path} {canvas.width}x{canvas.height}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
