# Language server support for the file editor — design

**Date:** 2026-09-14
**Status:** approved, not yet implemented
**Scope of this document:** the whole feature's shape, and sub-project 1
(`sirio_lsp`) in implementable detail. Sub-projects 2–5 are sketched here
and get their own specs when they start.

## Goal

Let a reader of code in Sirio's file editor ask the questions a reader
asks — what is this, where is it defined, where else is it used, what is
in this file — by talking to the language server the project already has.

## Why this is not "use Zed's editor"

The request that started this was "the same editor with LSP that Zed
has". That editor is not obtainable:

- Zed does not publish its `editor` crate. The crate named `editor` on
  crates.io belongs to an unrelated project (`blockglow/techkit`). Zed's
  lives in its own workspace and depends on `project`, `multi_buffer`,
  `language`, `lsp`, `settings` and `workspace` — taking it means taking
  most of Zed.
- bezel has no code editor either. Its catalogue is a markdown block
  model and an editor over it, a terminal, ui/theme/motion/agent/icons,
  and `bezel-syntax` — which classifies *isolated code blocks*, the
  fenced blocks inside a markdown document. No buffer, no rope, no LSP.

What Sirio does have is its own code surface, hand-written in
`sirio_ui/src/file_view.rs`: a virtualized list where each row is a
gutter plus an `EditableLine`, and — the part that matters here — **every
row already carries its own `start..end` byte range in the buffer**. That
is the hard prerequisite for LSP position mapping, and it is why this is
an addition rather than a rewrite.

## Decisions

Each of these was a fork in the road; recording the choice with its
reason is the point of this section.

| Decision | Choice | Why |
|---|---|---|
| Purpose | Read and navigate, not author | Editing stays occasional, which is what makes full-text document sync sufficient (see §4) and per-file re-highlighting affordable (sub-project 5). |
| Language coverage | User-configurable table | Sirio runs agents across many languages; a fixed list would be wrong for most users. Ships with defaults so it works before anyone edits anything. |
| Configuration home | `~/.config/sirio/languages.toml` | Helix's model. Sirio has no user config file today — this is the first. Chosen over a Settings UI because the UI work is the largest and least interesting slice of the project. |
| Server lifetime | Lazy start, live until Sirio quits | Navigation stays instant after the first index. The cost is monotonic memory growth across worktrees, accepted deliberately. |
| Capabilities | Hover, go-to-definition, diagnostics, references, document symbols | All four of the read-and-navigate surface. |
| Protocol stack | `lsp-types` + hand-rolled framing on gpui's executor | See §2 and the rejected alternatives below. |

### Rejected: `async-lsp`

It supplies request correlation, cancellation and concurrency limits
already. It is built on tokio's `AsyncRead`/`AsyncWrite` and a tower
service stack, while Sirio's async world is `futures` / `async-io` /
`async-process` on gpui's own executor. Bridging means either an ambient
tokio runtime or adapters that cost more than the framing code they
save — and an ambient runtime is precisely the failure this repo debugged
on 2026-09-14, when a portal query reached D-Bus on the `blocking`
crate's threadpool and gpui's deterministic test scheduler panicked with
"Your test is not deterministic". See `sirio_theme::looks_like_test_harness`.

### Rejected: a separate broker process

One extra binary owning every server would isolate crashes and memory.
It would also be a fourth artifact to build, sign, notarize and ship on
three platforms, with its own update path. Disproportionate.

## Decomposition

Five sub-projects. Each ends with software that works on its own.

1. **`sirio_lsp`** — protocol, transport, lifecycle, position mapping. A
   leaf crate with no UI. *Done when* a test talks to a fake server and
   gets a hover back.
2. **Configuration and supervision** — `languages.toml` with defaults and
   hot reload; a registry of servers keyed by (worktree, language), lazy
   start, shutdown on `cx.on_app_quit`. *Done when* opening a `.rs` file
   actually starts rust-analyzer.
3. **Surfaces needing no new UI** — hover, go-to-definition, diagnostics
   in the gutter.
