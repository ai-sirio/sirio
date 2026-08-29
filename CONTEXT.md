# Context

The project's glossary: terms whose meaning has been decided, so the same word
cannot quietly mean two things. Definitions only — no implementation detail, no
decisions (those live in issues and `docs/adr/`).

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
