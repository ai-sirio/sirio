//! Mermaid, rendered in-process by `mermaid-rs-renderer`.

use std::panic::{AssertUnwindSafe, catch_unwind};

use mermaid_rs_renderer::{RenderOptions, Theme};

use crate::{DiagramError, Palette, Svg, svg};

pub(crate) fn render(source: &str, palette: &Palette) -> Result<Svg, DiagramError> {
    let mut options = RenderOptions::modern();
    options.theme = theme(palette);
    // A panic inside the renderer is its bug, not the reader's: it becomes
    // one failed diagram rather than a crashed app.
    let rendered = catch_unwind(AssertUnwindSafe(|| {
        mermaid_rs_renderer::render_with_options(source, options)
    }))
    .map_err(|_| DiagramError::Io("the Mermaid renderer crashed".into()))?;
    let markup = rendered.map_err(|error| DiagramError::Syntax {
        message: format!("{error:#}"),
        line: None,
    })?;
    svg::double_for_hidpi(&markup)
        .ok_or_else(|| DiagramError::Io("the Mermaid renderer produced no SVG".into()))
}

/// The renderer's theme with the page's own colours laid over the built-in
/// light or dark one, so a diagram reads as part of the Preview.
fn theme(palette: &Palette) -> Theme {
    let mut theme = if palette.dark {
        Theme::dark()
    } else {
        Theme::modern()
    };
    theme.background = palette.background.clone();
    theme.primary_color = palette.node_fill.clone();
    theme.primary_text_color = palette.text.clone();
    theme.primary_border_color = palette.node_border.clone();
    theme.text_color = palette.text.clone();
    theme.line_color = palette.line.clone();
    theme.edge_label_background = palette.label_background.clone();
    theme.cluster_background = palette.label_background.clone();
    theme.cluster_border = palette.node_border.clone();
    theme.sequence_actor_fill = palette.node_fill.clone();
    theme.sequence_actor_border = palette.node_border.clone();
    theme.sequence_actor_line = palette.line.clone();
    theme.sequence_note_fill = palette.label_background.clone();
    theme.sequence_note_border = palette.node_border.clone();
    theme
}
