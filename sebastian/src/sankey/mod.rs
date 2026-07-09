//! sankey-beta support: a port of the sankeyDB CSV parser, the `d3-sankey`
//! iterative layout (`sankey.js`), `sankeyLinkHorizontal`, and the mermaid
//! `sankeyRenderer.ts`. The layout is deterministic (no rough.js / randomness)
//! but float-order-sensitive across its six relaxation iterations, so every
//! arithmetic op mirrors the JS exactly (incl. `Math.pow` via `core_math`).
//!
//! Note: mermaid sizes the viewBox via `setupGraphViewbox` (Blink `getBBox`),
//! which — as in the rest of this engine — ignores `<text>` extent. Fixtures
//! must therefore keep node labels within the node bounding box (the common
//! case, where the first/last columns span the full width/height).

#![allow(clippy::assigning_clones)]
// `(a + b) / 2.0` and `!(w > 0.0)` are byte-exact ports of the JS layout math
// (d3-sankey) and must not be rewritten to `f64::midpoint` / `partial_cmp`.
#![allow(clippy::manual_midpoint)]
#![allow(clippy::neg_cmp_op_on_partial_ord)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::cast_possible_wrap)]
use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};
use std::cmp::Ordering;
use std::fmt::Write as _;

/// A parse error for sankey source.
#[derive(Debug)]
pub struct SankeyParseError {
    pub message: String,
}

impl std::fmt::Display for SankeyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sankey parse error: {}", self.message)
    }
}

impl std::error::Error for SankeyParseError {}

// Fixed defaults from `DEFAULT_CONFIG.sankey` (config.schema.yaml).
const WIDTH: f64 = 600.0;
const HEIGHT: f64 = 400.0;
const NODE_WIDTH: f64 = 10.0;
const NODE_PADDING: f64 = 12.0;
const ITERATIONS: usize = 6;

/// `schemeTableau10` (d3-scale-chromatic), used by `scaleOrdinal`.
const TABLEAU10: [&str; 10] = [
    "#4e79a7", "#f28e2c", "#e15759", "#76b7b2", "#59a14f", "#edc949", "#af7aa1", "#ff9da7",
    "#9c755f", "#bab0ab",
];

#[derive(Default, Clone)]
struct Node {
    source_links: Vec<usize>,
    target_links: Vec<usize>,
    value: f64,
    depth: i64,
    height: i64,
    layer: i64,
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

#[derive(Clone)]
struct Link {
    source: usize,
    target: usize,
    value: f64,
    width: f64,
    y0: f64,
    y1: f64,
}

/// Split a CSV line into fields, honoring double-quoted fields with `""`
/// escapes (d3-dsv semantics, comma delimiter).
fn split_csv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            out.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

struct Graph {
    ids: Vec<String>,
    nodes: Vec<Node>,
    links: Vec<Link>,
}

fn parse(source: &str) -> Result<Graph, SankeyParseError> {
    let mut ids: Vec<String> = Vec::new();
    let index_of = |ids: &mut Vec<String>, id: &str| -> usize {
        if let Some(p) = ids.iter().position(|x| x == id) {
            p
        } else {
            ids.push(id.to_owned());
            ids.len() - 1
        }
    };
    let mut links: Vec<Link> = Vec::new();
    let mut found_header = false;
    for raw in source.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if !found_header {
            if trimmed.is_empty() || trimmed.starts_with("%%") {
                continue;
            }
            if trimmed == "sankey-beta" {
                found_header = true;
                continue;
            }
            return Err(SankeyParseError {
                message: format!("expected sankey-beta header, got {trimmed:?}"),
            });
        }
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        let fields = split_csv(line);
        if fields.len() < 3 {
            return Err(SankeyParseError {
                message: format!("bad sankey row (need source,target,value): {line}"),
            });
        }
        let src = fields[0].trim();
        let tgt = fields[1].trim();
        let value: f64 = fields[2].trim().parse().map_err(|_| SankeyParseError {
            message: format!("bad sankey value: {line}"),
        })?;
        let s = index_of(&mut ids, src);
        let t = index_of(&mut ids, tgt);
        links.push(Link {
            source: s,
            target: t,
            value,
            width: 0.0,
            y0: 0.0,
            y1: 0.0,
        });
    }
    if !found_header {
        return Err(SankeyParseError {
            message: "missing sankey-beta header".to_owned(),
        });
    }
    let nodes = vec![Node::default(); ids.len()];
    Ok(Graph { ids, nodes, links })
}

