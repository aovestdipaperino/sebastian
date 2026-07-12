//! Byte-exact port of mermaid 11.15.0 `treemap-beta` diagrams.
//!
//! Ports the langium indentation grammar, `buildHierarchy`, the d3-hierarchy
//! `sum`/`sort` + `treemap().round(true)` squarify layout (with the
//! per-node paddingTop/Inner/Left/Right/Bottom callbacks), and the SVG
//! renderer (sections, leaves, rotated-free labels/values, lazy `scaleOrdinal`
//! colors). Text-fitting (font shrink / ellipsis) is measurement-driven; the
//! corpus keeps cells large and labels short so it never triggers.

// These fire on faithful ports of d3's imperative squarify/positionNode code.
#![allow(
    clippy::cast_possible_truncation,
    clippy::too_many_arguments,
    clippy::if_not_else,
    clippy::manual_midpoint,
    clippy::manual_clamp,
    clippy::ptr_arg
)]

use std::collections::HashMap;

use crate::svg::{append, js_num, new_element, serialize, set_attr, set_text};

const SECTION_INNER_PADDING: f64 = 10.0;
const SECTION_HEADER_HEIGHT: f64 = 25.0;

/// Parse error for treemap diagrams.
#[derive(Debug)]
pub struct TreemapParseError(pub String);

impl std::fmt::Display for TreemapParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "treemap parse error: {}", self.0)
    }
}

impl std::error::Error for TreemapParseError {}

#[derive(Debug, Clone)]
struct FlatItem {
    level: usize,
    name: String,
    is_leaf: bool,
    value: Option<f64>,
}

/// A hierarchy node in the layout arena.
#[derive(Debug, Clone)]
struct HNode {
    name: String,
    value: f64,
    children: Vec<usize>,
    depth: usize,
    parent: Option<usize>,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

// ---------------------------------------------------------------------------
// Parser (langium treemap grammar, indentation-based rows)
// ---------------------------------------------------------------------------

fn parse(source: &str) -> Result<Vec<FlatItem>, TreemapParseError> {
    let mut items = Vec::new();
    let mut found_header = false;
    for raw in source.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        if !found_header {
            if trimmed == "treemap-beta" || trimmed == "treemap" {
                found_header = true;
                continue;
            }
            return Err(TreemapParseError(format!(
                "expected treemap header, got {trimmed:?}"
            )));
        }
        // accTitle/title/accDescr are ignored for layout.
        if trimmed.starts_with("title")
            || trimmed.starts_with("accTitle")
            || trimmed.starts_with("accDescr")
            || trimmed.starts_with("classDef")
        {
            continue;
        }
        // indentation = count of leading whitespace chars.
        let level = raw.len() - raw.trim_start().len();
        // item: "name" (Section) or "name": value / "name", value (Leaf).
        let Some(rest) = trimmed
            .strip_prefix('"')
            .or_else(|| trimmed.strip_prefix('\''))
        else {
            continue;
        };
        let quote = trimmed.as_bytes()[0] as char;
        let Some(end) = rest.find(quote) else {
            continue;
        };
        let name = rest[..end].to_owned();
        let after = rest[end + 1..].trim_start();
        // strip a `:::class` selector (ignored for layout)
        let after = after.split(":::").next().unwrap_or(after).trim();
        let (is_leaf, value) =
            if let Some(v) = after.strip_prefix(':').or_else(|| after.strip_prefix(',')) {
                let num: String = v
                    .trim()
                    .chars()
                    .filter(|c| *c != '_' && *c != ',')
                    .collect();
                (true, num.parse::<f64>().ok())
            } else {
                (false, None)
            };
        items.push(FlatItem {
            level,
            name,
            is_leaf,
            value,
        });
    }
    Ok(items)
}

// ---------------------------------------------------------------------------
// buildHierarchy + arena
// ---------------------------------------------------------------------------

