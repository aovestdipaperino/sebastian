//! **sebastian extension** — a `system_chart` diagram type with no mermaid
//! equivalent, so it is an original renderer (not byte-exact against `mmdc`).
//!
//! Boxes with typical system-component icons (queue, folder, DB, wiki, user,
//! router, LLM, …) connected by labelled arrows, expressing a system
//! architecture:
//!
//! ```text
//! system_chart
//!   title Query pipeline
//!   query: chat "AI Agent Query" "What is our churn rate?"
//!   rt: router "Router" "(Classify)"
//!   okf: wiki "OKF" "(Wiki)"
//!   rag: db "RAG" "(Vector DB)"
//!   ai: llm "LLM" "(Synthesize)"
//!   query --> rt
//!   rt --> okf : Canonical?
//!   rt --> rag : Exploratory?
//!   okf --> ai
//!   rag --> ai
//! ```
//!
//! Node lines are `id: symbol "Title"` with an optional quoted subtitle; edge
//! lines are `a --> b`, optionally `: label`. Three more edge operators encode
//! the connection type in the line style: `..>` (event trigger — dashed),
//! `==>` (message via queue/bus — thick, envelope at the midpoint), and `---`
//! (undirected association — thin, no arrowhead). Nodes are ranked top-to-bottom by
//! longest path from the sources, each symbol kind carries its own accent
//! colour, and edges inherit the accent of their source node. Deterministic
//! layout; validated by structural smoke tests.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::manual_midpoint
)]

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::svg::{Element, append, js_num, new_element, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

const TITLE_FS: f64 = 16.0;
const SUB_FS: f64 = 13.0;
const CHART_TITLE_H: f64 = 40.0;
const PAD: f64 = 28.0;
const NODE_GAP_X: f64 = 56.0;
const RANK_GAP: f64 = 76.0;
const ICON_AREA: f64 = 66.0;
const ICON_SIZE: f64 = 39.0;
const MIN_NODE_W: f64 = 150.0;
const EDGE_LABEL_FS: f64 = 13.0;
const LEGEND_INK: &str = "#64748B";
const LEGEND_FS: f64 = 12.0;
const LEGEND_ROW_H: f64 = 22.0;
const LEGEND_PAD: f64 = 12.0;
const LEGEND_SAMPLE_W: f64 = 36.0;

/// Every symbol kind: name, accent (stroke/icon) colour, box fill tint.
const SYMBOLS: [(&str, &str, &str); 30] = [
    ("user", "#0284C7", "#F0F9FF"),
    ("users", "#6366F1", "#EEF2FF"),
    ("chat", "#3B82F6", "#EFF6FF"),
    ("queue", "#D97706", "#FFFBEB"),
    ("folder", "#F59E0B", "#FFFBEB"),
    ("db", "#2563EB", "#EFF6FF"),
    ("wiki", "#16A34A", "#F0FDF4"),
    ("router", "#7C3AED", "#F5F3FF"),
    ("llm", "#EA580C", "#FFF7ED"),
    ("doc", "#64748B", "#F8FAFC"),
    ("cloud", "#0891B2", "#ECFEFF"),
    ("service", "#475569", "#F1F5F9"),
    ("lock", "#DC2626", "#FEF2F2"),
    ("server", "#059669", "#ECFDF5"),
    ("cache", "#CA8A04", "#FEFCE8"),
    ("api", "#0D9488", "#F0FDFA"),
    ("fn", "#8B5CF6", "#F5F3FF"),
    ("stream", "#0EA5E9", "#F0F9FF"),
    ("scheduler", "#B45309", "#FFFBEB"),
    ("browser", "#4F46E5", "#EEF2FF"),
    ("mobile", "#DB2777", "#FDF2F8"),
    ("metrics", "#65A30D", "#F7FEE7"),
    ("mail", "#E11D48", "#FFF1F2"),
    ("bucket", "#92400E", "#FEF3C7"),
    ("key", "#A16207", "#FEFCE8"),
    ("robot", "#C026D3", "#FDF4FF"),
    ("search", "#0369A1", "#F0F9FF"),
    ("file", "#78716C", "#FAFAF9"),
    ("files", "#4D7C0F", "#F7FEE7"),
    ("box", "#64748B", "#FFFFFF"),
];

/// How two components are connected; encoded in the arrow's line style.
#[derive(Clone, Copy, PartialEq)]
enum EdgeKind {
    /// `-->`: synchronous call / request.
    Call,
    /// `..>`: event trigger / async notification (dashed).
    Event,
    /// `==>`: message via a queue or bus (thick, envelope at midpoint).
    Message,
    /// `---`: undirected association (thin, no arrowhead).
    Assoc,
}

fn symbol_colors(kind: &str) -> (&'static str, &'static str) {
    SYMBOLS
        .iter()
        .find(|(k, _, _)| *k == kind)
        .map_or(("#64748B", "#FFFFFF"), |(_, a, f)| (*a, *f))
}

/// Parse error for system charts.
#[derive(Debug)]
pub struct SystemChartParseError(pub String);

impl std::fmt::Display for SystemChartParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "system_chart parse error: {}", self.0)
    }
}

