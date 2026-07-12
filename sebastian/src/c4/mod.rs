//! **Approximate** (non-byte-exact) renderer for mermaid C4 diagrams
//! (`C4Context` / `C4Container` / `C4Component` / `C4Dynamic` / `C4Deployment`).
//!
//! Byte-exact C4 is blocked on font metrics, not effort: mermaid's
//! `calculateTextDimensions` measures every label with Blink `getBBox()` **ink
//! extents** over `sans-serif` / `Open Sans`, `Math.round`ed and maxed across
//! faces — our engine only models Trebuchet *advances*, so the exact box
//! widths can't be reproduced without a whole Helvetica/Arial ink-metrics
//! subsystem (see `TODO.md`). This renderer therefore uses its own
//! deterministic row-based layout (shapes in rows within their boundary,
//! boundaries stacked) and Trebuchet advance sizing: a clean, stable,
//! structurally faithful C4 diagram that is *not* byte-identical to mmdc — an
//! opt-in approximation, the same stance as mindmap/architecture.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::needless_continue,
    clippy::manual_midpoint,
    clippy::assigning_clones
)]

use std::fmt::Write as _;

use crate::svg::{append, js_num, new_element, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

const FONT_SIZE: f64 = 14.0;
const SHAPE_W: f64 = 200.0;
const SHAPE_MARGIN: f64 = 40.0;
const SHAPE_PAD: f64 = 20.0;
const SHAPES_PER_ROW: usize = 4;
const BOUNDARY_PAD: f64 = 24.0;
const BOUNDARY_HEADER: f64 = 32.0;
const DIAGRAM_MARGIN: f64 = 40.0;
const LINE_H: f64 = FONT_SIZE * 1.4;

/// Parse error for C4 diagrams.
#[derive(Debug)]
pub struct C4ParseError(pub String);

impl std::fmt::Display for C4ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "c4 diagram parse error: {}", self.0)
    }
}

impl std::error::Error for C4ParseError {}

#[derive(Clone)]
struct Shape {
    alias: String,
    label: String,
    techn: String,
    descr: String,
    type_tag: String,
    external: bool,
    is_person: bool,
    boundary: Option<usize>,
    // layout
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct Boundary {
    label: String,
    type_tag: String,
    parent: Option<usize>,
    // layout
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Clone)]
struct Rel {
    from: String,
    to: String,
    label: String,
    techn: String,
    bidir: bool,
}

#[derive(Default)]
struct C4Db {
    shapes: Vec<Shape>,
    boundaries: Vec<Boundary>,
    rels: Vec<Rel>,
    title: String,
}

/// Splits the comma-separated argument list inside `Keyword( ... )`, honouring
/// double-quoted strings (which may themselves contain commas).
fn split_args(inner: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_str = !in_str;
            }
            '\\' if in_str => {
                if let Some(&n) = chars.peek() {
                    cur.push(n);
                    chars.next();
                }
            }
            ',' if !in_str => {
                args.push(cur.trim().to_owned());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() || !args.is_empty() {
        args.push(cur.trim().to_owned());
    }
    args
}

/// Descriptive tag + flags for a shape keyword.
fn shape_kind(kw: &str) -> Option<(&'static str, bool, bool)> {
    // (type_tag, external, is_person)
    let k = kw;
    Some(match k {
        "Person" => ("Person", false, true),
        "Person_Ext" => ("External Person", true, true),
        "System" => ("System", false, false),
        "System_Ext" => ("External System", true, false),
        "SystemDb" => ("System (database)", false, false),
        "SystemDb_Ext" => ("External System (database)", true, false),
        "SystemQueue" => ("System (queue)", false, false),
        "SystemQueue_Ext" => ("External System (queue)", true, false),
        "Container" => ("Container", false, false),
        "Container_Ext" => ("External Container", true, false),
        "ContainerDb" => ("Container (database)", false, false),
        "ContainerDb_Ext" => ("External Container (database)", true, false),
        "ContainerQueue" => ("Container (queue)", false, false),
        "ContainerQueue_Ext" => ("External Container (queue)", true, false),
        "Component" => ("Component", false, false),
        "Component_Ext" => ("External Component", true, false),
        "ComponentDb" => ("Component (database)", false, false),
        "ComponentDb_Ext" => ("External Component (database)", true, false),
        "ComponentQueue" => ("Component (queue)", false, false),
        "ComponentQueue_Ext" => ("External Component (queue)", true, false),
        _ => return None,
    })
}

