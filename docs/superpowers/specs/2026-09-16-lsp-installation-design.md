# Language servers Sirio can install

**Date:** 2026-09-16
**Status:** designed, not implemented
**Follows:** `2026-09-16-language-coverage-design.md`, which gave every
language a named server, and `2026-09-14-lsp-client-design.md`, which built
the client underneath.

## §0 The problem this exists for

0.18.0 shipped a table naming a server for every language the editor opens.
Opening a `.java` file on the maintainer's own machine then produced this,
on every token:

> No language server offers references here

The sentence was true and useless. `jdtls` was not on that machine — nor was
any JVM — and the message describes *Sirio* rather than the machine, in the
release whose entire subject was giving Java a server.

A differential probe settled the cause before any code was read. Same file,
same position, same binary; the only variable is whether a program called
`jdtls` exists on `PATH`:

```
=== PATH as it was ===            === with a jdtls that exists ===
launch    NOT INSTALLED `jdtls`   launch                OK
references_available  false       references_available  true
                                  references            1 target(s)
```

The plumbing was correct. What was missing was any path from "you cannot do
this" to "here is how you could". That path is what this document designs.

The immediate half — making the sentence true, `jdtls is not on PATH` — is a
separate, smaller change that precedes this one. This document is about the
half that removes the dead end entirely.

## §1 What Zed does, and where we follow it

Read from `zed-industries/zed` at the time of writing.

**Zed downloads its own servers.** `LspInstaller`
(`crates/language/src/language.rs:699`) resolves a binary in this order
(`language.rs:802-890`):

1. `check_if_user_installed` — the user's own binary on `PATH` wins
2. an in-memory cache
3. a previously downloaded binary on disk
4. `try_fetch_server_binary` — download from GitHub releases

The comment explaining why the user's binary is never cached is the same
reason it must win here:

```
//      worktree 1: user-installed at `.bin/gopls`
//      worktree 2: user-installed at `~/bin/gopls`
//      worktree 3: no gopls found in PATH -> fallback to Zed installation
```

**But only for thirteen languages.** `crates/languages/src/` holds bash, c,
cpp, css, eslint, go, json, python, rust, tailwind, typescript, vtsls and
yaml. No java, kotlin, php, ruby, lua, zig, swift, sql or xml. Each of the
thirteen is a hand-written adapter — `c.rs` is 731 lines for clangd alone,
matching GitHub asset names and checking the architecture. That cost is why
the list stops at thirteen.

**Everything else is an extension, and Zed offers to install it.**
`crates/extensions_ui/src/extension_suggest.rs:52` carries
`("java", &["java"])`; opening a `.java` file raises

