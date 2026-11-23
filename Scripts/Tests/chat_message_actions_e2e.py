#!/usr/bin/env python3
"""The scenarios of test-chat-message-actions-e2e.sh (read its header first).

Everything here runs against real, isolated debug Sirio instances on the script's private Xvfb; the harness
(chat_e2e_harness.py, shared with the richer messages E2E) holds the `World` that starts one and the helpers around it.
A scenario is a function that raises `Fail` at its first failed check; the run reports it and goes on with the next one.

What the scenarios read is the debug readout of `surface.chat.read` (per row `actions`, `actionsShown`,
`actionBounds.<copy|quote|continue|row|quoteCard>`, a user row's `quote`; for the chat `continue`, `continueRow`,
`composerQuote`, `composerQuoteBounds`, `queued`, `transcriptText`) and the debug verbs of `surface.chat.transcript`
(`continue`, `copy`, `quote`, `quoteSelection`, `toggleTurn`). Every press that the user makes with a pointer is a
real X click at the pixels the readout reports.
"""
import json
import os
import sys
import time
from pathlib import Path

# `python3 -I` leaves the script's own directory off the path.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from chat_e2e_harness import (  # noqa: E402
    E, ClipboardOwner, Fail, RpcError, World, assistant_rows, brief, byte_len, clipboard_get, error_row, kill_stub,
    last_reply, main, move, plain_by_row, press, row_with_text, rows_of, scenario, teardown, user_texts, x,
)

# ---------------------------------------------------------------- what a row and the chat read as


def actions_of(row):
    """The actions a row draws, as a list (`copy`, `quote`, `continue`)."""
    return [name for name in row.get("actions", "").split(",") if name]


def box(row, name):
    """`actionBounds.<name>` of a row as `[x, y, width, height]`, None when the last paint drew none."""
    value = row.get(f"actionBounds.{name}")
    return json.loads(value) if value else None


def middle(bounds):
    return bounds[0] + bounds[2] / 2, bounds[1] + bounds[3] / 2


def continue_rows(r):
    """The indexes of the rows that draw Continue: never more than one."""
    return [index for index, row in enumerate(r["rows"]) if "continue" in actions_of(row)]


def footers(r):
    return [row["text"] for _, row in rows_of(r, "turn")]


def quote_block(text, typed=None):
    """What a Quote puts at the head of a prompt (spec 5.4): every line behind `> ` (an empty one is `>`), then the typed
    text after an empty line. Written here from the spec, not from the product's own function."""
    block = "\n".join(f"> {line}" if line else ">" for line in text.split("\n"))
    return block if not typed else f"{block}\n\n{typed}"


def idle(r):
    return r["status"] in ("idle", "completed")


# ---------------------------------------------------------------- real input on a row's buttons

PARK = (6, 6)  # a pixel of the window over no message: the pointer waits here between presses


def park(w):
    move(w.window, *PARK)
    time.sleep(0.3)


def wait_bounds(w, index, name, timeout=8):
    """The row's `actionBounds.<name>` once a paint has drawn that button (a row shown on hover draws none until the
    pointer is over its message), read again until it is there."""
    deadline = time.time() + timeout
    while True:
        r = w.read()
        found = box(r["rows"][index], name)
        if found:
            return r, found
        if time.time() > deadline:
            raise Fail(f"row {index} drew no {name} button within {timeout} s (row: {brief(r)}; "
                       f"columns: { {k: v for k, v in r['rows'][index].items() if k.startswith('action')} })")
        time.sleep(0.15)


def hover_row(w, index):
    """The pointer over the message of a row: over the row's own place (`actionBounds.row`, reported shown or not)."""
    r = w.read()
    place = box(r["rows"][index], "row")
    if not place:
        raise Fail(f"row {index} reports no actionBounds.row: {brief(r)}")
    move(w.window, *middle(place))
    time.sleep(0.4)
    return r


def press_button(w, index, name, hover=False):
    """A real click on the row's button `name`: the pointer moves over the message first when the row is shown on hover,
    and the click lands at the centre of the button the last paint drew. Returns the read it was aimed from."""
    if hover:
        hover_row(w, index)
    r, found = wait_bounds(w, index, name)
    press(w.window, *middle(found), hold=0.12, settle=0.15)
    return r


def clipboard_after_press(scn, w, index, hover, sentinel):
    """What the system clipboard holds after a real click on Copy, with another application owning it until then: a Copy
    that did nothing leaves the sentinel, and a clipboard left over from an earlier press cannot pass for this one."""
    owner = ClipboardOwner(sentinel)
    try:
        press_button(w, index, "copy", hover=hover)
        copied = clipboard_get()
        for _ in range(40):
            if copied != sentinel:
                break
            time.sleep(0.1)
            copied = clipboard_get()
        return copied
    finally:
        owner.release()


def wait_idle(w, what="the turn has ended", timeout=40):
    return w.wait(what, idle, timeout=timeout)


def turn(w, prompt, timeout=40):
    """One turn: the prompt sent as typed and the chat idle again after it."""
    w.send(prompt)
    return w.wait(f"{prompt!r} is answered", lambda r: idle(r) and user_texts(r) and user_texts(r)[-1] == prompt
                  and rows_of(r, "turn") and r["rows"][-1].get("kind") == "turn", timeout=timeout)


# ---------------------------------------------------------------- the scenarios

OLDER = "Older **bold** reply\n\n- the first point\n- the second point"
LAST = "The last reply, with `code` in it"


def reply_turn(w, prompt, reply):
    """A turn whose reply is the text given (the stub answers `replyfile` with it, in one chunk)."""
    w.reply_file.write_text(reply, encoding="utf-8")
    return turn(w, prompt)


