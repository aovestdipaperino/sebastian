//! **Approximate** (non-byte-exact) renderer for mermaid `mindmap` diagrams.
//!
//! Mermaid lays mindmaps out with the `cose-bilkent` force-directed engine
//! (deterministic but a large physics simulation with no Rust equivalent — see
//! `TODO.md`). Reproducing its exact node coordinates is out of scope, so this
//! renderer uses its own deterministic **tidy-tree** layout (left-to-right):
//! output is a clean, stable mindmap but is *not* byte-identical to mmdc. This
//! is an opt-in approximation, the same stance as the hand-drawn look.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::manual_midpoint,
    clippy::single_match_else
)]

use std::fmt::Write as _;

use crate::svg::{append, js_num, new_element, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

const X_SPACING: f64 = 180.0;
const Y_SPACING: f64 = 54.0;
const FONT_SIZE: f64 = 16.0;
const NODE_PAD_X: f64 = 20.0;
const NODE_H: f64 = 40.0;

/// Parse error for mindmap diagrams.
#[derive(Debug)]
pub struct MindmapParseError(pub String);

impl std::fmt::Display for MindmapParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mindmap parse error: {}", self.0)
    }
}

impl std::error::Error for MindmapParseError {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Circle,
    Rounded,
    Rect,
    Hexagon,
    Cloud,
    Bang,
}

struct Node {
    text: String,
    shape: Shape,
    depth: usize,
    children: Vec<usize>,
    // layout
    x: f64,
    y: f64,
    w: f64,
}

fn extract(content: &str) -> (String, Shape) {
    // Ordered longest-first so `((` wins over `(`.
    const PAIRS: &[(&str, &str, Shape)] = &[
        ("((", "))", Shape::Circle),
        ("))", "((", Shape::Bang),
        (")", "(", Shape::Cloud),
        ("{{", "}}", Shape::Hexagon),
        ("(", ")", Shape::Rounded),
        ("[", "]", Shape::Rect),
    ];
    for (open, close, shape) in PAIRS {
        if let Some(o) = content.find(open) {
            let after = &content[o + open.len()..];
            if let Some(c) = after.rfind(close) {
                return (after[..c].trim().to_owned(), *shape);
            }
        }
    }
    (content.trim().to_owned(), Shape::Rounded)
}

fn parse(source: &str, measurer: &TextMeasurer) -> Result<Vec<Node>, MindmapParseError> {
    let mut nodes: Vec<Node> = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (indent, node index)
    let mut found = false;
    for raw in source.lines() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        if !found {
            if t == "mindmap" || t.starts_with("mindmap ") {
                found = true;
            } else {
                return Err(MindmapParseError(format!(
                    "expected mindmap header, got {t:?}"
                )));
            }
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let (text, shape) = extract(t);
        let w = measurer.measure_width(&text, FONT_SIZE) + NODE_PAD_X * 2.0;
        while let Some(&(ind, _)) = stack.last() {
            if ind >= indent {
                stack.pop();
            } else {
                break;
            }
        }
        let parent = stack.last().map(|&(_, i)| i);
        let depth = parent.map_or(0, |p| nodes[p].depth + 1);
        let idx = nodes.len();
        nodes.push(Node {
            text,
            shape,
            depth,
            children: Vec::new(),
            x: 0.0,
            y: 0.0,
            w,
        });
        if let Some(p) = parent {
            nodes[p].children.push(idx);
        }
        stack.push((indent, idx));
    }
    Ok(nodes)
}

/// Tidy-tree y assignment: leaves get sequential rows; parents centre on kids.
fn assign_y(nodes: &mut [Node], idx: usize, next_row: &mut f64) {
    let children = nodes[idx].children.clone();
    if children.is_empty() {
        nodes[idx].y = *next_row * Y_SPACING;
        *next_row += 1.0;
        return;
    }
    for c in &children {
        assign_y(nodes, *c, next_row);
    }
    let first = nodes[children[0]].y;
    let last = nodes[children[children.len() - 1]].y;
    nodes[idx].y = (first + last) / 2.0;
}