> Do you want to install the recommended 'java' extension for 'java' files?
> [Install] [Don't show again]

**Server state lives in the status bar.** `BinaryStatus { None,
CheckingForUpdate, Downloading, Starting, Stopping, Stopped, Failed }`
reaches an activity indicator that says `Downloading clangd...`,
`Checking for updates to clangd...`, `Failed to run clangd. Click to show
error.`

**The context menu never disables the LSP entries.**
`crates/editor/src/mouse_context_menu.rs:259` has `Go to Definition` and
`Find All References` as bare `.action(...)`. Zed *has*
`action_disabled_when` and uses it in the same menu — for `!has_git_repo` on
`Copy Permalink to Line`, for `!has_reveal_target` on `Open in Terminal` —
the same facts Sirio's menu uses. It simply does not apply it to the LSP
rows.

**An empty answer is silence.** `crates/editor/src/navigation.rs:1234`:

```rust
if locations.is_empty() {
    // totally normal - the cursor may be on something which is not
    // a symbol (e.g. a keyword)
    log::info!("no references found under cursor");
    return Ok(());
}
```

and `go_to_definition` falls back to `find_all_references` by default.

### What we take, and what we do not

| Zed | Sirio | why |
|---|---|---|
| downloads servers | yes | it is the thing that removes the dead end |
| one hand-written adapter per server | **no** — one table row | 731 lines per server is why Zed stops at thirteen |
| downloads without asking | **no** — offers, installs on a click | Sirio already decided this for ACP agents; see §3 |
| status in the status bar | **no** — card plus a settings page | the status bar has a written rule against reasons; see §5 |
| menu entries always enabled, silent | **no** — a third state | see §6 |
| extension marketplace | **no** | 21 compiled-in rows need no plugin format |
| `CheckingForUpdate` on every launch | **no** | a pinned version is the point; see §7 |
| per-directory shell environment | **no** | real gap, different problem |

Sirio already has Zed's `load_login_shell_environment`: `adopt_login_shell_path`
(`sirio/src/main.rs:18458`), written for agent discovery, covers the LSP spawn
for free because both resolve through the process environment.

## §2 Three corrections to the record

Found while grounding this design, and fixed as part of it.

**The default table has twenty-one entries, not twenty-two or nineteen.**
Four places claim otherwise: this spec's predecessor at lines 88 and 129
("twenty-two"), `sirio_lsp/src/config.rs:76` ("nineteen"),
`sirio/src/main.rs:19662` ("from four to nineteen"), and
`sirio/src/lsp.rs:171` ("twenty-two"). Twenty-three is the count of
*languages*; the entries are twenty-one because clangd covers two and
typescript covers six. A test pins the number so the prose cannot drift
again.

**`Distribution::Uvx` parses but does not install.** `Installer::install`
(`sirio_registry/src/installer.rs:185`) dispatches on `Binary` then `Npx`;
everything else, uvx included, returns `UnsupportedDistribution`. The
installable surface today is npm and per-platform binaries.

**`unpack_kind` mistakes a plain `.gz` for a bare executable.**
`installer.rs:77` recognises `.zip`, `.tar.gz` and `.tgz`, names `.tar.bz2`
and `.tar.xz` as unsupported, and falls through to `BareExecutable` for
everything else. A single-member gzip is not a tar, so
`rust-analyzer-x86_64-unknown-linux-gnu.gz` would install as a gzip file
marked executable, failing at exec with nothing to read. Latent today — no
ACP agent publishes `.gz` — and immediate here, on two of the most likely
servers. `UnpackKind` gains a `Gz` variant.

## §3 What is installable, measured

Every row below was checked against the project's real latest release, not
estimated.

| outcome | count | who |
|---|---|---|
| npm | 8 | typescript-language-server, pyright, intelephense, yaml-language-server, bash-language-server, and json + html + css |
| binary, format already supported | 5 | clangd `.zip`, lua-language-server `.tar.gz`, marksman (bare executable), sqls `.zip`, lemminx `.zip` |
| binary, needs a format added | 3 | rust-analyzer `.gz`, taplo `.gz`, zls `.tar.xz` |
| `Manual` — a toolchain must come first | 5 | gopls (Go), ruby-lsp (Ruby), jdtls (a JVM), kotlin-language-server (a JVM), sourcekit-lsp (Xcode) |

Sixteen of twenty-one installable; five honest about why not. Zed installs
thirteen and refers the rest to extensions.

## §4 The data

`LanguageEntry` gains one field, invisible to the TOML:

```rust
#[serde(skip)]
pub install: Option<Recipe>,
```

`Recipe` is plain data in `sirio_lsp`, with no dependency on
`sirio_registry` — both stay leaves:

```rust
pub enum Recipe {
    /// One npm package at one version. `bin` names which executable inside
    /// it, because three languages share `vscode-langservers-extracted`.
    Npm { package: &'static str, version: &'static str, bin: &'static str },
    /// A pinned release asset. See §7 for why version, hash and size are
    /// written here rather than queried.
    Release { url: &'static str, sha256: &'static str, bytes: u64, bin: &'static str },
    /// Why it cannot be installed, in terms a reader can act on.
    Manual { needs: &'static str, url: &'static str },
}
```

`Manual` is the arm Zed has no equivalent for. Where Zed says nothing, Sirio
says what has to come first.

Two consequences fall out of `#[serde(skip)]` rather than being designed:

- **A user's own entry carries no recipe**, and that is the right answer: if
  you point Sirio at your own `jdtls`, it must not offer to download another
  one underneath it.
- **The recipe cannot be written from `languages.toml`.** Not a trust
  boundary — that file can already name any command, which is already
  arbitrary execution — but a statement about whose knowledge this is. Which
  npm package is the right one is Sirio's to know, not the reader's to
  configure.

## §5 Resolution

The store is a sibling, not a change. `InstallStore::default_root` ends in
`…/sirio/agents`; servers go to `…/sirio/language-servers`, through the same
`InstallStore::new(root)`, with the same `<root>/<id>/<version>/` layout and
the same atomic manifest rename. No line of `sirio_registry`'s store changes.

**The `id` is the package or the repository, never the language.** That is
what makes json, html and css share one installation of
`vscode-langservers-extracted` instead of fetching it three times.

The ladder:

```
1. spawn by name             → PATH wins, always
2. a manifest in the store   → what Sirio installed, launched by absolute path
3. Recipe::Npm | Release     → absent: offer to install it
4. Recipe::Manual            → impossible: say what is needed first
5. no entry at all           → silence, unchanged
```

Step 1 needs no `which`. `Server::launch` already tells "there is no such
program" apart from "it ran and broke" — that is `LspError::NotInstalled`,
raised from the spawn's `ErrorKind::NotFound`. So step 1 *is* the launch and
the `NotFound` is its answer; step 2 is consulted only afterwards. Same
precedence as Zed, one fewer platform-specific `which` to write — and `which`
is exactly the piece that turns unpleasant on Windows, between `PATHEXT` and
implicit extensions. The cost is one failed fork+exec per key per session,
which the supervisor already remembers.

An installed binary is launched **by absolute path**, never by adding its
directory to the child's `PATH`. This is what Zed does
(`LanguageServerBinary { path, arguments, env }`) and it keeps installed
servers from seeing each other or displacing the reader's own choice.

### Dead, and the one way back

`mark_dead` is final for the session, and rightly: a server that crashes on a
file crashes again on restart, and on rust-analyzer that means re-indexing
the repository forever. But a successful install must be able to revive a
key — otherwise you install, nothing happens, and a thing that worked looks
broken.

`lsp::Dead` becomes the home of the whole ladder:

```rust
pub enum Dead {
    Installable { recipe: Recipe },   // step 3 — an offer
    Manual { needs, url },            // step 4 — an explanation
    Failed,                           // launched and broke — still final
}
```

**Exactly one event removes a key from `dead`: an install that succeeded.**
Not a timer, not another tab opening, not a refresh. Nothing removes
`Failed`, because reinstalling is not the cure for a crash.

When an install finishes, the key leaves `dead`, the server starts by the
ordinary path, and the menu facts are pushed again — that mechanism already
exists, added to the `NotInstalled` arm for the opposite reason.

## §6 The three surfaces

### The card, which grows actions

`TransientMessage` is `{ text, at }` and its contract is "a click dismisses
the card". It becomes `{ text, at, actions: Vec<MessageAction> }`: a click on
the body still dismisses, a click on an action does the action.

```
┌─ installable ───────────────────────┐   ┌─ Manual ────────────────────────────┐
│ clangd is not on PATH.              │   │ jdtls needs a JVM.                  │
│   [Install — 114 MB] [Don't ask]    │   │   [How to] [Don't ask again]        │
└─────────────────────────────────────┘   └─────────────────────────────────────┘
```

`at: None` — it settles in the corner. It answers no gesture; it is about the
file as a whole, which is the distinction `at` exists to draw.

**How often it appears** is what decides whether this is a good idea or the
return of the bug 0.18.0 closed:

- **Once per language per session**, always. Never once per file — ten
  `.java` files must not be ten cards, which is precisely what was banned.
- **Never again** once "Don't ask again" is pressed. It persists.

0.18.0's rule — *no card, ever, for a command that is not installed* — stands
unchanged for **error cards**. This is not an error arriving uninvited: it is
an offer, once, carrying its own way to end. Written in those terms so the
next reader does not mistake it for a convenient exception.

### Where "don't ask again" lives

`AppSettings` (`sirio_persistence/src/model.rs:449`) is flat: one scalar per
dotted key. So: one field, `lsp_silenced_languages: String`, key
`"lsp.silencedLanguages"`, holding a JSON array. Not twenty-one booleans, and
not a new table with its own migration — for a set bounded at twenty-one
short names the migration costs more than it returns. `session.rs` already
serialises stored values with `serde_json::to_string`.

### The menu, and a contract that has to split

`FileContextItem.disabled_reason: Option<String>` carries two facts in one —
*why*, and *clickable or not* — and the render site depends on the conflation:

```rust
let is_disabled = disabled_reason.is_some();
```

An entry that shows its reason **and** is clickable, on a different action,
cannot be expressed. The field becomes an explicit state:

```rust
pub enum ItemState {
    Ready,
    /// Present with its reason, no click handler. Today's contract,
    /// unchanged: outside a git repository, "Copy Permalink" stays this.
    Unavailable(String),
    /// Present with a note, and the click does something *else*: installing
    /// what is missing rather than the action it is labelled with.
    Redirected { note: String, to: FileContextAction },
}
```

`Go to Definition` with no server becomes
`Redirected { note: "clangd is not on PATH", to: InstallLanguageServer }`.
`Copy Permalink to Line` outside a repository stays `Unavailable` — the old
contract survives where it was right, rather than being weakened for
everything.

This is deliberately further than Zed goes. Zed needs no third state because
its install offer already caught you at file-open — but that offer has a
"Don't show again", and after pressing it Zed has nothing left to say. The
third state is what makes silencing reversible without going to look for it.

### The settings page

Modelled on `agents_page.rs`, which already has what is needed: one row per
item, `InstallState::{InFlight, Failed(String)}` for live state, the row
unclickable while the installer holds its lock, and the installer's own error
text shown verbatim — *"it names the format, the package or the remedy"*.

Twenty-one rows: language, command, state (`on PATH` / `installed by Sirio
v…` / `absent`), action. And the one place a silenced language is unsilenced.

## §7 Versions are pinned

Zed queries the GitHub API on every install to learn the latest version and
its digest. This design does the opposite: **every recipe names an exact
version at the time Sirio is compiled** — for a release asset that means URL,
sha256 and size as well.

```rust
Recipe::Release {
    url: "https://github.com/clangd/clangd/releases/download/22.1.6/clangd-linux-22.1.6.zip",
    sha256: "a9c77443af2e447ed467e84771848d3a6ac1c56f84bcfcde717e66318de77cfa",
    bytes: 114_790_601,
    bin: "clangd_22.1.6/bin/clangd",
}
```

Three consequences:

- **No call to `api.github.com`.** Only the download. No 60/hour rate limit,
  no network step that can fail before anything starts.
- **Every binary install is verified.** Not "verified when upstream
  remembered to publish a hash" — always. Better than the ACP path, where 47
  of 95 artifacts publish nothing and `Integrity::None` is an ordinary
  outcome.
- **The size is known before the click.** And it is needed: clangd is
  **114 MB**. An `[Install]` button that does not say so is a dishonest
  button.

The cost is that updating clangd takes a Sirio release. Consistent with
compiling the recipes in at all, and with the fifteen tree-sitter grammars
0.18.0 compiled in for the same reason. A reader who wants a newer version
already has the better route: install it and win at step 1.

An npm recipe pins the same way, through the package spec `npm install`
already accepts: `npm_install_argv` (`installer.rs:499`) passes `package`
straight through, so `typescript-language-server@4.3.3` needs no new
argument. It also passes `--ignore-scripts` unless told otherwise, which is
the right default and is worth re-checking per recipe: a server whose
postinstall step fetches its real payload will install empty and silent
otherwise.

### Node, for eight of the sixteen

The npm recipes run through `install_npx`, which shells out to `npm`. Without
Node they fail — *after* the click, which is the worst moment to find out. So
an npm recipe checks for `npm` before offering: with no `npm` on `PATH`, the
card says what the `Manual` arm says — *"typescript-language-server needs
Node."* An offer that cannot be kept is not made.

### What failure looks like

Everything routes through `InstallError`, which exists and whose messages
name the remedy. The settings row shows them verbatim, as the agents row
already does.

| what happens | what the reader sees |
|---|---|
| no network, download failed | the installer's text; the action stays offered |
| hash does not match | nothing is committed: staging is discarded, state stays "absent" |
| no artifact for this platform | `NoArtifactForPlatform` — the recipe should not have offered, so this is our bug, not theirs |
| `npm` fails | the head and tail of its stderr, which `install_npx` already captures and elides |
| the installed binary will not start | `Dead::Failed`, the existing arm: an error card, and reinstalling is not the cure |

Two tabs of one language make one offer: the offer is deduplicated by
language. A doubled install is already prevented by `InstallGuard`.

### Deliberately absent

No automatic updates. No automatic removal of a superseded version. No new
sweep — the agents' `sweep_staging` (`main.rs:5160`) covers orphaned staging
once given the second root.

## §8 Tests

The defect this design corrects cannot be seen by using the app, so the tests
that matter are again about **agreement between lists**.

| test | what it catches | crate |
|---|---|---|
| `every_default_entry_says_how_to_get_its_server` | a new row with no answer: neither recipe nor `Manual`. Silence stays banned | `sirio_lsp` |
| `no_recipe_names_an_archive_the_installer_cannot_unpack` | every `Release.url` through `unpack_kind`: `Unsupported`, or a `.gz` mistaken for a bare executable, goes red | `sirio` |
| `the_shipped_table_names_twenty_one_servers` | the number, pinned once, so the prose cannot drift again | `sirio_lsp` |
| `a_user_entry_carries_no_recipe` | a full `languages.toml` parsed: `install` must stay `None` | `sirio_lsp` |
| `three_languages_one_package_share_one_install` | json/html/css on one store id | `sirio` |
| `the_path_wins_over_what_sirio_installed` | step 1 | `sirio` |
| `a_successful_install_revives_a_dead_key` | the transition that does not exist today | `sirio` |
| `installing_does_not_revive_a_server_that_crashed` | `Dead::Failed` stays final | `sirio` |
| `a_missing_server_redirects_the_navigation_entries_to_installing_it` | the menu's third state | `sirio_ui` |
| `copy_permalink_outside_a_repo_is_still_merely_unavailable` | that the old contract **survived**, rather than being weakened | `sirio_ui` |

The second exists only because of the crate boundary. `sirio_lsp` cannot see
it — it does not know `UnpackKind`, and must not; they are two leaves.
`sirio_registry` cannot see it — it does not know what a recipe is. The test
lives in `sirio`, the only crate that depends on both, which is where the
boundary stops being a cost and becomes the vantage point. It is also the
test that would have caught the `.gz` bug before a line of install code was
written, so it is written first, against the recipe table and nothing else.

Three that need the real workspace, at roughly fifteen seconds each:

- `opening_a_second_file_of_the_same_language_does_not_offer_twice`
- `silencing_a_language_survives_a_restart`
- `an_npm_recipe_offers_nothing_when_node_is_absent`

And two scripts beside `test-update-e2e.sh`, which is already the home of the
things that touch the network and which `ci.sh` does not run:

- **`test-lsp-install-e2e.sh`** — installs a real server into a temporary
  store, launches it, asks it for a definition. The same shape as
  `sirio_lsp/examples/lsp_probe.rs`, which is already written.
- **`test-lsp-recipes.sh`** — the sixteen pinned recipes, each checked the
  way its own arm can be: the eight release URLs still resolve and still
  hash to what is written, and the eight pinned `package@version` specs
  still exist on the registry. Goes red when upstream deletes a release or
  unpublishes a version, which is the only way to learn it before a reader
  does.

Every new assertion is verified red-capable by mutating the code beneath it,
and the set of registered test names is diffed against the base branch. On
2026-09-16 a test passed with its own fix disabled; without the mutation it
would have shipped as a false guarantee.