/// True if the keyword has a `techn` arg between label and descr.
fn has_techn(kw: &str) -> bool {
    kw.starts_with("Container") || kw.starts_with("Component")
}

fn boundary_kind(kw: &str) -> Option<&'static str> {
    Some(match kw {
        "Enterprise_Boundary" => "Enterprise",
        "System_Boundary" => "System",
        "Container_Boundary" => "Container",
        "Boundary" => "Boundary",
        "Node" | "Node_L" | "Node_R" => "Node",
        "Deployment_Node" => "Deployment Node",
        _ => return None,
    })
}

fn rel_kind(kw: &str) -> Option<bool> {
    // returns Some(bidirectional)
    match kw {
        "Rel" | "Rel_U" | "Rel_D" | "Rel_L" | "Rel_R" | "Rel_Up" | "Rel_Down" | "Rel_Left"
        | "Rel_Right" | "Rel_Back" => Some(false),
        "BiRel" => Some(true),
        _ => None,
    }
}

fn parse(source: &str) -> Result<C4Db, C4ParseError> {
    let mut db = C4Db::default();
    let mut found = false;
    let mut boundary_stack: Vec<usize> = Vec::new();
    for raw in source.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found {
            if line.starts_with("C4Context")
                || line.starts_with("C4Container")
                || line.starts_with("C4Component")
                || line.starts_with("C4Dynamic")
                || line.starts_with("C4Deployment")
            {
                found = true;
            } else {
                return Err(C4ParseError(format!("expected C4 header, got {line:?}")));
            }
            continue;
        }
        // Closing boundary brace(s).
        while line.starts_with('}') {
            boundary_stack.pop();
            line = line[1..].trim();
        }
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title ") {
            db.title = rest.trim().to_owned();
            continue;
        }
        // Statement form: `Keyword(args) [{]`.
        let Some(paren) = line.find('(') else {
            continue;
        };
        let kw = line[..paren].trim();
        let after = &line[paren + 1..];
        let Some(close) = after.rfind(')') else {
            continue;
        };
        let inner = &after[..close];
        let trailing = after[close + 1..].trim();
        let opens_block = trailing.starts_with('{');
        let args = split_args(inner);

        if let Some((tag, external, is_person)) = shape_kind(kw) {
            let alias = args.first().cloned().unwrap_or_default();
            let label = args.get(1).cloned().unwrap_or_default();
            let (techn, descr) = if has_techn(kw) {
                (
                    args.get(2).cloned().unwrap_or_default(),
                    args.get(3).cloned().unwrap_or_default(),
                )
            } else {
                (String::new(), args.get(2).cloned().unwrap_or_default())
            };
            if alias.is_empty() {
                continue;
            }
            db.shapes.push(Shape {
                alias,
                label,
                techn,
                descr,
                type_tag: tag.to_owned(),
                external,
                is_person,
                boundary: boundary_stack.last().copied(),
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            });
            continue;
        }

        if let Some(tag) = boundary_kind(kw) {
            let (label, ty) = if kw == "Node" || kw.starts_with("Node_") || kw == "Deployment_Node"
            {
                // Node(alias, label, type, descr)
                (
                    args.get(1).cloned().unwrap_or_default(),
                    args.get(2).cloned().unwrap_or_else(|| tag.to_owned()),
                )
            } else {
                // Boundary(alias, label, type)
                (
                    args.get(1).cloned().unwrap_or_default(),
                    args.get(2).cloned().unwrap_or_else(|| tag.to_owned()),
                )
            };
            let idx = db.boundaries.len();
            db.boundaries.push(Boundary {
                label,
                type_tag: ty,
                parent: boundary_stack.last().copied(),
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            });
            if opens_block {
                boundary_stack.push(idx);
            }
            continue;
        }

        if let Some(bidir) = rel_kind(kw) {
            let from = args.first().cloned().unwrap_or_default();
            let to = args.get(1).cloned().unwrap_or_default();
            let label = args.get(2).cloned().unwrap_or_default();
            let techn = args.get(3).cloned().unwrap_or_default();
            if from.is_empty() || to.is_empty() {
                continue;
            }
            db.rels.push(Rel {
                from,
                to,
                label,
                techn,
                bidir,
            });
            continue;
        }
        // UpdateElementStyle / UpdateRelStyle / UpdateLayoutConfig etc. ignored.
    }
    if !found {
        return Err(C4ParseError("missing C4 header".to_owned()));
    }
    Ok(db)
}