/// Renders mermaid `mindmap` source to an SVG (approximate layout).
///
/// # Errors
/// Returns [`MindmapParseError`] when the source is not a valid mindmap.
pub fn render_mindmap(source: &str, id: &str) -> Result<String, MindmapParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let tv = |k: &str| crate::render::themes::get(&theme_vars, k);
    let measurer = TextMeasurer::new();
    let mut nodes = parse(source, &measurer)?;
    if nodes.is_empty() {
        return Ok(empty_svg(id));
    }

    // Layout: x by depth, y by tidy-tree.
    let mut next_row = 0.0;
    assign_y(&mut nodes, 0, &mut next_row);
    for n in &mut nodes {
        n.x = n.depth as f64 * X_SPACING;
    }

    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style_el = append(&svg, "style");
    crate::svg::set_text(&style_el, &mindmap_css(id, &tv));

    let font_family = tv("fontFamily");
    let edges_g = append(&svg, "g");
    set_attr(&edges_g, "class", "mindmap-edges");
    // Edges: cubic curve from parent right side to child left side.
    for i in 0..nodes.len() {
        for &c in &nodes[i].children.clone() {
            let (px, py, pw) = (nodes[i].x, nodes[i].y, nodes[i].w);
            let (cx, cy, cw) = (nodes[c].x, nodes[c].y, nodes[c].w);
            let x1 = px + pw / 2.0;
            let x2 = cx - cw / 2.0;
            let mx = (x1 + x2) / 2.0;
            let path = append(&edges_g, "path");
            let mut d = String::new();
            let _ = write!(
                d,
                "M{},{}C{},{},{},{},{},{}",
                js_num(x1),
                js_num(py),
                js_num(mx),
                js_num(py),
                js_num(mx),
                js_num(cy),
                js_num(x2),
                js_num(cy)
            );
            set_attr(&path, "d", d);
            set_attr(
                &path,
                "class",
                format!("edge section-edge-{}", section_of(&nodes, c)),
            );
        }
    }

    let nodes_g = append(&svg, "g");
    set_attr(&nodes_g, "class", "mindmap-nodes");
    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for i in 0..nodes.len() {
        let n = &nodes[i];
        let sect = section_of(&nodes, i);
        let g = append(&nodes_g, "g");
        set_attr(&g, "class", format!("mindmap-node section-{sect}"));
        set_attr(
            &g,
            "transform",
            format!("translate({}, {})", js_num(n.x), js_num(n.y)),
        );
        let (hw, hh) = (n.w / 2.0, NODE_H / 2.0);
        match n.shape {
            Shape::Circle => {
                let r = hw.max(hh);
                let c = append(&g, "circle");
                set_attr(&c, "class", "node-bkg");
                set_attr(&c, "r", js_num(r));
                set_attr(&c, "cx", "0");
                set_attr(&c, "cy", "0");
                minx = minx.min(n.x - r);
                maxx = maxx.max(n.x + r);
                miny = miny.min(n.y - r);
                maxy = maxy.max(n.y + r);
            }
            _ => {
                let rx = if matches!(n.shape, Shape::Rect | Shape::Hexagon) {
                    0.0
                } else {
                    12.0
                };
                let r = append(&g, "rect");
                set_attr(&r, "class", "node-bkg");
                set_attr(&r, "x", js_num(-hw));
                set_attr(&r, "y", js_num(-hh));
                set_attr(&r, "width", js_num(n.w));
                set_attr(&r, "height", js_num(NODE_H));
                set_attr(&r, "rx", js_num(rx));
                set_attr(&r, "ry", js_num(rx));
                minx = minx.min(n.x - hw);
                maxx = maxx.max(n.x + hw);
                miny = miny.min(n.y - hh);
                maxy = maxy.max(n.y + hh);
            }
        }
        let text = append(&g, "text");
        set_attr(&text, "class", "mindmap-label");
        set_attr(&text, "x", "0");
        set_attr(&text, "y", "0");
        set_attr(&text, "text-anchor", "middle");
        set_attr(&text, "dominant-baseline", "middle");
        set_attr(
            &text,
            "style",
            format!(
                "font-family:{font_family};font-size:{}px;",
                js_num(FONT_SIZE)
            ),
        );
        set_text(&text, &n.text);
    }

    let pad = 20.0;
    let (w, h) = (maxx - minx + 2.0 * pad, maxy - miny + 2.0 * pad);
    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} {} {} {}",
            js_num(minx - pad),
            js_num(miny - pad),
            js_num(w),
            js_num(h)
        ),
    );
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(w)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "mindmap");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// The section index (0..11) a node belongs to: the top-level ancestor's order.
fn section_of(nodes: &[Node], mut idx: usize) -> usize {
    // walk up to depth 1 (the branch under root); root itself is section -1→"root"
    if nodes[idx].depth == 0 {
        return 0;
    }
    while nodes[idx].depth > 1 {
        // find parent
        let me = idx;
        idx = (0..nodes.len())
            .find(|&p| nodes[p].children.contains(&me))
            .unwrap_or(0);
    }
    // index among root's children
    let root_children = &nodes[0].children;
    root_children.iter().position(|&c| c == idx).unwrap_or(0) % 12
}

fn mindmap_css(id: &str, tv: &dyn Fn(&str) -> String) -> String {
    let font = tv("fontFamily");
    let line = tv("lineColor");
    let text_color = tv("textColor");
    let mut o = String::new();
    let _ = write!(
        o,
        "#{id}{{font-family:{font};}}\
         #{id} .mindmap-label{{fill:{text_color};}}\
         #{id} .edge{{fill:none;stroke:{line};stroke-width:2px;}}\
         #{id} .node-bkg{{stroke:{line};stroke-width:1px;}}"
    );
    // section fills from theme cScale.
    for n in 0..12 {
        let c = tv(&format!("cScale{n}"));
        let _ = write!(o, "#{id} .section-{n} .node-bkg{{fill:{c};}}");
    }
    o
}

fn empty_svg(id: &str) -> String {
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "aria-roledescription", "mindmap");
    let mut out = String::new();
    serialize(&svg, &mut out);
    out
}