@scenario
def scenario_copy(scn):
    w = World(scn, "copy")
    try:
        w.start()
        w.open_chat("claude")
        reply_turn(w, "replyfile", OLDER)
        turn(w, "ok")
        r = reply_turn(w, "replyfile", LAST)
        users, replies = rows_of(r, "user"), assistant_rows(r)
        first_user, older, last = users[0], replies[0], replies[-1]
        scn.need(len(users) == 3 and len(replies) == 3 and older[1]["text"] == OLDER and last[1]["text"] == LAST,
                 "the chat holds three turns, the first reply with Markdown and the last one the newest", brief(r))
        park(w)
        # a paint follows the pointer leaving: the row shown on hover has no button painted a frame later
        r = w.wait("the user row has no Copy button painted with the pointer away", lambda r: box(r["rows"][first_user[0]], "copy") is None,
                   timeout=5)
        # ---- a user row, shown on hover: hidden until the pointer is over it
        scn.need(r["rows"][first_user[0]]["actionsShown"] == "hover" and "copy" in actions_of(r["rows"][first_user[0]])
                 and box(r["rows"][first_user[0]], "copy") is None,
                 "a user row draws Copy and Quote on hover only: with the pointer away it has no Copy button painted",
                 f"actionsShown={r['rows'][first_user[0]].get('actionsShown')} copy={box(r['rows'][first_user[0]], 'copy')}", read=r)
        copied = clipboard_after_press(scn, w, first_user[0], True, "sentinel before the user row's copy")
        scn.need(copied == "replyfile", "a real click on a hovered user row's Copy puts its text on the clipboard",
                 f"clipboard {copied!r}", detail={"clipboard": copied}, read=w.read())
        w.capture("copy-user-row-hovered")
        park(w)
        # ---- an older reply, shown on hover, whose text is Markdown: the clipboard holds the source
        r = w.wait("the older reply has no Copy button painted with the pointer away", lambda r: box(r["rows"][older[0]], "copy") is None,
                   timeout=5)
        scn.need(r["rows"][older[0]]["actionsShown"] == "hover" and box(r["rows"][older[0]], "copy") is None,
                 "an older reply draws its row on hover only", f"actionsShown={r['rows'][older[0]].get('actionsShown')}", read=r)
        copied = clipboard_after_press(scn, w, older[0], True, "sentinel before the older reply's copy")
        scn.need(copied == OLDER and "**bold**" in copied,
                 "a real click on a hovered older reply's Copy puts its Markdown source on the clipboard (the asterisks, not the rendered text)",
                 f"clipboard {copied!r}", detail={"clipboard": copied}, read=w.read())
        w.capture("copy-older-reply-hovered")
        park(w)
        # ---- the last reply shows its row always
        r = w.read()
        last_row = r["rows"][last[0]]
        scn.need(last_row["actionsShown"] == "always" and box(last_row, "copy") is not None,
                 "the last reply draws its row with the pointer away (always) and has a Copy button painted",
                 f"actionsShown={last_row.get('actionsShown')} copy={box(last_row, 'copy')}", read=r)
        copied = clipboard_after_press(scn, w, last[0], False, "sentinel before the last reply's copy")
        scn.need(copied == LAST, "a real click on the last reply's always-shown Copy puts its text on the clipboard",
                 f"clipboard {copied!r}", detail={"clipboard": copied}, read=w.read())
        w.capture("copy-last-reply")
    finally:
        teardown(w)


# ---------------------------------------------------------------- the menu, the keyboard, the perf trace

RESTARTED_BACK = "The agent restarted without this conversation. Your message is back in the composer."


def painted_point(r, text=None):
    """A point on text the transcript's last paint drew, wholly inside the transcript's viewport: the start of the first
    painted row (of `text` when given). The menu's rows are at known offsets from a press on text that is no link."""
    composer = r["composerRow"]
    top, bottom = composer["root"][1] + 40, composer["band"][1] - 60
    rows = [p for p in r["painted"] if (text is None or p["text"] == text)
            and top < float(p["y"]) and float(p["y"]) + float(p["h"]) < bottom]
    if not rows:
        raise Fail(f"no painted text to press on (painted {[p['text'][:24] for p in r['painted']][:12]})")
    found = rows[0]
    return found, int(float(found["x"]) + 12), int(float(found["y"]) + float(found["h"]) / 2)


def menu_item(w, point, offset, button="1"):
    """The transcript menu opened with a right click at `point`, and its row `offset` px below the click pressed."""
    press(w.window, *point, button="3", hold=0.08, settle=0.05)
    time.sleep(0.3)
    press(w.window, point[0] + 40, point[1] + offset, button=button, hold=0.1, settle=0.05)


MENU_COPY, MENU_SELECT_ALL, MENU_QUOTE_SELECTION = 18, 46, 74  # the rows of the menu: Copy, Select All, Quote selection


def focus_composer(w):
    """A click on the composer's field, so that typed keys land in it."""
    field = w.read()["composerRow"]["field"]
    press(w.window, field[0] + field[2] / 2, field[1] + field[3] / 2, hold=0.1, settle=0.1)
    time.sleep(0.3)


def type_text(text):
    x("type", "--delay", "60", text)
    time.sleep(0.5)


def accept_mention(w, prefix="see "):
    """`prefix` and an `@path` of the worktree accepted from the popup with Tab, typed with real keys: the composer reads
    `<prefix>@src/main.rs ` and holds the file as an accepted mention."""
    src = w.repo / "src"
    src.mkdir(exist_ok=True)
    (src / "main.rs").write_text("fn main() {}\n")
    focus_composer(w)
    type_text(f"{prefix}@main")
    time.sleep(3)  # the popup's walk of the worktree runs off the UI thread; Tab with no popup is not a no-op
    x("key", "Tab")
    time.sleep(0.5)
    want = f"{prefix}@src/main.rs "
    w.wait("Tab picked the file", lambda r: r["composerText"] == want, timeout=10)


def perf_rows(path):
    """The performance trace as (wall-clock ns, kind, name, entity) rows."""
    epoch, rows = None, []
    with open(path) as handle:
        for line in handle:
            line = line.rstrip("\n")
            if line.startswith("# sirio-perf-v1"):
                epoch = int(next(f for f in line.split("\t") if f.startswith("epoch_ns=")).split("=")[1])
            elif line and not line.startswith("#"):
                start, _, kind, name, entity, _ = line.split("\t")[:6]
                rows.append((epoch + int(start), kind, name, int(entity)))
    return rows


def perf_window(path, seconds, during=None):
    """What the app traced in the next `seconds` by the wall clock (`during` runs at the start of the window): a count per
    span or event name, and the events the trace dropped (the file's own `trace.dropped` rows, read once its writer has
    flushed past the window). No read of the chat is made in the window: a read wakes the app."""
    t0 = time.time_ns()
    if during:
        during()
    time.sleep(max(0.0, seconds - (time.time_ns() - t0) / 1e9))
    t1 = time.time_ns()
    deadline = time.time() + 15
    while True:
        rows = perf_rows(path)
        if any(name == "trace.dropped" and wall > t1 for wall, _, name, _ in rows):
            break
        if time.time() > deadline:
            raise Fail("the performance trace was not flushed past the window")
        time.sleep(0.25)
    counts = {}
    for wall, kind, name, entity in rows:
        if t0 <= wall <= t1 and name != "trace.dropped":
            counts[name] = counts.get(name, 0) + 1
    return counts, sum(entity for _, _, name, entity in rows if name == "trace.dropped")