/// The stacked text lines drawn inside a shape box.
fn shape_lines(s: &Shape) -> Vec<(String, bool)> {
    // (text, is_emphasis) — emphasis lines are the type/techn tags.
    let mut lines = vec![
        (format!("«{}»", s.type_tag), true),
        (s.label.clone(), false),
    ];
    if !s.techn.is_empty() {
        lines.insert(1, (format!("[{}]", s.techn), true));
    }
    if !s.descr.is_empty() {
        lines.push((s.descr.clone(), false));
    }
    lines
}

/// Renders mermaid C4 source to an SVG (approximate layout).
///
/// # Errors
/// Returns [`C4ParseError`] when the source is not a valid C4 diagram.
pub fn render_c4(source: &str, id: &str) -> Result<String, C4ParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let tv = |k: &str| crate::render::themes::get(&theme_vars, k);
    let measurer = TextMeasurer::new();
    let mut db = parse(source)?;
    if db.shapes.is_empty() && db.boundaries.is_empty() {
        return Ok(empty_svg(id));
    }

    // Size each shape box from its content.
    for s in &mut db.shapes {
        let lines = shape_lines(s);
        let mut maxw = 0.0f64;
        for (t, _) in &lines {
            maxw = maxw.max(measurer.measure_width(t, FONT_SIZE));
        }
        s.w = (maxw + SHAPE_PAD * 2.0).max(SHAPE_W);
        s.h = lines.len() as f64 * LINE_H + SHAPE_PAD * 2.0;
    }

    // Layout: recursively lay out boundaries (and top-level shapes) as a
    // vertical stack of row-packed groups. `parent = None` is the diagram root.
    let mut cursor_y = DIAGRAM_MARGIN;
    let root_w = layout_container(&mut db, None, DIAGRAM_MARGIN, &mut cursor_y);
    let total_h = cursor_y + DIAGRAM_MARGIN;
    let total_w = root_w + DIAGRAM_MARGIN;

    // Build SVG.
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style_el = append(&svg, "style");
    set_text(&style_el, &c4_css(id, &tv));

    // Arrow marker.
    let defs = append(&svg, "defs");
    let marker = append(&defs, "marker");
    set_attr(&marker, "id", format!("{id}_c4-arrow"));
    set_attr(&marker, "viewBox", "0 0 10 10");
    set_attr(&marker, "refX", "9");
    set_attr(&marker, "refY", "5");
    set_attr(&marker, "markerWidth", "8");
    set_attr(&marker, "markerHeight", "8");
    set_attr(&marker, "orient", "auto");
    let mp = append(&marker, "path");
    set_attr(&mp, "d", "M 0 0 L 10 5 L 0 10 z");
    set_attr(&mp, "class", "c4-arrowhead");

    // Boundaries first (behind shapes).
    let bgroup = append(&svg, "g");
    set_attr(&bgroup, "class", "c4-boundaries");
    for b in &db.boundaries {
        let g = append(&bgroup, "g");
        set_attr(&g, "class", "c4-boundary");
        let r = append(&g, "rect");
        set_attr(&r, "x", js_num(b.x));
        set_attr(&r, "y", js_num(b.y));
        set_attr(&r, "width", js_num(b.w));
        set_attr(&r, "height", js_num(b.h));
        set_attr(&r, "class", "c4-boundary-rect");
        let label = append(&g, "text");
        set_attr(&label, "x", js_num(b.x + 12.0));
        set_attr(&label, "y", js_num(b.y + 20.0));
        set_attr(&label, "class", "c4-boundary-label");
        set_text(&label, &b.label);
        if !b.type_tag.is_empty() {
            let t = append(&g, "text");
            set_attr(&t, "x", js_num(b.x + 12.0));
            set_attr(&t, "y", js_num(b.y + 20.0 + LINE_H));
            set_attr(&t, "class", "c4-boundary-type");
            set_text(&t, &format!("[{}]", b.type_tag));
        }
    }

    // Relationships (drawn under shapes' text but over boundaries).
    let rgroup = append(&svg, "g");
    set_attr(&rgroup, "class", "c4-rels");
    for rel in &db.rels {
        let (Some(a), Some(b)) = (find_shape(&db, &rel.from), find_shape(&db, &rel.to)) else {
            continue;
        };
        let (ax, ay) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
        let (bx, by) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let path = append(&rgroup, "line");
        set_attr(&path, "x1", js_num(ax));
        set_attr(&path, "y1", js_num(ay));
        set_attr(&path, "x2", js_num(bx));
        set_attr(&path, "y2", js_num(by));
        set_attr(&path, "class", "c4-rel-line");
        set_attr(&path, "marker-end", format!("url(#{id}_c4-arrow)"));
        if rel.bidir {
            set_attr(&path, "marker-start", format!("url(#{id}_c4-arrow)"));
        }
        let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
        if !rel.label.is_empty() {
            let lt = append(&rgroup, "text");
            set_attr(&lt, "x", js_num(mx));
            set_attr(&lt, "y", js_num(my));
            set_attr(&lt, "text-anchor", "middle");
            set_attr(&lt, "class", "c4-rel-label");
            set_text(&lt, &rel.label);
        }
        if !rel.techn.is_empty() {
            let tt = append(&rgroup, "text");
            set_attr(&tt, "x", js_num(mx));
            set_attr(&tt, "y", js_num(my + LINE_H));
            set_attr(&tt, "text-anchor", "middle");
            set_attr(&tt, "class", "c4-rel-techn");
            set_text(&tt, &format!("[{}]", rel.techn));
        }
    }

    // Shapes on top.
    let sgroup = append(&svg, "g");
    set_attr(&sgroup, "class", "c4-shapes");
    for s in &db.shapes {
        let g = append(&sgroup, "g");
        set_attr(
            &g,
            "class",
            if s.external {
                "c4-shape c4-external"
            } else {
                "c4-shape"
            },
        );
        if s.is_person {
            // Head circle above a rounded body.
            let head = append(&g, "circle");
            set_attr(&head, "cx", js_num(s.x + s.w / 2.0));
            set_attr(&head, "cy", js_num(s.y - 2.0));
            set_attr(&head, "r", "14");
            set_attr(&head, "class", "c4-person-head");
        }
        let r = append(&g, "rect");
        set_attr(&r, "x", js_num(s.x));
        set_attr(&r, "y", js_num(s.y));
        set_attr(&r, "width", js_num(s.w));
        set_attr(&r, "height", js_num(s.h));
        set_attr(&r, "rx", if s.is_person { "8" } else { "3" });
        set_attr(&r, "ry", if s.is_person { "8" } else { "3" });
        set_attr(&r, "class", "c4-shape-rect");
        let lines = shape_lines(s);
        let mut ty = s.y + SHAPE_PAD + FONT_SIZE;
        for (text, emph) in &lines {
            let t = append(&g, "text");
            set_attr(&t, "x", js_num(s.x + s.w / 2.0));
            set_attr(&t, "y", js_num(ty));
            set_attr(&t, "text-anchor", "middle");
            set_attr(
                &t,
                "class",
                if *emph {
                    "c4-shape-tag"
                } else {
                    "c4-shape-label"
                },
            );
            set_text(&t, text);
            ty += LINE_H;
        }
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
    set_attr(&svg, "aria-roledescription", "c4");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// Lays out all direct children (shapes + child boundaries) of `parent`,
/// starting at `(start_x, *cursor_y)`. Returns the right edge reached.
/// Advances `*cursor_y` past everything placed.
fn layout_container(db: &mut C4Db, parent: Option<usize>, start_x: f64, cursor_y: &mut f64) -> f64 {
    let mut max_x = start_x;

    // 1. Direct shapes, packed into rows.
    let shape_idxs: Vec<usize> = (0..db.shapes.len())
        .filter(|&i| db.shapes[i].boundary == parent)
        .collect();
    let mut col = 0;
    let mut row_x = start_x;
    let mut row_h = 0.0f64;
    for &i in &shape_idxs {
        if col == SHAPES_PER_ROW {
            *cursor_y += row_h + SHAPE_MARGIN;
            col = 0;
            row_x = start_x;
            row_h = 0.0;
        }
        let (w, h) = (db.shapes[i].w, db.shapes[i].h);
        db.shapes[i].x = row_x;
        db.shapes[i].y = *cursor_y;
        row_x += w + SHAPE_MARGIN;
        max_x = max_x.max(row_x - SHAPE_MARGIN);
        row_h = row_h.max(h);
        col += 1;
    }
    if !shape_idxs.is_empty() {
        *cursor_y += row_h + SHAPE_MARGIN;
    }

    // 2. Direct child boundaries, stacked vertically.
    let child_idxs: Vec<usize> = (0..db.boundaries.len())
        .filter(|&i| db.boundaries[i].parent == parent)
        .collect();
    for &bi in &child_idxs {
        let bx = start_x;
        let by = *cursor_y;
        let inner_x = bx + BOUNDARY_PAD;
        let mut inner_y = by + BOUNDARY_HEADER + BOUNDARY_PAD;
        let inner_right = layout_container(db, Some(bi), inner_x, &mut inner_y);
        let b_w = (inner_right - bx + BOUNDARY_PAD).max(SHAPE_W + BOUNDARY_PAD * 2.0);
        let b_h = inner_y - by + BOUNDARY_PAD - SHAPE_MARGIN;
        db.boundaries[bi].x = bx;
        db.boundaries[bi].y = by;
        db.boundaries[bi].w = b_w;
        db.boundaries[bi].h = b_h.max(BOUNDARY_HEADER + BOUNDARY_PAD * 2.0);
        max_x = max_x.max(bx + db.boundaries[bi].w);
        *cursor_y = by + db.boundaries[bi].h + SHAPE_MARGIN;
    }

    max_x
}

fn find_shape<'a>(db: &'a C4Db, alias: &str) -> Option<&'a Shape> {
    db.shapes.iter().find(|s| s.alias == alias)
}

