//! Accessibility statements (`accTitle`, `accDescr`).
//!
//! Every mermaid diagram grammar accepts `accTitle: text`, `accDescr: text`
//! and a multi-line `accDescr { ... }`. Mermaid renders them as a `<title>` and
//! `<desc>` at the top of the root `<svg>` plus `aria-labelledby` /
//! `aria-describedby`. The renderers here do not parse them, so [`extract`]
//! removes the statements before rendering and [`inject`] adds the markup to
//! the finished SVG.

use std::fmt::Write;

/// The accessibility text found in a diagram source.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct A11y {
    pub title: Option<String>,
    pub description: Option<String>,
}

fn keyword_value<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    let rest = rest.trim_start();
    rest.strip_prefix(':').map(str::trim)
}

/// Removes `accTitle` / `accDescr` statements from `source` and returns them.
///
/// The first non-blank line (the diagram header) is never treated as a
/// statement.
#[must_use]
pub fn extract(source: &str) -> (String, A11y) {
    if !source.contains("accTitle") && !source.contains("accDescr") {
        return (source.to_owned(), A11y::default());
    }
    let mut out = String::with_capacity(source.len());
    let mut a11y = A11y::default();
    let mut seen_header = false;
    let mut lines = source.split_inclusive('\n');
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if !seen_header {
            if !line.is_empty() && !line.starts_with("%%") && line != "---" {
                seen_header = true;
            }
            out.push_str(raw);
            continue;
        }
        if let Some(v) = keyword_value(line, "accTitle") {
            a11y.title = Some(v.to_owned());
        } else if let Some(v) = keyword_value(line, "accDescr") {
            a11y.description = Some(v.to_owned());
        } else if let Some(rest) = line
            .strip_prefix("accDescr")
            .map(str::trim_start)
            .and_then(|r| r.strip_prefix('{'))
        {
            let mut body = String::from(rest);
            if let Some(end) = body.find('}') {
                body.truncate(end);
            } else {
                for next in lines.by_ref() {
                    if let Some(end) = next.find('}') {
                        body.push('\n');
                        body.push_str(&next[..end]);
                        break;
                    }
                    body.push('\n');
                    body.push_str(next.trim_end_matches(['\n', '\r']));
                }
            }
            a11y.description = Some(collapse_indent(body.trim()));
        } else {
            out.push_str(raw);
        }
    }
    (out, a11y)
}

/// `commonDb.setAccDescription`: indentation after a line break is dropped.
fn collapse_indent(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_break = false;
    for c in text.chars() {
        if after_break && c.is_whitespace() {
            continue;
        }
        after_break = c == '\n';
        out.push(c);
    }
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Adds `<title>`, `<desc>` and the aria attributes to a rendered SVG.
#[must_use]
pub fn inject(svg: &str, id: &str, a11y: &A11y) -> String {
    if a11y.title.is_none() && a11y.description.is_none() {
        return svg.to_owned();
    }
    let Some(open_end) = svg.find('>') else {
        return svg.to_owned();
    };
    let mut attrs = String::new();
    let mut nodes = String::new();
    // Mermaid sets aria-describedby before aria-labelledby, and inserts the
    // <title> ahead of the <desc>.
    if a11y.description.is_some() {
        let _ = write!(attrs, " aria-describedby=\"chart-desc-{id}\"");
    }
    if let Some(t) = &a11y.title {
        let _ = write!(attrs, " aria-labelledby=\"chart-title-{id}\"");
        let _ = write!(
            nodes,
            "<title id=\"chart-title-{id}\">{}</title>",
            escape(t)
        );
    }
    if let Some(d) = &a11y.description {
        let _ = write!(nodes, "<desc id=\"chart-desc-{id}\">{}</desc>", escape(d));
    }
    let self_closing = svg[..open_end].ends_with('/');
    if self_closing {
        return svg.to_owned();
    }
    format!(
        "{}{attrs}>{nodes}{}",
        &svg[..open_end],
        &svg[open_end + 1..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_statements_is_untouched() {
        let (s, a) = extract("pie\n\"a\": 1\n");
        assert_eq!(s, "pie\n\"a\": 1\n");
        assert_eq!(a, A11y::default());
    }

    #[test]
    fn single_and_multi_line() {
        let src = "pie\n  accTitle: My title\n  accDescr {\n    line one\n    line two\n  }\n  \"a\": 1\n";
        let (s, a) = extract(src);
        assert_eq!(s, "pie\n  \"a\": 1\n");
        assert_eq!(a.title.as_deref(), Some("My title"));
        assert_eq!(a.description.as_deref(), Some("line one\nline two"));
    }

    #[test]
    fn single_line_description() {
        let (_, a) = extract("xychart\naccDescr: hello there\n");
        assert_eq!(a.description.as_deref(), Some("hello there"));
    }

    #[test]
    fn injection_order_and_escaping() {
        let a = A11y {
            title: Some("A & B".into()),
            description: Some("d".into()),
        };
        let out = inject("<svg id=\"x\" role=\"r\"><style/></svg>", "x", &a);
        assert_eq!(
            out,
            "<svg id=\"x\" role=\"r\" aria-describedby=\"chart-desc-x\" aria-labelledby=\"chart-title-x\"><title id=\"chart-title-x\">A &amp; B</title><desc id=\"chart-desc-x\">d</desc><style/></svg>"
        );
    }
}