impl std::error::Error for SystemChartParseError {}

struct Node {
    id: String,
    kind: String,
    title: String,
    subtitle: String,
}

struct EdgeDef {
    from: String,
    to: String,
    label: String,
    kind: EdgeKind,
}

#[derive(Default)]
struct Db {
    title: String,
    nodes: Vec<Node>,
    edges: Vec<EdgeDef>,
    /// `legend` line present: draw a key of the connection types used.
    legend: bool,
}

/// Pulls a leading double-quoted string off `s`, returning it and the rest.
fn take_quoted(s: &str) -> Option<(String, &str)> {
    let rest = s.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some((rest[..end].to_owned(), &rest[end + 1..]))
}

fn parse(source: &str) -> Result<Db, SystemChartParseError> {
    let mut db = Db::default();
    let mut found = false;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with('#') {
            continue;
        }
        if !found {
            if line == "system_chart" || line.starts_with("system_chart ") {
                found = true;
                continue;
            }
            return Err(SystemChartParseError(format!(
                "expected system_chart header, got {line:?}"
            )));
        }
        if let Some(rest) = line.strip_prefix("title ") {
            rest.trim().clone_into(&mut db.title);
            continue;
        }
        if line == "legend" {
            db.legend = true;
            continue;
        }
        let edge_op = [
            ("-->", EdgeKind::Call),
            ("..>", EdgeKind::Event),
            ("==>", EdgeKind::Message),
            ("---", EdgeKind::Assoc),
        ]
        .into_iter()
        .find_map(|(op, kind)| line.find(op).map(|pos| (pos, kind)));
        if let Some((pos, kind)) = edge_op {
            let from = line[..pos].trim().to_owned();
            let rest = line[pos + 3..].trim();
            let (to, label) = match rest.split_once(':') {
                Some((t, l)) => (t.trim().to_owned(), l.trim().to_owned()),
                None => (rest.to_owned(), String::new()),
            };
            if from.is_empty() || to.is_empty() {
                return Err(SystemChartParseError(format!("bad edge line {line:?}")));
            }
            db.edges.push(EdgeDef {
                from,
                to,
                label,
                kind,
            });
            continue;
        }
        // `id: symbol "Title" ["Subtitle"]`
        let Some((id, rest)) = line.split_once(':') else {
            return Err(SystemChartParseError(format!(
                "expected node or edge line, got {line:?}"
            )));
        };
        let rest = rest.trim_start();
        let (kind, rest) = match rest.find(char::is_whitespace) {
            Some(p) => (&rest[..p], &rest[p..]),
            None => (rest, ""),
        };
        if kind.is_empty() {
            return Err(SystemChartParseError(format!(
                "node {id:?} is missing a symbol kind"
            )));
        }
        let (title, rest) = take_quoted(rest)
            .ok_or_else(|| SystemChartParseError(format!("node {id:?} needs a quoted title")))?;
        let subtitle = take_quoted(rest).map(|(s, _)| s).unwrap_or_default();
        db.nodes.push(Node {
            id: id.trim().to_owned(),
            kind: kind.to_owned(),
            title,
            subtitle,
        });
    }
    if !found {
        return Err(SystemChartParseError(
            "missing system_chart header".to_owned(),
        ));
    }
    Ok(db)
}

struct Placed {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// Longest-path rank per node (cycle-safe: bounded relaxation).
fn ranks(db: &Db, index: &HashMap<&str, usize>) -> Vec<usize> {
    let n = db.nodes.len();
    let mut rank = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for e in &db.edges {
            let (Some(&a), Some(&b)) = (index.get(e.from.as_str()), index.get(e.to.as_str()))
            else {
                continue;
            };
            if a != b && rank[b] < rank[a] + 1 && rank[a] + 1 < n {
                rank[b] = rank[a] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    rank
}

/// Renders a `system_chart` diagram to an SVG.
///
/// # Errors
/// Returns [`SystemChartParseError`] when the source is not a valid system
/// chart, or references an undeclared node in an edge.
pub fn render_system_chart(source: &str, id: &str) -> Result<String, SystemChartParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let tv = |k: &str| crate::render::themes::get(&theme_vars, k);
    let hand_drawn = config.is_hand_drawn();
    let measurer = TextMeasurer::new();
    let db = parse(source)?;
    if db.nodes.is_empty() {
        return Ok(empty_svg(id));
    }
    let index: HashMap<&str, usize> = db
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    for e in &db.edges {
        for end in [&e.from, &e.to] {
            if !index.contains_key(end.as_str()) {
                return Err(SystemChartParseError(format!(
                    "edge references undeclared node {end:?}"
                )));
            }
        }
    }

    let rank = ranks(&db, &index);
    let n_ranks = rank.iter().max().copied().unwrap_or(0) + 1;
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); n_ranks];
    for (i, &r) in rank.iter().enumerate() {
        rows[r].push(i);
    }

