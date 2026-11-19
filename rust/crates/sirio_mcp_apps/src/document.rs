//! Reading a widget out of an `mcp_read_resource` answer.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

use crate::csp::UiCsp;

pub const MIME: &str = "text/html;profile=mcp-app";

/// A widget's page and what its resource declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetDocument {
    pub html: String,
    pub csp: UiCsp,
    /// The CSP entries that were refused, verbatim.
    pub dropped: Vec<String>,
    pub prefers_border: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    NoItem,
    WrongMime(String),
    NoBody,
    BadBlob,
    NotUtf8,
}

impl fmt::Display for DocumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoItem => f.write_str("the server returned nothing for the widget"),
            Self::WrongMime(m) if m.is_empty() => f.write_str("the widget has no content type"),
            Self::WrongMime(m) => write!(f, "the widget has the wrong content type ({m})"),
            Self::NoBody => f.write_str("the widget has no content"),
            Self::BadBlob => f.write_str("the widget content is not valid base64"),
            Self::NotUtf8 => f.write_str("the widget content is not valid text"),
        }
    }
}

impl std::error::Error for DocumentError {}

/// `contents`: the `contents` array of an `mcp_read_resource` answer.
pub fn widget_document(contents: &Value, uri: &str) -> Result<WidgetDocument, DocumentError> {
    let item = contents
        .as_array()
        .and_then(|items| items.iter().find(|i| i.get("uri").and_then(Value::as_str) == Some(uri)))
        .ok_or(DocumentError::NoItem)?;
    let mime = item.get("mimeType").and_then(Value::as_str).unwrap_or_default();
    if !is_widget_mime(mime) {
        return Err(DocumentError::WrongMime(mime.to_owned()));
    }
    let html = if let Some(text) = item.get("text").and_then(Value::as_str) {
        text.to_owned()
    } else if let Some(blob) = item.get("blob").and_then(Value::as_str) {
        let bytes = STANDARD.decode(blob).map_err(|_| DocumentError::BadBlob)?;
        String::from_utf8(bytes).map_err(|_| DocumentError::NotUtf8)?
    } else {
        return Err(DocumentError::NoBody);
    };
    let ui = item.get("_meta").and_then(|m| m.get("ui"));
    let (csp, dropped) = UiCsp::from_ui_meta(ui);
    let prefers_border = ui.and_then(|u| u.get("prefersBorder")).and_then(Value::as_bool);
    Ok(WidgetDocument { html, csp, dropped, prefers_border })
}

/// Case-insensitive, ignoring spaces around `;` and `=`.
fn is_widget_mime(mime: &str) -> bool {
    let squeezed: String = mime.chars().filter(|c| !c.is_whitespace()).collect();
    squeezed.eq_ignore_ascii_case(MIME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const URI: &str = "ui://probe/show.html";
    const HTML: &str = "<html><body><h1>probe widget</h1></body></html>";

    fn meta() -> serde_json::Value {
        json!({"ui": {
            "csp": {"connectDomains": ["https://api.probe.test"],
                    "resourceDomains": ["https://cdn.probe.test"]},
            "prefersBorder": true, "domain": "probe.test"}, "other/key": "x"})
    }

    // Failure list: taking an item for another URI; accepting `text/html` without the
    // profile; rejecting `text/html; profile=mcp-app` (a space, or upper case); accepting a
    // blob that is not base64 or not UTF-8; accepting an item with neither body.
    #[test]
    fn widget_document_refuses_what_is_not_the_widget() {
        let item = |v: serde_json::Value| json!([v]);
        let other = item(json!({"uri": "ui://probe/other.html", "mimeType": MIME, "text": HTML}));
        assert!(matches!(widget_document(&other, URI), Err(DocumentError::NoItem)));
        assert!(matches!(widget_document(&json!([]), URI), Err(DocumentError::NoItem)));
        assert!(matches!(widget_document(&json!({}), URI), Err(DocumentError::NoItem)));
        let plain = item(json!({"uri": URI, "mimeType": "text/html", "text": HTML}));
        assert!(matches!(widget_document(&plain, URI), Err(DocumentError::WrongMime(_))));
        let none = item(json!({"uri": URI, "text": HTML}));
        assert!(matches!(widget_document(&none, URI), Err(DocumentError::WrongMime(_))));
        let bare = item(json!({"uri": URI, "mimeType": MIME}));
        assert!(matches!(widget_document(&bare, URI), Err(DocumentError::NoBody)));
        let junk = item(json!({"uri": URI, "mimeType": MIME, "blob": "***not base64***"}));
        assert!(matches!(widget_document(&junk, URI), Err(DocumentError::BadBlob)));
        let raw = base64::engine::general_purpose::STANDARD.encode([0xff, 0xfe, 0x00]);
        let bin = item(json!({"uri": URI, "mimeType": MIME, "blob": raw}));
        assert!(matches!(widget_document(&bin, URI), Err(DocumentError::NotUtf8)));
        let spellings = [
            "text/html; profile=mcp-app",
            "TEXT/HTML;Profile=MCP-App",
            "text/html ; profile = mcp-app",
        ];
        for mime in spellings {
            let ok = item(json!({"uri": URI, "mimeType": mime, "text": HTML}));
            assert!(widget_document(&ok, URI).is_ok(), "refused: {mime:?}");
        }
    }

    #[test]
    fn widget_document_reads_text_or_blob_and_the_csp() {
        let text = json!([{"uri": "ui://x", "mimeType": "text/plain", "text": "n"},
            {"uri": URI, "mimeType": MIME, "text": HTML, "_meta": meta()}]);
        let doc = widget_document(&text, URI).expect("text widget");
        assert_eq!(doc.html, HTML);
        assert_eq!(doc.csp.connect, ["https://api.probe.test"]);
        assert_eq!(doc.csp.resource, ["https://cdn.probe.test"]);
        assert!(doc.dropped.is_empty());
        assert_eq!(doc.prefers_border, Some(true));

        let blob = base64::engine::general_purpose::STANDARD.encode(HTML);
        let item = json!([{"uri": URI, "mimeType": MIME, "blob": blob, "_meta": meta()}]);
        let doc = widget_document(&item, URI).expect("blob widget");
        assert_eq!(doc.html, HTML);

        let bare = json!([{"uri": URI, "mimeType": MIME, "text": HTML}]);
        let doc = widget_document(&bare, URI).expect("no meta");
        assert_eq!(doc.prefers_border, None);
        assert!(doc.csp.connect.is_empty());
    }
}