def redraws(counts):
    """The redraws of a window: the larger of the chat's own renders and the workspace's."""
    return max(counts.get("Chat.render", 0), counts.get("SirioWorkspace.render", 0))


# ---------------------------------------------------------------- rows


@scenario
def scenario_rows(scn):
    w = World(scn, "rows")
    try:
        w.start()
        w.open_chat("claude")
        reply_turn(w, "replyfile", OLDER)
        r = reply_turn(w, "replyfile", LAST)
        park(w)
        r = w.read()
        replies = assistant_rows(r)
        (i_old, old), (i_last, last) = replies[0], replies[-1]
        shown = {index: (row.get("actionsShown"), row.get("actions")) for index, row in enumerate(r["rows"])}
        scn.need(shown[i_last] == ("always", "copy,quote") and shown[i_old] == ("hover", "copy,quote"),
                 "the last reply shows its row always and the older one on hover, each with Copy and Quote", f"{shown}", read=r)
        others = {index: row for index, row in enumerate(r["rows"]) if index != i_last}
        scn.need(all(row.get("actionsShown") != "always" for row in others.values()),
                 "no other row of the transcript is shown always", f"{ {i: row.get('actionsShown') for i, row in others.items()} }", read=r)
        scn.need(all(row.get("actions") == "" and row.get("actionsShown") == "none" for _, row in rows_of(r, "turn")),
                 "a turn footer has no action row", f"{[(row.get('actions'), row.get('actionsShown')) for _, row in rows_of(r, 'turn')]}", read=r)
        # ---- a streaming reply has no row
        w.send("slow")
        s = w.wait("a reply is streaming", lambda r: r["status"] == "streaming" and last_reply(r)[1] is not None
                   and last_reply(r)[1].get("live") == "true" and byte_len(last_reply(r)[1]["text"]) > 40
                   and last_reply(r)[0] > i_last, timeout=30, every=0.1)
        index, live = last_reply(s)
        scn.need(live.get("actions") == "" and live.get("actionsShown") == "none" and box(live, "copy") is None,
                 "a reply that is still streaming has no actions: none, shown none, no button painted",
                 f"actions={live.get('actions')!r} shown={live.get('actionsShown')} copy={box(live, 'copy')}", read=s)
        scn.need(all(row.get("actionsShown") != "always" for row in s["rows"]) and s["rows"][i_last].get("actionsShown") == "hover",
                 "while a reply streams no row is shown always: the earlier last reply is back to hover",
                 f"{ {i: row.get('actionsShown') for i, row in enumerate(s['rows'])} }", read=s)
        w.capture("rows-streaming")
        w.stop_turn()
        e = w.wait("the stopped turn is over", lambda r: idle(r) and r["rows"][-1].get("kind") == "turn", timeout=30)
        index, stopped = last_reply(e)
        scn.need(stopped.get("actions", "").startswith("copy,quote") and stopped.get("actionsShown") == "always",
                 "when the reply is over its row appears: it is the last reply, shown always",
                 f"actions={stopped.get('actions')!r} shown={stopped.get('actionsShown')}", read=e)
        # ---- Select All and Copy through the transcript menu read the same text as the readout, with no button text in it
        park(w)
        r = w.read()
        found, px, py = painted_point(r)
        sentinel = "sentinel before select all and copy"
        owner = ClipboardOwner(sentinel)
        try:
            menu_item(w, (px, py), MENU_SELECT_ALL)
            selected = w.read()
            time.sleep(0.3)
            menu_item(w, (px, py), MENU_COPY)
            copied = clipboard_get()
            for _ in range(30):
                if copied != sentinel:
                    break
                time.sleep(0.1)
                copied = clipboard_get()
        finally:
            owner.release()
        plain = plain_by_row(selected)  # raises Fail when the rows do not tile the transcript text
        scn.need(copied == selected["transcript_text"] and copied != sentinel,
                 "Select All and Copy through the menu put the transcript text of the readout on the clipboard",
                 f"clipboard {len(copied)} chars, transcriptText {len(selected['transcript_text'])} chars", read=selected)
        reply_plain = [plain[i] for i, _ in assistant_rows(selected)]
        scn.need(all(text and text in copied for text in reply_plain) and "Quote" not in copied and "Copy" not in copied,
                 "the copied text holds every reply's text and none of the action row's own labels",
                 f"replies {[t[:20] for t in reply_plain]}", read=selected)
        # Ctrl+C copies the selection too
        owner = ClipboardOwner("sentinel before ctrl+c")
        try:
            x("key", "ctrl+c")
            by_key = clipboard_get()
            for _ in range(30):
                if by_key != "sentinel before ctrl+c":
                    break
                time.sleep(0.1)
                by_key = clipboard_get()
        finally:
            owner.release()
        scn.need(by_key == selected["transcript_text"],
                 "Ctrl+C with everything selected puts the same transcript text on the clipboard",
                 f"clipboard {len(by_key)} chars, transcriptText {len(selected['transcript_text'])} chars", read=selected)
    finally:
        teardown(w)


# ---------------------------------------------------------------- idle cost


