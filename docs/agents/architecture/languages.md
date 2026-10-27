# Languages: one list, two independent answers (`sirio_syntax`, `sirio_lsp`)

`sirio_ui::editor::Language` is the list of what Sirio recognises — twenty-three
languages plus plain text, resolved from the extension, never from content. Two
separate things hang off it, and they failed independently for most of the
project's life:

- **Colour** is compiled in. `sirio_syntax` holds one tree-sitter grammar per
  language `bezel-syntax` does not carry, registered through bezel's own
  `Lang::new` extension point so the spans are ordinary `HighlightKind`s and
  reach `Theme::syntax_palette` with no second code path. A grammar crate
  qualifies only if it reaches tree-sitter through `tree-sitter-language`
  rather than naming a `tree-sitter` version of its own — two tree-sitters in
  the graph are two unrelated `Language` types with one name.
- **Navigation** needs a program on `PATH`. `sirio_lsp::LanguageTable::defaults`
  names one server per language; naming it is not shipping it, and a command
  that is not installed resolves to `LspError::NotInstalled`. Nothing is
  *volunteered* about that — no card, ever, or there would be one on almost
  every file. The context menu is the exception, because it is asked: the
  row's hover tooltip names the missing program (`jdtls is not on PATH`)
  rather than describing Sirio. Menu rows are one line; no reason is drawn
  under a label. The name reaches it through `lsp::Dead::NotInstalled` and
  `FileContextFacts::missing_language_server`, and the failure path pushes the
  facts a second time because the tab computed its own before anyone had
  looked.

The built-in `java` entry also learns Lombok when the project uses it: if a build
file (`pom.xml`, `build.gradle(.kts)`, `settings.gradle(.kts)`,
`gradle/libs.versions.toml`) in the key's root or any directory above it up to the
worktree root mentions `lombok`, `jdtls` is launched with
`--jvm-arg=-javaagent:<newest lombok jar>`, taken from the local Maven and Gradle
caches and never downloaded. The first `ServiceReady` of such a launch is followed
by one `java/buildWorkspace(true)`, because a workspace built without the agent
keeps its Lombok errors in files nobody opened. When a jar appears that is newer
than the launch's, or any jar after a launch with none (checked on that key's
`publishDiagnostics`, at most every 2 s), the key's jdtls is restarted once; a
launch the agent breaks is repeated once without it. A hand-written `java` entry in
`languages.toml` is never touched, no card or message mentions Lombok, and every
language server's stderr is discarded, so a server that is noisy there can no
longer hang on a full pipe. Spec:
`docs/superpowers/specs/2026-10-26-jdtls-lombok-design.md`.

The failure mode both halves share is silence: an unlisted language is detected,
named in the status line, and then rendered as plain text with no action in its
context menu — indistinguishable from a file with nothing to say. The three
tests that make the lists agree are
`sirio_ui`'s `every_language_the_editor_recognises_has_a_grammar`,
`sirio`'s `every_language_the_editor_recognises_has_a_server`, and
`sirio_syntax`'s `every_query_compiles_against_its_grammar`.
`docs/superpowers/specs/2026-09-16-language-coverage-design.md` carries the table
of which server each language names and why a missing one says nothing.
