#!/usr/bin/env python3
"""Reads, from the bundled TTF files themselves, what a family's face of a given weight
draws ten `m` at 20 px as: the width test-font-setting-e2e.sh compares with the text
system's.

    python3 -I font_face_advance.py --dir DIR --family NAME --weight W [--italic]

Prints one JSON object on one line:
  {"status": "ok", "file", "weight", "upem", "samples", "em", "width",
   "passes_0600", "passes_0602"}   the face the CSS font matching rule picks and, in
                                   `samples`, for each of "m", "a" and "0" the cmap glyph's
                                   hmtx advance, the same-glyph kerning, and
                                   width = (advance * 10 + kern * 9) * 20 / unitsPerEm
                                   (`em`, `width` and the passes_* are those of "m")
  {"status": "skip", "reason": ...}  an italic is asked and the family has no italic face
                                   (Red Hat, Geist): nothing to compare

`passes_0600` and `passes_0602` say whether a fallback face whose `m` is 0.600 em
(120.0 px) or 0.602 em (DejaVu Sans Mono, 120.4 px) would draw within 0.5 px of `width`:
where either is true, the width proves neither the weight nor the family of that face.

Only the standard library; it parses the sfnt tables it needs (head, hhea, hmtx, cmap, name,
OS/2, and GPOS for the `kern` pair adjustment of m-m). No rounding is modelled: gpui's layout
width is the sum of the unrounded advances at the requested size, plus the kerning the shaper
applies between two `m` (nine pairs): Geist kerns m-m by -20 to -14 font units depending on the
weight, which is the whole of its 2 % difference from the bare hmtx sum, and the other
bundled families have no such pair. The E2E's tolerance (0.5 px) is the whole allowance.

The family of a file is its typographic name (nameID 16, else 1), the weight its OS/2
usWeightClass, italic its OS/2 fsSelection italic bit: what the app's own bundled-faces
test reads, so the two cannot disagree on which file is which. The faces of the family in
DIR are matched with the CSS rule (weight 400 tries 500 next, 500 tries 400 next, a lighter
request goes lighter first, a heavier one heavier first), which is how a request for a
weight the family does not bundle (Red Hat 500, 600) lands on a file.
"""
import argparse
import glob
import json
import os
import struct
import sys

SAMPLES = "ma0"  # the characters the app measures, ten of each
SIZE_PX = 20.0
COUNT = 10


def tables(data):
    (count,) = struct.unpack(">H", data[4:6])
    found = {}
    for index in range(count):
        tag, _checksum, offset, length = struct.unpack(">4sIII", data[12 + 16 * index : 28 + 16 * index])
        found[tag.decode("latin-1")] = (offset, length)
    return found


def table(data, found, tag):
    offset, length = found[tag]
    return data[offset : offset + length]


def decode_name(platform, raw):
    if platform in (0, 3):
        return raw.decode("utf-16-be", "replace")
    return raw.decode("latin-1", "replace")


def family_of(data, found):
    name = table(data, found, "name")
    _format, count, string_offset = struct.unpack(">HHH", name[:6])
    best = {}
    for index in range(count):
        platform, _encoding, _language, name_id, length, offset = struct.unpack(
            ">HHHHHH", name[6 + 12 * index : 18 + 12 * index]
        )
        if name_id not in (1, 16):
            continue
        text = decode_name(platform, name[string_offset + offset : string_offset + offset + length])
        # Windows (3) records win over Macintosh ones when both name the family.
        if name_id not in best or platform == 3:
            best[name_id] = text
    return best.get(16) or best.get(1)


def cmap_glyph(data, found, codepoint):
    cmap = table(data, found, "cmap")
    (count,) = struct.unpack(">H", cmap[2:4])
    candidates = []
    for index in range(count):
        platform, encoding, offset = struct.unpack(">HHI", cmap[4 + 8 * index : 12 + 8 * index])
        (fmt,) = struct.unpack(">H", cmap[offset : offset + 2])
        if fmt in (4, 12):
            rank = 0 if (platform, encoding, fmt) == (3, 10, 12) else 1 if platform == 3 else 2
            candidates.append((rank, offset, fmt))
    for _rank, offset, fmt in sorted(candidates):
        glyph = lookup(cmap, offset, fmt, codepoint)
        if glyph:
            return glyph
    raise SystemExit("no cmap glyph for U+%04X" % codepoint)