fn c4_css(id: &str, tv: &dyn Fn(&str) -> String) -> String {
    let font = tv("fontFamily");
    let text_color = tv("textColor");
    let line = tv("lineColor");
    let bold_stroke = crate::render::handdrawn::embolden_decls(&text_color);
    let mut o = String::new();
    let _ = write!(
        o,
        "#{id}{{font-family:{font};font-size:{FONT_SIZE}px;fill:{text_color};}}\
         #{id} .c4-boundary-rect{{fill:none;stroke:{line};stroke-width:1px;stroke-dasharray:7,5;}}\
         #{id} .c4-boundary-label{{font-weight:bold;fill:{text_color};{bold_stroke}}}\
         #{id} .c4-boundary-type{{fill:{text_color};font-size:12px;opacity:0.7;}}\
         #{id} .c4-shape-rect{{fill:#1168bd;stroke:#3c7fc0;stroke-width:1px;}}\
         #{id} .c4-external .c4-shape-rect{{fill:#8c8c8c;stroke:#6b6b6b;}}\
         #{id} .c4-person-head{{fill:#08427b;stroke:#073b6f;}}\
         #{id} .c4-external .c4-person-head{{fill:#6b6b6b;stroke:#4d4d4d;}}\
         #{id} .c4-shape-label{{fill:#ffffff;}}\
         #{id} .c4-shape-tag{{fill:#e6f0fa;font-size:11px;}}\
         #{id} .c4-rel-line{{stroke:{line};stroke-width:1px;stroke-dasharray:4,3;fill:none;}}\
         #{id} .c4-arrowhead{{fill:{line};}}\
         #{id} .c4-rel-label{{fill:{text_color};font-size:12px;}}\
         #{id} .c4-rel-techn{{fill:{text_color};font-size:11px;opacity:0.7;}}"
    );
    o
}

fn empty_svg(id: &str) -> String {
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "aria-roledescription", "c4");
    let mut out = String::new();
    serialize(&svg, &mut out);
    out
}
