//! `info` diagram: a port of mermaid's `infoRenderer.ts`, which draws only the
//! mermaid version the references were produced with.

use crate::svg::{append, serialize, set_attr, set_text};

/// The mermaid release whose output sebastian reproduces.
pub const MERMAID_VERSION: &str = "11.15.0";

/// A parse error for info source.
#[derive(Debug)]
pub struct InfoParseError {
    pub message: String,
}

impl std::fmt::Display for InfoParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "info parse error: {}", self.message)
    }
}

impl std::error::Error for InfoParseError {}

/// Renders mermaid `info` source to a complete SVG document string.
///
/// # Errors
/// Returns an [`InfoParseError`] when the source is not an info diagram.
pub fn render_info(source: &str, id: &str) -> Result<String, InfoParseError> {
    let header = source
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .unwrap_or("");
    let rest = header.strip_prefix("info").ok_or_else(|| InfoParseError {
        message: format!("expected info header, got {header:?}"),
    })?;
    if !(rest.is_empty() || rest.trim() == "showInfo") {
        return Err(InfoParseError {
            message: format!("unexpected text after info: {rest:?}"),
        });
    }
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(400.0)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "info");
    let style = append(&svg, "style");
    set_text(
        &style,
        &crate::render::css::themed_info_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");
    let group = append(&svg, "g");
    let text = append(&group, "text");
    set_attr(&text, "x", "100");
    set_attr(&text, "y", "40");
    set_attr(&text, "class", "version");
    set_attr(&text, "font-size", "32");
    set_attr(&text, "style", "text-anchor: middle;");
    set_text(&text, &format!("v{MERMAID_VERSION}"));

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
