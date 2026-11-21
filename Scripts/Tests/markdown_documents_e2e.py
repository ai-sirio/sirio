#!/usr/bin/env python3
"""The checks of test-markdown-documents-e2e.sh that need real input and parsing.

`preview` drives a real Sirio showing documents.md in the file Preview; `chat` drives a real Sirio
with a long chat reply. Both run under the script's private Xvfb, in the script's isolated world,
and write what they saw to the script's artifact directory. The checks are named as the script's
header names them; a failed check raises Fail, and the script's FAIL line carries its message.
"""
import base64
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

E = os.environ
DISP = E["DISP"]
OUT = Path(E["OUT"])
CHECKS = Path(E["CHECKS"])
CTL = E.get("CTL", "")
RUN_DIR = Path(E["RUN_DIR"])
ENV_X = dict(os.environ, DISPLAY=DISP)
MAGENTA = (250, 40, 190)


class Fail(Exception):
    pass


MAX_NESTING = 0  # read from the source by deepest_nesting_source


def log(message):
    print(message, flush=True)


def record(name, detail, status="OK"):
    with CHECKS.open("a") as handle:
        handle.write(json.dumps({"check": name, "status": status, "detail": detail}) + "\n")
    log(f"{status}: {name}: {detail}")


def need(condition, name, message):
    if not condition:
        raise Fail(f"{name}: {message}")


# ---------------------------------------------------------------- input and the display


def x(*arguments, timeout=30):
    subprocess.run(["xdotool", *arguments], env=ENV_X, check=True, timeout=timeout)


def move(window, wx, wy):
    x("mousemove", "--window", window, str(int(wx)), str(int(wy)))


def press(window, wx, wy, button="1", hold=0.3):
    """A real click, its press and release a hand's distance apart: text listens for the release in
    the frame after the press, and this window paints in software."""
    move(window, wx, wy)
    time.sleep(0.2)
    x("mousedown", button)
    time.sleep(hold)
    x("mouseup", button)


def drag(window, x1, y1, x2, y2):
    move(window, x1, y1)
    time.sleep(0.2)
    x("mousedown", "1")
    for step in range(1, 9):
        time.sleep(0.05)
        move(window, x1 + (x2 - x1) * step / 8, y1 + (y2 - y1) * step / 8)
    time.sleep(0.3)
    x("mouseup", "1")
    time.sleep(0.5)


def wheel(window, wx, wy, button, clicks):
    move(window, wx, wy)
    x("click", "--repeat", str(clicks), "--delay", "5", button, timeout=60)
    time.sleep(1.2)


def key(chord):
    x("key", "--clearmodifiers", chord)
    time.sleep(0.4)


GTK_CLIPBOARD = """
import sys
import gi
gi.require_version("Gtk", "3.0")
gi.require_version("Gdk", "3.0")
from gi.repository import Gdk, GLib, Gtk
clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
if sys.argv[1] == "get":
    text = clipboard.wait_for_text()
    sys.stdout.write("" if text is None else text)
else:
    clipboard.set_text(sys.argv[2], -1)
    GLib.timeout_add_seconds(int(sys.argv[3]), Gtk.main_quit)
    Gtk.main()
"""


def clipboard_command(*arguments):
    home = RUN_DIR / "clipboard-home"
    home.mkdir(exist_ok=True)
    return ["env", "-i", "PATH=/usr/local/bin:/usr/bin:/bin", f"HOME={home}", "GDK_BACKEND=x11", f"DISPLAY={DISP}",
            "python3", "-I", "-c", GTK_CLIPBOARD, *arguments]


def clipboard_get():
    done = subprocess.run(clipboard_command("get"), capture_output=True, text=True, timeout=20)
    return done.stdout


class Owner:
    """Another application owning the clipboard with a sentinel, until released."""

    def __init__(self, text, seconds=60):
        self.text = text
        self.process = subprocess.Popen(clipboard_command("own", text, str(seconds)))
        for _ in range(25):
            if clipboard_get() == text:
                return
            time.sleep(0.2)
        self.release()
        raise Fail(f"the harness could not own the clipboard with {text!r}")

    def release(self):
        self.process.kill()
        self.process.wait()


def find_window(pid):
    listing = subprocess.run(["xwininfo", "-display", DISP, "-root", "-children"], capture_output=True, text=True,
                             timeout=15).stdout
    best = None
    for line in listing.splitlines():
        found = re.match(r"\s+(0x[0-9a-fA-F]+).*?\s(\d+)x(\d+)\+", line)
        if not found:
            continue
        window, width, height = found.group(1), int(found.group(2)), int(found.group(3))
        prop = subprocess.run(["xprop", "-display", DISP, "-id", window, "_NET_WM_PID"], capture_output=True,
                              text=True, timeout=15).stdout
        owner = re.search(r"=\s*(\d+)\s*$", prop)
        if owner and int(owner.group(1)) == pid and (best is None or width * height > best[0]):
            best = (width * height, window)
    return best[1] if best else None


FRAMES = []


def shoot(window, name):
    """A PID-matched window capture, kept under frames/; returns its path."""
    destination = OUT / "frames" / f"{name}.png"
    destination.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["import", "-display", DISP, "-window", window, str(destination)], check=True, timeout=30)
    colours = int(subprocess.run(["identify", "-format", "%k", str(destination)], capture_output=True, text=True,
                                 check=True).stdout)
    need(colours >= 200, name, f"the capture is blank ({colours} colours)")
    FRAMES.append({"file": f"frames/{name}.png", "sha256": hashlib.sha256(destination.read_bytes()).hexdigest()})
    return destination