    // Node sizes from text metrics; boxes are icon area + centred text block.
    let sizes: Vec<(f64, f64)> = db
        .nodes
        .iter()
        .map(|node| {
            let tw = measurer.measure_width(&node.title, TITLE_FS) * 1.08;
            let sw = if node.subtitle.is_empty() {
                0.0
            } else {
                measurer.measure_width(&node.subtitle, SUB_FS)
            };
            let w = (ICON_AREA + tw.max(sw) + 28.0).max(MIN_NODE_W);
            let h = if node.subtitle.is_empty() { 58.0 } else { 74.0 };
            (w, h)
        })
        .collect();

    let row_w: Vec<f64> = rows
        .iter()
        .map(|row| {
            row.iter().map(|&i| sizes[i].0).sum::<f64>()
                + NODE_GAP_X * (row.len() as f64 - 1.0).max(0.0)
        })
        .collect();
    let row_h: Vec<f64> = rows
        .iter()
        .map(|row| row.iter().map(|&i| sizes[i].1).fold(0.0, f64::max))
        .collect();
    let content_w = row_w.iter().fold(0.0, |a: f64, &b| a.max(b));
    let title_offset = if db.title.is_empty() {
        0.0
    } else {
        CHART_TITLE_H
    };

    let mut placed: Vec<Placed> = (0..db.nodes.len())
        .map(|_| Placed {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        })
        .collect();
    let mut y = PAD + title_offset;
    for (r, row) in rows.iter().enumerate() {
        let mut x = PAD + (content_w - row_w[r]) / 2.0;
        for &i in row {
            let (w, h) = sizes[i];
            placed[i] = Placed {
                x,
                y: y + (row_h[r] - h) / 2.0,
                w,
                h,
            };
            x += w + NODE_GAP_X;
        }
        y += row_h[r] + RANK_GAP;
    }
    let total_w = content_w + PAD * 2.0;
    let total_h = y - RANK_GAP + PAD;

    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style_el = append(&svg, "style");
    set_text(&style_el, &system_chart_css(id, &tv));

    // One arrowhead marker per accent colour actually used by an edge.
    let defs = append(&svg, "defs");
    let mut marker_colors: Vec<&str> = Vec::new();
    for e in &db.edges {
        let (accent, _) = symbol_colors(&db.nodes[index[e.from.as_str()]].kind);
        if e.kind != EdgeKind::Assoc && !marker_colors.contains(&accent) {
            marker_colors.push(accent);
        }
    }
    let legend_kinds: Vec<EdgeKind> = if db.legend {
        [
            EdgeKind::Call,
            EdgeKind::Event,
            EdgeKind::Message,
            EdgeKind::Assoc,
        ]
        .into_iter()
        .filter(|k| db.edges.iter().any(|e| e.kind == *k))
        .collect()
    } else {
        Vec::new()
    };
    if legend_kinds.iter().any(|k| *k != EdgeKind::Assoc) {
        let m = append(&defs, "marker");
        set_attr(&m, "id", format!("{id}-arrow-legend"));
        set_attr(&m, "viewBox", "0 0 10 10");
        set_attr(&m, "refX", "9");
        set_attr(&m, "refY", "5");
        set_attr(&m, "markerWidth", "7");
        set_attr(&m, "markerHeight", "7");
        set_attr(&m, "orient", "auto-start-reverse");
        let p = append(&m, "path");
        set_attr(&p, "d", "M 0 0 L 10 5 L 0 10 z");
        set_attr(&p, "fill", LEGEND_INK);
    }
    for (k, color) in marker_colors.iter().enumerate() {
        let m = append(&defs, "marker");
        set_attr(&m, "id", format!("{id}-arrow-{k}"));
        set_attr(&m, "viewBox", "0 0 10 10");
        set_attr(&m, "refX", "9");
        set_attr(&m, "refY", "5");
        set_attr(&m, "markerWidth", "7");
        set_attr(&m, "markerHeight", "7");
        set_attr(&m, "orient", "auto-start-reverse");
        let p = append(&m, "path");
        set_attr(&p, "d", "M 0 0 L 10 5 L 0 10 z");
        set_attr(&p, "fill", *color);
    }

    if !db.title.is_empty() {
        let t = append(&svg, "text");
        set_attr(&t, "x", js_num(PAD + content_w / 2.0));
        set_attr(&t, "y", js_num(PAD + 4.0));
        set_attr(&t, "text-anchor", "middle");
        set_attr(&t, "class", "system-chart-title");
        set_text(&t, &db.title);
    }