@scenario
def scenario_idle_cost(scn):
    trace = os.path.join(E["RUN_DIR"], "idle-perf.tsv")  # the trace names a file that does not exist yet, one per launch
    w = World(scn, "idle", extra_env={"SIRIO_PERF_TRACE": trace})
    try:
        w.start()
        w.open_chat("claude")
        reply_turn(w, "replyfile", OLDER)
        r = reply_turn(w, "replyfile", LAST)
        index, last = last_reply(r)
        park(w)
        r, found = wait_bounds(w, index, "copy")
        scn.need(r["rows"][index].get("actionsShown") == "always",
                 "the chat is idle with its last reply's row shown", f"{r['rows'][index].get('actionsShown')}", read=r)
        time.sleep(3)  # whatever the turns left settles
        before, dropped_before = perf_window(trace, 5)
        scn.keep("redraw counts before any press (5 s)", before)
        # the control: the counter sees redraws when there are some (a press of Copy: the pointer arrives, the button animates)
        control, dropped_control = perf_window(trace, 3, during=lambda: press(w.window, *middle(found), hold=0.12, settle=0.1))
        scn.keep("redraw counts across a Copy press (3 s)", control)
        scn.need(redraws(control) > 0 and redraws(control) > redraws(before),
                 "control: a window with a Copy press in it counts redraws, more than the idle window did",
                 f"press window {redraws(control)} (Chat.render {control.get('Chat.render', 0)}, SirioWorkspace.render "
                 f"{control.get('SirioWorkspace.render', 0)}), idle window {redraws(before)}", detail={"before": before, "control": control})
        park(w)
        time.sleep(3)  # the press has settled: Ely's confirmation is over
        after, dropped_after = perf_window(trace, 5)
        scn.keep("redraw counts after the press has settled (5 s)", after)
        scn.need(dropped_before == 0 and dropped_control == 0 and dropped_after == 0,
                 "the trace dropped no event, so the counts can be trusted",
                 f"dropped {dropped_before}, {dropped_control}, {dropped_after}")
        b, a = redraws(before), redraws(after)
        scn.need(a <= b * 1.5 + 2,
                 "the idle chat with its row shown redraws no more after a Copy press than before it (after <= before x 1.5 + 2)",
                 f"before {b}, after {a} redraws in 5 s", detail={"before": before, "after": after})
        scn.need(b <= 15 and a <= 15,
                 "an idle chat redraws at most 3 times a second (a cursor blink's allowance), before and after",
                 f"before {b / 5:.1f}/s, after {a / 5:.1f}/s", detail={"before": before, "after": after})
        w.capture("idle-cost")
    finally:
        teardown(w)


# ---------------------------------------------------------------- Continue


def verb(w, action, **params):
    """A `surface.chat.transcript` verb: None when it was accepted, else the sentence the socket refused it with."""
    try:
        w.rpc("surface.chat.transcript", surfaceId=w.surface, action=action, **params)
    except RpcError as error:
        return str(error)
    return None


def closed(r):
    """A readout of a chat whose last turn is over: idle, and its last row is the turn's footer."""
    return idle(r) and r["rows"][-1].get("kind") == "turn"


def wait_closed(w, what="the turn is over", timeout=40):
    return w.wait(what, closed, timeout=timeout)


def stop_slow(w, prompt="slow", past=-1):
    """Sends `prompt` (a reply that streams for 8 s), waits until text of it is on screen and stops it; returns the read
    of the closed turn. `past` is the index of the last row before the prompt."""
    w.send(prompt)
    w.wait("the slow reply is streaming", lambda r: r["status"] == "streaming" and last_reply(r)[1] is not None
           and last_reply(r)[0] > past and last_reply(r)[1].get("live") == "true" and byte_len(last_reply(r)[1]["text"]) > 40,
           timeout=30, every=0.1)
    w.stop_turn()
    return wait_closed(w, "the stopped turn is over", timeout=30)


def footers_after(r, start):
    """The footers of the rows from index `start` on: those of the turns this part of a scenario began."""
    return [row["text"] for row in r["rows"][start:] if row.get("kind") == "turn"]


def none_offered(r):
    """Continue is offered nowhere: the chat says none, names no row and no row draws it."""
    return r.get("continue") == "none" and "continueRow" not in r and continue_rows(r) == []


def offered(scn, r, action, words, what):
    """Continue is offered as `action`, on the last reply's row and on no other row, and the turn's footer says `words`."""
    footer = footers(r)[-1]
    last = assistant_rows(r)[-1][0]
    earlier = [row.get("actions") for _, row in assistant_rows(r)[:-1]]
    scn.need(words in footer and r.get("continue") == action and r.get("continueRow") == str(last)
             and continue_rows(r) == [last] and actions_of(r["rows"][last]) == ["copy", "quote", "continue"]
             and r["rows"][last].get("actionsShown") == "always" and all(a == "copy,quote" for a in earlier),
             f"{what}: the footer says {words!r} and Continue is offered ({action}) on the last reply's row only",
             f"footer {footer!r}, continue={r.get('continue')} continueRow={r.get('continueRow')}, rows with continue "
             f"{continue_rows(r)}, earlier replies {earlier}", read=r)


def press_continue(scn, w, agent, reply, what, hover=False):
    """A real click on the Continue button, and what it sent: a visible user row `Continue`, which the agent's stub heard
    as a prompt on the process the chat had been talking to all along, and its answer."""
    r = w.read()
    press_button(w, int(r["continueRow"]), "continue", hover=hover)
    after = w.wait("Continue is a user row and its turn is over", lambda r: user_texts(r)[-1:] == ["Continue"]
                   and closed(r) and last_reply(r)[1] is not None and last_reply(r)[1]["text"] == reply, timeout=40)
    prompts, pid = w.prompts(), w.sessions(agent)[-1]
    scn.need(prompts[-1] == (pid, "Continue") and {p for p, _ in prompts} == {pid},
             f"{what}: the press sent the prompt Continue as a visible user row, and the stub heard it on the same process "
             "as every earlier prompt of the chat", f"prompts {prompts}, process {pid}", detail={"prompts": prompts}, read=after)
    if agent == "opencode":  # an ACP agent's conversation is its session: one `session/new`, and every prompt names it
        asked = [value for kind, p, value in w.records("session_new") if p == pid]
        named = {value["sessionId"] for kind, p, value in w.records("prompt") if p == pid}
        scn.need(len(asked) == 1 and len(named) == 1,
                 f"{what}: the agent was asked for one session and every prompt, Continue's too, names it",
                 f"session/new {len(asked)}, sessions named {sorted(named)}", detail={"sessionNew": asked, "named": sorted(named)})
    scn.need(none_offered(after), f"{what}: after the Continue turn Continue is offered nowhere", f"continue={after.get('continue')}", read=after)
    return after