def largest_square(frame):
    """The side of the largest square of the picture's colour in a capture (a 2% fuzz)."""
    width, height = (int(v) for v in subprocess.run(["identify", "-format", "%w %h", str(frame)],
                                                     capture_output=True, text=True, check=True).stdout.split())
    rgb = "rgb({},{},{})".format(*MAGENTA)
    data = subprocess.run(["magick", str(frame), "-fuzz", "2%", "-fill", "rgb(1,2,3)", "-opaque", rgb, "-fuzz", "0",
                           "-fill", "black", "+opaque", "rgb(1,2,3)", "-depth", "8", "gray:-"],
                          capture_output=True, check=True).stdout
    need(len(data) == width * height, "picture-width", f"the mask is {len(data)} bytes, not {width * height}")
    best = 0
    previous = [0] * (width + 1)
    for y in range(height):
        row = data[y * width:(y + 1) * width]
        current = [0] * (width + 1)
        for column in range(width):
            if row[column]:
                side = min(previous[column], previous[column + 1], current[column]) + 1
                current[column + 1] = side
                best = max(best, side)
        previous = current
    return best


def until(what, probe, timeout=15.0, every=0.3):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        last = probe()
        if last:
            return last
        time.sleep(every)
    raise Fail(f"timed out waiting for {what}")


# ---------------------------------------------------------------- the Preview's state


CALLS = []


def ctl(*arguments, json_out=True, check=True):
    CALLS.append(" ".join(arguments))
    done = subprocess.run([CTL, *arguments, *(["--json"] if json_out else [])], capture_output=True, text=True,
                          timeout=60)
    if check and done.returncode != 0:
        raise Fail(f"sirioctl {' '.join(arguments)} failed: {done.stdout}{done.stderr}")
    if not json_out:
        return done
    return json.loads(done.stdout)[0] if done.stdout.strip() else {}


def state():
    return ctl("surface", "editor", "state")


def pairs(text):
    return dict(part.split("=", 1) for part in text.split(";") if "=" in part)


def scroll_of(s):
    return {key: float(value) for key, value in pairs(s["preview_scroll"]).items()}


def rows_of(s, key):
    return json.loads(s.get(key) or "[]")


def drawn_texts(s):
    return [{"x": float(r["x"]), "y": float(r["y"]), "w": float(r["w"]), "h": float(r["h"]), "text": r["text"]}
            for r in rows_of(s, "preview_texts")]


def drawn_links(s):
    return [{"label": r["label"], "url": r["url"], "x": float(r["x"]), "y": float(r["y"]), "w": float(r["w"]),
             "h": float(r["h"])} for r in rows_of(s, "preview_links")]


def settled():
    """The state once the page has stopped moving."""
    last = None
    for _ in range(40):
        s = state()
        position = (scroll_of(s)["y"], len(s.get("preview_texts", "")))
        if position == last:
            return s
        last = position
        time.sleep(0.35)
    raise Fail("the page never settled")


def heading_top(s, title, which):
    """How far the `which`-th heading drawn with `title` sits below the page's top edge, in pixels.
    A heading is the first of its title in the document; a `[[toc]]` entry and a link repeat the
    title after it, so the headings are the first occurrences, in the order they were drawn."""
    found = [t for t in drawn_texts(s) if t["text"] == title]
    need(len(found) > which, "anchor", f"the Preview drew {len(found)} texts titled {title!r}, wanted {which + 1}")
    return found[which]["y"] - scroll_of(s)["top"]


def link_where(s, url, label):
    found = [link for link in drawn_links(s) if link["url"] == url and link["label"] == label]
    need(len(found) == 1, "anchor", f"{len(found)} links {label!r} to {url} drawn, wanted 1")
    return found[0]


PAGE_X, PAGE_Y = 880, 500


def to_top(window):
    wheel(window, PAGE_X, PAGE_Y, "4", 400)
    return settled()


def bring_into_view(window, first, last):
    """Scrolls until the rows starting with `first` and `last` are both inside the page's view."""
    for _ in range(40):
        s = settled()
        v = scroll_of(s)
        rows = drawn_texts(s)
        a = next((t for t in rows if t["text"].startswith(first)), None)
        b = next((t for t in rows if t["text"].startswith(last)), None)
        need(a and b, "scroll", f"no text starts with {first!r} and {last!r}")
        if a["y"] >= v["top"] + 10 and b["y"] + b["h"] <= v["top"] + v["height"] - 10:
            return s
        wheel(window, PAGE_X, PAGE_Y, "4" if a["y"] < v["top"] + 10 else "5", 3)
    raise Fail(f"scroll: {first!r} and {last!r} never fit the view together")


def to_bottom(window):
    wheel(window, PAGE_X, PAGE_Y, "5", 600)
    return settled()


def click_link(window, link):
    press(window, link["x"] + min(6.0, link["w"] / 2), link["y"] + link["h"] / 2)


def top_of_page(name, before, after, jump):
    """The heading reached the top: it sits within a heading's own padding of the page's top edge,
    and it was at least `jump` pixels away before."""
    need(abs(before - after) >= jump, name,
         f"the heading was {before:.0f} px below the top before and {after:.0f} after: it did not jump to the top")
    need(0 <= after <= 40, name, f"the heading is {after:.0f} px below the page's top edge, not at the top (0..40)")


# ---------------------------------------------------------------- leg 1