    let edges_g = append(&svg, "g");
    set_attr(&edges_g, "class", "system-chart-edges");
    for e in &db.edges {
        let a = &placed[index[e.from.as_str()]];
        let b = &placed[index[e.to.as_str()]];
        let (accent, _) = symbol_colors(&db.nodes[index[e.from.as_str()]].kind);
        // Anchor by relative position: downward edges leave the bottom and
        // enter the top; same-row edges connect the facing sides.
        let ((x1, y1), (x2, y2)) = if b.y > a.y + a.h {
            ((a.x + a.w / 2.0, a.y + a.h), (b.x + b.w / 2.0, b.y))
        } else if a.y > b.y + b.h {
            ((a.x + a.w / 2.0, a.y), (b.x + b.w / 2.0, b.y + b.h))
        } else if b.x > a.x {
            ((a.x + a.w, a.y + a.h / 2.0), (b.x, b.y + b.h / 2.0))
        } else {
            ((a.x, a.y + a.h / 2.0), (b.x + b.w, b.y + b.h / 2.0))
        };
        let path = append(&edges_g, "path");
        let (c1, c2) = if (y2 - y1).abs() >= (x2 - x1).abs() {
            ((x1, (y1 + y2) / 2.0), (x2, (y1 + y2) / 2.0))
        } else {
            (((x1 + x2) / 2.0, y1), ((x1 + x2) / 2.0, y2))
        };
        let d = if hand_drawn {
            // One rough pass through the bezier, linearized so the wobble
            // follows the curve (arrowhead markers stay crisp by design,
            // matching the flowchart hand-drawn look).
            let pts: Vec<crate::dagre::types::Point> = (0..=8)
                .map(|i| {
                    let t = f64::from(i) / 8.0;
                    let u = 1.0 - t;
                    crate::dagre::types::Point {
                        x: u * u * u * x1
                            + 3.0 * u * u * t * c1.0
                            + 3.0 * u * t * t * c2.0
                            + t * t * t * x2,
                        y: u * u * u * y1
                            + 3.0 * u * u * t * c1.1
                            + 3.0 * u * t * t * c2.1
                            + t * t * t * y2,
                    }
                })
                .collect();
            crate::render::handdrawn::hd_edge_d(&pts, crate::render::handdrawn::seed_from(x1, y1))
        } else {
            format!(
                "M{},{} C{},{} {},{} {},{}",
                js_num(x1),
                js_num(y1),
                js_num(c1.0),
                js_num(c1.1),
                js_num(c2.0),
                js_num(c2.1),
                js_num(x2),
                js_num(y2)
            )
        };
        set_attr(&path, "d", d);
        let kind_class = match e.kind {
            EdgeKind::Call => "",
            EdgeKind::Event => " system-chart-edge-event",
            EdgeKind::Message => " system-chart-edge-message",
            EdgeKind::Assoc => " system-chart-edge-assoc",
        };
        set_attr(&path, "class", format!("system-chart-edge{kind_class}"));
        set_attr(&path, "style", format!("stroke:{accent};"));
        if e.kind != EdgeKind::Assoc {
            let marker = marker_colors.iter().position(|c| *c == accent).unwrap();
            set_attr(&path, "marker-end", format!("url(#{id}-arrow-{marker})"));
        }
        // Bezier midpoint (t = 0.5): envelope glyph for message edges, label
        // nudged up so the text sits on the line.
        let mx = (x1 + 3.0 * c1.0 + 3.0 * c2.0 + x2) / 8.0;
        let my = (y1 + 3.0 * c1.1 + 3.0 * c2.1 + y2) / 8.0;
        if e.kind == EdgeKind::Message {
            let env = append(&edges_g, "g");
            set_attr(&env, "class", "system-chart-envelope");
            set_attr(&env, "style", format!("stroke:{accent};"));
            let r = append(&env, "rect");
            set_attr(&r, "x", js_num(mx - 8.0));
            set_attr(&r, "y", js_num(my - 5.5));
            set_attr(&r, "width", "16");
            set_attr(&r, "height", "11");
            set_attr(&r, "rx", "1.5");
            let flap = append(&env, "path");
            set_attr(
                &flap,
                "d",
                format!(
                    "M{},{} L{},{} L{},{}",
                    js_num(mx - 8.0),
                    js_num(my - 4.5),
                    js_num(mx),
                    js_num(my + 1.5),
                    js_num(mx + 8.0),
                    js_num(my - 4.5)
                ),
            );
        }
        if !e.label.is_empty() {
            let label_y = if e.kind == EdgeKind::Message {
                my - 10.0
            } else {
                my - 6.0
            };
            let t = append(&edges_g, "text");
            set_attr(&t, "x", js_num(mx));
            set_attr(&t, "y", js_num(label_y));
            set_attr(&t, "text-anchor", "middle");
            set_attr(&t, "class", "system-chart-edge-label");
            set_attr(&t, "style", format!("fill:{accent};"));
            set_text(&t, &e.label);
        }
    }

