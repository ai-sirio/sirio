#!/usr/bin/env python3
r"""Pixel diff of two sets of screen captures, the evidence every bezel-off PR is judged by.

Usage: Scripts/capture-diff.py BEFORE AFTER --out DIR [--masks TSV] [--allow TSV] [--tolerance N]

BEFORE and AFTER are directories of PNG frames (Scripts/bezel-off-captures.sh writes one).
A frame is named by its path under its directory, without the ".png": "changes-dark/local-split".
Frames are paired by that name. A frame only in AFTER is `added`, only in BEFORE `removed`, in
both at different sizes `size`, in both at the same size with a pixel more than --tolerance
levels apart `changed`.

--masks and --allow share one tab-separated format, one rectangle per line (a line that is
blank or starts with # is skipped):

    frame_glob<TAB>x<TAB>y<TAB>w<TAB>h<TAB>why

frame_glob is matched (fnmatch, so `*` also crosses `/`) against the frame name, or against it
with the ".png". The rectangle is x,y (top left) and w,h in pixels of the frame.
  * A mask is a region driven by a clock (a caret, a spinner, a timestamp): it is painted black
    on a copy of both images before they are compared, so a difference inside it does not exist.
    `why` names the clock.
  * An allow is a region a PR is expected to change: a difference inside it is attributed.
    `why` is the first cell of the inventory row that explains it (`ui::tooltip::Tooltip`).

--tolerance N (levels on the 8-bit scale, default 1) is the noise floor. After the masks are
painted, a pixel counts as changed iff the largest difference over its channels (R, G, B and
alpha) between the two images is greater than N. The default exists because Mesa's software
Vulkan rasterizes a handful of glyph pixels one level apart in one channel from one app process to
the next (docs/testing/bezel-off.md); it cannot hide a change of 2 levels or more. --tolerance 0
compares strictly. Every number below (the verdict, `pixels`, `bbox`, `unattributed_pixels`, the
rows) is counted with it.

OUT gets changed.tsv (frame, status, pixels, bbox, unattributed_pixels, rows) and index.html, a
static page that shows each listed frame as before | after | diff with the allowed rectangles
in outline. The listed frames' images are copied under OUT, so OUT is self-contained.

Exit 0: no frame is added, removed or size-changed and no changed pixel lies outside every mask
and allow rectangle (prints CAPTURE DIFF IDENTICAL when nothing is listed at all, else
CAPTURE DIFF OK (...)). Exit 1: the diff found something unattributed (CAPTURE DIFF FAILED).
Exit 2: the tool could not do its job (a missing tree, a malformed table, an unreadable image, a
tolerance outside 0..255, no frame at all).

How a pixel is counted (the pipeline, and why): `magick \( \( A -colorspace sRGB -alpha off \)
\( B -colorspace sRGB -alpha off \) -compose difference -composite -separate \) \( \( A -colorspace
sRGB -alpha extract \) \( B -colorspace sRGB -alpha extract \) -compose difference -composite \)
-evaluate-sequence max` gives, per pixel, the largest |A - B| over R, G, B and alpha (a grayscale
PNG, which ImageMagick's encoder writes for a neutral window, is first widened to sRGB, so it is
compared in every channel too), and written as 8-bit gray it is one byte per pixel holding
exactly that number of levels, so "changed" is `byte > N` and nothing else. `magick compare -fuzz` is not used for the count: its
fuzz is a colour distance (the root of the summed squared channel differences, scaled), which is
not "no channel is more than N levels apart", and the AE figure `compare` prints is not a pixel
count on ImageMagick 7.1 HDRI (a one-pixel change printed 0.6, a change of 1/255 in one channel
0.0013 with exit 0). The page's diff image is drawn from the same counted pixels.

Needs ImageMagick 7 (`magick`) and nothing beyond the Python standard library.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import fnmatch
import hashlib
import html
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import quote


class Fatal(Exception):
    """The tool cannot do its job: exit 2, never a verdict about the frames."""


@dataclass(frozen=True)
class Rect:
    glob: str
    x: int
    y: int
    w: int
    h: int
    why: str

    def applies_to(self, frame: str) -> bool:
        return fnmatch.fnmatchcase(frame, self.glob) or fnmatch.fnmatchcase(frame + ".png", self.glob)

    def clamp(self, width: int, height: int) -> tuple[int, int, int, int]:
        """x0, y0, x1, y1 (exclusive) of the part of the rectangle inside a width x height frame."""
        return (min(self.x, width), min(self.y, height), min(self.x + self.w, width), min(self.y + self.h, height))


@dataclass
class Result:
    frame: str
    status: str  # changed | added | removed | size
    pixels: int | None = None
    bbox: str = "-"
    unattributed: int | None = None
    rows: list[str] = field(default_factory=list)
    before_size: tuple[int, int] | None = None
    after_size: tuple[int, int] | None = None
    allows: list[Rect] = field(default_factory=list)


def read_table(path: str | None) -> list[Rect]:
    if path is None:
        return []
    try:
        lines = Path(path).read_text().splitlines()
    except OSError as error:
        raise Fatal(f"cannot read {path}: {error.strerror}") from error
    rects = []
    for number, line in enumerate(lines, 1):
        if not line.strip() or line.startswith("#"):
            continue
        cells = line.split("\t", 5)
        if len(cells) != 6:
            raise Fatal(f"{path}:{number}: want frame_glob<TAB>x<TAB>y<TAB>w<TAB>h<TAB>why, got {len(cells)} cells")
        glob, why = cells[0], cells[5].strip()
        if not glob or not why:
            raise Fatal(f"{path}:{number}: the frame glob and the why must not be empty")
        try:
            x, y, w, h = (int(cell) for cell in cells[1:5])
        except ValueError as error:
            raise Fatal(f"{path}:{number}: x, y, w and h must be integers") from error
        if x < 0 or y < 0 or w < 1 or h < 1:
            raise Fatal(f"{path}:{number}: need x >= 0, y >= 0, w >= 1 and h >= 1")
        rects.append(Rect(glob, x, y, w, h, why))
    return rects


def run(args: list[str], *, ok: tuple[int, ...] = (0,)) -> bytes:
    try:
        done = subprocess.run(args, capture_output=True)
    except FileNotFoundError as error:
        raise Fatal("ImageMagick 7 (`magick`) is required on PATH") from error
    if done.returncode not in ok:
        detail = done.stderr.decode(errors="replace").strip().splitlines()
        raise Fatal(f"{' '.join(args[:3])} failed ({done.returncode}): {detail[0] if detail else 'no message'}")
    return done.stdout


def size_of(path: Path) -> tuple[int, int]:
    out = run(["magick", "identify", "-format", "%w %h\n", str(path)]).decode().split("\n")[0].split()
    if len(out) != 2:
        raise Fatal(f"cannot read the size of {path}")
    return int(out[0]), int(out[1])


def frames_in(root: str) -> dict[str, Path]:
    base = Path(root)
    if not base.is_dir():
        raise Fatal(f"{root} is not a directory")
    return {
        path.relative_to(base).as_posix()[: -len(".png")]: path
        for path in sorted(base.rglob("*.png"))
        if path.is_file()
    }


def sha(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def paint(source: Path, rects: list[Rect], width: int, height: int, target: Path) -> Path:
    """A copy of source with every rectangle opaque black. Rectangles are inclusive."""
    args = ["magick", str(source), "-fill", "black", "+antialias"]
    for rect in rects:
        x0, y0, x1, y1 = rect.clamp(width, height)
        if x1 > x0 and y1 > y0:
            args += ["-draw", f"rectangle {x0},{y0} {x1 - 1},{y1 - 1}"]
    run([*args, f"PNG32:{target}"])
    return target


def channel_deltas(before: Path, after: Path, width: int, height: int) -> bytes:
    """One byte per pixel: the largest difference over R, G, B and alpha, in 8-bit levels.

    `-set colorspace Gray` before the write keeps ImageMagick from converting the gray image
    through a gamma curve (without it a difference of 17 levels is written as 4).
    """
    data = run(
        [
            "magick",
            # Each image sits in its own parentheses: an operator such as -alpha acts on every
            # image read so far, so `A -alpha extract B -alpha extract` would extract A twice.
            # Each is first made sRGB (a no-op for an RGB PNG, an exact copy of the gray into R, G
            # and B for a one-channel PNG): `-separate` of a gray BEFORE would otherwise leave one
            # channel, and a change in AFTER's green or blue would never be compared.
            "(", "(", str(before), "-colorspace", "sRGB", "-alpha", "off", ")",
            "(", str(after), "-colorspace", "sRGB", "-alpha", "off", ")",
            "-compose", "difference", "-composite", "-separate", ")",
            "(", "(", str(before), "-colorspace", "sRGB", "-alpha", "extract", ")",
            "(", str(after), "-colorspace", "sRGB", "-alpha", "extract", ")",
            "-compose", "difference", "-composite", ")",
            "-evaluate-sequence", "max", "-set", "colorspace", "Gray", "-depth", "8", "gray:-",
        ]
    )
    if len(data) != width * height:
        raise Fatal(f"comparing {before} with {after} drew {len(data)} bytes, want {width * height}")
    return data


def bbox_of(data: bytes, width: int, height: int) -> str:
    rows = [y for y in range(height) if data.count(0, y * width, (y + 1) * width) != width]
    left, right = width, 0
    for y in rows:
        line = data[y * width : (y + 1) * width]
        left = min(left, width - len(line.lstrip(b"\0")))
        right = max(right, len(line.rstrip(b"\0")))
    return f"{left},{rows[0]},{right - left},{rows[-1] + 1 - rows[0]}"


def count_in(data: bytes, width: int, height: int, rect: Rect) -> int:
    x0, y0, x1, y1 = rect.clamp(width, height)
    return sum((x1 - x0) - data.count(0, y * width + x0, y * width + x1) for y in range(y0, y1))


def compare_frame(
    frame: str, before: Path, after: Path, masks: list[Rect], allows: list[Rect], out: Path, scratch: Path,
    tolerance: int,
) -> Result | None:
    """None when no pixel is more than `tolerance` levels apart once the masks are painted over."""
    if sha(before) == sha(after):
        return None
    size_before, size_after = size_of(before), size_of(after)
    if size_before != size_after:
        return Result(frame, "size", before_size=size_before, after_size=size_after)
    width, height = size_before
    mine = [rect for rect in masks if rect.applies_to(frame)]
    left, right = before, after
    if mine:
        key = hashlib.sha256(frame.encode()).hexdigest()[:16]
        left = paint(before, mine, width, height, scratch / f"{key}-before.png")
        right = paint(after, mine, width, height, scratch / f"{key}-after.png")
    over = bytes(255 if level > tolerance else 0 for level in range(256))
    data = channel_deltas(left, right, width, height).translate(over)
    pixels = width * height - data.count(0)
    if pixels == 0:
        return None
    attributed = bytearray(data)
    allowed = [rect for rect in allows if rect.applies_to(frame)]
    holding = []
    for rect in allowed:
        if count_in(data, width, height, rect):
            holding.append(rect.why)
        x0, y0, x1, y1 = rect.clamp(width, height)
        for y in range(y0, y1):
            attributed[y * width + x0 : y * width + x1] = bytes(x1 - x0)
    diff = out / "diff" / f"{frame}.png"
    diff.parent.mkdir(parents=True, exist_ok=True)
    counted = scratch / f"{hashlib.sha256(frame.encode()).hexdigest()[:16]}-counted.gray"
    counted.write_bytes(data)
    run(
        [
            "magick", str(right), "-colorspace", "sRGB", "-alpha", "off", "-fill", "white", "-colorize", "60%",
            "(", "-size", f"{width}x{height}", "-depth", "8", f"gray:{counted}",
            "-fill", "red", "-opaque", "white", "-transparent", "black", ")",
            "-composite", f"PNG24:{diff}",
        ]
    )
    return Result(
        frame,
        "changed",
        pixels=pixels,
        bbox=bbox_of(data, width, height),
        unattributed=width * height - attributed.count(0),
        rows=list(dict.fromkeys(holding)),
        before_size=size_before,
        after_size=size_after,
        allows=allowed,
    )


def copy_into(out: Path, side: str, frame: str, source: Path) -> None:
    target = out / side / f"{frame}.png"
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


def url(*parts: str) -> str:
    return "/".join(quote(segment) for part in parts for segment in part.split("/"))


def figure(label: str, source: str, size: tuple[int, int] | None, allows: list[Rect]) -> str:
    boxes = ""
    if size:
        for rect in allows:
            x0, y0, x1, y1 = rect.clamp(*size)
            if x1 > x0 and y1 > y0:
                boxes += (
                    f'<div class="allow" title="{html.escape(rect.why)}" style="left:{100 * x0 / size[0]:.4f}%;'
                    f"top:{100 * y0 / size[1]:.4f}%;width:{100 * (x1 - x0) / size[0]:.4f}%;"
                    f'height:{100 * (y1 - y0) / size[1]:.4f}%"></div>'
                )
    return (
        f'<figure><figcaption>{label}</figcaption><div class="shot">'
        f'<img src="{source}" alt="{label}" loading="lazy">{boxes}</div></figure>'
    )


PAGE_STYLE = """
:root { color-scheme: light dark; --bg: #f6f6f8; --fg: #1b1b20; --line: #c9c9d1; --accent: #d9480f; }
@media (prefers-color-scheme: dark) { :root { --bg: #16161a; --fg: #e6e6eb; --line: #3a3a44; --accent: #ffa94d; } }
body { background: var(--bg); color: var(--fg); font: 14px/1.5 system-ui, sans-serif; margin: 0; padding: 16px; }
h1 { font-size: 18px; margin: 0 0 4px; }
h2 { font-size: 15px; margin: 0; font-family: ui-monospace, monospace; }
section { border-top: 1px solid var(--line); margin-top: 20px; padding-top: 12px; }
.status { font: 12px ui-monospace, monospace; border: 1px solid var(--line); border-radius: 4px; padding: 0 6px; margin-left: 8px; }
.meta { margin: 2px 0 8px; opacity: .8; }
.trio { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }
@media (max-width: 720px) { .trio { grid-template-columns: minmax(0, 1fr); } }
figure { margin: 0; }
figcaption { font-size: 12px; opacity: .7; margin-bottom: 2px; }
.shot { position: relative; line-height: 0; border: 1px solid var(--line); }
.shot img { width: 100%; height: auto; display: block; }
.allow { position: absolute; box-sizing: border-box; outline: 2px solid var(--accent); pointer-events: auto; }
table { border-collapse: collapse; font-size: 13px; }
td, th { border: 1px solid var(--line); padding: 2px 8px; text-align: left; }
"""


def write_index(out: Path, results: list[Result], counts: dict[str, int], verdict: str, tolerance: int) -> None:
    table = "".join(
        f'<tr><td><a href="#f-{n}">{html.escape(r.frame)}</a></td><td>{r.status}</td>'
        f'<td>{"" if r.pixels is None else r.pixels}</td>'
        f'<td>{"" if r.unattributed is None else r.unattributed}</td><td>{html.escape("; ".join(r.rows))}</td></tr>'
        for n, r in enumerate(results)
    )
    sections = []
    for n, r in enumerate(results):
        if r.status == "changed":
            meta = f"{r.pixels} px changed, bbox {r.bbox}, {r.unattributed} outside every mask and allow rectangle"
            if r.rows:
                meta += f"; rows: {'; '.join(r.rows)}"
        elif r.status == "size":
            meta = f"size {r.before_size[0]}x{r.before_size[1]} before, {r.after_size[0]}x{r.after_size[1]} after"
        else:
            meta = "only in AFTER" if r.status == "added" else "only in BEFORE"
        shots = []
        if r.status != "added":
            shots.append(figure("before", url("before", f"{r.frame}.png"), r.before_size, r.allows))
        if r.status != "removed":
            shots.append(figure("after", url("after", f"{r.frame}.png"), r.after_size, r.allows))
        if r.status == "changed":
            shots.append(figure("diff", url("diff", f"{r.frame}.png"), r.after_size, r.allows))
        sections.append(
            f'<section class="frame" id="f-{n}" data-frame="{html.escape(r.frame, quote=True)}">'
            f'<h2>{html.escape(r.frame)}<span class="status">{r.status}</span></h2>'
            f'<p class="meta">{html.escape(meta)}</p><div class="trio">{"".join(shots)}</div></section>'
        )
    summary = ", ".join(f"{counts[key]} {key}" for key in ("changed", "added", "removed", "size"))
    body = (
        f"<h1>Capture diff</h1><p>{html.escape(verdict)}. Listed: {summary}. A pixel counts as changed when "
        f"a channel differs by more than {tolerance} level(s). Orange outlines are the allowed rectangles: the regions "
        "this change is expected to touch.</p>"
        + (f"<table><tr><th>frame</th><th>status</th><th>pixels</th><th>unattributed</th><th>rows</th></tr>{table}</table>" if results else "")
        + "".join(sections)
    )
    (out / "index.html").write_text(
        '<!doctype html><html lang="en"><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width, initial-scale=1">'
        f"<title>Capture diff</title><style>{PAGE_STYLE}</style></head><body>{body}</body></html>\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="Pixel diff of two sets of screen captures.")
    parser.add_argument("before", metavar="BEFORE")
    parser.add_argument("after", metavar="AFTER")
    parser.add_argument("--out", required=True, metavar="DIR")
    parser.add_argument("--masks", metavar="TSV", help="regions driven by a clock: painted black before comparing")
    parser.add_argument("--allow", metavar="TSV", help="regions the change is expected to touch")
    parser.add_argument(
        "--tolerance", type=int, default=1, metavar="N",
        help="levels (8-bit) a channel may differ before a pixel counts as changed (default 1; 0 is strict)",
    )
    arguments = parser.parse_args()
    if not 0 <= arguments.tolerance <= 255:
        raise Fatal("--tolerance must be between 0 and 255")

    masks, allows = read_table(arguments.masks), read_table(arguments.allow)
    before, after = frames_in(arguments.before), frames_in(arguments.after)
    if not before and not after:
        raise Fatal("neither BEFORE nor AFTER holds a PNG frame; nothing was captured")

    out = Path(arguments.out)
    for stale in ("before", "after", "diff"):
        shutil.rmtree(out / stale, ignore_errors=True)
    for stale in ("changed.tsv", "index.html"):
        (out / stale).unlink(missing_ok=True)
    out.mkdir(parents=True, exist_ok=True)

    results: list[Result] = [Result(frame, "added") for frame in after.keys() - before.keys()]
    results += [Result(frame, "removed") for frame in before.keys() - after.keys()]
    both = sorted(before.keys() & after.keys())
    with tempfile.TemporaryDirectory(prefix="capture-diff-") as scratch, concurrent.futures.ThreadPoolExecutor(
        max_workers=min(8, os.cpu_count() or 2)
    ) as pool:
        futures = [
            pool.submit(
                compare_frame, frame, before[frame], after[frame], masks, allows, out, Path(scratch), arguments.tolerance
            )
            for frame in both
        ]
        results += [done for done in (future.result() for future in futures) if done is not None]
    for result in results:
        if result.status == "added":
            result.after_size = size_of(after[result.frame])
            result.allows = [rect for rect in allows if rect.applies_to(result.frame)]
        elif result.status == "removed":
            result.before_size = size_of(before[result.frame])
            result.allows = [rect for rect in allows if rect.applies_to(result.frame)]
        elif result.status == "size":
            result.allows = [rect for rect in allows if rect.applies_to(result.frame)]
    results.sort(key=lambda result: result.frame)

    for result in results:
        if result.status != "added":
            copy_into(out, "before", result.frame, before[result.frame])
        if result.status != "removed":
            copy_into(out, "after", result.frame, after[result.frame])

    counts = {key: sum(1 for result in results if result.status == key) for key in ("changed", "added", "removed", "size")}
    unattributed = [result for result in results if result.status == "changed" and result.unattributed]
    failed = bool(counts["added"] or counts["removed"] or counts["size"] or unattributed)
    if not results:
        verdict = "CAPTURE DIFF IDENTICAL"
    elif failed:
        verdict = (
            f"CAPTURE DIFF FAILED ({counts['changed']} changed, {counts['added']} added, "
            f"{counts['removed']} removed, {counts['size']} size; {len(unattributed)} with unattributed pixels)"
        )
    else:
        verdict = f"CAPTURE DIFF OK ({counts['changed']} changed, all attributed)"

    with open(out / "changed.tsv", "w") as handle:
        handle.write("frame\tstatus\tpixels\tbbox\tunattributed_pixels\trows\n")
        for result in results:
            fields = [
                result.frame,
                result.status,
                "-" if result.pixels is None else str(result.pixels),
                result.bbox,
                "-" if result.unattributed is None else str(result.unattributed),
                ";".join(result.rows),
            ]
            handle.write("\t".join(fields) + "\n")
    write_index(out, results, counts, verdict, arguments.tolerance)

    print(
        f"compared {len(both)} frames present on both sides ({len(before)} before, {len(after)} after), "
        f"tolerance {arguments.tolerance}"
    )
    for result in results:
        if result.status == "changed":
            print(f"changed {result.frame}: {result.pixels} px, bbox {result.bbox}, {result.unattributed} unattributed")
        elif result.status == "size":
            print(f"size {result.frame}: {result.before_size[0]}x{result.before_size[1]} -> {result.after_size[0]}x{result.after_size[1]}")
        else:
            print(f"{result.status} {result.frame}")
    for result in unattributed:
        print(f"UNATTRIBUTED {result.frame}: {result.unattributed} px outside every mask and allow rectangle")
    print(f"changed.tsv: {out / 'changed.tsv'}")
    print(f"index.html: {out / 'index.html'}")
    print(verdict)
    return 1 if failed else 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Fatal as error:
        print(f"capture-diff: {error}", file=sys.stderr)
        sys.exit(2)
    except Exception as error:  # an unreadable file, a full disk: undecidable, never a verdict
        print(f"capture-diff: {type(error).__name__}: {error}", file=sys.stderr)
        sys.exit(2)