def deepest_nesting_source():
    """The deepest document the app draws as written: `MAX_NESTING - 1` quotes around one line, which
    the guard counts as MAX_NESTING levels (each quote, and the paragraph in the last)."""
    global MAX_NESTING
    found = re.search(r"const MAX_NESTING: usize = (\d+);", (Path(__file__).resolve().parents[2] / "rust/crates/sirio_ui/src/markdown/mod.rs").read_text())
    need(found, "preview-deep", "MAX_NESTING is not found in markdown/mod.rs")
    MAX_NESTING = int(found.group(1))
    return "> " * (MAX_NESTING - 1) + "deep-end\n"


def preview():
    window = E["WIN"]
    mutate = E.get("MUTATE", "")
    s = settled()
    notes = {}

    # draws: what the tree holds.
    counts = {key: int(value) for key, value in (part.split("=") for part in s["preview_nodes"].split())}
    log(f"nodes: {counts}; headings {s['preview_headings']}")
    need(counts["math"] >= 2, "draws", f"math nodes {counts['math']}, wanted at least 2")
    need(counts["alerts"] == 5, "draws", f"alerts {counts['alerts']}, wanted 5")
    need(counts["footnotes"] >= 1, "draws", f"footnotes {counts['footnotes']}, wanted at least 1")
    need(counts["images"] >= 2, "draws", f"pictures {counts['images']}, wanted at least 2")
    need(counts["tables"] == 1, "draws", f"tables {counts['tables']}, wanted 1")
    need(counts["code"] >= 1, "draws", f"code blocks {counts['code']}, wanted at least 1")
    need(counts["toc"] == 1, "draws", f"tables of contents {counts['toc']}, wanted 1")
    need(s["preview_headings"] == "4", "draws", f"headings {s['preview_headings']}, wanted 4")
    top = to_top(window)
    need(scroll_of(top)["max"] > 1000, "draws", f"the page scrolls {scroll_of(top)['max']:.0f} px, wanted more than 1000")
    shoot(window, "preview-top")
    record("draws", f"nodes {counts}, 4 headings, the page scrolls {scroll_of(top)['max']:.0f} px")

    # anchors. The headings are, in order: the title, "Second section" twice, "Late section".
    # anchor-toc: the toc entry for the second "Second section".
    s = top
    before = heading_top(s, "Second section", 1)
    link = link_where(s, "#second-section-1", "Second section")
    click_link(window, link)
    try:
        s = until("the page to scroll after the toc click", lambda: (lambda v: v if scroll_of(v)["y"] > 5 else None)(state()),
                  timeout=8)
    except Fail:
        raise Fail("anchor-toc: a click on the toc entry for the second 'Second section' did not scroll the page")
    s = settled()
    after = heading_top(s, "Second section", 1)
    top_of_page("anchor-toc", before, after, 60)
    need(heading_top(s, "Second section", 0) < 0, "anchor-toc", "the first 'Second section' is not above the top: the jump went to it, not to second-section-1")
    shoot(window, "anchor-toc")
    record("anchor-toc", f"the second 'Second section' went from {before:.0f} to {after:.0f} px below the top; the first is above it")

    # anchor-slug: the in-text link to #late-section.
    s = to_top(window)
    link = link_where(s, "#late-section", "jump to the late section")
    before = heading_top(s, "Late section", 1)
    click_link(window, link)
    try:
        until("the page to scroll after the slug click", lambda: (lambda v: v if scroll_of(v)["y"] > 1000 else None)(state()),
              timeout=8)
    except Fail:
        raise Fail("anchor-slug: a click on the #late-section link did not scroll the page")
    s = settled()
    after = heading_top(s, "Late section", 1)
    top_of_page("anchor-slug", before, after, 500)
    shoot(window, "anchor-slug")
    record("anchor-slug", f"'Late section' went from {before:.0f} to {after:.0f} px below the top")

    # anchor-outline: open the panel, then click the row of the second "Second section" from the bottom.
    s = to_bottom(window)
    need(s["preview_outline"] == "closed", "anchor-outline", f"the outline starts {s['preview_outline']!r}, wanted closed")
    toggle = [float(v) for v in s["preview_outline_toggle_at"].split(",")]
    press(window, toggle[0] + toggle[2] / 2, toggle[1] + toggle[3] / 2)
    s = until("the outline to open", lambda: (lambda v: v if v["preview_outline"] == "open" and v["preview_outline_at"] else None)(state()))
    s = settled()
    bottom_y = scroll_of(s)["y"]
    ox, oy, ow, oh = [float(v) for v in s["preview_outline_at"].split(",")]
    rows = int(s["preview_headings"])
    row = (oh - 2 * (rows - 1)) / rows
    row_two = (ox + min(60.0, ow / 2), oy + (row + 2) * 2 + row / 2)
    shoot(window, "anchor-outline-open")
    before = heading_top(s, "Second section", 1)
    press(window, *row_two)
    try:
        until("the page to scroll up after the outline click", lambda: (lambda v: v if scroll_of(v)["y"] < bottom_y - 300 else None)(state()),
              timeout=8)
    except Fail:
        raise Fail("anchor-outline: a click on the outline's row for the second 'Second section' did not scroll the page")
    s = settled()
    after = heading_top(s, "Second section", 1)
    top_of_page("anchor-outline", before, after, 500)
    shoot(window, "anchor-outline-clicked")
    record("anchor-outline", f"from the bottom the second 'Second section' went from {before:.0f} to {after:.0f} px below the top")
    press(window, toggle[0] + toggle[2] / 2, toggle[1] + toggle[3] / 2)  # the outline closes again

    # progress.
    s = to_top(window)
    p0 = float(s["preview_progress"]) + 0.0
    p0 = abs(p0)
    wheel(window, PAGE_X, PAGE_Y, "5", 12)
    p1 = float(settled()["preview_progress"])
    wheel(window, PAGE_X, PAGE_Y, "5", 12)
    p2 = float(settled()["preview_progress"])
    need(p0 < p1 < p2, "progress", f"the reading share did not grow with the scroll: {p0} {p1} {p2}")
    s = to_bottom(window)
    p3 = float(s["preview_progress"])
    need(p3 >= 0.99, "progress", f"at the bottom the share reads {p3}")
    record("progress", f"the share grew {p0:.2f} -> {p1:.2f} -> {p2:.2f} -> {p3:.2f}")

    # picture-width: scroll down the page in steps; the 120 px square shows in some frame.
    s = to_top(window)
    best, sides = 0, []
    for step in range(60):
        frame = shoot(window, f"scan-{step}")
        side = largest_square(frame)
        sides.append(side)
        best = max(best, side)
        if 114 <= side <= 122:
            break
        wheel(window, PAGE_X, PAGE_Y, "5", 8)
    # Only the frame that shows the picture is kept as evidence.
    keep = len(sides) - 1 if 114 <= sides[-1] <= 122 else None
    for step in range(len(sides)):
        if step != keep:
            (OUT / "frames" / f"scan-{step}.png").unlink(missing_ok=True)
    FRAMES[:] = [f for f in FRAMES if not re.match(r"frames/scan-(\d+)\.png$", f["file"])
                 or int(re.match(r"frames/scan-(\d+)\.png$", f["file"]).group(1)) == keep]
    need(114 <= best <= 122, "picture-width", f"the largest picture-coloured square in {len(sides)} frames is {best} px, wanted about 120")
    notes["picture_square"] = best
    record("picture-width", f"<img width=120> draws as a {best} px square (a natural 64 px picture scans as 61)")

    # diagrams.
    s = settled()
    ready, failed, pending, paths = (s["preview_diagrams"].split(";", 3) + ["", "", "", ""])[:4]
    ready, failed, pending = (int(v.split("=")[1]) for v in (ready, failed, pending))
    listed = [p for p in s["preview_diagrams"].split("paths=", 1)[1].replace("\\\\", "\\").split("|") if p]
    cache = E["XDG_CACHE"]
    need(ready >= 1 and listed, "diagrams", f"no diagram is ready: {s['preview_diagrams']}")
    for path in listed:
        need(path.startswith(cache + os.sep) and os.path.getsize(path) > 0, "diagrams", f"{path} is not a file under the cache directory {cache}")
    if shutil.which("plantuml"):
        need(ready >= 2, "diagrams", f"plantuml is on PATH but only {ready} diagram(s) are ready: {s['preview_diagrams']}")
        record("diagrams", f"{ready} diagrams rendered into the cache directory: {listed}")
    else:
        record("diagrams", f"Mermaid rendered into the cache directory ({listed}); {failed} diagram(s) failed (PlantUML)")
        log("SKIP plantuml (not on PATH)")
        record("plantuml", "SKIP plantuml (not on PATH)", status="SKIP")

    # copy-drag: a real drag across rows, then Ctrl+C. The first drag crosses a paragraph holding
    # emoji and inline math, display math and two alerts; the second a footnote reference and its
    # definition. Both are in the one clipboard text the check pins.
    gots = []
    for name, first, last in (("math", "Jump to the", "A tip alert."), ("footnote", "A footnote reference", "The footnote definition.")):
        s = bring_into_view(window, first, last)
        rows = drawn_texts(s)
        start = next((t for t in rows if t["text"].startswith(first)), None)
        end = next((t for t in rows if t["text"].startswith(last)), None)
        need(start and end, "copy-drag", f"the rows {first!r} and {last!r} were not drawn")
        sentinel = Owner("sentinel before the drag")
        try:
            drag(window, start["x"] + 2, start["y"] + start["h"] / 2, end["x"] + end["w"] - 2, end["y"] + end["h"] / 2)
            shoot(window, f"copy-drag-{name}")
            key("ctrl+c")
            got = until("the clipboard to change after Ctrl+C", lambda: (lambda c: c if c != "sentinel before the drag" else None)(clipboard_get()), timeout=6)
        finally:
            sentinel.release()
        (OUT / f"clipboard-drag-{name}.txt").write_text(got)
        log(f"clipboard after the {name} drag:\n{got}\n----")
        gots.append(got)
    got = "\n".join(gots)
    for needle, why in COPY_EXPECTED:
        need(needle in got, "copy-drag", f"the clipboard lacks {needle!r} ({why}); it holds {got!r}")
    for banned in COPY_BANNED:
        need(banned not in got, "copy-drag", f"the clipboard holds {banned!r}: {got!r}")
    record("copy-drag", f"{len(got)} characters on the clipboard, with " + ", ".join(why for _, why in COPY_EXPECTED))
    s = bring_into_view(window, "A footnote reference", "The footnote definition.")
    start = next(t for t in drawn_texts(s) if t["text"].startswith("A footnote reference"))

    # copy-menu: the right press on the selection, near the top of the page so the menu is not
    # moved to fit, then the menu's Copy.
    s = settled()
    for _ in range(20):
        start = next(t for t in drawn_texts(s) if t["text"].startswith("A footnote reference"))
        if start["y"] - scroll_of(s)["top"] < 150:
            break
        wheel(window, PAGE_X, PAGE_Y, "5", 1)
        s = settled()
    sentinel = Owner("sentinel before the menu")
    try:
        at = (start["x"] + 40, start["y"] + start["h"] / 2)
        press(window, *at, button="3", hold=0.1)
        time.sleep(0.6)
        shoot(window, "copy-menu-open")
        press(window, at[0] + MENU_COPY[0], at[1] + MENU_COPY[1])
        menu_got = until("the menu's Copy to reach the clipboard", lambda: (lambda c: c if c != "sentinel before the menu" else None)(clipboard_get()), timeout=6)
    finally:
        sentinel.release()
    (OUT / "clipboard-menu.txt").write_text(menu_got)
    need(menu_got == gots[-1], "copy-menu", f"the menu's Copy put {menu_got!r} on the clipboard, Ctrl+C put {gots[-1]!r}")
    record("copy-menu", "the right-click menu's Copy put the same text on the clipboard as Ctrl+C")

    # terminal-ctrl-c: the project's terminal tab shares the window, and the Markdown selection
    # from the menu's Copy above is still made. A click in the terminal gives it the keys; Ctrl+C
    # there is the interrupt: the PTY echoes ^C, and the clipboard is as another application left it.
    panels = [line.split("\t") for line in ctl("panel", "list", json_out=False).stdout.splitlines()]
    terminal = next((row[0] for row in panels if len(row) > 1 and row[1] == "Terminal"), None)
    need(terminal, "terminal-ctrl-c", f"no Terminal panel in {panels}")

    def screen():
        data = ctl("panel", "read", terminal).get("data", "")
        return base64.b64decode(data).decode("utf-8", "replace")

    need("^C" not in screen(), "terminal-ctrl-c", "the terminal already shows ^C before the keypress")
    sentinel = Owner("sentinel before the terminal")
    try:
        row = next(t for t in drawn_texts(settled()) if t["text"].startswith("A footnote reference"))
        area = f"{int(row['w'])}x{int(row['h'])}+{int(row['x'])}+{int(row['y'])}"

        def crop(frame):
            return subprocess.run(["magick", str(frame), "-crop", area, "+repage", "-depth", "8", "rgb:-"],
                                  capture_output=True, check=True).stdout

        # The baseline: a press on the page's blank margin, beside the row, clears the selection; the
        # row's pixels then are the unselected ones. Selecting the row again must change them (the
        # highlight), or the comparison below would pass with no selection at all.
        press(window, row["x"] - 10, row["y"] + row["h"] / 2)
        time.sleep(0.5)
        plain = crop(shoot(window, "terminal-selection-none"))
        drag(window, row["x"] + 2, row["y"] + row["h"] / 2, row["x"] + row["w"] - 2, row["y"] + row["h"] / 2)
        selected = shoot(window, "terminal-selection-before-click")
        need(crop(selected) != plain, "terminal-ctrl-c", f"selecting the row ({area}) left its pixels as they were: the baseline has no highlight")
        # A real click into the terminal gives it the keys. The click clears the Markdown selection
        # (one selection at a time): the row is plain again, the pixels of the baseline. (With the
        # selection kept alive and the focus moved by the pane-focus verb instead, Ctrl+C copied the
        # selection and never reached the PTY: recorded as a finding in the task report.)
        press(window, 500, 400)
        time.sleep(0.5)
        clicked = shoot(window, "terminal-selection-after-click")
        need(crop(clicked) == plain, "terminal-ctrl-c", f"after the click into the terminal the row ({area}) is neither selected nor plain")
        key("ctrl+c")
        try:
            shown = until("the PTY to echo ^C", lambda: "^C" in screen() and screen(), timeout=8)
        except Fail:
            shoot(window, "terminal-no-echo")
            raise Fail(f"terminal-ctrl-c: the PTY never echoed ^C; the clipboard holds {clipboard_get()!r}, the screen {screen()!r}")
        after = clipboard_get()
    finally:
        sentinel.release()
    need(after == "sentinel before the terminal", "terminal-ctrl-c", f"Ctrl+C in the terminal changed the clipboard to {after!r}")
    shoot(window, "terminal-after-ctrl-c")
    (OUT / "terminal-screen.txt").write_text(shown)
    record("terminal-ctrl-c", "Ctrl+C in the terminal pane after a Markdown selection echoed ^C at the PTY and left the clipboard alone (the click that focused the terminal had cleared the Markdown selection, which the check asserts)")

    # preview-deep: the deepest nesting the app draws, opened in a Preview, draws and the app lives.
    deep = deepest_nesting_source()
    path = Path(E["WT"]) / "deep.md"
    path.write_text(deep)
    ctl("surface", "editor", "open", f"--path={path}")
    try:
        s = until("the deep document to draw in the Preview",
                  lambda: (lambda v: v if any(r["text"] == "deep-end" for r in drawn_texts(v)) else None)(state()), timeout=30)
    except Fail:
        # The plain-paragraph fallback draws one row, "> > ... deep-end": not the quotes.
        rows = [r["text"][:60] for r in drawn_texts(state())]
        raise Fail(f"preview-deep: no drawn row is exactly 'deep-end', so the {MAX_NESTING} nested levels were not drawn as written (rows: {rows})")
    os.kill(int(E["APP_PID"]), 0)  # raises if the process is gone
    record("preview-deep", f"{MAX_NESTING} nested levels (as deep as the guard allows) opened in the Preview and drew as quotes, a row of exactly 'deep-end', and the process lives")