    let nodes_g = append(&svg, "g");
    set_attr(&nodes_g, "class", "system-chart-nodes");
    for (i, node) in db.nodes.iter().enumerate() {
        let p = &placed[i];
        let (accent, fill) = symbol_colors(&node.kind);
        let g = append(&nodes_g, "g");
        set_attr(&g, "class", "system-chart-node");
        if hand_drawn {
            crate::render::handdrawn::hd_rect(&g, p.x, p.y, p.w, p.h, fill, accent, "1.5", "");
        } else {
            let rect = append(&g, "rect");
            set_attr(&rect, "x", js_num(p.x));
            set_attr(&rect, "y", js_num(p.y));
            set_attr(&rect, "width", js_num(p.w));
            set_attr(&rect, "height", js_num(p.h));
            set_attr(&rect, "rx", "10");
            set_attr(&rect, "ry", "10");
            set_attr(&rect, "class", "system-chart-node-rect");
            set_attr(&rect, "style", format!("fill:{fill};stroke:{accent};"));
        }

        draw_icon(
            &g,
            &node.kind,
            p.x + 12.0,
            p.y + (p.h - ICON_SIZE) / 2.0,
            accent,
        );

        let tx = p.x + ICON_AREA + (p.w - ICON_AREA) / 2.0 - 6.0;
        let cy = p.y + p.h / 2.0;
        let title = append(&g, "text");
        set_attr(&title, "x", js_num(tx));
        set_attr(&title, "text-anchor", "middle");
        set_attr(&title, "class", "system-chart-node-title");
        if node.subtitle.is_empty() {
            set_attr(&title, "y", js_num(cy));
            set_attr(&title, "dominant-baseline", "central");
            set_text(&title, &node.title);
        } else {
            set_attr(&title, "y", js_num(cy - 4.0));
            set_text(&title, &node.title);
            let sub = append(&g, "text");
            set_attr(&sub, "x", js_num(tx));
            set_attr(&sub, "y", js_num(cy + SUB_FS + 2.0));
            set_attr(&sub, "text-anchor", "middle");
            set_attr(&sub, "class", "system-chart-node-sub");
            set_text(&sub, &node.subtitle);
        }
    }

    let mut total_h = total_h;
    if !legend_kinds.is_empty() {
        draw_legend(
            &svg,
            id,
            &legend_kinds,
            &placed,
            total_w,
            &mut total_h,
            PAD + title_offset,
            &measurer,
            hand_drawn,
        );
    }

    set_attr(
        &svg,
        "viewBox",
        format!("0 0 {} {}", js_num(total_w), js_num(total_h)),
    );
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(total_w)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "system_chart");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

fn legend_kind_label(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Call => "call",
        EdgeKind::Event => "event",
        EdgeKind::Message => "message",
        EdgeKind::Assoc => "association",
    }
}

/// Draws a key of the connection types used, in a free corner of the chart
/// (checked against the node boxes); when every corner is occupied the canvas
/// grows and the legend goes below the last rank.
#[allow(clippy::too_many_arguments)]
fn draw_legend(
    svg: &Element,
    id: &str,
    kinds: &[EdgeKind],
    placed: &[Placed],
    total_w: f64,
    total_h: &mut f64,
    content_top: f64,
    measurer: &TextMeasurer,
    hand_drawn: bool,
) {
    let label_w = kinds
        .iter()
        .map(|k| measurer.measure_width(legend_kind_label(*k), LEGEND_FS))
        .fold(0.0, f64::max);
    let w = LEGEND_PAD + LEGEND_SAMPLE_W + 10.0 + label_w + LEGEND_PAD;
    let h = LEGEND_PAD * 2.0 + LEGEND_ROW_H * kinds.len() as f64 - 8.0;

    let candidates = [
        (PAD, content_top),
        (total_w - PAD - w, content_top),
        (PAD, *total_h - PAD - h),
        (total_w - PAD - w, *total_h - PAD - h),
    ];
    let free = |x: f64, y: f64| {
        placed.iter().all(|p| {
            x + w + 12.0 < p.x || p.x + p.w + 12.0 < x || y + h + 12.0 < p.y || p.y + p.h + 12.0 < y
        })
    };
    let (lx, ly) = candidates
        .into_iter()
        .find(|(x, y)| free(*x, *y))
        .unwrap_or_else(|| {
            // No free corner: grow the canvas and sit below the last rank.
            let y = *total_h - PAD + 16.0;
            *total_h = y + h + PAD;
            (PAD, y)
        });

    let g = append(svg, "g");
    set_attr(&g, "class", "system-chart-legend");
    if hand_drawn {
        crate::render::handdrawn::hd_rect(&g, lx, ly, w, h, "#FFFFFF", LEGEND_INK, "1", "");
    } else {
        let rect = append(&g, "rect");
        set_attr(&rect, "x", js_num(lx));
        set_attr(&rect, "y", js_num(ly));
        set_attr(&rect, "width", js_num(w));
        set_attr(&rect, "height", js_num(h));
        set_attr(&rect, "rx", "8");
        set_attr(&rect, "ry", "8");
        set_attr(&rect, "class", "system-chart-legend-box");
    }
    for (i, kind) in kinds.iter().enumerate() {
        let cy = ly + LEGEND_PAD + LEGEND_ROW_H * (i as f64 + 0.5) - 4.0;
        let (x1, x2) = (lx + LEGEND_PAD, lx + LEGEND_PAD + LEGEND_SAMPLE_W);
        let line = append(&g, "path");
        let d = if hand_drawn {
            use crate::dagre::types::Point;
            crate::render::handdrawn::hd_edge_d(
                &[Point { x: x1, y: cy }, Point { x: x2, y: cy }],
                crate::render::handdrawn::seed_from(x1, cy),
            )
        } else {
            format!(
                "M{},{} L{},{}",
                js_num(x1),
                js_num(cy),
                js_num(x2),
                js_num(cy)
            )
        };
        set_attr(&line, "d", d);
        let kind_class = match kind {
            EdgeKind::Call => "",
            EdgeKind::Event => " system-chart-edge-event",
            EdgeKind::Message => " system-chart-edge-message",
            EdgeKind::Assoc => " system-chart-edge-assoc",
        };
        set_attr(&line, "class", format!("system-chart-edge{kind_class}"));
        set_attr(&line, "style", format!("stroke:{LEGEND_INK};"));
        if *kind != EdgeKind::Assoc {
            set_attr(&line, "marker-end", format!("url(#{id}-arrow-legend)"));
        }
        if *kind == EdgeKind::Message {
            let mx = (x1 + x2) / 2.0;
            let env = append(&g, "g");
            set_attr(&env, "class", "system-chart-envelope");
            set_attr(&env, "style", format!("stroke:{LEGEND_INK};"));
            let r = append(&env, "rect");
            set_attr(&r, "x", js_num(mx - 6.0));
            set_attr(&r, "y", js_num(cy - 4.0));
            set_attr(&r, "width", "12");
            set_attr(&r, "height", "8");
            set_attr(&r, "rx", "1");
            let flap = append(&env, "path");
            set_attr(
                &flap,
                "d",
                format!(
                    "M{},{} L{},{} L{},{}",
                    js_num(mx - 6.0),
                    js_num(cy - 3.2),
                    js_num(mx),
                    js_num(cy + 1.0),
                    js_num(mx + 6.0),
                    js_num(cy - 3.2)
                ),
            );
        }
        let t = append(&g, "text");
        set_attr(&t, "x", js_num(x2 + 10.0));
        set_attr(&t, "y", js_num(cy));
        set_attr(&t, "dominant-baseline", "central");
        set_attr(&t, "class", "system-chart-legend-label");
        set_text(&t, legend_kind_label(*kind));
    }
}