def lookup(cmap, offset, fmt, codepoint):
    if fmt == 12:
        (groups,) = struct.unpack(">I", cmap[offset + 12 : offset + 16])
        for index in range(groups):
            start, end, glyph = struct.unpack(">III", cmap[offset + 16 + 12 * index : offset + 28 + 12 * index])
            if start <= codepoint <= end:
                return glyph + codepoint - start
        return 0
    (segments_x2,) = struct.unpack(">H", cmap[offset + 6 : offset + 8])
    segments = segments_x2 // 2
    ends = offset + 14
    starts = ends + segments_x2 + 2
    deltas = starts + segments_x2
    ranges = deltas + segments_x2
    for index in range(segments):
        (end,) = struct.unpack(">H", cmap[ends + 2 * index : ends + 2 * index + 2])
        if codepoint > end:
            continue
        (start,) = struct.unpack(">H", cmap[starts + 2 * index : starts + 2 * index + 2])
        if codepoint < start:
            return 0
        (delta,) = struct.unpack(">h", cmap[deltas + 2 * index : deltas + 2 * index + 2])
        (range_offset,) = struct.unpack(">H", cmap[ranges + 2 * index : ranges + 2 * index + 2])
        if range_offset == 0:
            return (codepoint + delta) & 0xFFFF
        position = ranges + 2 * index + range_offset + 2 * (codepoint - start)
        (glyph,) = struct.unpack(">H", cmap[position : position + 2])
        return (glyph + delta) & 0xFFFF if glyph else 0
    return 0


def advance(data, found, glyph):
    (upem,) = struct.unpack(">H", table(data, found, "head")[18:20])
    (metrics,) = struct.unpack(">H", table(data, found, "hhea")[34:36])
    hmtx = table(data, found, "hmtx")
    # Glyphs past the last full metric share its advance.
    index = min(glyph, metrics - 1)
    (units,) = struct.unpack(">H", hmtx[4 * index : 4 * index + 2])
    return upem, units


def u16(b, o):
    return struct.unpack(">H", b[o : o + 2])[0]


def s16(b, o):
    return struct.unpack(">h", b[o : o + 2])[0]


def coverage_index(b, o, glyph):
    fmt = u16(b, o)
    if fmt == 1:
        for i in range(u16(b, o + 2)):
            if u16(b, o + 4 + 2 * i) == glyph:
                return i
        return None
    for i in range(u16(b, o + 2)):
        start, end, first = u16(b, o + 4 + 6 * i), u16(b, o + 6 + 6 * i), u16(b, o + 8 + 6 * i)
        if start <= glyph <= end:
            return first + glyph - start
    return None


def class_of(b, o, glyph):
    fmt = u16(b, o)
    if fmt == 1:
        first, count = u16(b, o + 2), u16(b, o + 4)
        return u16(b, o + 6 + 2 * (glyph - first)) if first <= glyph < first + count else 0
    for i in range(u16(b, o + 2)):
        start, end, cls = u16(b, o + 4 + 6 * i), u16(b, o + 6 + 6 * i), u16(b, o + 8 + 6 * i)
        if start <= glyph <= end:
            return cls
    return 0


def value_record_size(fmt):
    return 2 * bin(fmt & 0xFF).count("1")


def x_advance(b, o, fmt):
    """The XAdvance of the value record at o (0 when the format has none)."""
    if not fmt & 0x0004:
        return 0
    skipped = bin(fmt & 0x0003).count("1")
    return s16(b, o + 2 * skipped)


