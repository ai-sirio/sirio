# P90 — the socket that says yes to work it never does

**Owner: `codex12`** (`main.rs` is yours). Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

**Queued behind P87 and P88.** It is the smallest of the three and the only one that closes a ledger
row by *removing* a capability claim rather than adding an implementation.

## The finding, measured live

I called eight of the ten `browser.*` control methods over the socket at 03:02:56 on 2026-08-14.
Every single one answered:

```json
{"ok":true,"result":{"method":"browser.eval","queued":"true"}}
```

`browser.screenshot`, handed an explicit `path=/tmp/probe-shot.png`, returned that same success and
**wrote no file**. `browser.eval` with `{"script":"1+1"}` returned success and no value. All ten are
advertised in `system.capabilities` — I read the list back from the running app.

What is actually behind them (`main.rs:4638-4643`):

```rust
"browser.get" | "browser.screenshot" | "browser.snapshot" | "browser.wait"
| "browser.eval" | "browser.console" | "browser.errors" => {
    let _ = surface.state();          // seven no-ops
}
```

`browser.act` reads only a `driving` flag (`:4631`). `browser.navigate` handles only a URL and has no
back/forward/reload route (`:4621`). Only `browser.open` does its job — and it returns no surface id,
URL or title, and defaults a missing URL to `example.com` rather than rejecting it (`:4607`).

The reply is unconditional and is sent **before the action runs** (`main.rs:1683-1700`), and
`ControlAction::Browser` (`main.rs:2673`) is the only action variant with **no `reply` field** — every
sibling has one. So no browser method can ever return a result, by construction.

## Why this is worse than not implementing them

A documented "unsupported" error is honest: a caller learns the truth and adapts. `ok:true` for work
that never happened is a lie the caller cannot detect. Any agent driving Tiller through the socket —
which is the entire point of the control surface — will believe it took a screenshot.

This is what pass 6 recorded as "documented unsupported responses, all 10 methods exercised". That
was **accurate when written**. The code has since gained a partial implementation that answers
success, so the behaviour got *less* truthful while looking more finished.

## What to build

This is deliberately not "implement browser automation". It is: **stop claiming what is not true.**

1. **Answer unimplemented methods with an explicit failure**, carrying the method name and a reason,
   using the dispatcher's existing failure path (`ControlResponse::failure`, already used at
   `main.rs:1685`). The seven no-ops, `browser.act`'s unsupported verbs, and `browser.navigate`'s
   back/forward/reload all qualify.
2. **Stop advertising what is not implemented.** `system.capabilities` should list a browser method
   only if calling it does something. Whatever you leave advertised, a caller is entitled to rely on.
3. **`browser.open` should reject a missing URL** instead of substituting `example.com`, and should
   return the surface id, URL and title the contract asks for. This needs a `reply` channel on
   `ControlAction::Browser` — currently the one variant without one. That is the real structural
   change here, and it is what unblocks any future browser method returning data.
4. Leave `browser.open`'s tab creation working. It is the one piece that genuinely works.

**Also fix the surprise side effect:** every non-`open` browser method silently *creates* a browser
tab when none exists (`main.rs:4614-4619`). An observation method must not mutate the workspace — I
hit this myself and accidentally opened a browser tab in another agent's running app. Return a
failure saying there is no browser surface.

## Done means

1. Each of the ten methods either does its job or returns an explicit failure. No `ok:true` for a
   no-op remains.
2. `system.capabilities` and the real behaviour agree. Say in your report which methods you left
   advertised.
3. `ControlAction::Browser` carries a `reply`, and `browser.open` returns id/URL/title.
4. No browser method creates a tab as a side effect except `browser.open`.
5. **Proved live over the socket**, not by unit test — this is a transport contract. The app must be
   running; drive it with a raw client:

   ```bash
   python3 -c "
   import socket,json
   s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM); s.connect('/run/user/1000/TillerRust/control.sock')
   s.sendall((json.dumps({'id':'1','method':'browser.eval','params':{'script':'1+1'}})+'\n').encode())
   print(s.recv(65536).decode())
   "
   ```

   Paste the actual responses. `tillerctl` has no browser builder command, which is correct and
   should stay that way for anything you do not implement.
6. Report what `F-AUTO-09` and `F-CTRL-BROWSER-01`..`-06` should now read. **Do not edit
   `INVENTORY-LEDGER.md`.** Note that `F-AUTO-09`'s clause explicitly accepts "explicit unsupported
   errors" — item 1 alone should close it, which makes this the cheapest row in the queue.

---

## Answer — orchestrator, 2026-08-14 10:55

**Yes, that is the policy. Confirmed — advertise only what actually does something.** Your reading
of the ambiguity is right and your exploration found the real shape: dispatch queues with no reply,
a tab fabricated for every non-`open` method, ten methods advertised, and the tests encoding it.
Three refinements before you build, and then one process note.

**1. An unsupported method must return an explicit error, never silence and never a fabricated
success.** Dropping a method from `system.capabilities` and having it return "unknown method" is a
regression in a different direction — the caller then cannot distinguish *"this build does not do
that"* from *"you typed it wrong"*. Each unimplemented verb should fail with a reason that names
itself: not implemented on this platform / no browser surface. **The honest error is the
deliverable**, as much as the working method is.

**2. Do not treat this as deleting the feature.** The nine `F-BRW` rows are in scope and a webview
is explicitly allowed for that surface alone — this is the one place in the project where that is
true. So the target state is not "three methods forever". It is: three methods that work and are
advertised, seven that are *visibly* unimplemented. An honest error keeps the gap countable; a
silent success spends credit the row never earned. Correspondingly, do not touch any `F-BRW`
verdict — say in your report which of the nine your change makes newly true and which it exposes
as false, and a critic will move them.

**3. The tests are part of the defect, and changing them is correct here.** You found tests that
encode the false behaviour. That is mechanism (c) from `QUEUE.md` — a test that pins the stub in
place — and rewriting them to assert the honest error is not weakening the suite, it is the fix.
Say plainly in your commit which assertions you inverted and why, so the change is not mistaken
later for a test being relaxed to go green.

On `browser.act` limited to the driving flag: your call, but **state in the report what `act`
actually performs** when the flag is on. If the answer is "queues an action nothing consumes", it
belongs with the seven, not the three.

### Process note, worth more than the answer

You went idle waiting for this and lost roughly an hour of wall-clock while three other agents
worked. The standing rule for the roster: **ask the question, then keep going on everything that
does not depend on the answer.** Almost all of P90 was independent of this one policy call — the
reply channel on `ControlAction::Browser`, the dispatch split, the error type, the test rewrite.
Only the final contents of the `system.capabilities` list turned on my answer, and you could have
built it behind a single list constant and changed that one line when I replied. Ask and proceed;
block only when proceeding under either answer would be wasted work.
