#!/usr/bin/env python3
"""Pixel probe for comparing a candidate render against a frozen reference shot.

  probe.py px <png> <x> <y> [...]        -> print #rrggbb at each point
  probe.py diff <a.png> <b.png>          -> mean/max channel delta + worst rows

Pure stdlib: PNG decode via zlib, so it runs anywhere without Pillow.
"""
import sys
import zlib


def load(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", f"{path} is not a PNG"
    pos, idat, w = 8, b"", None
    while pos < len(data):
        ln = int.from_bytes(data[pos:pos + 4], "big")
        typ = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + ln]
        if typ == b"IHDR":
            w = int.from_bytes(body[0:4], "big")
            h = int.from_bytes(body[4:8], "big")
            depth, ctype = body[8], body[9]
            assert depth == 8 and ctype in (2, 6), f"unsupported PNG {depth}/{ctype}"
            nch = 3 if ctype == 2 else 4
        elif typ == b"IDAT":
            idat += body
        elif typ == b"IEND":
            break
        pos += 12 + ln

    raw = zlib.decompress(idat)
    stride = w * nch
    out = bytearray(h * stride)
    prev = bytearray(stride)
    p = 0
    for y in range(h):
        ft = raw[p]
        p += 1
        line = bytearray(raw[p:p + stride])
        p += stride
        for i in range(stride):
            a = line[i - nch] if i >= nch else 0
            b = prev[i]
            c = prev[i - nch] if i >= nch else 0
            if ft == 1:
                line[i] = (line[i] + a) & 255
            elif ft == 2:
                line[i] = (line[i] + b) & 255
            elif ft == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif ft == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                pr = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                line[i] = (line[i] + pr) & 255
        out[y * stride:(y + 1) * stride] = line
        prev = line
    return w, h, nch, out


def main():
    cmd = sys.argv[1]
    if cmd == "px":
        w, h, nch, buf = load(sys.argv[2])
        pts = list(map(int, sys.argv[3:]))
        for x, y in zip(pts[::2], pts[1::2]):
            o = y * w * nch + x * nch
            print(f"({x},{y}) #{buf[o]:02x}{buf[o+1]:02x}{buf[o+2]:02x}")
    elif cmd == "diff":
        wa, ha, na, a = load(sys.argv[2])
        wb, hb, nb, b = load(sys.argv[3])
        if (wa, ha) != (wb, hb):
            print(f"SIZE MISMATCH {wa}x{ha} vs {wb}x{hb}")
            return 1
        total = worst = 0
        rows = []
        for y in range(ha):
            racc = 0
            for x in range(wa):
                oa, ob = y * wa * na + x * na, y * wb * nb + x * nb
                d = max(abs(a[oa + c] - b[ob + c]) for c in range(3))
                racc += d
                worst = max(worst, d)
            total += racc
            rows.append((racc / wa, y))
        print(f"mean_delta={total / (wa * ha):.2f} max_delta={worst}")
        for m, y in sorted(rows, reverse=True)[:8]:
            print(f"  row y={y} mean={m:.1f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
