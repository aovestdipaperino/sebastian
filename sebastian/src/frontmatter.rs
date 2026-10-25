//! YAML front matter (`---` ... `---`) at the top of a diagram.
//!
//! Mermaid reads `config:` as configuration (the same keys an `%%{init}%%`
//! directive takes) and `title:` as the diagram title. This module understands
//! the small YAML subset those blocks use: nested maps by indentation, plain
//! and quoted scalars, and inline `{}` / `[]` values. [`normalize`] removes the
//! block and re-expresses its `config:` as an init directive so every renderer
//! sees one configuration path.

use std::fmt::Write;

use serde_json::{Map, Value};

/// Splits a leading front-matter block off `source`.
///
/// Returns the block's lines (without the fences) and the remaining source, or
/// `None` when the diagram does not start with one.
fn split(source: &str) -> Option<(Vec<&str>, &str)> {
    let mut lines = source.split_inclusive('\n');
    let mut consumed = 0;
    // Blank lines may precede the opening fence.
    let first = loop {
        let l = lines.next()?;
        consumed += l.len();
        if !l.trim().is_empty() {
            break l;
        }
    };
    if first.trim_end() != "---" {
        return None;
    }
    let mut block = Vec::new();
    for l in lines {
        consumed += l.len();
        if l.trim_end() == "---" {
            return Some((block, &source[consumed..]));
        }
        block.push(l.trim_end_matches(['\n', '\r']));
    }
    None
}

fn scalar(raw: &str) -> Value {
    let t = raw.trim();
    if t.is_empty() {
        return Value::Null;
    }
    if let Some(inner) = t
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .or_else(|| t.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
    {
        return Value::String(inner.to_owned());
    }
    if t.starts_with('{') || t.starts_with('[') {
        if let Ok(v) = serde_json::from_str::<Value>(t) {
            return v;
        }
        return Value::String(t.to_owned());
    }
    match t {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        "null" | "~" => return Value::Null,
        _ => {}
    }
    if let Ok(n) = t.parse::<i64>() {
        return Value::from(n);
    }
    if let Ok(f) = t.parse::<f64>()
        && f.is_finite()
        && let Some(n) = serde_json::Number::from_f64(f)
    {
        return Value::Number(n);
    }
    Value::String(t.to_owned())
}

/// Drops a trailing ` # comment` that is not inside quotes.
fn strip_comment(line: &str) -> &str {
    let mut quote: Option<char> = None;
    let mut prev = ' ';
    for (i, c) in line.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
        } else if c == '#' && prev.is_whitespace() {
            return line[..i].trim_end();
        }
        prev = c;
    }
    line
}

/// Parses an indentation-structured YAML subset into a JSON object.
fn parse_map(lines: &[&str]) -> Map<String, Value> {
    // Each entry: (indent, key, inline value). A key with an empty value opens
    // a nested map for the following more-indented lines.
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut root = Map::new();
    for raw in lines {
        let line = strip_comment(raw);
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        let body = line.trim_start();
        let Some((key, rest)) = body.split_once(':') else {
            continue;
        };
        // `key:value` without a space is part of a scalar, not a mapping.
        if !(rest.is_empty() || rest.starts_with(' ')) {
            continue;
        }
        let key = key.trim().trim_matches(['"', '\'']).to_owned();
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        let mut node = &mut root;
        for (_, k) in &stack {
            node = node
                .get_mut(k)
                .and_then(Value::as_object_mut)
                .expect("parent map");
        }
        if rest.trim().is_empty() {
            node.insert(key.clone(), Value::Object(Map::new()));
            stack.push((indent, key));
        } else {
            node.insert(key, scalar(rest));
        }
    }
    root
}

