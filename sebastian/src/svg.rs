//! Minimal SVG/XHTML DOM tree matching the structure mermaid builds via d3,
//! and a serializer replicating Chrome's `XMLSerializer` output.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// Formats an f64 like JavaScript `String(number)`.
#[must_use]
pub fn js_num(n: f64) -> String {
    if n == 0.0 {
        // JS prints -0 as "0".
        return "0".to_owned();
    }
    if n.fract() == 0.0 && n.abs() < 1e21 {
        // Integers print without a decimal point. Only the i64-representable
        // range can use the fast cast; larger integers (up to 1e21) fall
        // through to the digit-exact expansion below to avoid `as i64`
        // saturation. (Such magnitudes never arise in diagram coordinates,
        // but the cast would silently misformat them.)
        if n.abs() < i64::MAX as f64 {
            return format!("{}", n as i64);
        }
    }
    // Shortest representation that round-trips, with ties resolved by
    // round-half-to-even like V8 (Rust's default formatter differs on ties).
    for p in 1..=17 {
        let s = format!("{:.*e}", p - 1, n);
        if s.parse::<f64>() == Ok(n) {
            return sci_to_plain(&s);
        }
    }
    format!("{n}")
}

/// Expands `1.234e2`-style scientific notation to plain decimal (JS keeps
/// plain notation for exponents in (-7, 21)).
fn sci_to_plain(s: &str) -> String {
    let (mantissa, exp) = s.split_once('e').expect("scientific notation");
    let exp: i32 = exp.parse().expect("exponent");
    if !(-7..21).contains(&exp) {
        return s.to_owned();
    }
    let neg = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let digits = digits.trim_end_matches('0');
    let digits = if digits.is_empty() { "0" } else { digits };
    let point = exp + 1;
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if point <= 0 {
        out.push_str("0.");
        for _ in 0..-point {
            out.push('0');
        }
        out.push_str(digits);
    } else if point as usize >= digits.len() {
        out.push_str(digits);
        for _ in 0..(point as usize - digits.len()) {
            out.push('0');
        }
    } else {
        out.push_str(&digits[..point as usize]);
        out.push('.');
        out.push_str(&digits[point as usize..]);
    }
    out
}

/// Rounds like d3-path with `digits = 3` (d3-shape's default for `line()`).
#[must_use]
pub fn d3_round(n: f64) -> f64 {
    (n * 1000.0).round() / 1000.0
}

#[derive(Debug)]
pub struct ElementData {
    pub tag: String,
    /// Attributes in insertion order; setting an existing name updates in place.
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    /// True for elements in the XHTML namespace (div/span/p): never self-close.
    pub xhtml: bool,
    pub parent: Option<Weak<RefCell<ElementData>>>,
}

pub type Element = Rc<RefCell<ElementData>>;

#[derive(Debug, Clone)]
pub enum Node {
    Element(Element),
    Text(String),
}

#[must_use]
pub fn new_element(tag: &str) -> Element {
    Rc::new(RefCell::new(ElementData {
        tag: tag.to_owned(),
        attrs: Vec::new(),
        children: Vec::new(),
        xhtml: false,
        parent: None,
    }))
}

#[must_use]
pub fn new_xhtml_element(tag: &str) -> Element {
    let el = new_element(tag);
    el.borrow_mut().xhtml = true;
    el
}

pub fn set_attr(el: &Element, name: &str, value: impl Into<String>) {
    let value = value.into();
    let mut data = el.borrow_mut();
    if let Some(entry) = data.attrs.iter_mut().find(|(n, _)| n == name) {
        entry.1 = value;
    } else {
        data.attrs.push((name.to_owned(), value));
    }
}

pub fn get_attr(el: &Element, name: &str) -> Option<String> {
    el.borrow()
        .attrs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
}