@scenario
def scenario_continue_claude(scn):
    w = World(scn, "cc")
    try:
        w.start()
        w.open_chat("claude")
        r = turn(w, "ok")
        scn.need(none_offered(r) and verb(w, "continue") is not None and "no turn to continue" in verb(w, "continue"),
                 "after an ordinary end Continue is offered nowhere, and the socket's continue says there is no turn to continue",
                 f"continue={r.get('continue')} verb={verb(w, 'continue')!r}", read=r)
        # ---- max tokens
        r = turn(w, "max tokens")
        offered(scn, r, "resend", "token limit", "a turn that stopped at the token limit")
        w.capture("continue-max-tokens")
        # a folded turn draws no Continue, and the socket refuses to press what nobody can see
        footer = len(r["rows"]) - 1
        scn.need(verb(w, "toggleTurn", turn=str(footer)) is None, "the turn folds", f"footer row {footer}")
        folded = w.read()
        refusal = verb(w, "continue")
        scn.need(refusal is not None and refusal.endswith("the turn is folded") and continue_rows(folded) == [],
                 "with the turn folded no row draws Continue and the socket's continue is refused: the turn is folded",
                 f"{refusal!r}, rows with continue {continue_rows(folded)}", read=folded)
        scn.need(verb(w, "toggleTurn", turn=str(footer)) is None, "the turn unfolds", f"footer row {footer}")
        r = w.wait("Continue is drawn again", lambda r: len(continue_rows(r)) == 1)
        press_continue(scn, w, "claude", "ok", "max tokens")
        # ---- the turn limit
        turn(w, "fail max turns")
        r = w.read()
        offered(scn, r, "resend", "turn-request limit", "a turn that stopped at the turn-request limit")
        press_continue(scn, w, "claude", "ok", "max turns")
        # ---- Stop
        r = stop_slow(w, past=len(w.read()["rows"]) - 1)
        offered(scn, r, "resend", "cancelled", "a Stopped turn")
        press_continue(scn, w, "claude", "ok", "Stop")
        w.capture("continue-after-stop")
    finally:
        teardown(w)


@scenario
def scenario_continue_acp(scn):
    w = World(scn, "ca")
    try:
        w.start()
        w.open_chat("opencode")
        turn(w, "hello")
        # ---- max tokens, with a draft and an accepted mention in the composer
        r = turn(w, "max tokens")
        offered(scn, r, "resend", "token limit", "an ACP turn that stopped at the token limit")
        accept_mention(w, prefix="see ")
        type_text("now")
        draft = "see @src/main.rs now"
        w.wait("the draft is typed", lambda r: r["composerText"] == draft)
        press_continue(scn, w, "opencode", "reply ok", "ACP max tokens")
        r = w.read()
        scn.need(r["composerText"] == draft,
                 "Continue left the typed draft and its accepted mention in the composer",
                 f"composer {r['composerText']!r}", read=r)
        focus_composer(w)
        x("key", "Return")
        w.wait("the draft is sent", lambda r: user_texts(r)[-1:] == ["see now"], timeout=30)
        sent = [value for kind, pid, value in w.records("prompt")][-1]
        links = [b for b in sent.get("blocks", []) if b.get("type") != "text"]
        scn.need(sent["text"] == "see now" and len(links) == 1 and links[0].get("type") == "resource_link"
                 and links[0].get("uri", "").endswith("/src/main.rs") and w.read()["composerText"] == "",
                 "the accepted mention survived the Continue: the draft, sent after it, reached the agent as the text and a link to src/main.rs",
                 f"blocks {sent.get('blocks')}", detail={"prompt": sent})
        wait_closed(w)
        # ---- the turn limit
        r = turn(w, "max turns")
        offered(scn, r, "resend", "turn-request limit", "an ACP turn that stopped at the turn-request limit")
        press_continue(scn, w, "opencode", "reply ok", "ACP max turns")
        # ---- Stop
        r = stop_slow(w, past=len(w.read()["rows"]) - 1)
        offered(scn, r, "resend", "cancelled", "an ACP Stop")
        press_continue(scn, w, "opencode", "reply ok", "ACP Stop")
        # ---- the agent answers the cancel with an error: the late end of a stopped turn (Task 4's finding 2)
        w.send("cancel fail")
        w.wait("the partial reply streams", lambda r: r["status"] == "streaming"
               and any(a["text"].startswith("I started") for _, a in assistant_rows(r)), timeout=30)
        w.stop_turn()
        r = w.wait("the stopped turn is closed", lambda r: closed(r) and "cancelled" in footers(r)[-1], timeout=30)
        scn.need("ended" not in footers(r)[-1] and error_row(r) is None,
                 "an agent's error answer to the cancel is the late end of the stopped turn: the footer says cancelled, not ended, and there is no error row",
                 f"footer {footers(r)[-1]!r}", read=r)
        offered(scn, r, "resend", "cancelled", "an ACP Stop answered with an error")
    finally:
        teardown(w)


@scenario
def scenario_continue_hidden(scn):
    a = World(scn, "ha")
    c = World(scn, "hc")
    try:
        # ---- an ACP chat that lost its agent: the conversation cannot come back, so there is nothing to continue
        a.start()
        a.open_chat("opencode")
        a.send("max tokens drop")
        r = a.wait("the lost agent is an error row", lambda r: error_row(r) is not None and r["status"] != "streaming", timeout=30)
        index, row = error_row(r)
        refusal = verb(a, "continue")
        scn.need(any("token limit" in f for f in footers(r)) and none_offered(r) and refusal is not None
                 and refusal.endswith("the agent is not connected"),
                 "an ACP chat whose turn stopped at the token limit and then lost its agent offers no Continue (it would restart without "
                 "the conversation), and the socket says the agent is not connected",
                 f"footers {footers(r)}, continue={r.get('continue')}, refusal {refusal!r}, error row {row.get('errorKind')}/{row.get('retry')}",
                 detail={"row": row}, read=r)
        a.capture("continue-hidden-acp-lost")
        a.retry(index)
        r = a.wait("the relaunched agent is connected", lambda r: idle(r) and error_row(r) is None, timeout=40)
        refusal = verb(a, "continue")
        scn.need(none_offered(r) and refusal is not None and refusal.endswith("no turn to continue"),
                 "an ACP chat after its agent restarted offers no Continue, and the socket says there is no turn to continue",
                 f"continue={r.get('continue')} refusal {refusal!r}", read=r)
        a.keep_artifacts()
        a.quit()

        # ---- an ordinary end, a turn in progress, and a restored chat
        c.start()
        c.open_chat("claude")
        r = turn(c, "ok")
        scn.need(none_offered(r), "after an ordinary end of turn Continue is offered nowhere", f"continue={r.get('continue')}", read=r)
        past = len(r["rows"]) - 1
        c.send("slow")
        s = c.wait("the slow reply is streaming", lambda r: r["status"] == "streaming" and last_reply(r)[1] is not None
                   and last_reply(r)[0] > past and byte_len(last_reply(r)[1]["text"]) > 40, timeout=30, every=0.1)
        refusal = verb(c, "continue")
        scn.need(none_offered(s) and refusal is not None and refusal.endswith("a turn is running"),
                 "while a turn is in progress Continue is offered nowhere, and the socket says a turn is running",
                 f"continue={s.get('continue')} refusal {refusal!r}", read=s)
        c.stop_turn()
        stopped = wait_closed(c, "the stopped turn is over", timeout=30)
        offered(scn, stopped, "resend", "cancelled", "the stopped turn, before the app is closed")
        c.quit()
        c.start()
        c.reopen_chat()
        r = c.read()
        refusal = verb(c, "continue")
        scn.need(none_offered(r) and refusal is not None and refusal.endswith("no turn to continue")
                 and any("cancelled" in f for f in footers(r)),
                 "a restored chat has no Continue, though its last turn was stopped: nothing is offered and the socket says there is no turn to continue",
                 f"continue={r.get('continue')} refusal {refusal!r} footers {footers(r)}", read=r)
        c.capture("continue-hidden-restored")
    finally:
        teardown([a, c])