def pair_kern(data, found, glyph):
    """The 'kern' feature's x advance adjustment of the pair (glyph, glyph), in font units
    (0 when the font has no GPOS or no such pair): the shaper applies it between two `m`.
    Device and variation tables are not applied: a variable face is read at its default
    instance, which is what the text system lays out."""
    if "GPOS" not in found:
        return 0
    b = table(data, found, "GPOS")
    feature_list, lookup_list = u16(b, 6), u16(b, 8)
    lookups = set()
    for i in range(u16(b, feature_list)):
        tag = b[feature_list + 2 + 6 * i : feature_list + 6 + 6 * i]
        if tag == b"kern":
            feature = feature_list + u16(b, feature_list + 6 + 6 * i)
            for j in range(u16(b, feature + 2)):
                lookups.add(u16(b, feature + 4 + 2 * j))
    for index in sorted(lookups):
        lookup = lookup_list + u16(b, lookup_list + 2 + 2 * index)
        kind = u16(b, lookup)
        for k in range(u16(b, lookup + 4)):
            sub = lookup + u16(b, lookup + 6 + 2 * k)
            sub_kind = kind
            if kind == 9:  # extension: the real subtable is behind a 32-bit offset
                sub_kind = u16(b, sub + 2)
                sub = sub + struct.unpack(">I", b[sub + 4 : sub + 8])[0]
            if sub_kind != 2:
                continue
            fmt1, fmt2 = u16(b, sub + 4), u16(b, sub + 6)
            covered = coverage_index(b, sub + u16(b, sub + 2), glyph)
            if covered is None:
                continue
            record = value_record_size(fmt1) + value_record_size(fmt2)
            if u16(b, sub) == 1:
                pair_set = sub + u16(b, sub + 10 + 2 * covered)
                for n in range(u16(b, pair_set)):
                    at = pair_set + 2 + n * (2 + record)
                    if u16(b, at) == glyph:
                        return x_advance(b, at + 2, fmt1)
            else:
                c1 = class_of(b, sub + u16(b, sub + 8), glyph)
                c2 = class_of(b, sub + u16(b, sub + 10), glyph)
                count2 = u16(b, sub + 14)
                at = sub + 16 + (c1 * count2 + c2) * record
                value = x_advance(b, at, fmt1)
                if value:
                    return value
    return 0


def describe(path):
    data = open(path, "rb").read()
    found = tables(data)
    os2 = table(data, found, "OS/2")
    (weight,) = struct.unpack(">H", os2[4:6])
    (selection,) = struct.unpack(">H", os2[62:64])
    return {
        "path": path,
        "file": os.path.basename(path),
        "family": family_of(data, found),
        "weight": weight,
        "italic": bool(selection & 1),
        "data": data,
        "found": found,
    }


def css_pick(faces, wanted):
    """The face the CSS font matching rule picks among upright faces for a weight."""
    weights = sorted({face["weight"] for face in faces})
    if wanted in weights:
        order = [wanted]
    elif 400 <= wanted <= 500:
        order = [w for w in weights if wanted < w <= 500] + [w for w in reversed(weights) if w < wanted] + [
            w for w in weights if w > 500
        ]
    elif wanted < 400:
        order = [w for w in reversed(weights) if w < wanted] + [w for w in weights if w > wanted]
    else:
        order = [w for w in weights if w > wanted] + [w for w in reversed(weights) if w < wanted]
    chosen = order[0]
    return next(face for face in faces if face["weight"] == chosen)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", required=True)
    parser.add_argument("--family", required=True)
    parser.add_argument("--weight", required=True, type=int)
    parser.add_argument("--italic", action="store_true")
    args = parser.parse_args()
    faces = [describe(path) for path in sorted(glob.glob(os.path.join(args.dir, "*.ttf")))]
    mine = [face for face in faces if face["family"] == args.family]
    if not mine:
        raise SystemExit("no face of family %r in the directory" % args.family)
    matching = [face for face in mine if face["italic"] == args.italic]
    if not matching:
        print(json.dumps({"status": "skip", "reason": "the family bundles no italic face"}))
        return
    face = css_pick(matching, args.weight)
    samples = {}
    for char in SAMPLES:
        glyph = cmap_glyph(face["data"], face["found"], ord(char))
        upem, units = advance(face["data"], face["found"], glyph)
        # Ten glyphs, nine pairs: the kerning the shaper applies between two of the same glyph
        # (Geist kerns m-m).
        kern = pair_kern(face["data"], face["found"], glyph)
        samples[char] = {
            "advance": units,
            "kern": kern,
            "width": round((units * COUNT + kern * (COUNT - 1)) * SIZE_PX / upem, 4),
        }
    m = samples["m"]
    print(
        json.dumps(
            {
                "status": "ok",
                "file": face["file"],
                "weight": face["weight"],
                "upem": upem,
                "samples": samples,
                "em": round(m["advance"] / upem, 6),
                "width": m["width"],
                "passes_0600": abs(m["width"] - 0.600 * SIZE_PX * COUNT) <= 0.5,
                "passes_0602": abs(m["width"] - 0.602 * SIZE_PX * COUNT) <= 0.5,
            }
        )
    )


if __name__ == "__main__":
    sys.exit(main())
