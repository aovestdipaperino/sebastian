//! Port of `handDrawnShapeStyles.ts` style compilation and Chrome CSSOM
//! style-attribute serialization.

use indexmap::IndexMap;

#[must_use]
pub fn is_label_style(key: &str) -> bool {
    matches!(
        key,
        "color"
            | "font-size"
            | "font-family"
            | "font-weight"
            | "font-style"
            | "text-decoration"
            | "text-align"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "text-shadow"
            | "text-overflow"
            | "white-space"
            | "word-wrap"
            | "word-break"
            | "overflow-wrap"
            | "hyphens"
    )
}

#[derive(Debug, Clone, Default)]
pub struct CompiledStyles {
    pub label_styles: String,
    pub node_styles: String,
}

/// `styles2String`: split compiled+direct styles into label vs node styles.
#[must_use]
pub fn styles2string(
    css_compiled_styles: &[String],
    css_styles: &[String],
    label_style: &[String],
) -> CompiledStyles {
    let mut map: IndexMap<String, String> = IndexMap::new();
    for style in css_compiled_styles
        .iter()
        .chain(css_styles)
        .chain(label_style)
    {
        let mut parts = style.splitn(2, ':');
        let key = parts.next().unwrap_or("").trim().to_owned();
        let value = parts.next().unwrap_or("").trim().to_owned();
        map.insert(key, value);
    }
    let mut label_styles = Vec::new();
    let mut node_styles = Vec::new();
    for (key, value) in &map {
        let decl = format!("{key}:{value} !important");
        if is_label_style(key) {
            label_styles.push(decl);
        } else {
            node_styles.push(decl);
        }
    }
    CompiledStyles {
        label_styles: label_styles.join(";"),
        node_styles: node_styles.join(";"),
    }
}

/// Serializes the html label div's style attribute the way Chrome's CSSOM
/// does after `applyStyle(div, labelStyle)` + the base `.style()` calls.
#[must_use]
pub fn div_style_attr(label_style: &str, base: &str) -> String {
    let mut out = String::new();
    for decl in label_style.split(';') {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let mut parts = decl.splitn(2, ':');
        let prop = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        let (value, important) = match value.strip_suffix("!important") {
            Some(v) => (v.trim(), true),
            None => (value, false),
        };
        let value = super::css::cssom_color_value(prop, value);
        out.push_str(prop);
        out.push_str(": ");
        out.push_str(&value);
        if important {
            out.push_str(" !important");
        }
        out.push_str("; ");
    }
    out.push_str(base);
    out
}