/// Appends a `path` with the given data to `g`, stroked (not filled).
fn stroke_path(g: &Element, d: &str) {
    let p = append(g, "path");
    set_attr(&p, "d", d);
    set_attr(&p, "class", "system-chart-icon-stroke");
}

/// Draws the icon for `kind` in a 24x24 grid, translated to (`x`, `y`) and
/// scaled to [`ICON_SIZE`], in the accent colour.
fn draw_icon(parent: &Element, kind: &str, x: f64, y: f64, accent: &str) {
    let g = append(parent, "g");
    set_attr(&g, "class", "system-chart-icon");
    set_attr(
        &g,
        "transform",
        format!(
            "translate({},{}) scale({})",
            js_num(x),
            js_num(y),
            js_num(ICON_SIZE / 24.0)
        ),
    );
    set_attr(&g, "style", format!("stroke:{accent};fill:{accent};"));
    match kind {
        "user" => {
            stroke_path(&g, "M12,4 a4,4 0 1 1 -0.01,0 Z");
            stroke_path(&g, "M4,20 C4,15.5 8,14 12,14 C16,14 20,15.5 20,20");
        }
        "users" => {
            stroke_path(&g, "M9,4.5 a3.5,3.5 0 1 1 -0.01,0 Z");
            stroke_path(
                &g,
                "M2.5,19 C2.5,15 5.5,13.8 9,13.8 C12.5,13.8 15.5,15 15.5,19",
            );
            stroke_path(&g, "M16.5,6.5 a3,3 0 1 1 -0.01,0 Z");
            stroke_path(&g, "M17.5,13.9 C20,14.4 21.5,15.8 21.5,18.5");
        }
        "chat" => {
            stroke_path(&g, "M4,5 H20 V16 H11 L7,20 V16 H4 Z");
            stroke_path(&g, "M7.5,9 H16.5 M7.5,12 H13.5");
        }
        "queue" => {
            stroke_path(&g, "M3,9 H7.5 V15 H3 Z");
            stroke_path(&g, "M9.5,9 H14 V15 H9.5 Z");
            stroke_path(&g, "M16,9 H20.5 V15 H16 Z");
            stroke_path(&g, "M21.5,12 H23");
        }
        "folder" => {
            stroke_path(&g, "M3,6 H10 L12,8 H21 V19 H3 Z");
            stroke_path(&g, "M3,10 H21");
        }
        "db" => {
            stroke_path(&g, "M5,6 a7,3 0 1 1 14,0 a7,3 0 1 1 -14,0");
            stroke_path(
                &g,
                "M5,6 V18 C5,19.7 8.1,21 12,21 C15.9,21 19,19.7 19,18 V6",
            );
            stroke_path(&g, "M5,12 C5,13.7 8.1,15 12,15 C15.9,15 19,13.7 19,12");
        }
        "wiki" => {
            stroke_path(
                &g,
                "M12,6 C10,4.5 7,4 4,4.5 V18 C7,17.5 10,18 12,19.5 C14,18 17,17.5 20,18 V4.5 C17,4 14,4.5 12,6 Z",
            );
            stroke_path(&g, "M12,6 V19.5");
        }
        "router" => {
            stroke_path(&g, "M12,3 a9,9 0 1 1 -0.01,0 Z");
            stroke_path(&g, "M12,7 V17 M7,12 H17");
            stroke_path(&g, "M10,8.5 L12,6.5 L14,8.5 M10,15.5 L12,17.5 L14,15.5");
            stroke_path(&g, "M8.5,10 L6.5,12 L8.5,14 M15.5,10 L17.5,12 L15.5,14");
        }
        "llm" => {
            stroke_path(
                &g,
                "M11,4.5 C8.5,3.5 6,5 6.2,7.5 C4.2,8.5 4,11.5 5.8,12.8 C4.8,15.2 6.8,17.6 9.2,17 C9.6,19 11,19.6 11,19.6 Z",
            );
            stroke_path(
                &g,
                "M13,4.5 C15.5,3.5 18,5 17.8,7.5 C19.8,8.5 20,11.5 18.2,12.8 C19.2,15.2 17.2,17.6 14.8,17 C14.4,19 13,19.6 13,19.6 Z",
            );
            stroke_path(&g, "M8,9 H10 M14,9 H16 M8.5,13 H10 M14,13 H15.5");
        }
        "doc" => {
            stroke_path(&g, "M6,3 H14 L18,7 V21 H6 Z");
            stroke_path(&g, "M14,3 V7 H18");
            stroke_path(&g, "M9,11 H15 M9,14 H15 M9,17 H13");
        }
        "cloud" => {
            stroke_path(
                &g,
                "M7,18 H16.5 A4,4 0 0 0 17,10 A5.5,5.5 0 0 0 6.5,9.8 A4.2,4.2 0 0 0 7,18 Z",
            );
        }
        "service" => {
            stroke_path(&g, "M12,8.8 a3.2,3.2 0 1 1 -0.01,0 Z");
            for k in 0..8 {
                let a = f64::from(k) * std::f64::consts::FRAC_PI_4;
                let (s, c) = a.sin_cos();
                stroke_path(
                    &g,
                    &format!(
                        "M{},{} L{},{}",
                        js_num(12.0 + 5.6 * c),
                        js_num(12.0 + 5.6 * s),
                        js_num(12.0 + 8.2 * c),
                        js_num(12.0 + 8.2 * s)
                    ),
                );
            }
        }
        "lock" => {
            stroke_path(&g, "M6,11 H18 V20 H6 Z");
            stroke_path(&g, "M8.5,11 V8 a3.5,3.5 0 0 1 7,0 V11");
            let dot = append(&g, "circle");
            set_attr(&dot, "cx", "12");
            set_attr(&dot, "cy", "15.5");
            set_attr(&dot, "r", "1.4");
            set_attr(&dot, "class", "system-chart-icon-fill");
        }
        "server" => {
            stroke_path(&g, "M4,4.5 H20 V10.5 H4 Z");
            stroke_path(&g, "M4,13.5 H20 V19.5 H4 Z");
            for cy in ["7.5", "16.5"] {
                let dot = append(&g, "circle");
                set_attr(&dot, "cx", "7.5");
                set_attr(&dot, "cy", cy);
                set_attr(&dot, "r", "1.1");
                set_attr(&dot, "class", "system-chart-icon-fill");
            }
            stroke_path(&g, "M11,7.5 H17 M11,16.5 H17");
        }
        "api" => {
            stroke_path(
                &g,
                "M9,4 C7,4 7,5.5 7,7.5 C7,9.5 5,10 4.5,12 C5,14 7,14.5 7,16.5 C7,18.5 7,20 9,20",
            );
            stroke_path(
                &g,
                "M15,4 C17,4 17,5.5 17,7.5 C17,9.5 19,10 19.5,12 C19,14 17,14.5 17,16.5 C17,18.5 17,20 15,20",
            );
            stroke_path(&g, "M10.5,12 H10.6 M13.5,12 H13.6");
        }
        "fn" => {
            stroke_path(&g, "M7,4.5 C10,4 10,6 11.3,9.5 L15.5,19.5");
            stroke_path(&g, "M11.3,9.5 L7,19.5");
        }
        "stream" => {
            stroke_path(&g, "M3,7 C6,5 9,9 12,7 C15,5 18,9 21,7");
            stroke_path(&g, "M3,12 C6,10 9,14 12,12 C15,10 18,14 21,12");
            stroke_path(&g, "M3,17 C6,15 9,19 12,17 C15,15 18,19 21,17");
        }
        "scheduler" => {
            stroke_path(&g, "M12,4 a8,8 0 1 1 -0.01,0 Z");
            stroke_path(&g, "M12,7.5 V12 L15.5,14");
        }
        "browser" => {
            stroke_path(&g, "M3,5 H21 V19 H3 Z");
            stroke_path(&g, "M3,9 H21");
            stroke_path(&g, "M5.5,7 H5.6 M8,7 H8.1");
        }
        "mobile" => {
            stroke_path(&g, "M8,3 H16 V21 H8 Z");
            stroke_path(&g, "M11,18.3 H13");
        }
        "metrics" => {
            stroke_path(&g, "M4,4 V20 H20");
            stroke_path(&g, "M8,20 V14 M12,20 V8 M16,20 V12");
        }
        "mail" => {
            stroke_path(&g, "M3,6 H21 V19 H3 Z");
            stroke_path(&g, "M3,7 L12,14 L21,7");
        }
        "bucket" => {
            stroke_path(&g, "M5,6 a7,2.2 0 1 1 14,0 a7,2.2 0 1 1 -14,0");
            stroke_path(
                &g,
                "M5,6 L6.8,19.5 C7,20.5 9.5,21 12,21 C14.5,21 17,20.5 17.2,19.5 L19,6",
            );
        }
        "key" => {
            stroke_path(&g, "M8,4.5 a3.5,3.5 0 1 1 -0.01,0 Z");
            stroke_path(&g, "M10.7,10.7 L19.5,19.5");
            stroke_path(&g, "M16,16 L18.5,13.5");
        }
        "robot" => {
            stroke_path(&g, "M5,8 H19 V18 H5 Z");
            stroke_path(&g, "M12,8 V5");
            stroke_path(&g, "M9.5,15.3 H14.5");
            for cx in ["9", "15"] {
                let dot = append(&g, "circle");
                set_attr(&dot, "cx", cx);
                set_attr(&dot, "cy", "12");
                set_attr(&dot, "r", "1.2");
                set_attr(&dot, "class", "system-chart-icon-fill");
            }
            let ant = append(&g, "circle");
            set_attr(&ant, "cx", "12");
            set_attr(&ant, "cy", "4");
            set_attr(&ant, "r", "1.1");
            set_attr(&ant, "class", "system-chart-icon-fill");
        }
        "file" => {
            // A single blank page with a folded corner (`doc` adds text lines).
            stroke_path(&g, "M6,3 H14 L18,7 V21 H6 Z");
            stroke_path(&g, "M14,3 V7 H18");
        }
        "files" => {
            // Two overlapping pages: a back page peeking out behind the front.
            stroke_path(&g, "M9,3 H16 L19.5,6.5 V16");
            stroke_path(&g, "M4.5,7 H12 L15.5,10.5 V21 H4.5 Z");
            stroke_path(&g, "M12,7 V10.5 H15.5");
        }
        "search" => {
            stroke_path(&g, "M10,4 a6,6 0 1 1 -0.01,0 Z");
            stroke_path(&g, "M14.5,14.5 L20,20");
        }
        "cache" => {
            let bolt = append(&g, "path");
            set_attr(&bolt, "d", "M13,3 L6,13.5 H11 L10,21 L18,10 H12.5 Z");
            set_attr(&bolt, "class", "system-chart-icon-fill");
        }
        // "box" and unknown kinds: no icon glyph, just the coloured frame.
        _ => {
            stroke_path(&g, "M5,7 L12,3.5 L19,7 V17 L12,20.5 L5,17 Z");
            stroke_path(&g, "M5,7 L12,10.5 L19,7 M12,10.5 V20.5");
        }
    }
}