# What the two drags must put on the clipboard. The paragraph holding emoji and inline math is a
# silent part (Ely draws its TeX without pushing it): it is copied as its words, the emoji turned
# and the TeX as written; so are display math, an alert's title and a footnote's label.
COPY_EXPECTED = [
    ("emoji 🎉 🚀.", "the emoji turned"),
    ("Inline math e^{i\\pi}+1=0 sits", "inline math as TeX"),
    ("\\int_0^1 x\\,dx", "display math as TeX"),
    ("Note\nA note alert.", "the alert's title"),
    ("Tip\nA tip alert.", "the second alert's title"),
    ("A footnote reference\u00b9 and a link around one: see\u00b9.", "the footnote's marker"),
    ("[1]\nThe footnote definition.", "the footnote's label"),
]
COPY_BANNED = [":tada:", "~~", "[^1]", "[!NOTE]", "**"]
# The first row of the menu, from the press that opened it (the menu's corner is at the press while
# the menu fits under it): its Copy is 35 px right of the corner and 21 px below.
MENU_COPY = (35, 21)


# ---------------------------------------------------------------- leg 2


def long_reply(showcase, parts):
    """`parts` identical copies of the showcase, each opened by a marker line with a link
    ('Marker N: link-N') and the last closed by 'End: end-link'. The copies are the same bytes, so what
    a copy holds between two markers must be the same for every pair, whatever the chat drew to make
    the copy; the chat reports only the links of blocks it drew, which are those near the window."""
    blocks = [f"Marker {n}: [link-{n}](https://example.com/marker-{n})\n\n" + showcase + "\n\n"
              for n in range(1, parts + 1)]
    return "".join(blocks) + "End: [end-link](https://example.com/end)\n"