@scenario
def scenario_continue_resume(scn):
    w = World(scn, "cr")
    try:
        w.start()
        w.open_chat("claude")
        first = w.sessions("claude")[-1]
        turn(w, "ok")
        r = turn(w, "max tokens")
        offered(scn, r, "resend", "token limit", "a turn that stopped at the token limit")
        kill_stub(first)
        r = w.wait("the lost CLI is an error row and Continue resumes", lambda r: error_row(r) is not None and r.get("continue") == "resume",
                   timeout=30)
        scn.need(r.get("continueRow") == str(assistant_rows(r)[-1][0]) and continue_rows(r) == [assistant_rows(r)[-1][0]],
                 "with the CLI gone Continue is offered as a resume, on the same reply's row",
                 f"continue={r.get('continue')} continueRow={r.get('continueRow')}", read=r)
        w.capture("continue-resume-offered")
        row = int(r["continueRow"])
        press_button(w, row, "continue")
        after = w.wait("the resumed chat answers Continue", lambda r: user_texts(r)[-1:] == ["Continue"] and closed(r)
                       and last_reply(r)[1]["text"] == "ok", timeout=60)
        launches = w.sessions("claude")
        args = {pid: a for kind, pid, a in w.records("claude")}
        second = launches[-1]
        scn.need(second != first and "--resume fixture-session-1" in args[second] and "--resume" not in args[first],
                 "Continue relaunched the agent with --resume of its session (the stub recorded it); the first launch had none",
                 f"first {args[first][-60:]!r}, second {args[second][-80:]!r}", read=after)
        prompts = w.prompts()
        scn.need(prompts[-1] == (second, "Continue") and prompts.count((first, "Continue")) == 0
                 and prompts[-2] == (first, "max tokens"),
                 "the prompt Continue reached the resumed process, after --resume, and not the dead one",
                 f"{prompts}", detail={"prompts": prompts}, read=after)
        scn.need(none_offered(after), "after the resumed Continue turn Continue is offered nowhere", f"continue={after.get('continue')}", read=after)
    finally:
        teardown(w)


@scenario
def scenario_continue_once(scn):
    w = World(scn, "co")
    try:
        w.start()
        w.open_chat("claude")
        turn(w, "max tokens")
        Path(f"{w.gate}.hold-continue").write_text("hold")  # the stub holds the turn a Continue starts until gate.1
        scn.need(verb(w, "continue") is None, "the first continue is accepted", "")
        s = w.wait("the Continue turn is running", lambda r: r["status"] == "streaming" and user_texts(r)[-1:] == ["Continue"], timeout=20)
        second = verb(w, "continue")
        scn.need(second is not None and second.endswith("a turn is running") and none_offered(s),
                 "a second continue while the first one's turn runs is refused: a turn is running, and Continue is offered nowhere",
                 f"{second!r}, continue={s.get('continue')}", read=s)
        w.release_gate(1)
        after = wait_closed(w, "the Continue turn is over")
        time.sleep(1)
        prompts = [text for _, text in w.prompts()]
        scn.need(prompts.count("Continue") == 1 and user_texts(after).count("Continue") == 1,
                 "the stub heard one Continue, and the transcript holds one Continue row",
                 f"prompts {prompts}, user rows {user_texts(after)}", detail={"prompts": prompts}, read=after)
    finally:
        teardown(w)


# ---------------------------------------------------------------- Claude's Stop