/// Builds the layout arena. Index 0 is the synthetic root (name "", depth 0).
fn build_arena(items: &[FlatItem]) -> Vec<HNode> {
    let mut arena: Vec<HNode> = vec![HNode {
        name: String::new(),
        value: 0.0,
        children: Vec::new(),
        depth: 0,
        parent: None,
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    }];
    // stack of (arena index, level) for parents that can have children.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for item in items {
        let idx = arena.len();
        // find the parent
        while let Some(&(_, lvl)) = stack.last() {
            if lvl >= item.level {
                stack.pop();
            } else {
                break;
            }
        }
        let parent = stack.last().map_or(0, |&(p, _)| p);
        let depth = arena[parent].depth + 1;
        arena.push(HNode {
            name: item.name.clone(),
            value: item.value.unwrap_or(0.0),
            children: Vec::new(),
            depth,
            parent: Some(parent),
            x0: 0.0,
            y0: 0.0,
            x1: 0.0,
            y1: 0.0,
        });
        arena[parent].children.push(idx);
        if !item.is_leaf {
            stack.push((idx, item.level));
        }
    }
    arena
}

/// d3 hierarchy `.sum()`: post-order value accumulation (leaves keep their
/// parsed value; internal nodes sum their children).
fn sum_values(arena: &mut [HNode], idx: usize) -> f64 {
    let children = arena[idx].children.clone();
    if children.is_empty() {
        return arena[idx].value;
    }
    let mut s = 0.0;
    for c in children {
        s += sum_values(arena, c);
    }
    arena[idx].value = s;
    s
}

