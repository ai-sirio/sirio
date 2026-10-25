# File paths in a chat response open in the editor

**Date:** 2026-10-24
**Status:** design

## §0 Intent

An agent finished a turn by telling the user where it had put a file: *"l'ho messo in
`/docs/NOME-FILE`"*. The user could read the path but could not click it. To see the
file they had to find it by hand in the Files panel.

What the user wants: **a path to a file, written by the agent in its response, is
clickable, and a click opens that file in Sirio's editor**. A path that carries a
line (`src/main.rs:42`) opens the file at that line.

Success is that moment going away. The agent says where the file is, the user clicks,
and the file is in front of them in a file tab.

## §1 Decided with the user

In the order they were settled:

1. **Only paths that exist.** A path becomes a link only if it names a file that
   exists on disk. A wrong, invented or not-yet-created path stays plain text, so
   nothing in the transcript looks clickable and then does nothing.
2. **Any inline form.** Plain prose, `inline code`, and a Markdown link
   `[text](path)` all count.
3. **Line and column.** `path:42` opens the file with the caret on line 42.
   `path:42:7` also places it on column 7.
4. **Not inside fenced code blocks.** Multi-line code blocks (command output, file
   lists) stay exactly as they are today. Prose and inline code are enough.
5. **The play-back below was approved as a whole** ("va bene tutto"). That includes
   the calls in §2, which were the designer's, presented to the user and not
   objected to.

## §2 Behaviour

### Where links appear

- In **assistant responses** only: every `Entry::Assistant` in the transcript, live or
  restored, whatever the transport (ACP or the native Claude transport). *(designer's
  call, approved in play-back)*
