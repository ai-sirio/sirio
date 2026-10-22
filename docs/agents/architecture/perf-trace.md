# Performance trace (`sirio_perf`, `sirio/src/native_perf.rs`)

`sirio_perf` is off unless `SIRIO_PERF_TRACE` names a file that does not yet
exist. Unset, `event` and `span` are a `OnceLock` read and a branch, which is
why they sit directly in hot `render` bodies. Set, one writer thread drains a
bounded channel (32768) to a TSV; a full channel drops events and reports how
many in a once-a-second `health` row named `trace.dropped` (the count rides in
the `entity` column), so **a trace is allowed to lose events and says when it
did** — read those rows before trusting a count. Instrumentation is
deliberately shallow: a `span` at the top of a `Render::render`, an `event`
where something decides to `notify`. The vendored `ely-gpui-component` takes
`sirio_perf` for the code editor's `CodeEditor.*` spans (the render and its
steps, each with the editor's entity id in the `entity` column).

**Every `name` is a content-free `&'static str`** — that is the crate's one
hard rule, pinned by a test that asserts a fixture's text never reaches the
trace (`acp_redraw_trace_names_notifications_without_recording_content`, in
`sirio_ui`'s `chat`). Never interpolate a title, a path, a prompt or scrollback
into a label; the `entity` u64 is how a row is told apart from its siblings.

Two separate switches, easily confused: the env var above turns the TSV on in
any build, while `sirio`'s `perf-native` **cargo feature** additionally compiles
`native_perf.rs`, which exports GPUI's bounded foreground journal (frames,
input, task polls) as JSON and pulls in `gpui/profiler` plus a pinned
`bezel-zed-scheduler`. A default build has no journal at all. `Scripts/perf/`
holds the harness and report scripts; `docs/perf/` holds the captures, each
recording its own commands, hashes and bounds.