4. **Surfaces needing new UI** — a references list and a document symbol
   outline.
5. **Syntax colour fidelity** — independent of LSP, deliberately
   sequenced last (see below).

### Sub-project 5, recorded now because it is already diagnosed

Sirio paints three colours where it already holds twenty-four. In
`file_view.rs`, `code_spans` maps `bezel::theme::HighlightKind` (24
variants, one per colour in `SyntaxPalette`) onto a local `CodeSpanKind`
with three (`Keyword`, `Literal`, `Comment`) and discards the rest with
`_ => return None` — types, functions, macros, properties, parameters,
operators, constructors, attributes, all painted as plain text. Two
further losses compound it: `syntax::highlight` is called **per line**,
so tree-sitter never sees the file's structure; and `bezel_syntax_tag`
covers 8 of the 24 languages `Language::ALL` recognises.

None of that needs a language server. It is sequenced after the client
anyway, so that LSP `textDocument/semanticTokens` — type-aware colouring,
the other half of what Zed shows — can be folded into the same work
instead of requiring a third pass over the same code.

## Sub-project 1: `sirio_lsp`

### §1 Placement and boundaries

A leaf crate: `sirio_perf` for instrumentation, plus `lsp-types`,
`async-process`, `async-channel`, `futures`, `serde_json`. No local
dependencies beyond `sirio_perf`, the same shape as `sirio_git` and
`sirio_agents`.

Who consumes it is the load-bearing decision:

- **`sirio_ui`** takes only the *types* (`Hover`, `Diagnostic`,
  `Location`, `DocumentSymbol`) because it must draw them. There is an
  exact precedent: `sirio_ui` already depends on `sirio_acp` this way.
- **`sirio`/`main.rs`** is the only crate that constructs and owns
  clients. This is the repo's standing rule, not a preference:
  `sirio_ui` touches neither `sirio_terminal` nor `sirio_control` nor
  `sirio_activity`; processes are wired in `main.rs`.