fn f64cmp(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

/// Sort a node's `sourceLinks` by `ascendingTargetBreadth` (target y0, then
/// link index).
fn sort_source_links(nodes: &mut [Node], links: &[Link], ni: usize) {
    let mut sl = std::mem::take(&mut nodes[ni].source_links);
    sl.sort_by(|&a, &b| {
        f64cmp(nodes[links[a].target].y0, nodes[links[b].target].y0).then(a.cmp(&b))
    });
    nodes[ni].source_links = sl;
}

/// Sort a node's `targetLinks` by `ascendingSourceBreadth` (source y0, then
/// link index).
fn sort_target_links(nodes: &mut [Node], links: &[Link], ni: usize) {
    let mut tl = std::mem::take(&mut nodes[ni].target_links);
    tl.sort_by(|&a, &b| {
        f64cmp(nodes[links[a].source].y0, nodes[links[b].source].y0).then(a.cmp(&b))
    });
    nodes[ni].target_links = tl;
}

fn reorder_node_links(nodes: &mut [Node], links: &[Link], ni: usize) {
    let target_links = nodes[ni].target_links.clone();
    for &l in &target_links {
        sort_source_links(nodes, links, links[l].source);
    }
    let source_links = nodes[ni].source_links.clone();
    for &l in &source_links {
        sort_target_links(nodes, links, links[l].target);
    }
}

/// `targetTop(source, target)` — the ideal `target.y0` for this link.
fn target_top(nodes: &[Node], links: &[Link], s: usize, t: usize, py: f64) -> f64 {
    let mut y = nodes[s].y0 - (nodes[s].source_links.len() as f64 - 1.0) * py / 2.0;
    for &l in &nodes[s].source_links {
        if links[l].target == t {
            break;
        }
        y += links[l].width + py;
    }
    for &l in &nodes[t].target_links {
        if links[l].source == s {
            break;
        }
        y -= links[l].width;
    }
    y
}

/// `sourceTop(source, target)` — the ideal `source.y0` for this link.
fn source_top(nodes: &[Node], links: &[Link], s: usize, t: usize, py: f64) -> f64 {
    let mut y = nodes[t].y0 - (nodes[t].target_links.len() as f64 - 1.0) * py / 2.0;
    for &l in &nodes[t].target_links {
        if links[l].source == s {
            break;
        }
        y += links[l].width + py;
    }
    for &l in &nodes[s].source_links {
        if links[l].target == t {
            break;
        }
        y -= links[l].width;
    }
    y
}

fn resolve_collisions_top_to_bottom(
    nodes: &mut [Node],
    col: &[usize],
    mut y: f64,
    start: usize,
    alpha: f64,
    py: f64,
) {
    let mut i = start;
    while i < col.len() {
        let node = &mut nodes[col[i]];
        let dy = (y - node.y0) * alpha;
        if dy > 1e-6 {
            node.y0 += dy;
            node.y1 += dy;
        }
        y = node.y1 + py;
        i += 1;
    }
}

fn resolve_collisions_bottom_to_top(
    nodes: &mut [Node],
    col: &[usize],
    mut y: f64,
    start: isize,
    alpha: f64,
    py: f64,
) {
    let mut i = start;
    while i >= 0 {
        let node = &mut nodes[col[i as usize]];
        let dy = (node.y1 - y) * alpha;
        if dy > 1e-6 {
            node.y0 -= dy;
            node.y1 -= dy;
        }
        y = node.y0 - py;
        i -= 1;
    }
}

fn resolve_collisions(nodes: &mut [Node], col: &[usize], alpha: f64, py: f64, y0: f64, y1: f64) {
    let i = col.len() >> 1;
    let subject_y0 = nodes[col[i]].y0;
    let subject_y1 = nodes[col[i]].y1;
    resolve_collisions_bottom_to_top(nodes, col, subject_y0 - py, i as isize - 1, alpha, py);
    resolve_collisions_top_to_bottom(nodes, col, subject_y1 + py, i + 1, alpha, py);
    resolve_collisions_bottom_to_top(nodes, col, y1, col.len() as isize - 1, alpha, py);
    resolve_collisions_top_to_bottom(nodes, col, y0, 0, alpha, py);
}

#[allow(clippy::too_many_lines)]
fn layout(g: &mut Graph) {
    let (x0, y0, x1, y1) = (0.0, 0.0, WIDTH, HEIGHT);
    let dx = NODE_WIDTH;
    // showValues is true by default → nodePadding + 15.
    let dy = NODE_PADDING + 15.0;
    let nodes = &mut g.nodes;
    let links = &g.links;

    // computeNodeLinks
    for (i, link) in links.iter().enumerate() {
        nodes[link.source].source_links.push(i);
        nodes[link.target].target_links.push(i);
    }

    // computeNodeValues
    for node in nodes.iter_mut() {
        let s: f64 = node.source_links.iter().map(|&l| links[l].value).sum();
        let t: f64 = node.target_links.iter().map(|&l| links[l].value).sum();
        node.value = s.max(t);
    }

    // computeNodeDepths (longest path from a source)
    compute_ranks(nodes, links, true);
    // computeNodeHeights (longest path to a sink)
    compute_ranks(nodes, links, false);

    // computeNodeLayers
    let max_depth = nodes.iter().map(|n| n.depth).max().unwrap_or(0);
    let x = max_depth + 1;
    let kx = (x1 - x0 - dx) / (x - 1) as f64;
    let mut columns: Vec<Vec<usize>> = vec![Vec::new(); x as usize];
    for (ni, node) in nodes.iter_mut().enumerate() {
        // align = justify
        let a = if node.source_links.is_empty() {
            (x - 1) as f64
        } else {
            node.depth as f64
        };
        let i = (a.floor() as i64).clamp(0, x - 1);
        node.layer = i;
        node.x0 = x0 + i as f64 * kx;
        node.x1 = node.x0 + dx;
        columns[i as usize].push(ni);
    }

    // computeNodeBreadths
    let max_col = columns.iter().map(Vec::len).max().unwrap_or(0);
    let py = dy.min((y1 - y0) / (max_col as f64 - 1.0));

    // initializeNodeBreadths
    let ky = columns
        .iter()
        .map(|c| {
            let sum: f64 = c.iter().map(|&n| nodes[n].value).sum();
            (y1 - y0 - (c.len() as f64 - 1.0) * py) / sum
        })
        .fold(f64::INFINITY, f64::min);
    init_breadths(nodes, &mut g.links, &columns, y0, y1, py, ky);

    // iterations
    let links = &g.links;
    for iter in 0..ITERATIONS {
        let alpha = crate::mathx::pow(0.99, iter as f64);
        let beta = (1.0 - alpha).max((iter as f64 + 1.0) / ITERATIONS as f64);
        relax_right_to_left(nodes, links, &columns, alpha, beta, py, y0, y1);
        relax_left_to_right(nodes, links, &columns, alpha, beta, py, y0, y1);
    }

    // computeLinkBreadths
    let links = &mut g.links;
    for node in nodes.iter() {
        let mut yy0 = node.y0;
        let mut yy1 = node.y0;
        for &l in &node.source_links {
            links[l].y0 = yy0 + links[l].width / 2.0;
            yy0 += links[l].width;
        }
        for &l in &node.target_links {
            links[l].y1 = yy1 + links[l].width / 2.0;
            yy1 += links[l].width;
        }
    }
}

fn init_breadths(
    nodes: &mut [Node],
    links: &mut [Link],
    columns: &[Vec<usize>],
    y0: f64,
    y1: f64,
    py: f64,
    ky: f64,
) {
    for col in columns {
        let mut y = y0;
        for &ni in col {
            nodes[ni].y0 = y;
            nodes[ni].y1 = y + nodes[ni].value * ky;
            y = nodes[ni].y1 + py;
            let sl = nodes[ni].source_links.clone();
            for l in sl {
                links[l].width = links[l].value * ky;
            }
        }
        y = (y1 - y + py) / (col.len() as f64 + 1.0);
        for (i, &ni) in col.iter().enumerate() {
            nodes[ni].y0 += y * (i as f64 + 1.0);
            nodes[ni].y1 += y * (i as f64 + 1.0);
        }
        for &ni in col {
            sort_source_links(nodes, links, ni);
            sort_target_links(nodes, links, ni);
        }
    }
}

fn relax_left_to_right(
    nodes: &mut [Node],
    links: &[Link],
    columns: &[Vec<usize>],
    alpha: f64,
    beta: f64,
    py: f64,
    y0: f64,
    y1: f64,
) {
    for col_ref in columns.iter().skip(1) {
        let col = col_ref.clone();
        for &ti in &col {
            let mut y = 0.0;
            let mut w = 0.0;
            let tl = nodes[ti].target_links.clone();
            for &l in &tl {
                let s = links[l].source;
                let v = links[l].value * (nodes[ti].layer - nodes[s].layer) as f64;
                y += target_top(nodes, links, s, ti, py) * v;
                w += v;
            }
            if !(w > 0.0) {
                continue;
            }
            let dy = (y / w - nodes[ti].y0) * alpha;
            nodes[ti].y0 += dy;
            nodes[ti].y1 += dy;
            reorder_node_links(nodes, links, ti);
        }
        let mut sorted = col.clone();
        sorted.sort_by(|&a, &b| f64cmp(nodes[a].y0, nodes[b].y0));
        resolve_collisions(nodes, &sorted, beta, py, y0, y1);
    }
}

fn relax_right_to_left(
    nodes: &mut [Node],
    links: &[Link],
    columns: &[Vec<usize>],
    alpha: f64,
    beta: f64,
    py: f64,
    y0: f64,
    y1: f64,
) {
    let last = columns.len().saturating_sub(1);
    for col_ref in columns[..last].iter().rev() {
        let col = col_ref.clone();
        for &si in &col {
            let mut y = 0.0;
            let mut w = 0.0;
            let sl = nodes[si].source_links.clone();
            for &l in &sl {
                let t = links[l].target;
                let v = links[l].value * (nodes[t].layer - nodes[si].layer) as f64;
                y += source_top(nodes, links, si, t, py) * v;
                w += v;
            }
            if !(w > 0.0) {
                continue;
            }
            let dy = (y / w - nodes[si].y0) * alpha;
            nodes[si].y0 += dy;
            nodes[si].y1 += dy;
            reorder_node_links(nodes, links, si);
        }
        let mut sorted = col.clone();
        sorted.sort_by(|&a, &b| f64cmp(nodes[a].y0, nodes[b].y0));
        resolve_collisions(nodes, &sorted, beta, py, y0, y1);
    }
}

/// BFS-style longest-path ranking. `depth = true` walks source→target
/// (computeNodeDepths); `false` walks target→source (computeNodeHeights).
fn compute_ranks(nodes: &mut [Node], links: &[Link], depth: bool) {
    let n = nodes.len();
    let mut current: Vec<usize> = (0..n).collect();
    let mut x = 0i64;
    while !current.is_empty() {
        for &ni in &current {
            if depth {
                nodes[ni].depth = x;
            } else {
                nodes[ni].height = x;
            }
        }
        // next = ordered unique set of neighbors
        let mut next: Vec<usize> = Vec::new();
        let mut seen = vec![false; n];
        for &ni in &current {
            let list = if depth {
                &nodes[ni].source_links
            } else {
                &nodes[ni].target_links
            };
            for &l in list {
                let nb = if depth {
                    links[l].target
                } else {
                    links[l].source
                };
                if !seen[nb] {
                    seen[nb] = true;
                    next.push(nb);
                }
            }
        }
        x += 1;
        current = next;
    }
}

/// Renders mermaid sankey-beta source to a complete SVG document string.
///
/// # Errors
/// Returns a [`SankeyParseError`] when the source is not a valid sankey diagram.
pub fn render_sankey(source: &str, id: &str) -> Result<String, SankeyParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let mut g = parse(source)?;
    layout(&mut g);

    let node_color = |i: usize| TABLEAU10[i % 10];

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    // viewBox is filled in after computing the content bbox.
    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_sankey_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    let mut uid = 0usize;
    let mut next_uid = || {
        uid += 1;
        uid
    };

    // Nodes.
    let nodes_g = append(&svg, "g");
    set_attr(&nodes_g, "class", "nodes");
    for (i, node) in g.nodes.iter().enumerate() {
        let ng: Element = append(&nodes_g, "g");
        set_attr(&ng, "class", "node");
        set_attr(&ng, "id", format!("node-{}", next_uid()));
        set_attr(
            &ng,
            "transform",
            format!("translate({},{})", js_num(node.x0), js_num(node.y0)),
        );
        set_attr(&ng, "x", js_num(node.x0));
        set_attr(&ng, "y", js_num(node.y0));
        let rect = append(&ng, "rect");
        set_attr(&rect, "height", js_num(node.y1 - node.y0));
        set_attr(&rect, "width", js_num(node.x1 - node.x0));
        set_attr(&rect, "fill", node_color(i));
    }

    // Node labels (showValues = true).
    let labels_g = append(&svg, "g");
    set_attr(&labels_g, "class", "node-labels");
    set_attr(&labels_g, "font-size", "14");
    for (i, node) in g.nodes.iter().enumerate() {
        let (lx, anchor) = if node.x0 < WIDTH / 2.0 {
            (node.x1 + 6.0, "start")
        } else {
            (node.x0 - 6.0, "end")
        };
        let text = append(&labels_g, "text");
        set_attr(&text, "x", js_num(lx));
        set_attr(&text, "y", js_num((node.y1 + node.y0) / 2.0));
        set_attr(&text, "dy", "0em");
        set_attr(&text, "text-anchor", anchor);
        let rounded = (node.value * 100.0).round() / 100.0;
        set_text(&text, &format!("{}\n{}", g.ids[i], js_num(rounded)));
    }

    // Links.
    let links_g = append(&svg, "g");
    set_attr(&links_g, "class", "links");
    set_attr(&links_g, "fill", "none");
    set_attr(&links_g, "stroke-opacity", "0.5");
    for link in &g.links {
        let lg = append(&links_g, "g");
        set_attr(&lg, "class", "link");
        set_attr(&lg, "style", "mix-blend-mode: multiply;");
        let gid = next_uid();
        let s = &g.nodes[link.source];
        let t = &g.nodes[link.target];
        let grad = append(&lg, "linearGradient");
        set_attr(&grad, "id", format!("linearGradient-{gid}"));
        set_attr(&grad, "gradientUnits", "userSpaceOnUse");
        set_attr(&grad, "x1", js_num(s.x1));
        set_attr(&grad, "x2", js_num(t.x0));
        let stop0 = append(&grad, "stop");
        set_attr(&stop0, "offset", "0%");
        set_attr(&stop0, "stop-color", node_color(link.source));
        let stop1 = append(&grad, "stop");
        set_attr(&stop1, "offset", "100%");
        set_attr(&stop1, "stop-color", node_color(link.target));
        // sankeyLinkHorizontal cubic: M sx,sy C xi,sy,xi,ty,tx,ty
        let (sx, sy, tx, ty) = (s.x1, link.y0, t.x0, link.y1);
        let xi = (sx + tx) / 2.0;
        let mut d = String::new();
        let _ = write!(
            d,
            "M{},{}C{},{},{},{},{},{}",
            js_num(sx),
            js_num(sy),
            js_num(xi),
            js_num(sy),
            js_num(xi),
            js_num(ty),
            js_num(tx),
            js_num(ty)
        );
        let path = append(&lg, "path");
        set_attr(&path, "d", d);
        set_attr(&path, "stroke", format!("url(#linearGradient-{gid})"));
        set_attr(&path, "stroke-width", js_num(link.width.max(1.0)));
    }

    // setupGraphViewbox: bbox of rendered geometry (text ignored), padding 0.
    let bounds = crate::render::bbox::element_bbox(&svg);
    let f32q = |v: f64| f64::from(v as f32);
    let (bx, by, bw, bh) = if bounds.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (
            f32q(bounds.min_x),
            f32q(bounds.min_y),
            f32q(bounds.width()),
            f32q(bounds.height()),
        )
    };
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(bw)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} {} {} {}",
            js_num(bx),
            js_num(by),
            js_num(bw),
            js_num(bh)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "sankey");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
