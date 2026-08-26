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

Decided in [#137](https://github.com/tillerai/tiller/issues/137): the two are
never referred to by the same word, because they share only the English one.