The seam already exists. The file context menu (#478) established
*event up, setter down*: `FileView` emits a `FileViewEvent`, `main.rs`
handles it and returns the answer through a setter such as
`set_shell_facts`. LSP navigation uses the same seam:

```rust
// existing, from #478
FileViewEvent::OpenFile(PathBuf),
FileViewEvent::ViewFileHistory(PathBuf),

// added
FileViewEvent::GoToDefinition { path: PathBuf, offset: usize },
FileViewEvent::Hover         { path: PathBuf, offset: usize },
```

`offset` is a **byte offset**, never an LSP `Position`. The conversion
belongs to `sirio_lsp` (§4), so `sirio_ui` never learns UTF-16.

### §2 Transport and correlation

The only delta from `sirio_acp`, which is otherwise the same thing: a
spawned child, JSON-RPC over stdio, stderr read separately, no tokio.
ACP is line-delimited; LSP frames with a header:

```
Content-Length: 245\r\n
\r\n
{"jsonrpc":"2.0","id":1,"result":{…}}
```

Headers are read line by line to the blank line, then **exactly N
bytes**. Not `.lines()`: LSP permits raw newlines inside the JSON
payload, and a hover's markdown documentation is exactly where they
appear — a line reader would work in trivial tests and break on the
first useful hover.

Incoming messages take three shapes, and the middle one is the one first
implementations forget:

| Shape | Is | Action |
|---|---|---|
| `id` + `result`/`error` | a response | resolve the registered oneshot |
| `method` **+ `id`** | a **server→client request** | **must be answered** |
| `method`, no `id` | a notification | `publishDiagnostics`, `logMessage`, `$/progress` |

rust-analyzer sends `workspace/configuration`,
`client/registerCapability` and `window/workDoneProgress/create`. Treat
those as notifications and the server waits forever, silently, and
navigation simply never arrives — a failure with no symptom.

Correlation is a `HashMap<RequestId, oneshot::Sender<…>>` with a
monotonic id. One task per server reads stdout, on **gpui's** executor.

### §3 Lifecycle

```
spawn (cwd = worktree root)
  → initialize      (request)       rootUri, our capabilities, processId
  ← InitializeResult                its capabilities
  → initialized     (notification)
  … work …
  → shutdown        (request)       and wait for the response
  → exit            (notification)
  wait for process exit, kill after a timeout
```

**Negotiated capabilities drive the UI, not assumptions.**
`InitializeResult` states whether the server offers `hoverProvider`,
`definitionProvider`, `referencesProvider`, `documentSymbolProvider`. A
missing one means the corresponding entry is absent or explained — never
present and inert. Same principle as the context menu's
`disabled_reason`.

**Shutdown is two-step and has a deadline.** Because servers live as long
as the app, `cx.on_app_quit` must run `shutdown` → `exit` → wait → kill,
mirroring what `PaneRegistry::shutdown` does for panes. Without the final
kill, a server that ignores `exit` keeps indexing after Sirio is gone.

**There is a third state the UI must know about.** rust-analyzer indexes
for minutes before it is useful and reports progress over `$/progress`.
Requests during that window come back empty or with `ContentModified`.
"Still indexing" must be distinguishable from "no definition found".

### §4 Document sync and position mapping

Read-and-navigate makes the simplest correct sync sufficient:

| Event | Message |
|---|---|
| file tab opened | `didOpen`, full text |
| edit | `didChange`, **full text**, debounced |
| save | `didSave` |
| tab closed | `didClose` |

No incremental sync: it is the protocol's most error-prone corner and
nothing here needs it. The server's declared `textDocumentSync` is still
honoured; in practice every server accepts `Full`.

A `LineIndex` inside `sirio_lsp` converts byte offset ↔
`Position { line, character }`, where `character` counts **UTF-16 code
units**. `FileView` already holds each line's `start..end`, so the input
is there.

### §5 Error handling

The house rule applies: refusing loudly beats guessing. `sirio_apply`
already prefers to fail over inventing an install path.

| Failure | Behaviour |
|---|---|
| binary not found | **silence**; the pair is marked dead so it is tried once. Revised 2026-09-16: the original rule was a visible notice naming the command, which was right while the shipped table held four servers — a missing one then meant a typo in `languages.toml`. The table now names one server per language the editor can open, nineteen of them, and nobody has all nineteen, so the notice would appear on nearly every file and drown the row below it. `LspError::NotInstalled` carries the command for any caller that does want to name it |
| server crashed | that (worktree, language) pair is marked dead; **no automatic restart loop**; restart is offered |
| malformed frame | the stream is desynchronised and cannot be recovered mid-stream: terminate that server and say so |
| request unanswered | a per-request timeout answers "no response" instead of hanging the UI |
| `ContentModified` | normal while indexing: retry silently |

No automatic restart is deliberate. A server that crashes on a given file
crashes again on restart, and a restart loop on rust-analyzer means
re-indexing the repository forever.

### §6 Testing

The protocol layer takes an `AsyncRead`/`AsyncWrite` pair, so **most
tests spawn no process at all**: in-memory pipes and a fake responder.
Where a real process is needed, shell-script servers as in `sirio_acp`'s
fixtures, plus the `Content-Length` header.

`LineIndex` is tested pure, without async, on the cases that break
everyone: ASCII, accented characters (2 UTF-8 bytes → 1 UTF-16 unit),
emoji (4 bytes → **2** units, a surrogate pair), and CRLF against LF.

**`Scripts/ci.sh` must never need rust-analyzer.** A test that downloads
a language server and waits for indexing would be machine-dependent, and
this repo deleted all six of its machine-dependent tests on 2026-09-14
for exactly that reason.

## Global constraints

- Async runs on gpui's executor. **No tokio, ambient or otherwise.**
- `sirio_ui` never spawns a process; `main.rs` owns every client.
- `sirio_ui` never sees an LSP `Position`; it speaks byte offsets.
- Every `sirio_perf` trace name stays a content-free `&'static str`.
- `lsp-types` is pinned exactly, like every other dependency here.
- Conventional Commits; the workspace version moves once per cycle.
