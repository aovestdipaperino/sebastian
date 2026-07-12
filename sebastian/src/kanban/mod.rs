//! Byte-exact port of mermaid 11.15.0 `kanban` diagrams.
//!
//! Ports the mindmap-style indentation parser, `kanbanDb` (sections + items),
//! the bespoke arithmetic column layout (`kanbanRenderer.ts`), the section
//! cluster (`insertCluster` rect) and the `kanbanItem` shape (rect + a markdown
//! title label + empty ticket/assigned labels). Labels reuse the shared
//! `build_html_label_classed` foreignObject builder. Corpus keeps items to
//! single-line labels with no ticket/priority/assigned metadata.

#![allow(clippy::cast_possible_truncation)]

use crate::render::shapes::{build_html_label_classed, measure_label_sized};
use crate::svg::{Element, append, insert_first, js_num, new_element, serialize, set_attr};
use crate::text::TextMeasurer;

const WIDTH: f64 = 200.0;
const PADDING: f64 = 10.0;
const FONT_SIZE: f64 = 16.0;

/// Parse error for kanban diagrams.
#[derive(Debug)]
pub struct KanbanParseError(pub String);

impl std::fmt::Display for KanbanParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "kanban parse error: {}", self.0)
    }
}

impl std::error::Error for KanbanParseError {}

#[derive(Debug)]
struct Section {
    id: String,
    label: String,
    items: Vec<Item>,
}

#[derive(Debug)]
struct Item {
    id: String,
    label: String,
}

// ---------------------------------------------------------------------------
// Parser (mindmap-style indentation)
// ---------------------------------------------------------------------------

fn parse(source: &str) -> Result<Vec<Section>, KanbanParseError> {
    let mut rows: Vec<(usize, String)> = Vec::new();
    let mut found = false;
    for raw in source.lines() {
        let t = raw.trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        if !found {
            if t == "kanban" || t.starts_with("kanban ") || t.starts_with("kanban:") {
                found = true;
            } else {
                return Err(KanbanParseError(format!(
                    "expected kanban header, got {t:?}"
                )));
            }
            continue;
        }
        let level = raw.len() - raw.trim_start().len();
        // node: `[descr]` (or other brackets) or bare id.
        let (id, label) = parse_node(t);
        rows.push((level, format!("{id}\u{0}{label}")));
    }
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let section_level = rows[0].0;
    let mut sections: Vec<Section> = Vec::new();
    for (level, packed) in rows {
        let mut parts = packed.splitn(2, '\u{0}');
        let id = parts.next().unwrap_or("").to_owned();
        let label = parts.next().unwrap_or("").to_owned();
        if level == section_level {
            sections.push(Section {
                id,
                label,
                items: Vec::new(),
            });
        } else if let Some(sec) = sections.last_mut() {
            sec.items.push(Item { id, label });
        }
    }
    Ok(sections)
}

/// Extracts (id, descr) from a node token, mirroring the jison node rules.
fn parse_node(t: &str) -> (String, String) {
    // Bracketed forms: `[descr]`, `(descr)`, `((descr))`, `{{descr}}`, etc.
    let bytes = t.as_bytes();
    if let Some(&first) = bytes.first()
        && matches!(first, b'[' | b'(' | b'{')
    {
        // find the descr between the opening and closing delimiters
        let inner = t
            .trim_start_matches(['[', '(', '{'])
            .trim_end_matches([']', ')', '}']);
        return (inner.to_owned(), inner.to_owned());
    }
    // bare id (may be followed by a bracket shape — split at first bracket)
    let end = t.find(['[', '(', '{']).unwrap_or(t.len());
    if end < t.len() {
        let id = t[..end].to_owned();
        let inner = t[end..]
            .trim_start_matches(['[', '(', '{'])
            .trim_end_matches([']', ')', '}']);
        return (id, inner.to_owned());
    }
    (t.to_owned(), t.to_owned())
}

// ---------------------------------------------------------------------------
// Label helpers (reuse the shared foreignObject builder)
// ---------------------------------------------------------------------------