/// Removes a leading front-matter block and re-expresses its `config:` as an
/// `%%{init: ...}%%` directive. Sources without front matter are returned
/// unchanged.
///
/// `title:` has no portable equivalent across diagram grammars, so it is
/// dropped (renderers that draw a title read their own `title` statement).
#[must_use]
pub fn normalize(source: &str) -> String {
    let Some((block, rest)) = split(source) else {
        return source.to_owned();
    };
    let map = parse_map(&block);
    match map.get("config") {
        Some(Value::Object(cfg)) if !cfg.is_empty() => {
            let json = serde_json::to_string(cfg).unwrap_or_default();
            format!("%%{{init: {json}}}%%\n{rest}")
        }
        _ => rest.to_owned(),
    }
}

/// The front-matter `title:` value, if any.
#[must_use]
pub fn title(source: &str) -> Option<String> {
    let (block, _) = split(source)?;
    match parse_map(&block).get("title")? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Draws a front-matter title above a finished SVG, as mermaid's
/// `insertTitle` does: centred text at y = -25 with the viewBox grown upward
/// by 50. `class` is the diagram's title class (e.g. `flowchartTitleText`).
#[must_use]
pub fn inject_title(svg: &str, title: &str, class: &str, font_size: f64) -> String {
    let Some(at) = svg.find("viewBox=\"") else {
        return svg.to_owned();
    };
    let start = at + "viewBox=\"".len();
    let Some(len) = svg[start..].find('"') else {
        return svg.to_owned();
    };
    let nums: Vec<f64> = svg[start..start + len]
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    let [x, y, w, h] = nums[..] else {
        return svg.to_owned();
    };
    let text = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    // The title joins the bounding box before the viewBox is taken, so a title
    // wider than the diagram widens it (max-width keeps the original width).
    let cx = x + w / 2.0;
    let tw =
        f64::from(crate::text::TextMeasurer::new().measure_advance_svg(&text, font_size) as f32);
    let left = x.min(cx - tw / 2.0);
    let right = (x + w).max(cx + tw / 2.0);
    let mut out = String::with_capacity(svg.len() + 100);
    out.push_str(&svg[..start]);
    // The text box (baseline -25, font ascent above it) joins the bbox, then
    // the 8px viewBox padding is added.
    let top = (-25.0 - (1923.0 * font_size / 2048.0).round() - 8.0).min(y);
    let _ = write!(out, "{left} {top} {} {}", right - left, y + h - top);
    out.push_str(&svg[start + len..]);
    let tail = out.rfind("</svg>").unwrap_or(out.len());
    out.insert_str(
        tail,
        &format!(
            "<text text-anchor=\"middle\" x=\"{cx}\" y=\"-25\" class=\"{class}\">{text}</text>"
        ),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_source_is_untouched() {
        assert_eq!(normalize("graph TD\n A-->B\n"), "graph TD\n A-->B\n");
    }

    #[test]
    fn config_becomes_init_directive() {
        let src = "---\ntitle: T\nconfig:\n  theme: forest\n  flowchart:\n    curve: linear\n---\ngraph TD\n A-->B\n";
        let out = normalize(src);
        assert!(out.starts_with("%%{init: "), "{out}");
        assert!(out.contains("\"theme\":\"forest\""));
        assert!(out.contains("\"flowchart\":{\"curve\":\"linear\"}"));
        assert!(out.ends_with("graph TD\n A-->B\n"));
    }

    #[test]
    fn front_matter_without_config_is_stripped() {
        assert_eq!(normalize("---\ntitle: T\n---\npie\n"), "pie\n");
    }

    #[test]
    fn scalars_inline_values_and_comments() {
        let m = parse_map(&[
            "config:",
            "  themeVariables: { \"primaryColor\": \"#ff0\" }  # note",
            "  look: handDrawn",
            "  n: 3",
            "  on: true",
            "#  theme: dark",
        ]);
        let cfg = &m["config"];
        assert_eq!(cfg["themeVariables"]["primaryColor"], "#ff0");
        assert_eq!(cfg["look"], "handDrawn");
        assert_eq!(cfg["n"], 3);
        assert_eq!(cfg["on"], true);
        assert!(cfg.get("theme").is_none());
    }

    #[test]
    fn unterminated_block_is_left_alone() {
        let src = "---\ntitle: T\ngraph TD\n";
        assert_eq!(normalize(src), src);
    }
}