/// d3 `selection.append(tag)` — adds as last child.
pub fn append(parent: &Element, tag: &str) -> Element {
    let child = new_element(tag);
    child.borrow_mut().parent = Some(Rc::downgrade(parent));
    parent
        .borrow_mut()
        .children
        .push(Node::Element(child.clone()));
    child
}

pub fn append_xhtml(parent: &Element, tag: &str) -> Element {
    let child = append(parent, tag);
    child.borrow_mut().xhtml = true;
    child
}

pub fn append_element(parent: &Element, child: &Element) {
    child.borrow_mut().parent = Some(Rc::downgrade(parent));
    parent
        .borrow_mut()
        .children
        .push(Node::Element(child.clone()));
}

/// d3 `selection.insert(tag)` (no selector) — same as append for our usage,
/// and `insert(tag, ':first-child')` — prepend.
pub fn insert_first(parent: &Element, tag: &str) -> Element {
    let child = new_element(tag);
    child.borrow_mut().parent = Some(Rc::downgrade(parent));
    parent
        .borrow_mut()
        .children
        .insert(0, Node::Element(child.clone()));
    child
}

/// Moves `child` from its current parent to the end of `new_parent`
/// (DOM `appendChild` semantics).
pub fn move_element(child: &Element, new_parent: &Element) {
    let old_parent = child
        .borrow()
        .parent
        .as_ref()
        .and_then(std::rc::Weak::upgrade);
    if let Some(old) = old_parent {
        let mut data = old.borrow_mut();
        data.children.retain(|c| match c {
            Node::Element(e) => !Rc::ptr_eq(e, child),
            Node::Text(_) => true,
        });
    }
    append_element(new_parent, child);
}

pub fn set_text_append(el: &Element, text: &str) {
    el.borrow_mut().children.push(Node::Text(text.to_owned()));
}

pub fn set_text(el: &Element, text: &str) {
    let mut data = el.borrow_mut();
    data.children.clear();
    data.children.push(Node::Text(text.to_owned()));
}

fn escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\t', "&#9;")
        .replace('\n', "&#10;")
        .replace('\r', "&#13;")
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Serializes like Chrome's `XMLSerializer`: SVG empty elements self-close,
/// XHTML elements always get an explicit end tag.
pub fn serialize(el: &Element, out: &mut String) {
    let data = el.borrow();
    out.push('<');
    out.push_str(&data.tag);
    for (name, value) in &data.attrs {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        // mermaid's final DOMPurify pass trims attribute values.
        out.push_str(&escape_attr(value.trim()));
        out.push('"');
    }
    if data.children.is_empty() && !data.xhtml {
        out.push_str("/>");
        return;
    }
    if data.children.is_empty()
        && data.xhtml
        && matches!(data.tag.as_str(), "br" | "hr" | "img" | "input" | "wbr")
    {
        // XMLSerializer emits XHTML void elements with a space before the slash.
        out.push_str(" />");
        return;
    }
    out.push('>');
    for child in &data.children {
        match child {
            Node::Element(child) => serialize(child, out),
            Node::Text(text) => out.push_str(&escape_text(text)),
        }
    }
    out.push_str("</");
    out.push_str(&data.tag);
    out.push('>');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_num_formatting() {
        assert_eq!(js_num(54.0), "54");
        assert_eq!(js_num(-47.507_812_5), "-47.5078125");
        assert_eq!(js_num(0.0), "0");
        assert_eq!(js_num(-0.0), "0");
        assert_eq!(js_num(235.531_25), "235.53125");
    }

    #[test]
    fn serializes_self_closing_and_xhtml() {
        let g = new_element("g");
        set_attr(&g, "class", "label");
        let _rect = append(&g, "rect");
        let div = append_xhtml(&g, "div");
        let span = append_xhtml(&div, "span");
        set_text(&span, "A & B");
        let mut out = String::new();
        serialize(&g, &mut out);
        assert_eq!(
            out,
            "<g class=\"label\"><rect/><div><span>A &amp; B</span></div></g>"
        );
    }
}