@scenario
def scenario_claude_stop(scn):
    w = World(scn, "cs")
    try:
        w.start()
        w.open_chat("claude")
        turn(w, "ok")
        # ---- a Stop of a streaming reply
        past = len(w.read()["rows"]) - 1
        r = stop_slow(w, past=past)
        index, reply = last_reply(r)
        text = reply["text"]
        scn.need("cancelled" in footers(r)[-1] and error_row(r) is None,
                 "a Stop of a Claude reply ends the turn with a footer that says cancelled, and no error row",
                 f"footer {footers(r)[-1]!r}", read=r)
        time.sleep(1.5)
        later = w.read()
        scn.need(last_reply(later)[1]["text"] == text and len(assistant_rows(later)) == len(assistant_rows(r))
                 and not any("nterrupted" in row.get("text", "") for row in later["rows"]),
                 "nothing is added to the reply after the Stop: the same text, no new reply row, and no sentence of the CLI's about the interrupt",
                 f"reply {len(text)} chars then {len(last_reply(later)[1]['text'])}; rows {[row.get('kind') for row in later['rows']][-5:]}", read=later)
        offered(scn, later, "resend", "cancelled", "a Stopped Claude turn")
        # ---- a Stop with an item queued sends the item, and Continue is not offered over it
        number = [text for _, text in w.prompts()].count("ok hold") + 1
        past = len(later["rows"]) - 1
        w.send("slow")
        w.wait("the slow reply is streaming", lambda r: r["status"] == "streaming" and last_reply(r)[0] > past
               and byte_len(last_reply(r)[1]["text"]) > 40, timeout=30, every=0.1)
        w.send("ok hold", queued=True)
        w.wait("the item is queued", lambda r: r["queue"] == ["ok hold"], timeout=10)
        w.stop_turn()
        s = w.wait("the queued item is running", lambda r: user_texts(r)[-1:] == ["ok hold"] and r["status"] == "streaming"
                   and any("cancelled" in f for f in footers_after(r, past + 1)) and r["queue"] == [], timeout=30)
        scn.need(none_offered(s),
                 "a Stop with an item queued sends the item, and Continue is offered nowhere while the item's turn runs",
                 f"continue={s.get('continue')} rows with continue {continue_rows(s)}", read=s)
        w.release_gate(number)
        e = wait_closed(w, "the item's turn is over")
        scn.need(none_offered(e) and [text for _, text in w.prompts()][-2:] == ["slow", "ok hold"],
                 "after the item's turn Continue is offered nowhere, and the stub heard the slow prompt and then the item, in order",
                 f"continue={e.get('continue')} prompts {[text for _, text in w.prompts()][-3:]}", read=e)
        # ---- Stop, then another prompt before the stopped turn's late end arrives, then the late end
        number = [text for _, text in w.prompts()].count("ok hold") + 1
        past = len(e["rows"]) - 1
        w.send("slow late")
        w.wait("the slow reply is streaming", lambda r: r["status"] == "streaming" and last_reply(r)[0] > past
               and byte_len(last_reply(r)[1]["text"]) > 40, timeout=30, every=0.1)
        w.stop_turn()
        w.send("ok hold")
        b = w.wait("the second prompt is running, the stopped turn has not ended", lambda r: user_texts(r)[-1:] == ["ok hold"]
                   and footers_after(r, past + 1) == [], timeout=20)
        scn.need(b["status"] == "streaming" and none_offered(b),
                 "Stop, then another prompt before the stopped turn's end: the stopped turn has no footer yet and Continue is offered nowhere",
                 f"status {b['status']} continue={b.get('continue')}", read=b)
        Path(f"{w.gate}.late").write_text("go")  # the stopped turn's late end
        after = w.wait("the late end of the stopped turn arrived", lambda r: any("cancelled" in f for f in footers_after(r, past + 1)), timeout=30)
        scn.need(none_offered(after),
                 "the stopped turn's late end, arriving while the next prompt runs, does not offer Continue: continue none and no row draws it",
                 f"continue={after.get('continue')} rows with continue {continue_rows(after)}", read=after)
        w.release_gate(number)
        done = w.wait("the second prompt is answered", lambda r: closed(r) and last_reply(r)[1]["text"] == "ok" and user_texts(r)[-1] == "ok hold",
                      timeout=40)
        scn.need(none_offered(done), "once the second prompt is answered Continue is offered nowhere", f"continue={done.get('continue')}", read=done)
    finally:
        teardown(w)


# ---------------------------------------------------------------- Quote

QUOTED = "A reply to quote.\n\nIts second paragraph says so and ends here."
QUOTED_PATH = "Look at @src/main.rs for the entry point."
SELECTABLE = "alpha beta gamma delta"
CLOSE_FROM_RIGHT = 14  # px: the quote card's cross, from the right end of `composerQuoteBounds` (Ely's cross has no selector)


def reply_row(r, text):
    """The index of the reply whose text is `text`."""
    found = [index for index, row in assistant_rows(r) if row["text"] == text]
    if not found:
        raise Fail(f"no reply with the text {text!r}: {brief(r)}")
    return found[-1]


def quote_reply(w, text, hover=True):
    """A real click on Quote on the reply `text` (with the pointer over it first), and the read once the composer holds the
    quote and a paint has drawn its card (the bounds follow the state by a frame)."""
    press_button(w, reply_row(w.read(), text), "quote", hover=hover)
    return w.wait(f"the composer holds the quote of {text[:20]!r}",
                  lambda r: r.get("composerQuote") == text and "composerQuoteBounds" in r, timeout=10)


def remove_quote(w, r):
    """A real click on the cross of the composer's quote card, at the right end of its bounds; the read once the quote is gone.
    A click that misses leaves the quote, and the wait fails."""
    card = json.loads(r["composerQuoteBounds"])
    press(w.window, card[0] + card[2] - CLOSE_FROM_RIGHT, card[1] + card[3] / 2, hold=0.12, settle=0.15)
    return w.wait("the cross removes the quote", lambda r: "composerQuote" not in r and "composerQuoteBounds" not in r, timeout=6)


def sent(w, text, what):
    """The read once the user row `text` is answered and the turn is over."""
    return w.wait(what, lambda r: closed(r) and user_texts(r)[-1:] == [text], timeout=40)