def band_ink(frame, left, right, top, bottom):
    """The colours in each 100 px band of a capture's column: a band with text has many, a blank one
    has the background's one or two."""
    counts = []
    for y in range(top, bottom - 99, 100):
        counts.append(int(subprocess.run(["magick", str(frame), "-crop", f"{right - left}x100+{left}+{y}", "+repage",
                                         "-format", "%k", "info:"], capture_output=True, text=True, check=True).stdout))
    return counts


def row_gaps(result):
    """The vertical gaps in what the chat's last paint drew inside the transcript's viewport: the viewport
    runs from the column's top to the composer band's top; the drawn texts, in window pixels, are sorted by
    their top and merged; a gap is a stretch of the viewport with no drawn text. Returns (gaps, viewport),
    each gap as (top, height)."""
    composer = json.loads(result["composerRow"])
    top, bottom = composer["root"][1], composer["band"][1]
    boxes = sorted((float(r["y"]), float(r["y"]) + float(r["h"])) for r in json.loads(result["transcriptTextRows"] or "[]"))
    gaps, reach = [], top
    for start, end in boxes:
        if end <= top or start >= bottom:
            continue
        if start > reach:
            gaps.append((reach, start - reach))
        reach = max(reach, end)
    if reach < bottom:
        gaps.append((reach, bottom - reach))
    return gaps, (top, bottom)


