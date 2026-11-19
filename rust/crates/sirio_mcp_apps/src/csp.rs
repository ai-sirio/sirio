//! The origins a widget may reach, and the view's Content-Security-Policy built from them.

use serde_json::Value;

/// The origins a widget declared and the host accepted, each already normalized.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UiCsp {
    /// `connectDomains`: where the view may fetch, XHR or open a socket.
    pub connect: Vec<String>,
    /// `resourceDomains`: where its scripts, styles, images, fonts and media may load from.
    pub resource: Vec<String>,
}

impl UiCsp {
    /// From `_meta.ui` (the object); returns the entries it dropped, verbatim (a non-string
    /// entry is reported as its JSON text).
    pub fn from_ui_meta(ui: Option<&Value>) -> (UiCsp, Vec<String>) {
        let mut csp = UiCsp::default();
        let mut dropped = Vec::new();
        let declared = ui.and_then(|u| u.get("csp"));
        for (key, connect, into) in [
            ("connectDomains", true, &mut csp.connect),
            ("resourceDomains", false, &mut csp.resource),
        ] {
            let Some(entries) = declared.and_then(|c| c.get(key)).and_then(Value::as_array) else {
                continue;
            };
            for entry in entries {
                let text = entry.as_str().map(str::to_owned).unwrap_or_else(|| entry.to_string());
                match entry.as_str().and_then(|s| allowed_origin(s, connect)) {
                    Some(origin) => into.push(origin),
                    None => dropped.push(text),
                }
            }
        }
        (csp, dropped)
    }
}

/// The normalized origin when `entry` is acceptable, else None. `connect` also allows `wss://`.
///
/// Accepted: `https://` (and `wss://` for connect) plus a lowercase ASCII host of dot-separated
/// labels `[a-z0-9-]` (no label empty or starting or ending with `-`), at least two labels, an
/// optional `*.` first label that must be followed by at least two labels, an optional port
/// written as given (1-65535), and one trailing `/`, which is dropped. Everything else is
/// refused: other schemes, IP literals, `localhost` and the `.localhost`, `.local` and
/// `.internal` names, non-ASCII hosts (no punycode conversion), and any path, query, fragment,
/// userinfo or character that could end a CSP source.
///
/// Known gap: there is no public-suffix list, so a wildcard over a multi-label public suffix
/// such as `https://*.co.uk` is accepted.
pub fn allowed_origin(entry: &str, connect: bool) -> Option<String> {
    if !entry.is_ascii() {
        return None;
    }
    let lower = entry.to_ascii_lowercase();
    let (scheme, rest) = if let Some(r) = lower.strip_prefix("https://") {
        ("https://", r)
    } else if let Some(r) = connect.then(|| lower.strip_prefix("wss://")).flatten() {
        ("wss://", r)
    } else {
        return None;
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    let (host, port) = match rest.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (rest, None),
    };
    if let Some(p) = port {
        let digits = !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit());
        if !digits || p.starts_with('0') || p.parse::<u32>().ok()? > 65535 {
            return None;
        }
    }
    let (wildcard, base) = match host.strip_prefix("*.") {
        Some(b) => (true, b),
        None => (false, host),
    };
    let labels: Vec<&str> = base.split('.').collect();
    if labels.len() < 2 || !labels.iter().all(|l| label_ok(l)) {
        return None;
    }
    if base.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    let local = base == "localhost"
        || [".localhost", ".local", ".internal"].iter().any(|s| base.ends_with(s));
    if local {
        return None;
    }
    let star = if wildcard { "*." } else { "" };
    let port = port.map(|p| format!(":{p}")).unwrap_or_default();
    Some(format!("{scheme}{star}{base}{port}"))
}

