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
        press(window, 500, 400)
        time.sleep(0.5)
        shoot(window, "terminal-before-ctrl-c")
        key("ctrl+c")
        shown = until("the PTY to echo ^C", lambda: "^C" in screen() and screen(), timeout=8)
        after = clipboard_get()
    finally:
        sentinel.release()
    need(after == "sentinel before the terminal", "terminal-ctrl-c", f"Ctrl+C in the terminal changed the clipboard to {after!r}")
    shoot(window, "terminal-after-ctrl-c")
    (OUT / "terminal-screen.txt").write_text(shown)
    record("terminal-ctrl-c", "Ctrl+C in the terminal pane after the Markdown selection echoed ^C at the PTY and left the clipboard alone")


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
    """The showcase `parts` times, each part headed and marked so that a block the copy drops is
    named by the source alone: 'Marker N' opens a part and 'End N' closes it, each with a link, and the
    chat reports only the links of blocks it drew, which are those near the window."""
    blocks = []
    for n in range(1, parts + 1):
        blocks.append(f"# Part {n}\n\nMarker {n}: [link-{n}](https://example.com/marker-{n})\n\n" + showcase + f"\n\nEnd {n}: [end-link-{n}](https://example.com/end-{n})\n")
    return "\n\n".join(blocks) + "\n"


def band_ink(frame, left, right, top, bottom):
    """The colours in each 100 px band of a capture's column: a band with text has many, a blank one
    has the background's one or two."""
    counts = []
    for y in range(top, bottom - 99, 100):
        counts.append(int(subprocess.run(["magick", str(frame), "-crop", f"{right - left}x100+{left}+{y}", "+repage",
                                         "-format", "%k", "info:"], capture_output=True, text=True, check=True).stdout))
    return counts


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

        tail()
        at_tail = labels()
        log(f"links drawn at the tail: {sorted(at_tail)}")
        need("end-link-9" in at_tail, "chat-select-all", f"the tail's own end-link-9 is not drawn: {sorted(at_tail)}")
        need(not ({"link-1", "end-link-1"} & at_tail), "chat-select-all", f"an early block's link is drawn at the tail: {sorted(at_tail)}")
        shoot_chat(run, pid, "chat-long-tail")

        # A right press on plain text, away from every link: the menu is Copy and Select All.
        result, links = read()
        width, height = (int(v) for v in E["WINDOW_SIZE"].split("x"))
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
        # The menu's rows, from the press that opened it: Copy 40 px right and 18 below, Select All 46 below.
        sentinel = Owner("sentinel before the chat copy")
        try:
            press(window, *point, button="3", hold=0.1)
            time.sleep(0.8)
            press(window, point[0] + 40, point[1] + 46)  # Select All
            time.sleep(1.0)
            shoot_chat(run, pid, "chat-select-all-selected")
            need(clipboard_get() == "sentinel before the chat copy", "chat-select-all", "Select All alone changed the clipboard")
            press(window, *point, button="3", hold=0.1)
            time.sleep(0.8)
            press(window, point[0] + 40, point[1] + 18)  # Copy
            copied = until("the menu's Copy to reach the clipboard",
                           lambda: (lambda c: c if c != "sentinel before the chat copy" else None)(clipboard_get()), timeout=10)
        finally:
            sentinel.release()
        result, _ = read()
        expected = result["transcriptText"]
        (OUT / "clipboard-chat.txt").write_text(copied)
        (OUT / "expected-chat-transcript.txt").write_text(expected)
        need(len(expected) > 20000, "chat-select-all", f"the transcript text is only {len(expected)} characters")
        need(copied == expected, "chat-select-all",
             f"the clipboard ({len(copied)} characters) is not the transcript text ({len(expected)}): "
             f"first difference at {next((i for i, (a, b) in enumerate(zip(copied, expected)) if a != b), min(len(copied), len(expected)))}")
        # The source alone says nothing was dropped: every part's opening and closing line, in order.
        at = 0
        for n in range(1, 10):
            for line in (f"Marker {n}: link-{n}", f"End {n}: end-link-{n}"):
                found = copied.find(line, at)
                need(found >= 0, "chat-select-all", f"the copy lacks {line!r} (after offset {at})")
                at = found
        record("chat-select-all", f"Select All then Copy from the tail of a {len(reply.encode()) // 1024} KiB reply put all "
                                  f"{len(copied)} characters of the transcript on the clipboard (links drawn at the tail: {sorted(at_tail)})")

        # A jump to the top of the reply draws its first blocks, and nothing visible is blank.
        wheel(window, 528, 350, "4", 2500)
        time.sleep(1.5)
        at_top = labels()
        need("link-1" in at_top and "end-link-9" not in at_top, "chat-jump",
             f"after a jump to the top the links drawn are {sorted(at_top)}: want link-1 and not end-link-9")
        frames = [shoot_chat(run, pid, "chat-long-top")]
        wheel(window, 528, 350, "5", 30)
        time.sleep(1.5)
        frames.append(shoot_chat(run, pid, "chat-long-after-short-wheel"))
        wheel(window, 528, 350, "5", 2500)
        time.sleep(1.5)
        frames.append(shoot_chat(run, pid, "chat-long-back-at-tail"))
        for frame in frames:
            ink = band_ink(frame, 330, 1340, 80, 840)
            log(f"colours per 100 px band of {frame.name}: {ink}")
            need(min(ink) >= 100, "chat-jump", f"{frame.name} has a blank band: colours per band {ink}")
        record("chat-jump", f"after a jump to the top link-1 is drawn and end-link-9 is not; three captures have no blank band")
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