/// d3 `.sort((a,b) => (b.value ?? 0) - (a.value ?? 0))` — stable, descending.
fn sort_children(arena: &mut Vec<HNode>) {
    let n = arena.len();
    for i in 0..n {
        let mut ch = arena[i].children.clone();
        ch.sort_by(|&a, &b| {
            arena[b]
                .value
                .partial_cmp(&arena[a].value)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        arena[i].children = ch;
    }
}

// ---------------------------------------------------------------------------
// treemap layout (d3-hierarchy treemap + squarify + round)
// ---------------------------------------------------------------------------

const PHI: f64 = 1.618_033_988_749_895; // (1 + sqrt(5)) / 2

fn pad_top(arena: &[HNode], idx: usize) -> f64 {
    if arena[idx].children.is_empty() {
        0.0
    } else {
        SECTION_HEADER_HEIGHT + SECTION_INNER_PADDING
    }
}

fn pad_side(arena: &[HNode], idx: usize) -> f64 {
    if arena[idx].children.is_empty() {
        0.0
    } else {
        SECTION_INNER_PADDING
    }
}

fn layout(arena: &mut Vec<HNode>, width: f64, height: f64, inner_padding: f64, round: bool) {
    arena[0].x0 = 0.0;
    arena[0].y0 = 0.0;
    arena[0].x1 = width;
    arena[0].y1 = height;
    // eachBefore(positionNode)
    let mut padding_stack: Vec<f64> = vec![0.0];
    let mut stack: Vec<usize> = vec![0];
    while let Some(idx) = stack.pop() {
        position_node(arena, idx, &mut padding_stack, inner_padding);
        // push children in reverse for pre-order processing
        for &c in arena[idx].children.clone().iter().rev() {
            stack.push(c);
        }
    }
    if round {
        for n in arena.iter_mut() {
            n.x0 = n.x0.round();
            n.y0 = n.y0.round();
            n.x1 = n.x1.round();
            n.y1 = n.y1.round();
        }
    }
}

fn position_node(
    arena: &mut Vec<HNode>,
    idx: usize,
    padding_stack: &mut Vec<f64>,
    inner_padding: f64,
) {
    let depth = arena[idx].depth;
    let p = padding_stack[depth];
    let mut x0 = arena[idx].x0 + p;
    let mut y0 = arena[idx].y0 + p;
    let mut x1 = arena[idx].x1 - p;
    let mut y1 = arena[idx].y1 - p;
    if x1 < x0 {
        x0 = (x0 + x1) / 2.0;
        x1 = x0;
    }
    if y1 < y0 {
        y0 = (y0 + y1) / 2.0;
        y1 = y0;
    }
    arena[idx].x0 = x0;
    arena[idx].y0 = y0;
    arena[idx].x1 = x1;
    arena[idx].y1 = y1;
    if !arena[idx].children.is_empty() {
        let p = inner_padding / 2.0;
        if padding_stack.len() <= depth + 1 {
            padding_stack.resize(depth + 2, 0.0);
        }
        padding_stack[depth + 1] = p;
        let pl = pad_side(arena, idx);
        let pt = pad_top(arena, idx);
        let pr = pad_side(arena, idx);
        let pb = pad_side(arena, idx);
        x0 += pl - p;
        y0 += pt - p;
        x1 -= pr - p;
        y1 -= pb - p;
        if x1 < x0 {
            x0 = (x0 + x1) / 2.0;
            x1 = x0;
        }
        if y1 < y0 {
            y0 = (y0 + y1) / 2.0;
            y1 = y0;
        }
        squarify_ratio(arena, idx, x0, y0, x1, y1);
    }
}

fn dice(arena: &mut [HNode], children: &[usize], value: f64, x0: f64, y0: f64, x1: f64, y1: f64) {
    let k = if value != 0.0 { (x1 - x0) / value } else { 0.0 };
    let mut acc = x0;
    for &n in children {
        arena[n].y0 = y0;
        arena[n].y1 = y1;
        arena[n].x0 = acc;
        acc += arena[n].value * k;
        arena[n].x1 = acc;
    }
}

fn slice(arena: &mut [HNode], children: &[usize], value: f64, x0: f64, y0: f64, x1: f64, y1: f64) {
    let k = if value != 0.0 { (y1 - y0) / value } else { 0.0 };
    let mut acc = y0;
    for &n in children {
        arena[n].x0 = x0;
        arena[n].x1 = x1;
        arena[n].y0 = acc;
        acc += arena[n].value * k;
        arena[n].y1 = acc;
    }
}

/// Port of d3 `squarifyRatio(phi, parent, x0, y0, x1, y1)`.
fn squarify_ratio(arena: &mut Vec<HNode>, parent: usize, x0: f64, y0: f64, x1: f64, y1: f64) {
    let ratio = PHI;
    let nodes = arena[parent].children.clone();
    let n = nodes.len();
    let mut value = arena[parent].value;
    let (mut x0, mut y0, x1) = (x0, y0, x1);
    let mut i0 = 0;
    let mut i1 = 0;
    while i0 < n {
        let dx = x1 - x0;
        let dy = y1 - y0;
        // next non-empty node
        let mut sum_value;
        loop {
            sum_value = arena[nodes[i1]].value;
            i1 += 1;
            if sum_value != 0.0 || i1 >= n {
                break;
            }
        }
        let mut min_value = sum_value;
        let mut max_value = sum_value;
        let alpha = (dy / dx).max(dx / dy) / (value * ratio);
        let mut beta = sum_value * sum_value * alpha;
        let mut min_ratio = (max_value / beta).max(beta / min_value);
        while i1 < n {
            let node_value = arena[nodes[i1]].value;
            sum_value += node_value;
            if node_value < min_value {
                min_value = node_value;
            }
            if node_value > max_value {
                max_value = node_value;
            }
            beta = sum_value * sum_value * alpha;
            let new_ratio = (max_value / beta).max(beta / min_value);
            if new_ratio > min_ratio {
                sum_value -= node_value;
                break;
            }
            min_ratio = new_ratio;
            i1 += 1;
        }
        let row: Vec<usize> = nodes[i0..i1].to_vec();
        let is_dice = dx < dy;
        if is_dice {
            if value != 0.0 {
                let ny = y0 + dy * sum_value / value;
                dice(arena, &row, sum_value, x0, y0, x1, ny);
                y0 = ny;
            } else {
                dice(arena, &row, sum_value, x0, y0, x1, y1);
            }
        } else if value != 0.0 {
            let nx = x0 + dx * sum_value / value;
            slice(arena, &row, sum_value, x0, y0, nx, y1);
            x0 = nx;
        } else {
            slice(arena, &row, sum_value, x0, y0, x1, y1);
        }
        value -= sum_value;
        i0 = i1;
    }
}

// ---------------------------------------------------------------------------
// Lazy ordinal color scale (d3 scaleOrdinal)
// ---------------------------------------------------------------------------

struct Ordinal {
    range: Vec<String>,
    domain: HashMap<String, usize>,
    next: usize,
}

impl Ordinal {
    fn new(range: Vec<String>) -> Self {
        Ordinal {
            range,
            domain: HashMap::new(),
            next: 0,
        }
    }
    fn get(&mut self, name: &str) -> String {
        let idx = *self.domain.entry(name.to_owned()).or_insert_with(|| {
            let i = self.next;
            self.next += 1;
            i
        });
        self.range[idx % self.range.len()].clone()
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// d3 `format(',')` for a value (thousands separators; integers common).
fn value_format(value: f64) -> String {
    // Match d3's default `,` format: group integer part, keep decimals as-is.
    if value.fract() == 0.0 {
        let neg = value < 0.0;
        let digits = format!("{}", value.abs() as i64);
        let bytes = digits.as_bytes();
        let mut out = String::new();
        let len = bytes.len();
        for (i, b) in bytes.iter().enumerate() {
            if i > 0 && (len - i) % 3 == 0 {
                out.push(',');
            }
            out.push(*b as char);
        }
        if neg { format!("-{out}") } else { out }
    } else {
        js_num(value)
    }
}

/// Renders mermaid `treemap-beta` source to a complete SVG document string.
///
/// # Errors
/// Returns [`TreemapParseError`] when the source is not a valid treemap.
pub fn render_treemap(source: &str, id: &str) -> Result<String, TreemapParseError> {
    let config = crate::render::config::detect_init(source);
    let hand_drawn = config.is_hand_drawn();
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let tv = |k: &str| crate::render::themes::get(&theme_vars, k);

    let measurer = crate::text::TextMeasurer::new();
    let items = parse(source)?;
    let mut arena = build_arena(&items);
    sum_values(&mut arena, 0);
    sort_children(&mut arena);

    // Config defaults: nodeWidth=100, nodeHeight=40, padding=10.
    let width = 100.0 * SECTION_INNER_PADDING; // 1000
    let height = 40.0 * SECTION_INNER_PADDING; // 400
    layout(&mut arena, width, height, SECTION_INNER_PADDING, true);

    // eachBefore order of node indices.
    let mut order: Vec<usize> = Vec::new();
    {
        let mut stack = vec![0usize];
        while let Some(idx) = stack.pop() {
            order.push(idx);
            for &c in arena[idx].children.clone().iter().rev() {
                stack.push(c);
            }
        }
    }
    let branch_nodes: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| !arena[i].children.is_empty())
        .collect();
    let leaf_nodes: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| arena[i].children.is_empty())
        .collect();

    // Color scales.
    let scale = |prefix: &str, with_transparent: bool| -> Vec<String> {
        let mut r = Vec::new();
        if with_transparent {
            r.push("transparent".to_owned());
        }
        for n in 0..12 {
            r.push(tv(&format!("{prefix}{n}")));
        }
        r
    };
    let mut color_scale = Ordinal::new(scale("cScale", true));
    let mut color_peer = Ordinal::new(scale("cScalePeer", true));
    let mut color_label = Ordinal::new(scale("cScaleLabel", false));

    // SVG scaffold.
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    // viewBox + style filled in after bbox.
    let style_el = append(&svg, "style");
    crate::svg::set_text(
        &style_el,
        &crate::render::css::themed_treemap_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");
    let g = append(&svg, "g");
    set_attr(&g, "transform", "translate(0, 0)");
    set_attr(&g, "class", "treemapContainer");

    // Manual bbox accumulation of visible rects (display:none root excluded).
    let mut bx0 = f64::INFINITY;
    let mut by0 = f64::INFINITY;
    let mut bx1 = f64::NEG_INFINITY;
    let mut by1 = f64::NEG_INFINITY;
    let mut acc = |x: f64, y: f64, w: f64, h: f64| {
        bx0 = bx0.min(x);
        by0 = by0.min(y);
        bx1 = bx1.max(x + w);
        by1 = by1.max(y + h);
    };

    // Sections.
    for (i, &ni) in branch_nodes.iter().enumerate() {
        let n = &arena[ni];
        let (nx0, ny0, nw, nh) = (n.x0, n.y0, n.x1 - n.x0, n.y1 - n.y0);
        let is_root = n.depth == 0;
        let sec = append(&g, "g");
        set_attr(&sec, "class", "treemapSection");
        set_attr(
            &sec,
            "transform",
            format!("translate({},{})", js_num(nx0), js_num(ny0)),
        );
        // header rect
        let hr = append(&sec, "rect");
        set_attr(&hr, "width", js_num(nw));
        set_attr(&hr, "height", js_num(SECTION_HEADER_HEIGHT));
        set_attr(&hr, "class", "treemapSectionHeader");
        set_attr(&hr, "fill", "none");
        set_attr(&hr, "fill-opacity", "0.6");
        set_attr(&hr, "stroke-width", "0.6");
        set_attr(&hr, "style", if is_root { "display: none;" } else { "" });
        // clipPath
        let cp = append(&sec, "clipPath");
        set_attr(&cp, "id", format!("clip-section-{id}-{i}"));
        let cpr = append(&cp, "rect");
        set_attr(&cpr, "width", js_num((nw - 12.0).max(0.0)));
        set_attr(&cpr, "height", js_num(SECTION_HEADER_HEIGHT));
        // section rect
        let name = arena[ni].name.clone();
        let sr = append(&sec, "rect");
        set_attr(&sr, "width", js_num(nw));
        set_attr(&sr, "height", js_num(nh));
        set_attr(&sr, "class", format!("treemapSection section{i}"));
        set_attr(&sr, "fill", color_scale.get(&name));
        set_attr(&sr, "fill-opacity", "0.6");
        set_attr(&sr, "stroke", color_peer.get(&name));
        set_attr(&sr, "stroke-width", "2");
        set_attr(&sr, "stroke-opacity", "0.4");
        if hand_drawn && !is_root {
            set_attr(&sr, "style", "stroke:none");
            let ol = crate::render::handdrawn::hd_overlay_rect(
                &sec,
                0.0,
                0.0,
                nw,
                nh,
                &color_peer.get(&name),
                "",
            );
            set_attr(&ol, "stroke-width", "2");
            set_attr(&ol, "stroke-opacity", "0.4");
        }
        set_attr(
            &sr,
            "style",
            if is_root {
                "display: none;".to_owned()
            } else {
                ";".to_owned()
            },
        );
        // label
        let lbl = append(&sec, "text");
        set_attr(&lbl, "class", "treemapSectionLabel");
        set_attr(&lbl, "x", "6");
        set_attr(&lbl, "y", js_num(SECTION_HEADER_HEIGHT / 2.0));
        set_attr(&lbl, "dominant-baseline", "middle");
        set_attr(&lbl, "font-weight", "bold");
        if is_root {
            set_attr(&lbl, "style", "display: none;");
        } else {
            let c = color_label.get(&name);
            set_attr(
                &lbl,
                "style",
                format!(
                    "dominant-baseline: middle; font-size: 12px; fill:{c}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;"
                ),
            );
            set_text(&lbl, &name);
        }
        // value
        let val = append(&sec, "text");
        set_attr(&val, "class", "treemapSectionValue");
        set_attr(&val, "x", js_num(nw - 10.0));
        set_attr(&val, "y", js_num(SECTION_HEADER_HEIGHT / 2.0));
        set_attr(&val, "text-anchor", "end");
        set_attr(&val, "dominant-baseline", "middle");
        set_attr(&val, "font-style", "italic");
        if is_root {
            set_attr(&val, "style", "display: none;");
        } else {
            let c = color_label.get(&name);
            set_attr(
                &val,
                "style",
                format!(
                    "text-anchor: end; dominant-baseline: middle; font-size: 10px; fill:{c}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;"
                ),
            );
        }
        set_text(&val, &value_format(arena[ni].value));
        if !is_root {
            acc(nx0, ny0, nw, SECTION_HEADER_HEIGHT);
            acc(nx0, ny0, nw, nh);
        }
    }

    // Leaves.
    for (i, &ni) in leaf_nodes.iter().enumerate() {
        let n = &arena[ni];
        let (nx0, ny0, nw, nh) = (n.x0, n.y0, n.x1 - n.x0, n.y1 - n.y0);
        let parent_name = n
            .parent
            .map_or_else(|| n.name.clone(), |p| arena[p].name.clone());
        let name = n.name.clone();
        let cell = append(&g, "g");
        set_attr(
            &cell,
            "class",
            format!("treemapNode treemapLeafGroup leaf{i}x"),
        );
        set_attr(
            &cell,
            "transform",
            format!("translate({},{})", js_num(nx0), js_num(ny0)),
        );
        let rect = append(&cell, "rect");
        set_attr(&rect, "width", js_num(nw));
        set_attr(&rect, "height", js_num(nh));
        set_attr(&rect, "class", "treemapLeaf");
        let fill = color_scale.get(&parent_name);
        set_attr(&rect, "fill", &fill);
        set_attr(&rect, "style", "");
        set_attr(&rect, "fill-opacity", "0.3");
        set_attr(&rect, "stroke", &fill);
        if hand_drawn {
            set_attr(&rect, "style", "stroke:none");
            crate::render::handdrawn::hd_overlay_rect(&cell, 0.0, 0.0, nw, nh, &fill, "");
        }
        set_attr(&rect, "stroke-width", "3");
        let cp = append(&cell, "clipPath");
        set_attr(&cp, "id", format!("clip-{id}-{i}"));
        let cpr = append(&cp, "rect");
        set_attr(&cpr, "width", js_num((nw - 4.0).max(0.0)));
        set_attr(&cpr, "height", js_num((nh - 4.0).max(0.0)));
        // label (font-size fixed at 38px; corpus guarantees it fits)
        let lbl = append(&cell, "text");
        set_attr(&lbl, "class", "treemapLabel");
        set_attr(&lbl, "x", js_num(nw / 2.0));
        set_attr(&lbl, "y", js_num(nh / 2.0));
        let lc = color_label.get(&name);
        let raw_color = lc.clone();
        // Label font fitting (renderer.ts leafLabels.each). getComputedTextLength
        // is Trebuchet advance rounded to 1/64 (measure_advance_svg).
        let avail_w = nw - 8.0;
        let avail_h = nh - 8.0;
        let mut font = 38.0f64;
        let label_hidden;
        if avail_w < 10.0 || avail_h < 10.0 {
            label_hidden = true;
        } else {
            while measurer.measure_advance_svg(&name, font) > avail_w && font > 8.0 {
                font -= 1.0;
            }
            let pv = |f: f64| (f * 0.6).round().min(28.0).max(6.0);
            let mut comb = font + 2.0 + pv(font);
            while comb > avail_h && font > 8.0 {
                font -= 1.0;
                if pv(font) < 6.0 && font == 8.0 {
                    break;
                }
                comb = font + 2.0 + pv(font);
            }
            label_hidden =
                measurer.measure_advance_svg(&name, font) > avail_w || font < 8.0 || avail_h < font;
        }
        let shrunk = (font - 38.0).abs() > f64::EPSILON;
        if label_hidden {
            // display:none via a later .style() call → CSSOM-canonical.
            set_attr(
                &lbl,
                "style",
                format!(
                    "text-anchor: middle; dominant-baseline: middle; font-size: {}px; fill: {}; display: none;",
                    js_num(font),
                    crate::render::css::cssom_color_value("fill", &lc)
                ),
            );
        } else if shrunk {
            set_attr(
                &lbl,
                "style",
                format!(
                    "text-anchor: middle; dominant-baseline: middle; font-size: {}px; fill: {};",
                    js_num(font),
                    crate::render::css::cssom_color_value("fill", &lc)
                ),
            );
        } else {
            set_attr(
                &lbl,
                "style",
                format!(
                    "text-anchor: middle; dominant-baseline: middle; font-size: 38px;fill:{raw_color};"
                ),
            );
        }
        set_attr(&lbl, "clip-path", format!("url(#clip-{id}-{i})"));
        set_text(&lbl, &name);
        // value
        let val_fs = (font * 0.6).round().min(28.0).max(6.0);
        let value_top = nh / 2.0 + font / 2.0 + 2.0;
        let avail_w_val = nw - 8.0;
        let value_hidden = label_hidden
            || measurer.measure_advance_svg(&value_format(arena[ni].value), val_fs) > avail_w_val
            || value_top + val_fs > nh - 4.0
            || val_fs < 6.0;
        let val = append(&cell, "text");
        set_attr(&val, "class", "treemapValue");
        set_attr(&val, "x", js_num(nw / 2.0));
        set_attr(&val, "y", js_num(value_top));
        // The leaf value always gets a later `.style()` call → CSSOM-canonical.
        let vc = crate::render::css::cssom_color_value("fill", &color_label.get(&name));
        let disp = if value_hidden { " display: none;" } else { "" };
        set_attr(
            &val,
            "style",
            format!(
                "text-anchor: middle; dominant-baseline: hanging; font-size: {}px; fill: {vc};{disp}",
                js_num(val_fs)
            ),
        );
        set_attr(&val, "clip-path", format!("url(#clip-{id}-{i})"));
        set_text(&val, &value_format(arena[ni].value));
        acc(nx0, ny0, nw, nh);
    }

    // viewBox = content bbox ± padding 8 (setupViewPortForSVG).
    let padding = 8.0;
    let (cw, ch) = (bx1 - bx0, by1 - by0);
    let vw = cw + 2.0 * padding;
    let vh = ch + 2.0 * padding;
    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} {} {} {}",
            js_num(bx0 - padding),
            js_num(by0 - padding),
            js_num(vw),
            js_num(vh)
        ),
    );
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(vw)
        ),
    );
    set_attr(&svg, "class", "flowchart");
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "treemap");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
