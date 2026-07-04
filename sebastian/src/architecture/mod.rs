//! **Approximate** (non-byte-exact) renderer for mermaid `architecture-beta`.
//!
//! Mermaid lays architecture diagrams out with cytoscape-`fcose`, a
//! force-directed engine seeded from `Math.random()` — its output is not even
//! deterministic run-to-run, so there is no byte stream to match (see
//! `TODO.md`). This renderer instead uses a deterministic **directional grid**:
//! each service is placed relative to a neighbour along the edge's port
//! direction (`R`→right, `L`→left, `T`→up, `B`→down). Output is a clean, stable
//! diagram but is *not* byte-identical to mmdc — an opt-in approximation.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::type_complexity
)]

use std::collections::HashMap;

use crate::svg::{append, js_num, new_element, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

const CELL: f64 = 130.0;
const ICON: f64 = 80.0;
const FONT_SIZE: f64 = 14.0;

/// Parse error for architecture diagrams.
#[derive(Debug)]
pub struct ArchitectureParseError(pub String);

impl std::fmt::Display for ArchitectureParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "architecture parse error: {}", self.0)
    }
}

impl std::error::Error for ArchitectureParseError {}

struct Service {
    id: String,
    icon: String,
    title: String,
    group: Option<String>,
    gx: i64,
    gy: i64,
}

struct Group {
    id: String,
    title: String,
}

struct Edge {
    lhs: String,
    lhs_dir: char,
    rhs: String,
}

fn strip_brackets(s: &str, open: char, close: char) -> Option<String> {
    let o = s.find(open)?;
    let rest = &s[o + 1..];
    let c = rest.find(close)?;
    Some(rest[..c].trim_matches(['"', '\'']).to_owned())
}

fn parse(source: &str) -> Result<(Vec<Service>, Vec<Group>, Vec<Edge>), ArchitectureParseError> {
    let mut services = Vec::new();
    let mut groups = Vec::new();
    let mut edges = Vec::new();
    let mut found = false;
    for raw in source.lines() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        if !found {
            if t == "architecture-beta" || t.starts_with("architecture-beta ") {
                found = true;
            } else {
                return Err(ArchitectureParseError(format!(
                    "expected architecture-beta header, got {t:?}"
                )));
            }
            continue;
        }
        if let Some(rest) = t.strip_prefix("group ") {
            let id = rest.split([' ', '(', '[']).next().unwrap_or("").to_owned();
            let title = strip_brackets(rest, '[', ']').unwrap_or_else(|| id.clone());
            groups.push(Group { id, title });
        } else if let Some(rest) = t.strip_prefix("service ") {
            let id = rest.split([' ', '(', '[']).next().unwrap_or("").to_owned();
            let icon = strip_brackets(rest, '(', ')').unwrap_or_default();
            let title = strip_brackets(rest, '[', ']').unwrap_or_else(|| id.clone());
            let group = rest
                .split_whitespace()
                .skip_while(|w| *w != "in")
                .nth(1)
                .map(str::to_owned);
            services.push(Service {
                id,
                icon,
                title,
                group,
                gx: 0,
                gy: 0,
            });
        } else if let Some(rest) = t.strip_prefix("junction ") {
            let id = rest.split_whitespace().next().unwrap_or("").to_owned();
            let group = rest
                .split_whitespace()
                .skip_while(|w| *w != "in")
                .nth(1)
                .map(str::to_owned);
            services.push(Service {
                id,
                icon: "junction".to_owned(),
                title: String::new(),
                group,
                gx: 0,
                gy: 0,
            });
        } else if let Some(e) = parse_edge(t) {
            edges.push(e);
        }
    }
    Ok((services, groups, edges))
}

/// Parses `A:R -- L:B` / `A:R --> L:B` (ports + optional arrowheads/labels).
fn parse_edge(t: &str) -> Option<Edge> {
    // split on the arrow (`--`, `-`, with optional `<`/`>` and `{group}`).
    let arrow_pos = t.find("--").or_else(|| t.find(" - "))?;
    let left = t[..arrow_pos].trim();
    let right = t[arrow_pos..].trim_start_matches(['-', '<', '>', ' ']);
    // left: `A:R` (maybe `A{group}:R`)
    let (lhs, lhs_dir) = split_port(left)?;
    // right: `R:B` — leading arrowhead/group already stripped; form is `<dir>:<id>`
    let right = right.trim_start_matches(['<', '>']);
    let mut rp = right.splitn(2, ':');
    let rdir = rp.next()?.trim();
    let rhs = rp.next()?.trim().split(['{', ' ']).next()?.to_owned();
    let _ = rdir;
    let lhs = lhs.split(['{']).next()?.to_owned();
    Some(Edge { lhs, lhs_dir, rhs })
}