@scenario
def scenario_quote(scn):
    w = World(scn, "q")
    b = World(scn, "qa")
    try:
        w.start()
        w.open_chat("claude")
        pid = w.sessions("claude")[-1]
        reply_turn(w, "replyfile", "an older reply")
        reply_turn(w, "replyfile", QUOTED)
        park(w)
        # ---- Quote on a reply, with a real click; the cross removes it
        r = quote_reply(w, QUOTED, hover=False)
        box_ = json.loads(r["composerQuoteBounds"]) if "composerQuoteBounds" in r else None
        scn.need(box_ and len(box_) == 4 and box_[2] > 0 and box_[3] > 0,
                 "Quote puts the reply in the composer: composerQuote is its text and the quote card is drawn (composerQuoteBounds)",
                 f"composerQuote {r.get('composerQuote')!r}, bounds {box_}", read=r)
        w.capture("quote-in-composer")
        r = remove_quote(w, r)
        scn.ok("the cross at the right end of the quote card removes the quote (composerQuote and its bounds are gone)", read=r)
        # ---- a quote and typed text are sent as a block, and the user row reads the quote back
        quote_reply(w, QUOTED, hover=False)
        w.send("my comment")
        expected = quote_block(QUOTED, "my comment")
        after = sent(w, expected, "the quoted prompt is answered")
        scn.need(w.prompts()[-1] == (pid, expected) and "composerQuote" not in after,
                 "the stub heard the quote as a `> ` block, an empty line and the typed text; the composer's quote is spent",
                 f"{w.prompts()[-1]!r}", detail={"prompt": w.prompts()[-1][1]}, read=after)
        index, user = rows_of(after, "user")[-1]
        scn.need(user.get("quote") == QUOTED and user["text"] == expected,
                 "the user row of the sent prompt reads its quote back (quote column) next to the whole text it sent",
                 f"quote {user.get('quote')!r}", read=after)
        w.capture("quote-sent-row")
        # ---- Quote selection through the context menu
        r = reply_turn(w, "replyfile", SELECTABLE)
        park(w)
        r = w.read()
        found = next((p for p in r["painted"] if p["text"] == SELECTABLE), None)
        if found is None:
            raise Fail(f"the reply is not painted as one text row: {[p['text'][:30] for p in r['painted']]}")
        left, top, width, height = (float(found[k]) for k in ("x", "y", "w", "h"))
        middle_y = top + height / 2
        move(w.window, left + 1, middle_y)
        time.sleep(0.1)
        x("mousedown", "1")
        for step in range(1, 9):
            move(w.window, left + 1 + (width + 10) * step / 8, middle_y)
            time.sleep(0.03)
        x("mouseup", "1")
        time.sleep(0.4)
        menu_item(w, (int(left + width / 2) - 40, int(middle_y)), MENU_QUOTE_SELECTION)
        r = w.wait("the selection is quoted", lambda r: "composerQuote" in r, timeout=8)
        scn.need(r["composerQuote"] == SELECTABLE,
                 "Quote selection in the transcript menu (right click, then the item) quotes the selected text",
                 f"composerQuote {r['composerQuote']!r}", read=r)
        # a second quote replaces the first
        r = quote_reply(w, QUOTED)
        scn.need(r["composerQuote"] == QUOTED, "a second quote replaces the first one", f"composerQuote {r['composerQuote']!r}", read=r)
        remove_quote(w, r)  # (a prompt sent now would carry it)
        # ---- a quote held while a turn streams is queued with its block
        reply_turn(w, "replyfile", QUOTED)  # the reply to quote is on screen: the transcript scrolls to its end
        number = [text for _, text in w.prompts()].count("ok hold") + 1
        w.send("ok hold")
        w.wait("the held turn is running", lambda r: r["status"] == "streaming", timeout=20)
        quote_reply(w, QUOTED)
        w.send("queued comment", queued=True)
        queued = quote_block(QUOTED, "queued comment")
        r = w.wait("the item is queued", lambda r: r["queue"] == [queued], timeout=10)
        scn.need("composerQuote" not in r,
                 "a message sent while a turn streams is queued with its quote block (the queued readout), and the composer's quote is spent",
                 f"queued {r['queue']!r}", read=r)
        w.release_gate(number)
        after = sent(w, queued, "the queued item is sent when the turn ends")
        scn.need(w.prompts()[-1] == (pid, queued),
                 "the queued item reached the stub with its quote block", f"{w.prompts()[-1]!r}", detail={"prompt": w.prompts()[-1][1]}, read=after)
        # ---- a command goes without the quote, and keeps it; a path that is no command goes with it
        reply_turn(w, "replyfile", QUOTED)
        quote_reply(w, QUOTED)
        w.send("/compact")
        after = sent(w, "/compact", "the command is answered")
        scn.need(w.prompts()[-1] == (pid, "/compact") and after.get("composerQuote") == QUOTED,
                 "a command typed over a held quote is sent without the quote, which stays in the composer",
                 f"{w.prompts()[-1]!r}, composerQuote {after.get('composerQuote')!r}", read=after)
        w.send("/etc/hosts is wrong")
        block = quote_block(QUOTED, "/etc/hosts is wrong")
        after = sent(w, block, "the path is answered")
        scn.need(w.prompts()[-1] == (pid, block) and "composerQuote" not in after,
                 "text that starts with a path is no command: it is sent with the quote block, and the quote is spent",
                 f"{w.prompts()[-1]!r}", read=after)
        # ---- an @path inside the quote is not an accepted mention: it reaches the agent unchanged
        reply_turn(w, "replyfile", QUOTED_PATH)
        accept_mention(w, prefix="see ")
        quote_reply(w, QUOTED_PATH, hover=False)
        x("key", "Return")
        after = w.wait("the prompt with the quoted path is sent", lambda r: closed(r) and user_texts(r)[-1].startswith("> Look at"), timeout=40)
        prompt = w.prompts()[-1][1]
        head = quote_block(QUOTED_PATH)
        typed = prompt[len(head) + 2:]
        scn.need(prompt.startswith(head + "\n\n") and "> Look at @src/main.rs for the entry point." in prompt
                 and "@src/main.rs" not in typed and "see" in typed,
                 "the @path in the quoted text reaches the stub inside the `> ` block unchanged, while the one typed after it was taken as the mention",
                 f"{prompt!r}", detail={"prompt": prompt}, read=after)
        # ---- after a relaunch the restored row still shows its quote
        w.keep_artifacts()
        w.quit()
        w.start()
        w.reopen_chat()
        r = w.read()
        row = next((row for _, row in rows_of(r, "user") if row.get("text") == expected), None)
        scn.need(row is not None and row.get("quote") == QUOTED,
                 "after an app relaunch the restored user row still shows its quote",
                 f"user rows {[(u.get('text', '')[:30], u.get('quote', '')[:20]) for _, u in rows_of(r, 'user')]}", read=r)
        w.capture("quote-restored")

        # ---- a hand-back refills the quote and the field (the ACP agent is lost with the message)
        b.start()
        b.open_chat("opencode")
        r = turn(b, "hello")
        quote_reply(b, "reply ok", hover=False)
        b.send("die")
        r = b.wait("the lost agent is an error row", lambda r: error_row(r) is not None and r["status"] != "streaming", timeout=30)
        index, row = error_row(r)
        scn.need(row.get("retry") == "composer", "the message sent with the quote was lost mid-turn: the row says retry: composer",
                 f"retry={row.get('retry')}", read=r)
        prompts = b.prompts()
        b.retry(index)
        r = b.wait("the relaunched agent is connected with the message back", lambda r: r["composerText"] == "die"
                   and idle(r) and error_row(r) is None, timeout=40)
        time.sleep(1.5)  # a prompt the retry wrongly sent would be in the stub's log by now
        scn.need(r.get("composerQuote") == "reply ok" and row_with_text(r, RESTARTED_BACK) is not None and b.prompts() == prompts,
                 "Retry relaunched the agent and handed the message back: the quote is back in the composer's card and the typed text in the "
                 "field, with the notice, and nothing was sent",
                 f"composerQuote {r.get('composerQuote')!r}, composer {r['composerText']!r}", detail={"prompts": b.prompts()}, read=r)
        b.capture("quote-handed-back")
    finally:
        teardown([w, b])


if __name__ == "__main__":
    main("MESSAGE ACTIONS")