# The largest stretch of the transcript's viewport that may hold no drawn text. Ely draws a paragraph
# that holds math, display math and a picture without a text row, so the showcase leaves gaps: measured
# 143 px under the viewport's top (above the reply), 153 px (a display-math paragraph with its margins),
# 120 px (the footer under the reply's end), 62-64 px (the 64 px pictures). 160 px clears them all; a
# stand-in for a table, a code block or a long paragraph left in the window is a gap at least that tall.
# A shorter one is caught by the comparison with the same view redrawn by a one-click scroll away and back.
MAX_GAP = 160


def row_boxes(result):
    """The boxes of the drawn texts that overlap the transcript's viewport, as sorted (top, height)
    pairs. The chat also draws blocks a margin beyond the viewport; which of those exist may differ
    between two passes without anything being wrong."""
    composer = json.loads(result["composerRow"])
    top, bottom = composer["root"][1], composer["band"][1]
    boxes = [(float(r["y"]), float(r["h"])) for r in json.loads(result["transcriptTextRows"] or "[]")]
    return sorted(b for b in boxes if b[0] < bottom and b[0] + b[1] > top)


def same_boxes(a, b, tolerance=2.0):
    return len(a) == len(b) and all(abs(p[0] - q[0]) <= tolerance and abs(p[1] - q[1]) <= tolerance for p, q in zip(a, b))