fn system_chart_css(id: &str, tv: &dyn Fn(&str) -> String) -> String {
    let font = tv("fontFamily");
    let text_color = tv("textColor");
    let title_stroke = crate::render::handdrawn::embolden_decls(&text_color);
    let node_title_stroke = crate::render::handdrawn::embolden_decls("#1a1a1a");
    let mut o = String::new();
    let _ = write!(
        o,
        "#{id}{{font-family:{font};}}\
         #{id} .system-chart-title{{font-size:20px;font-weight:bold;fill:{text_color};{title_stroke}}}\
         #{id} .system-chart-node-rect{{stroke-width:1.5px;}}\
         #{id} .system-chart-node-title{{font-size:{TITLE_FS}px;font-weight:bold;fill:#1a1a1a;{node_title_stroke}}}\
         #{id} .system-chart-node-sub{{font-size:{SUB_FS}px;fill:#52525B;}}\
         #{id} .system-chart-edge{{fill:none;stroke-width:2px;}}\
         #{id} .system-chart-edge-event{{stroke-dasharray:6 5;}}\
         #{id} .system-chart-edge-message{{stroke-width:3px;}}\
         #{id} .system-chart-edge-assoc{{stroke-width:1.5px;}}\
         #{id} .system-chart-envelope{{fill:white;stroke-width:1.5px;stroke-linejoin:round;}}\
         #{id} .system-chart-envelope path{{fill:none;}}\
         #{id} .system-chart-edge-label{{font-size:{EDGE_LABEL_FS}px;font-weight:bold;\
           paint-order:stroke;stroke:white;stroke-width:4px;stroke-linejoin:round;}}\
         #{id} .system-chart-legend-box{{fill:#FFFFFF;stroke:{LEGEND_INK};stroke-width:1px;}}\
         #{id} .system-chart-legend-label{{font-size:{LEGEND_FS}px;fill:#3F3F46;}}\
         #{id} .system-chart-icon-stroke{{fill:none;stroke-width:1.7px;\
           stroke-linecap:round;stroke-linejoin:round;}}\
         #{id} .system-chart-icon-fill{{stroke:none;}}"
    );
    o
}

fn empty_svg(id: &str) -> String {
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "aria-roledescription", "system_chart");
    let mut out = String::new();
    serialize(&svg, &mut out);
    out
}