- Not in the user's own messages, not in thought/reasoning blocks, and not in tool
  cards. Tool cards already have their own "open" control (`ChatEvent::OpenFile` in
  `chat/tool_calls.rs`). *(designer's call, approved)*
- Not inside fenced code blocks (§1.4).
- **Not in a Chat History resume's flattened transcript.** `resume_chat` in
  `main.rs` rebuilds a chat through `restore_transcript(transcript_for_resume())`.
  That joins every entry's plain text (user prompts, thoughts and tool text
  included) into one `Entry::Assistant`, and drops fences, backticks and link
  targets. Linking it would break the rules above, so that entry is never linked.
  The database reload path (`restored_entry`) keeps each response's Markdown source
  and is linked normally. *(designer's call, from the builder check)*

### What counts as a path

A candidate is a token in a prose run or an inline-code span. For example:

- `docs/NOME-FILE.md`, `./src/lib.rs`, `src/main.rs:42`, `src/main.rs:42:7`
- `/docs/NOME-FILE` (see resolution, below)
- `/home/me/project/README.md` (an absolute path that exists)
- `README.md` (a bare file name, resolved against the worktree)

Surrounding punctuation is not part of the path: quotes, brackets, parentheses and a
trailing `.`, `,`, `;`, `:` or `!` are trimmed before resolution. So `(see
docs/x.md).` links `docs/x.md`. In inline code the whole span is the candidate, after
trimming whitespace.

Further shapes, all *designer's calls from the builder check*:

- `~/…` expands to the user's home directory before resolution, because agents
  often write `~/Downloads/x.pdf`.
- `path:42-50` (a range) and `path#L42` (a GitHub-style anchor) open at line 42.
- In prose, a path containing spaces is not detected, because the token boundary is
  whitespace. In inline code it is, because the whole span is the candidate.
- `../sibling/x` stays plain text: the resolver already rejects relative paths that
  climb out of the worktree.

The existence check is the real gate, so the tokenizer can be generous. The builder
decides the exact token rules (§5). Two things it must not do. It must not turn a
URL (`https://…`, `mailto:`) into a file link. And a candidate that does not exist
must cost nothing visible.

### Resolution

The base is the chat's working directory (`Chat::agent_cwd`), which is the
worktree the agent runs in.

1. Split an optional `:line[:column]` suffix. `sirio_project::resolve_file_link`
   already does this, and also handles `file://` URLs and rejects relative paths that
   climb out with `..`.
2. A relative path is joined to the base.
3. An absolute path is tried as written. **If nothing exists there**, it is tried
   again relative to the base, with the leading separator dropped. That is the
   user's `/docs/NOME-FILE` case: the agent meant "docs/ in the project", not the
   filesystem root. *(designer's call, approved)*
4. The result is a link only if it is an existing **regular file**. Directories are
   not linked in this version. *(designer's call, approved)*

### Look and click

- A file link looks like the transcript's existing links: link colour and the
  pointer cursor on hover. An inline-code path keeps its code styling and is also a
  link. *(designer's call, approved)*
- A click is a press and a release on the same spot, the same rule the
  transcript's links follow today. A drag that starts on a path selects text and
  opens nothing. The text stays selectable and copyable exactly as now (see
  `docs/agents/architecture/text-selection.md`).
- A click opens the file in a file tab through the same path the Files panel and
  the tool cards use (`Workspace::add_file_tab`). If the file is already open, that
  tab is selected instead of a duplicate being opened. When there is a line, the
  file view goes to it (`FileView::go_to_line`, plus the column when given).
- **Markdown files and a line.** A Markdown file opens in Preview by default
  (`initial_markdown_mode`), and `go_to_line` refuses in Preview ("go to line needs
  the code"). So:
  - a `.md` path **without** a line opens as any Markdown file does today
    (Preview);
  - a `.md` path **with** a line switches that tab, new or reused, to the source
    view and goes to the line. The agent pointed at a line, and the line only
    exists in the source. *(designer's call, from the builder check)*
- A freshly opened tab also refuses `go_to_line` until its async load finishes
  (`item_refusal`). The line must be applied once the load completes, not dropped.
- Cmd/Ctrl+Shift+click, which sends a web link to the system browser (F-BRW-09),
  does **not** apply to a file link. A file always goes to the editor.
  *(designer's call, approved)*
- A Markdown link whose target resolves to an existing file now opens in the
  editor and no longer in the internal browser tab. Web URLs keep opening in the
  browser exactly as today.

### When the check runs

- Existence is checked when a response's document is (re)built, not on every
  frame. Scrolling a long transcript must not touch the disk. *(designer's call,
  approved)*
- While a response streams, the document is re-parsed on every chunk
  (`AcpEvent::AgentMessageChunk` in `chat/mod.rs`). The builder must keep this
  cheap. For example, keep a per-entry memo from candidate string to resolution, so
  each distinct candidate is checked once per entry, not once per chunk.
- Accepted limitation: a file named in a finished response that only comes into
  existence later stays unlinked until that response's document is rebuilt (for
  example, on restore). The user was told and accepted this. *(designer's call,
  approved)*
- If a linked file is deleted afterwards, a click on it does nothing harmful. It
  either does nothing or shows the editor's normal missing-file state. The builder
  picks the one that is already the norm for a vanished file passed to
  `add_file_tab`.

## §3 Goals, non-goals, anti-goals

**Goals:** clicking an agent-named file opens it, at the line when one is given.

**Non-goals (this version):**
- paths in fenced code blocks, user messages, thoughts, tool cards;
- directories (revealing them in the Files panel);
- linking files that do not exist yet, or re-checking finished responses
  live;
- the terminal pane (agents running in a terminal tab). This is chat only.

**Anti-goals, failures even if it technically works:**
- something underlined or hand-cursored that does nothing on click;
- a URL, a version number (`1.2.3`), a date (`2026/10/24`) or a ratio turned into a
  "file" link. The existence gate is what prevents this, so it must never be
  skipped;
- breaking selection or Ctrl+C on the transcript, or a drag that opens a file;
- a visible stutter while streaming or scrolling, caused by filesystem calls on
  the render path;
- a path or response text appearing in a `sirio_perf` label (CLAUDE.md: labels are
  content-free `&'static str`).

## §4 Where it lives today (orientation for the builder)

- Assistant responses: `Entry::Assistant { text, document }` in
  `rust/crates/sirio_ui/src/chat/mod.rs`. `document` comes from
  `parse_chat_markdown` (bezel_markdown `Doc`) and is rebuilt on each chunk.
  Inline code is `markdown::Mark::Code` and links are `markdown::Mark::Link(target)`
  spans over `Text::text` (`rust/vendor/bezel_markdown/src/doc.rs`).
- Rendering and hit-testing: `MarkdownBody` / `MarkdownBody::selectable` in
  `chat/mod.rs`. bezel paints a response's links as `InteractiveText`, whose click
  calls `cx.open_url(url)`. So today a link click in a response goes to the system
  opener. It does **not** go through `ChatEvent::OpenLink`, which only
  `TranscriptSelectableText` emits, and both of its call sites pass no links. "Web
  links exactly as today" means this system-opener route stays.
- Workspace side: `Workspace::bind_chat` in `rust/crates/sirio/src/main.rs` maps
  `ChatEvent::OpenFile(path)` to `add_file_tab` and `ChatEvent::OpenLink(url)` to
  `WorkspaceAction::OpenBrowserLink`.
- Resolver: `sirio_project::resolve_file_link` (`rust/crates/sirio_project/src/file_link.rs`),
  already used by the Markdown Preview's link clicks (`FileView::open_markdown_link`).
- Go to line: `FileView::go_to_line` (`rust/crates/sirio_ui/src/file_view/status_actions.rs`).
  `open_change_request_file` in `main.rs` is prior art for "open a file tab, then
  move to a line".

## §5 Left to the builder

- The exact tokenizer for prose candidates (§2 "What counts as a path"), within the
  anti-goals.
- How a file link is represented in the `Doc`. A `Mark::Link` with a resolved
  target is the obvious route, but bezel must then paint a link mark that overlaps a
  code mark correctly. And how the click is told apart from a web link: resolve at
  build time and carry the resolved target, or extend `ChatEvent` with a variant
  that carries path, line and column. `ChatEvent::OpenFile(PathBuf)` today has no
  line.
- How `add_file_tab` gains a line/column (a sibling function or an extra
  parameter), and the column handling, given `go_to_line` takes a line only.
- The streaming memo (§2 "When the check runs").
- Windows shape of the `/docs/x` fallback (`Path::is_absolute` differs there).

## §6 Proof

The repo's rule is end to end (CLAUDE.md, Conventions). The feature is proven against
a real, isolated Sirio by extending the existing chat E2E harness
(`Scripts/Tests/test-chat-session-panel-e2e.sh` or `Scripts/Tests/test-ely-chat-ui-e2e.py`,
see `docs/agents/e2e-tests.md`). A stub agent answers with a response that
contains, at minimum:

- a worktree-relative path in prose, an inline-code path, and `path:LINE`;
- the `/docs/NAME` shape (leading slash, file inside the worktree);
- a path that does not exist, a URL, a version number and a fenced block holding
  an existing path.

Then a real click on each candidate. The first group must open a file tab, at the
line for `path:LINE`, with the tab reused on a second click. The second group must
open nothing and be painted as plain text, and the URL must still route to the
browser. A drag across a linked path must select without opening. The run ends in
the harness's usual re-runnable artifact and success line. Any `#[test]` added for
the tokenizer or the resolver fallback follows the repo rule: the ways it can fail
are written first, then the code.

Platforms: developed and proven on Linux. macOS (the release platform) and Windows
are checked by the PR workflows. The Windows path shape (§5) needs an explicit look.