fn label_ok(label: &str) -> bool {
    !label.is_empty()
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The view's CSP header value: the spec default plus the declared origins, each only in the
/// directives it belongs to. Entries are re-checked here and deduplicated in first-seen order,
/// so nothing unaccepted can reach the header whatever built the `UiCsp`.
pub fn view_csp(csp: &UiCsp) -> String {
    let resource = accepted(&csp.resource, false);
    let connect = accepted(&csp.connect, true);
    let r = if resource.is_empty() { String::new() } else { format!(" {}", resource.join(" ")) };
    let c = if connect.is_empty() { "'none'".to_owned() } else { connect.join(" ") };
    [
        "default-src 'none'".to_owned(),
        format!("script-src 'self' 'unsafe-inline'{r}"),
        format!("style-src 'self' 'unsafe-inline'{r}"),
        format!("img-src 'self' data:{r}"),
        format!("font-src 'self'{r}"),
        format!("media-src 'self' data:{r}"),
        format!("connect-src {c}"),
        "frame-src 'none'".to_owned(),
        "object-src 'none'".to_owned(),
        "base-uri 'none'".to_owned(),
        "form-action 'none'".to_owned(),
    ]
    .join("; ")
}

fn accepted(entries: &[String], connect: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for origin in entries.iter().filter_map(|e| allowed_origin(e, connect)) {
        if !out.contains(&origin) {
            out.push(origin);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Failure list for `allowed_origin`. It may fail to reject: `*`; `http:`; `data:`/`blob:`;
    // an IP literal (v4 or `[`v6); localhost, `*.localhost`, `.local`, `.internal`;
    // `https://*.com`; a path or query; a quote, `;`, `,`, `@`, backslash, space, CR/LF; an
    // empty label; a port outside 1-65535; `wss:` when not `connect`. It may also fail to
    // accept and lowercase the normal https origins.
    #[test]
    fn allowed_origin_rejects_every_unsafe_form() {
        let bad = [
            "*",
            "http://a.widget.test",
            "http:",
            "data:text/html,x",
            "blob:https://a.widget.test/x",
            "https://127.0.0.1",
            "https://10.0.0.1:8080",
            "https://[::1]",
            "https://[2001:db8::1]",
            "https://localhost",
            "https://*.localhost",
            "https://app.localhost",
            "https://printer.local",
            "https://*.printer.local",
            "https://db.internal",
            "https://*.corp.internal",
            "https://*.com",
            "https://*",
            "https://a.widget.test/path",
            "https://a.widget.test//",
            "https://a.widget.test?q=1",
            "https://a.widget.test#frag",
            "https://a.widget.test\"",
            "https://a.widget.test'",
            "https://a.widget.test; script-src *",
            "https://a.widget.test,https://b.widget.test",
            "https://user@a.widget.test",
            "https://a.widget.test\\x",
            "https://a.widget.test x",
            "https://a.widget.test\r\nx",
            "https://a.widget.test\n",
            "https://a..widget.test",
            "https://.widget.test",
            "https://-a.widget.test",
            "https://a-.widget.test",
            "https://a.widget.test:0",
            "https://a.widget.test:65536",
            "https://a.widget.test:",
            "https://a.widget.test:80a",
            "https://caf\u{e9}.widget.test",
            "",
            "https://",
            "wss://ws.widget.test",
        ];
        for entry in bad {
            assert_eq!(allowed_origin(entry, false), None, "accepted: {entry:?}");
        }
        let bad_connect =
            ["wss://127.0.0.1", "wss://*.com", "ws://a.widget.test", "wss://a.widget.test/x"];
        for entry in bad_connect {
            assert_eq!(allowed_origin(entry, true), None, "accepted: {entry:?}");
        }
    }

    #[test]
    fn allowed_origin_accepts_and_normalizes_https_origins() {
        let ok = |e: &str, connect: bool| allowed_origin(e, connect);
        let cdn = Some("https://cdn.widget.test");
        assert_eq!(ok("https://cdn.widget.test", false).as_deref(), cdn);
        assert_eq!(ok("https://CDN.Widget.test/", false).as_deref(), cdn);
        assert_eq!(ok("https://*.widget.test", false).as_deref(), Some("https://*.widget.test"));
        assert_eq!(
            ok("https://api.widget.test:8443", true).as_deref(),
            Some("https://api.widget.test:8443")
        );
        assert_eq!(ok("wss://ws.widget.test", true).as_deref(), Some("wss://ws.widget.test"));
        // The documented gap: no public-suffix list.
        assert_eq!(ok("https://*.co.uk", false).as_deref(), Some("https://*.co.uk"));
    }

    // Failure list for `view_csp`: loosening `connect-src` to 'self' when empty; dropping a
    // directive; leaking an entry that was dropped.
    #[test]
    fn view_csp_is_the_spec_default_plus_declared_origins() {
        let empty = UiCsp::default();
        assert_eq!(
            view_csp(&empty),
            "default-src 'none'; script-src 'self' 'unsafe-inline'; \
             style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
             media-src 'self' data:; connect-src 'none'; frame-src 'none'; object-src 'none'; \
             base-uri 'none'; form-action 'none'"
        );
        let csp = UiCsp {
            connect: vec!["https://api.widget.test".into()],
            resource: vec!["https://cdn.widget.test".into()],
        };
        let out = view_csp(&csp);
        assert!(out.contains("script-src 'self' 'unsafe-inline' https://cdn.widget.test;"));
        assert!(out.contains("font-src 'self' https://cdn.widget.test;"));
        assert!(out.contains("connect-src https://api.widget.test;"));
        assert_eq!(out.matches("https://cdn.widget.test").count(), 5);
        assert_eq!(out.matches("https://api.widget.test").count(), 1);
    }

    #[test]
    fn view_csp_repeats_nothing_and_never_carries_a_bad_entry() {
        let csp = UiCsp {
            connect: vec!["https://a.widget.test".into(), "https://a.widget.test".into()],
            resource: vec!["https://x; script-src *".into(), "https://c.widget.test".into()],
        };
        let out = view_csp(&csp);
        assert_eq!(out.matches("https://a.widget.test").count(), 1);
        assert!(!out.contains("script-src *"));
        assert!(out.contains("img-src 'self' data: https://c.widget.test;"));
    }

    #[test]
    fn from_ui_meta_drops_and_reports_bad_entries() {
        let ui = json!({"csp": {
            "connectDomains": ["https://api.widget.test", "http://bad.widget.test", 7],
            "resourceDomains": ["https://CDN.widget.test/", "*"],
        }});
        let (csp, dropped) = UiCsp::from_ui_meta(Some(&ui));
        assert_eq!(csp.connect, ["https://api.widget.test"]);
        assert_eq!(csp.resource, ["https://cdn.widget.test"]);
        assert_eq!(dropped, ["http://bad.widget.test", "7", "*"]);
        let (none, nothing) = UiCsp::from_ui_meta(None);
        assert!(none.connect.is_empty() && none.resource.is_empty() && nothing.is_empty());
    }
}