fn split_port(s: &str) -> Option<(String, char)> {
    let i = s.rfind(':')?;
    let id = s[..i].to_owned();
    let dir = s[i + 1..].trim().chars().next()?;
    Some((id, dir))
}

fn dir_delta(d: char) -> (i64, i64) {
    match d {
        'L' => (-1, 0),
        'R' => (1, 0),
        'T' => (0, -1),
        _ => (0, 1), // 'B'
    }
}

/// Renders mermaid `architecture-beta` source (approximate directional grid).
///
/// # Errors
/// Returns [`ArchitectureParseError`] when the source is invalid.
pub fn render_architecture(source: &str, id: &str) -> Result<String, ArchitectureParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let tv = |k: &str| crate::render::themes::get(&theme_vars, k);
    let measurer = TextMeasurer::new();
    let (mut services, groups, edges) = parse(source)?;
    if services.is_empty() {
        return Ok(empty_svg(id));
    }

    // Directional grid placement.
    let index: HashMap<String, usize> = services
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();
    let mut placed = vec![false; services.len()];
    let mut occupied: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
    placed[0] = true;
    occupied.insert((0, 0));
    // Repeatedly apply edges until no progress, then drop leftovers into a row.
    let mut progress = true;
    while progress {
        progress = false;
        for e in &edges {
            let (Some(&li), Some(&ri)) = (index.get(&e.lhs), index.get(&e.rhs)) else {
                continue;
            };
            if placed[li] && !placed[ri] {
                let (dx, dy) = dir_delta(e.lhs_dir);
                let mut pos = (services[li].gx + dx, services[li].gy + dy);
                while occupied.contains(&pos) {
                    pos = (pos.0 + dx.signum().max(1), pos.1 + dy);
                }
                services[ri].gx = pos.0;
                services[ri].gy = pos.1;
                placed[ri] = true;
                occupied.insert(pos);
                progress = true;
            }
        }
    }
    let mut spare = 0i64;
    for i in 0..services.len() {
        if !placed[i] {
            let mut pos = (spare, 3);
            while occupied.contains(&pos) {
                spare += 1;
                pos = (spare, 3);
            }
            services[i].gx = pos.0;
            services[i].gy = pos.1;
            occupied.insert(pos);
            placed[i] = true;
            spare += 1;
        }
    }

    // Pixel positions (centre of each cell).
    let px = |gx: i64| gx as f64 * CELL;
    let py = |gy: i64| gy as f64 * CELL;

    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style_el = append(&svg, "style");
    crate::svg::set_text(&style_el, &arch_css(id, &tv));

    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );

    // Group boxes (behind), sized to their member services.
    let groups_g = append(&svg, "g");
    set_attr(&groups_g, "class", "architecture-groups");
    for group in &groups {
        let members: Vec<&Service> = services
            .iter()
            .filter(|s| s.group.as_deref() == Some(group.id.as_str()))
            .collect();
        if members.is_empty() {
            continue;
        }
        let gx0 = members
            .iter()
            .map(|s| px(s.gx))
            .fold(f64::INFINITY, f64::min)
            - ICON / 2.0
            - 16.0;
        let gy0 = members
            .iter()
            .map(|s| py(s.gy))
            .fold(f64::INFINITY, f64::min)
            - ICON / 2.0
            - 30.0;
        let gx1 = members
            .iter()
            .map(|s| px(s.gx))
            .fold(f64::NEG_INFINITY, f64::max)
            + ICON / 2.0
            + 16.0;
        let gy1 = members
            .iter()
            .map(|s| py(s.gy))
            .fold(f64::NEG_INFINITY, f64::max)
            + ICON / 2.0
            + 16.0;
        let g = append(&groups_g, "g");
        set_attr(&g, "class", "architecture-group");
        let rect = append(&g, "rect");
        set_attr(&rect, "class", "node-bkg");
        set_attr(&rect, "x", js_num(gx0));
        set_attr(&rect, "y", js_num(gy0));
        set_attr(&rect, "width", js_num(gx1 - gx0));
        set_attr(&rect, "height", js_num(gy1 - gy0));
        set_attr(&rect, "rx", "8");
        set_attr(&rect, "ry", "8");
        let text = append(&g, "text");
        set_attr(&text, "class", "architecture-group-label");
        set_attr(&text, "x", js_num(gx0 + 10.0));
        set_attr(&text, "y", js_num(gy0 + 18.0));
        set_text(&text, &group.title);
        minx = minx.min(gx0);
        miny = miny.min(gy0);
        maxx = maxx.max(gx1);
        maxy = maxy.max(gy1);
    }

    // Edges.
    let edges_g = append(&svg, "g");
    set_attr(&edges_g, "class", "architecture-edges");
    for e in &edges {
        let (Some(&li), Some(&ri)) = (index.get(&e.lhs), index.get(&e.rhs)) else {
            continue;
        };
        let line = append(&edges_g, "line");
        set_attr(&line, "class", "edge");
        set_attr(&line, "x1", js_num(px(services[li].gx)));
        set_attr(&line, "y1", js_num(py(services[li].gy)));
        set_attr(&line, "x2", js_num(px(services[ri].gx)));
        set_attr(&line, "y2", js_num(py(services[ri].gy)));
    }

    // Service nodes.
    let nodes_g = append(&svg, "g");
    set_attr(&nodes_g, "class", "architecture-services");
    for s in &services {
        let (cx, cy) = (px(s.gx), py(s.gy));
        let g = append(&nodes_g, "g");
        set_attr(&g, "class", "architecture-service");
        set_attr(
            &g,
            "transform",
            format!("translate({}, {})", js_num(cx), js_num(cy)),
        );
        let hw = ICON / 2.0;
        let rect = append(&g, "rect");
        set_attr(&rect, "class", "node-bkg");
        set_attr(&rect, "x", js_num(-hw));
        set_attr(&rect, "y", js_num(-hw));
        set_attr(&rect, "width", js_num(ICON));
        set_attr(&rect, "height", js_num(ICON));
        set_attr(&rect, "rx", "6");
        set_attr(&rect, "ry", "6");
        // icon name (placeholder for the real icon glyph)
        let icon = append(&g, "text");
        set_attr(&icon, "class", "architecture-icon");
        set_attr(&icon, "x", "0");
        set_attr(&icon, "y", "0");
        set_attr(&icon, "text-anchor", "middle");
        set_attr(&icon, "dominant-baseline", "middle");
        set_text(&icon, &s.icon);
        // title below
        if !s.title.is_empty() {
            let title = append(&g, "text");
            set_attr(&title, "class", "architecture-label");
            set_attr(&title, "x", "0");
            set_attr(&title, "y", js_num(hw + 16.0));
            set_attr(&title, "text-anchor", "middle");
            set_text(&title, &s.title);
        }
        let tw = measurer.measure_width(&s.title, FONT_SIZE);
        minx = minx.min(cx - hw).min(cx - tw / 2.0);
        maxx = maxx.max(cx + hw).max(cx + tw / 2.0);
        miny = miny.min(cy - hw);
        maxy = maxy.max(cy + hw + 22.0);
    }

    let pad = 16.0;
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
    set_attr(&svg, "aria-roledescription", "architecture");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

fn arch_css(id: &str, tv: &dyn Fn(&str) -> String) -> String {
    use std::fmt::Write as _;
    let font = tv("fontFamily");
    let line = tv("lineColor");
    let text_color = tv("textColor");
    let main_bkg = tv("mainBkg");
    let node_border = tv("nodeBorder");
    let cluster_bkg = tv("clusterBkg");
    let cluster_border = tv("clusterBorder");
    let mut o = String::new();
    let _ = write!(
        o,
        "#{id}{{font-family:{font};font-size:{FONT_SIZE}px;}}\
         #{id} .architecture-service .node-bkg{{fill:{main_bkg};stroke:{node_border};stroke-width:1px;}}\
         #{id} .architecture-group .node-bkg{{fill:{cluster_bkg};stroke:{cluster_border};stroke-width:1px;}}\
         #{id} .architecture-label,#{id} .architecture-icon,#{id} .architecture-group-label{{fill:{text_color};}}\
         #{id} .edge{{stroke:{line};stroke-width:1.5px;fill:none;}}"
    );
    o
}

fn empty_svg(id: &str) -> String {
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "aria-roledescription", "architecture");
    let mut out = String::new();
    serialize(&svg, &mut out);
    out
}