/// Builds a `<g class="cluster-label ">` with a markdown foreignObject label
/// (no inner rect). Returns the measured (width, height).
fn cluster_label(parent: &Element, label: &str, measurer: &TextMeasurer) -> (Element, f64, f64) {
    let g = append(parent, "g");
    set_attr(&g, "class", "cluster-label ");
    let bbox = measure_label_sized(measurer, label, WIDTH, FONT_SIZE);
    build_html_label_classed(&g, label, bbox, "nodeLabel", false, WIDTH, "");
    (g, bbox.width, bbox.height)
}

/// Builds a `<g class="label">` for an item (inner rect + foreignObject).
fn item_label(
    parent: &Element,
    label: &str,
    span_class: &str,
    label_style: &str,
    wrap_width: f64,
    measurer: &TextMeasurer,
) -> (Element, f64, f64) {
    let g = append(parent, "g");
    set_attr(&g, "class", "label");
    set_attr(&g, "style", label_style);
    let bbox = measure_label_sized(measurer, label, wrap_width, FONT_SIZE);
    build_html_label_classed(&g, label, bbox, span_class, false, wrap_width, label_style);
    insert_first(&g, "rect");
    (g, bbox.width, bbox.height)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Renders mermaid `kanban` source to a complete SVG document string.
///
/// # Errors
/// Returns [`KanbanParseError`] when the source is not a valid kanban diagram.
pub fn render_kanban(source: &str, id: &str) -> Result<String, KanbanParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let hand_drawn = config.is_hand_drawn();
    let tv = |k: &str, dflt: &str| -> String {
        theme_vars
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or(dflt)
            .to_owned()
    };
    let measurer = TextMeasurer::new();
    let sections = parse(source)?;

    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style_el = append(&svg, "style");
    crate::svg::set_text(
        &style_el,
        &crate::render::css::themed_kanban_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    let sections_g = append(&svg, "g");
    set_attr(&sections_g, "class", "sections");
    let items_g = append(&svg, "g");
    set_attr(&items_g, "class", "items");

    // bbox accumulation of visible rects.
    let (mut bx0, mut by0, mut bx1, mut by1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut acc = |x: f64, y: f64, w: f64, h: f64| {
        bx0 = bx0.min(x);
        by0 = by0.min(y);
        bx1 = bx1.max(x + w);
        by1 = by1.max(y + h);
    };

    // Pass 1: draw section clusters (rect height is a placeholder for now).
    let mut section_x: Vec<f64> = Vec::new();
    let mut section_rects: Vec<Element> = Vec::new();
    let mut section_gs: Vec<Element> = Vec::new();
    let mut max_label_height = 25.0f64;
    for (idx, section) in sections.iter().enumerate() {
        let cnt = (idx + 1) as f64;
        let x = WIDTH * cnt + (cnt - 1.0) * PADDING / 2.0;
        section_x.push(x);
        let g = append(&sections_g, "g");
        section_gs.push(g.clone());
        set_attr(
            &g,
            "class",
            format!("cluster undefined section-{}", idx + 1),
        );
        set_attr(&g, "id", format!("{id}-{}", section.id));
        set_attr(&g, "data-look", "classic");
        // rect (initial height WIDTH*3 = 600; height overwritten in pass 2).
        let rect = append(&g, "rect");
        set_attr(&rect, "style", "");
        set_attr(&rect, "rx", "5");
        set_attr(&rect, "ry", "5");
        set_attr(&rect, "x", js_num(x - WIDTH / 2.0));
        set_attr(&rect, "y", js_num(-WIDTH * 3.0 / 2.0));
        set_attr(&rect, "width", js_num(WIDTH));
        // label
        let (lg, lw, lh) = cluster_label(&g, &section.label, &measurer);
        set_attr(
            &lg,
            "transform",
            format!(
                "translate({}, {})",
                js_num(x - lw / 2.0),
                js_num(-WIDTH * 3.0 / 2.0)
            ),
        );
        max_label_height = max_label_height.max(lh);
        section_rects.push(rect);
    }

    // Pass 2: items + finalize section heights.
    for (idx, section) in sections.iter().enumerate() {
        let x = section_x[idx];
        let top = -WIDTH * 3.0 / 2.0 + max_label_height;
        let mut y = top;
        for item in &section.items {
            let item_g = append(&items_g, "g");
            set_attr(&item_g, "class", "node undefined");
            set_attr(&item_g, "id", format!("{id}-{}", item.id));
            let total_width = WIDTH - 1.5 * PADDING; // 185
            let wrap_width = total_width - 10.0; // node.width - 10 = 175
            // title label (markdown)
            let (title_g, _tw, th) = item_label(
                &item_g,
                &item.label,
                "nodeLabel markdown-node-label",
                "text-align:left !important",
                wrap_width,
                &measurer,
            );
            // ticket + assigned (empty)
            let (ticket_g, _, _) = item_label(
                &item_g,
                "",
                "nodeLabel",
                "text-align:left !important",
                wrap_width,
                &measurer,
            );
            let (assigned_g, aw, _) = item_label(
                &item_g,
                "",
                "nodeLabel",
                "text-align:left !important",
                wrap_width,
                &measurer,
            );
            let height_adj = 0.0f64; // max(ticket.h, assigned.h)/2, both empty
            let total_height = (th + 2.0 * 10.0).max(0.0) + height_adj; // labelPaddingY=10
            let padding = 10.0;
            let label_padding_x = 10.0;
            set_attr(
                &title_g,
                "transform",
                format!(
                    "translate({}, {})",
                    js_num(padding - total_width / 2.0),
                    js_num(-height_adj - th / 2.0)
                ),
            );
            set_attr(
                &ticket_g,
                "transform",
                format!(
                    "translate({}, {})",
                    js_num(padding - total_width / 2.0),
                    js_num(-height_adj + th / 2.0)
                ),
            );
            set_attr(
                &assigned_g,
                "transform",
                format!(
                    "translate({}, {})",
                    js_num(padding + total_width / 2.0 - aw - 2.0 * label_padding_x),
                    js_num(-height_adj + th / 2.0)
                ),
            );
            // container rect (inserted first-child)
            let rect = insert_first(&item_g, "rect");
            set_attr(&rect, "class", "basic label-container __APA__");
            set_attr(&rect, "style", "");
            set_attr(&rect, "rx", "5");
            set_attr(&rect, "ry", "5");
            set_attr(&rect, "x", js_num(-total_width / 2.0));
            set_attr(&rect, "y", js_num(-total_height / 2.0));
            set_attr(&rect, "width", js_num(total_width));
            set_attr(&rect, "height", js_num(total_height));
            if hand_drawn {
                set_attr(&rect, "style", "stroke:none");
                crate::render::handdrawn::hd_overlay_rect(
                    &item_g,
                    -total_width / 2.0,
                    -total_height / 2.0,
                    total_width,
                    total_height,
                    &tv("nodeBorder", "#9370DB"),
                    "",
                );
            }
            // position
            let item_y = y + total_height / 2.0;
            set_attr(
                &item_g,
                "transform",
                format!("translate({}, {})", js_num(x), js_num(item_y)),
            );
            acc(
                x - total_width / 2.0,
                item_y - total_height / 2.0,
                total_width,
                total_height,
            );
            y = item_y + total_height / 2.0 + PADDING / 2.0;
        }
        // finalize section rect height
        let height = (y - top + 3.0 * PADDING).max(50.0) + (max_label_height - 25.0);
        set_attr(&section_rects[idx], "height", js_num(height));
        if hand_drawn {
            set_attr(&section_rects[idx], "style", "stroke:none");
            crate::render::handdrawn::hd_overlay_rect(
                &section_gs[idx],
                x - WIDTH / 2.0,
                -WIDTH * 3.0 / 2.0,
                WIDTH,
                height,
                &tv(&format!("cScale{}", idx % 12), "#9370DB"),
                "",
            );
        }
        acc(x - WIDTH / 2.0, -WIDTH * 3.0 / 2.0, WIDTH, height);
    }

    // viewBox = content bbox ± padding (setupGraphViewbox, padding 10).
    let p = PADDING;
    let (cw, ch) = (bx1 - bx0, by1 - by0);
    let vw = cw + 2.0 * p;
    let vh = ch + 2.0 * p;
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(vw)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} {} {} {}",
            js_num(bx0 - p),
            js_num(by0 - p),
            js_num(vw),
            js_num(vh)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "kanban");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