def chat():
    import argparse
    root = Path(E["ROOT"])
    spec = importlib.util.spec_from_file_location("chatui", root / "Scripts/Tests/test-ely-chat-ui-e2e.py")
    chatui = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(chatui)
    fixture = root / "Scripts/Tests/fixtures/markdown-documents"
    showcase = (fixture / "showcase.md").read_text().replace("{{REMOTE_PORT}}", E["PORT"])
    reply = long_reply(showcase, 9)
    need(len(reply.encode()) >= 40 * 1024, "chat-select-all", f"the reply is {len(reply.encode())} bytes, wanted at least 40 KiB")
    args = argparse.Namespace(out_dir=str(OUT / "logs" / "chat-run"), state_only=False, display=DISP,
                              window_size=E["WINDOW_SIZE"], sirio_bin=E["SIRIO"])
    run = chatui.Run(args)
    failed = None
    try:
        run.preflight()
        scenario = chatui.Scenario(run, "long-reply", "reply-file")
        (scenario.agent_dir / "reply.md").write_text(reply)
        shutil.copy(fixture / "pixel.png", scenario.repo / "pixel.png")
        chat_app = scenario.instance
        chat_app.start()
        chat_app.send("show the long reply")
        chat_app.until("the turn completes with the long reply",
                       lambda st: st["status"] == "completed" and "Marker 9:" in "".join(chatui.texts(st, "assistant")),
                       timeout=90)
        time.sleep(3)
        pid = chat_app.process.pid
        window = run.window_for(pid)

        def read():
            result = chat_app.call("surface.chat.read", surfaceId=chat_app.surface)["result"]
            return result, json.loads(result["transcriptLinks"] or "[]")

        def labels():
            return {row["label"] for row in read()[1]}

        def tail():
            chatui.scroll_pane(run, pid, 600, "5")
            time.sleep(1.5)

        width, height = (int(v) for v in E["WINDOW_SIZE"].split("x"))

        def select_all_copy(tag):
            """A right press on plain text, away from every link; the menu's Select All, then its Copy.
            The menu's rows, from the press that opened it: Copy 40 px right and 18 below, Select All
            46 below. Returns the clipboard text."""
            _, links = read()
            point = None
            for fx in (0.33, 0.4, 0.28, 0.45):
                for fy in (0.35, 0.5, 0.25, 0.6):
                    px, py = int(width * fx), int(height * fy)
                    if all(not (float(l["x"]) - 30 <= px <= float(l["x"]) + float(l["w"]) + 30 and abs(float(l["y"]) - py) < 30) for l in links):
                        point = (px, py)
                        break
                if point:
                    break
            need(point, "chat-select-all", "no plain-text point in the transcript to press on")
            sentinel = Owner(f"sentinel before the {tag} copy")
            try:
                press(window, *point, button="3", hold=0.1)
                time.sleep(0.8)
                press(window, point[0] + 40, point[1] + 46)  # Select All
                time.sleep(1.0)
                shoot_chat(run, pid, f"chat-select-all-{tag}")
                need(clipboard_get() == f"sentinel before the {tag} copy", "chat-select-all", "Select All alone changed the clipboard")
                press(window, *point, button="3", hold=0.1)
                time.sleep(0.8)
                press(window, point[0] + 40, point[1] + 18)  # Copy
                return until("the menu's Copy to reach the clipboard",
                             lambda: (lambda c: c if c != f"sentinel before the {tag} copy" else None)(clipboard_get()), timeout=10)
            finally:
                sentinel.release()

        def first_difference(a, b):
            return next((i for i, (x_, y_) in enumerate(zip(a, b)) if x_ != y_), min(len(a), len(b)))

        def jump_view(tag, forth, back):
            """What a jump left in the window, judged three ways. (1) The drawn texts leave no gap taller
            than MAX_GAP in the transcript's viewport. (2) They are the texts of the same view redrawn by
            a one-click scroll `forth` and `back` (a stand-in the jump left in the window is replaced by
            the next frame). (3) The capture is not blank: a band of 100 colours or fewer is excused only
            where a drawn text overlaps it."""
            result, _ = read()
            first = row_boxes(result)
            gaps, view = row_gaps(result)
            log(f"gaps at {tag} (viewport {view}): {[(round(t), round(h)) for t, h in gaps]}")
            worst = max(gaps, key=lambda g: g[1], default=(0, 0))
            need(worst[1] <= MAX_GAP, "chat-jump", f"{tag}: nothing is drawn in a {worst[1]:.0f} px stretch at {worst[0]:.0f} (viewport {view}, limit {MAX_GAP}): {gaps}")
            frame = shoot_chat(run, pid, f"chat-long-{tag}")
            wheel(window, 528, 350, forth, 1)
            wheel(window, 528, 350, back, 1)
            time.sleep(1.0)
            again = row_boxes(read()[0])
            need(same_boxes(first, again), "chat-jump",
                 f"{tag}: the window held {len(first)} drawn texts after the jump and {len(again)} once redrawn by a scroll away and back: the jump left stand-ins in the window")
            ink = band_ink(frame, 330, 1340, 80, 840)
            log(f"colours per 100 px band of {frame.name}: {ink}")
            for i, colours in enumerate(ink):
                top_y = 80 + 100 * i
                drawn = any(y < top_y + 100 and y + h > top_y for y, h in first)
                need(colours >= 100 or drawn, "chat-jump", f"{tag}: the band at y={top_y} is blank: {colours} colours and no drawn text over it")
            return len(first)

        # Run 1: following the tail, so the early blocks are culled.
        tail()
        at_tail = labels()
        log(f"links drawn at the tail: {sorted(at_tail)}")
        need("end-link" in at_tail, "chat-select-all", f"the tail's own end-link is not drawn: {sorted(at_tail)}")
        need("link-1" not in at_tail, "chat-select-all", f"an early block's link is drawn at the tail: {sorted(at_tail)}")
        shoot_chat(run, pid, "chat-long-tail")
        copied_tail = select_all_copy("tail")
        (OUT / "clipboard-chat-tail.txt").write_text(copied_tail)

        # Run 2: after a jump to the top, so the late blocks are culled instead.
        wheel(window, 528, 350, "4", 2500)
        time.sleep(1.5)
        at_top = labels()
        need("link-1" in at_top and "end-link" not in at_top, "chat-jump",
             f"after a jump to the top the links drawn are {sorted(at_top)}: want link-1 and not end-link")
        jump_view("top", "5", "4")
        copied_top = select_all_copy("top")
        (OUT / "clipboard-chat-top.txt").write_text(copied_top)

        # The copies against each other, against the app's own text, and against the source.
        result, _ = read()
        expected = result["transcriptText"]
        (OUT / "expected-chat-transcript.txt").write_text(expected)
        need(len(copied_tail) > 20000, "chat-select-all", f"the copy is only {len(copied_tail)} characters")
        need(copied_tail == copied_top, "chat-select-all",
             f"the copy made at the tail ({len(copied_tail)} characters) differs from the one made at the top ({len(copied_top)}), "
             f"first at {first_difference(copied_tail, copied_top)}: what the copy holds depends on what was drawn")
        need(copied_tail == expected, "chat-select-all",
             f"the clipboard ({len(copied_tail)} characters) is not the transcript text ({len(expected)}), first at {first_difference(copied_tail, expected)}")
        # Every segment between two markers is the same bytes: the source repeated one document.
        lines = [f"Marker {n}: link-{n}" for n in range(1, 10)] + ["End: end-link"]
        starts = []
        at = 0
        for line in lines:
            found = copied_tail.find(line, at)
            need(found >= 0, "chat-select-all", f"the copy lacks {line!r} (after offset {at})")
            starts.append(found)
            at = found + len(line)
        segments = [copied_tail[starts[i] + len(lines[i]):starts[i + 1]] for i in range(9)]
        for n, segment in enumerate(segments, 1):
            need(segment == segments[0], "chat-select-all",
                 f"the copy between 'Marker {n}' and the next marker ({len(segment)} characters) is not the first one's ({len(segments[0])}); "
                 f"first difference at {first_difference(segment, segments[0])}")
        need(len(segments[0]) > 2000, "chat-select-all", f"a segment holds only {len(segments[0])} characters")
        record("chat-select-all", f"Select All then Copy of a {len(reply.encode()) // 1024} KiB reply made at the tail and again at the top "
                                  f"are byte-equal ({len(copied_tail)} characters), equal to the transcript text, and hold nine identical "
                                  f"{len(segments[0])}-character segments between the markers (tail drew {sorted(at_tail)}, top drew {sorted(at_top)})")

        wheel(window, 528, 350, "5", 30)
        time.sleep(1.5)
        mid = jump_view("middle", "4", "5")
        wheel(window, 528, 350, "5", 2500)
        time.sleep(1.5)
        end = jump_view("back-at-tail", "4", "5")
        record("chat-jump", f"after a jump to the top link-1 is drawn and end-link is not; at the top, after a short wheel and at the tail no stretch over {MAX_GAP} px of the viewport lacks drawn text, the window holds the same texts once redrawn by a scroll away and back, and no capture band is blank")
        # chat-deep: the deepest nesting as a reply: the turn completes, the body draws, the app lives.
        (scenario.agent_dir / "reply.md").write_text(deepest_nesting_source())
        chat_app.send("show the deep reply")
        chat_app.until("the deep reply to complete",
                       lambda st: st["status"] == "completed" and any("deep-end" in t for t in chatui.texts(st, "assistant")),
                       timeout=90)
        time.sleep(2)
        need(chat_app.process.poll() is None, "chat-deep", "the app exited on the deepest nesting")
        # What the chat drew: the quotes' paragraph is a row of exactly 'deep-end'. The plain-paragraph
        # fallback is one row, '> > ... deep-end', and the reply text matches either.
        def deep_rows():
            result = chat_app.call("surface.chat.read", surfaceId=chat_app.surface)["result"]
            return [r["text"] for r in json.loads(result["transcriptTextRows"] or "[]")]
        try:
            until("the deep reply to draw as quotes", lambda: "deep-end" in deep_rows() or None, timeout=20)
        except Fail:
            raise Fail(f"chat-deep: no drawn row is exactly 'deep-end', so the {MAX_NESTING} nested levels were not drawn as written (rows: {[t[:60] for t in deep_rows()]})")
        record("chat-deep", f"{MAX_NESTING} nested levels as a chat reply: the turn completed, the chat drew them as quotes (a row of exactly 'deep-end'), and the process lives")
        scenario.finish()
    except BaseException as error:
        failed = error
    finally:
        for instance in run.instances:
            instance.quit()
        chatui.stop_hosts(run)
    if failed is not None:
        raise failed if isinstance(failed, Fail) else Fail(f"chat-select-all: {failed!r}")


def shoot_chat(run, pid, name):
    """The chat window's capture, taken by the chat harness into its own directory and kept in frames/."""
    taken = run.out / "frames" / f"{name}.png"
    run.capture(pid, taken, settle=0.5)
    destination = OUT / "frames" / f"{name}.png"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy(taken, destination)
    FRAMES.append({"file": f"frames/{name}.png", "sha256": hashlib.sha256(destination.read_bytes()).hexdigest()})
    return destination


def write_leg(leg, extra):
    """What this leg saw, for the script's transcript.json to fold in."""
    (OUT / f".leg-{leg}.json").write_text(json.dumps({"frames": FRAMES, "sirioctl_calls": CALLS, **extra}))


def main():
    leg = sys.argv[1]
    try:
        {"preview": preview, "chat": chat}[leg]()
    except Fail as failure:
        log(f"FAIL: {failure}")
        write_leg(leg, {"failed": str(failure)})
        sys.exit(1)
    write_leg(leg, {})


if __name__ == "__main__":
    main()
