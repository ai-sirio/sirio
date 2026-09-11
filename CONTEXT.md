# Context

The project's glossary: terms whose meaning has been decided, so the same word
cannot quietly mean two things. Definitions only — no implementation detail, no
decisions (those live in issues and `docs/adr/`).

## Workspace layout

### Pane

A region of the **split tree inside a single tab** — what `SplitPaneRight` and
`SplitPaneDown` create, and what the control socket's `pane.*` verbs drive. A
pane never spans tabs, and it is never one of the two halves of the
[center split](#center-split).

The word is deliberately reserved for this one meaning. It has been used
loosely for two other things — the halves of the center split, and the pty
records the control socket's `panel.*` verbs address — and
[#319](https://github.com/ai-sirio/sirio/issues/319) settled that neither may
be called a pane. Those have their own names below and in `sirio_control`.

### Center split

The division of the work area between the sidebars into two halves, each with
a fixed role and its own tab strip. Not a general N-way split: there are always
exactly two halves, and which one a tab belongs to is decided by what the tab
*is*, never chosen or stored.

### Primary pane role

The half of the [center split](#center-split) that holds conversations with
agents and terminals. It is always shown, and takes the whole work area
whenever the [secondary](#secondary-pane-role) half is not.

Despite the name it is not a [pane](#pane) — "role" is part of the term, not a
qualifier that can be dropped.

### Secondary pane role

The half of the [center split](#center-split) that holds everything the user
looks at rather than talks to — a browser, a file being edited, a diff. Absent
by default, present only once something of that kind is open, and gone again
when the last of them closes.

Despite the name it is not a [pane](#pane); see
[primary pane role](#primary-pane-role).

## Control socket

### Panel

A pty record addressed by the control socket's `panel.*` verbs — created over
the socket itself, or published by the app for a terminal it renders. Decided
in [#319](https://github.com/ai-sirio/sirio/issues/319): never called a
[pane](#pane), which is reserved for the split tree inside a tab.

### Scrollback source

The live handle a [panel](#panel)'s scrollback is read through: consulted only
at the moment a read asks for the bytes, never eagerly. A panel without one —
still spawning, failed, or already shut down — reads as empty scrollback.

## Browser

### Origin grant

Permission for an **agent** to drive the browser to a given origin. Raised as a
doorhanger when an agent-driven navigation reaches an origin that has not been
granted; granted origins persist. It gates an agent's reach and says nothing
about what the page may then do.

This is what the control socket's `browser.permission` resolves.

### Capability permission

Permission for a **page** to use a device capability — microphone, camera,
geolocation, notifications. Requested by the page, mediated by the web engine
(WebKitGTK, WKWebView, WebView2), and distinct from an [origin grant](#origin-grant)
in who asks, who decides, and what is at stake.

Decided in [#137](https://github.com/ai-sirio/sirio/issues/137): the two are
never referred to by the same word, because they share only the English one.

### Window hosting

Hosting a web engine's page in a **native child window** of the app's own window
— an X11 child on Linux, an `NSView` on macOS, a child `HWND` on Windows. What
`build_as_child` produces, and the only mode wry 0.56.1 offers.

### Visual hosting

Hosting the page as a **composition visual** placed inside the host's own
composition tree, rather than as a child window. On Windows this is WebView2's
`CreateCoreWebView2CompositionController`. Distinguished from
[window hosting](#window-hosting) because a host that composes its own output —
as GPUI does on Windows — never displays child-window content at all, which is
what [#144](https://github.com/ai-sirio/sirio/issues/144) turned on.

## Worktree loading

### Snapshot coerente del worktree

Il risultato unico e coerente del caricamento di un worktree: layout tab,
`tab_states`, session refs degli agenti, tab attivo e secondary pane role.
Caricato fuori dal render thread dietro adapter blocking, è applicato solo se
generation e `repo_root` corrispondono ancora.

### Stato di estensione del grafo History

Lo stato append-only in `sirio_git` che estende il layout del grafo commit
pagina per pagina, con equivalenza stretta al layout completo. Si resetta su
filtro o ricerca.
