# P126 — `CONTROL_ACTION_TIMEOUT` silently truncates any longer control timeout

Found by the wave-E `E-C-2` critic while judging `F-CTRL-BROWSER-05`.

## The defect

`CONTROL_ACTION_TIMEOUT` (`rust/crates/tiller/src/main.rs:196`) is a hardcoded 5 s bound applied to
control-socket dispatch. `browser.wait` accepts a caller-supplied `timeoutMs`, but any value above
~5 s is cut short by the outer dispatch bound — and the error the caller receives is
`control action timed out`, which reads as *the waited-for condition never happened* rather than
*your timeout was ignored*.

That makes it worse than a plain limitation: a caller asking for a 30 s wait gets a failure at 5 s and
a message that misattributes the cause. Any agent driving the socket will conclude the page never
loaded.

## Scope

The bound is on the dispatch path, not on `browser.wait` specifically, so **every** control method
that legitimately takes longer than 5 s is affected the same way. `browser.wait` is simply the first
one with a caller-supplied timeout large enough to collide with it.

## Suggested shape of a fix

Either raise the dispatch bound to at least the largest caller-supplied timeout plus a margin, or —
better — make the dispatch bound derive from the request's own timeout when it carries one, so the
two cannot disagree. Whichever way, the timeout error should distinguish *the dispatch bound fired*
from *the awaited condition did not occur*, since those tell the caller to do opposite things.

## Related

`F-CTRL-BROWSER-05` and `F-CTRL-BROWSER-06` are both `half-proven` partly because of this: the critic
could not tell a real "browser child is unavailable" from a truncated wait. See also
`P127-browser-child-unavailable.md` for the separate, larger question of whether the webview child
starts at all on this box.
