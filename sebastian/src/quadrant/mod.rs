//! quadrantChart support: parser subset, `quadrantBuilder.ts` layout, and a
//! direct port of `quadrantRenderer.ts`.

#![allow(clippy::assigning_clones)]
use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};

/// A parse error for quadrantChart source.
#[derive(Debug)]
pub struct QuadrantParseError {
    pub message: String,
}

impl std::fmt::Display for QuadrantParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "quadrantChart parse error: {}", self.message)
    }
}

impl std::error::Error for QuadrantParseError {}

const CHART_WIDTH: f64 = 500.0;
const CHART_HEIGHT: f64 = 500.0;
const TITLE_PADDING: f64 = 10.0;
const TITLE_FONT_SIZE: f64 = 20.0;
const QUADRANT_PADDING: f64 = 5.0;
const X_AXIS_LABEL_PADDING: f64 = 5.0;
const Y_AXIS_LABEL_PADDING: f64 = 5.0;
const X_AXIS_LABEL_FONT_SIZE: f64 = 16.0;
const Y_AXIS_LABEL_FONT_SIZE: f64 = 16.0;
const QUADRANT_LABEL_FONT_SIZE: f64 = 16.0;
const QUADRANT_TEXT_TOP_PADDING: f64 = 5.0;
const POINT_TEXT_PADDING: f64 = 5.0;
const POINT_LABEL_FONT_SIZE: f64 = 12.0;
const POINT_RADIUS: f64 = 5.0;
const INTERNAL_BORDER_WIDTH: f64 = 1.0;
const EXTERNAL_BORDER_WIDTH: f64 = 2.0;

#[derive(Debug, Default)]
struct Db {
    title: String,
    x_left: String,
    x_right: String,
    y_bottom: String,
    y_top: String,
    q1: String,
    q2: String,
    q3: String,
    q4: String,
    points: Vec<(String, f64, f64)>,
}

fn parse(source: &str) -> Result<Db, QuadrantParseError> {
    let mut db = Db::default();
    let mut found_header = false;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found_header {
            if line.starts_with("quadrantChart") {
                found_header = true;
                continue;
            }
            return Err(QuadrantParseError {
                message: format!("expected quadrantChart header, got {line:?}"),
            });
        }
        if let Some(rest) = line.strip_prefix("title") {
            db.title = rest.trim().to_owned();
            continue;
        }
        if let Some(rest) = line.strip_prefix("x-axis") {
            let rest = rest.trim();
            if let Some((l, r)) = rest.split_once("-->") {
                db.x_left = l.trim().to_owned();
                db.x_right = r.trim().to_owned();
            } else {
                db.x_left = rest.to_owned();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("y-axis") {
            let rest = rest.trim();
            if let Some((b, t)) = rest.split_once("-->") {
                db.y_bottom = b.trim().to_owned();
                db.y_top = t.trim().to_owned();
            } else {
                db.y_bottom = rest.to_owned();
            }
            continue;
        }
        for (kw, field) in [
            ("quadrant-1", 1),
            ("quadrant-2", 2),
            ("quadrant-3", 3),
            ("quadrant-4", 4),
        ] {
            if let Some(rest) = line.strip_prefix(kw) {
                let t = rest.trim().to_owned();
                match field {
                    1 => db.q1 = t,
                    2 => db.q2 = t,
                    3 => db.q3 = t,
                    _ => db.q4 = t,
                }
                break;
            }
        }
        if line.starts_with("quadrant-") {
            continue;
        }
        // Point: name: [x, y]
        if let Some((name, rest)) = line.split_once(':') {
            let rest = rest.trim();
            if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']'))
                && let Some((xs, ys)) = inner.split_once(',')
            {
                let x: f64 = xs.trim().parse().map_err(|_| QuadrantParseError {
                    message: format!("bad quadrant point: {line}"),
                })?;
                let y: f64 = ys.trim().parse().map_err(|_| QuadrantParseError {
                    message: format!("bad quadrant point: {line}"),
                })?;
                db.points.push((name.trim().to_owned(), x, y));
                continue;
            }
        }
        return Err(QuadrantParseError {
            message: format!("unsupported quadrantChart statement: {line}"),
        });
    }
    if !found_header {
        return Err(QuadrantParseError {
            message: "missing quadrantChart header".to_owned(),
        });
    }
    Ok(db)
}

/// One text element to draw.
struct TextItem {
    text: String,
    fill: String,
    x: f64,
    y: f64,
    font_size: f64,
    /// verticalPos maps to text-anchor.
    anchor_left: bool,
    /// horizontalPos maps to dominant-baseline.
    hanging: bool,
    rotation: f64,
}

/// Renders quadrantChart source to a complete SVG document string.
///
/// # Errors
/// Returns a [`QuadrantParseError`] when the source is not a valid chart.
#[allow(clippy::too_many_lines)]
pub fn render_quadrant(source: &str, id: &str) -> Result<String, QuadrantParseError> {
    let config = crate::render::config::detect_init(source);
    let hand_drawn = config.is_hand_drawn();
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;
    let v = |k: &str| crate::render::themes::get(&theme_vars, k);

    let show_x = !db.x_left.is_empty() || !db.x_right.is_empty();
    let show_y = !db.y_top.is_empty() || !db.y_bottom.is_empty();
    let show_title = !db.title.is_empty();
    let x_axis_position = if db.points.is_empty() {
        "top"
    } else {
        "bottom"
    };

    // calculateSpace.
    let x_axis_calc = X_AXIS_LABEL_PADDING.mul_add(2.0, X_AXIS_LABEL_FONT_SIZE);
    let x_top = if x_axis_position == "top" && show_x {
        x_axis_calc
    } else {
        0.0
    };
    let x_bottom = if x_axis_position == "bottom" && show_x {
        x_axis_calc
    } else {
        0.0
    };
    let y_axis_calc = Y_AXIS_LABEL_PADDING.mul_add(2.0, Y_AXIS_LABEL_FONT_SIZE);
    let y_left = if show_y { y_axis_calc } else { 0.0 };
    let title_calc = TITLE_PADDING.mul_add(2.0, TITLE_FONT_SIZE);
    let title_top = if show_title { title_calc } else { 0.0 };

    let q_left = QUADRANT_PADDING + y_left;
    let q_top = QUADRANT_PADDING + x_top + title_top;
    let q_width = CHART_WIDTH - QUADRANT_PADDING * 2.0 - y_left;
    let q_height = CHART_HEIGHT - QUADRANT_PADDING * 2.0 - x_top - x_bottom - title_top;
    let q_half_w = q_width / 2.0;
    let q_half_h = q_height / 2.0;

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(CHART_WIDTH)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!("0 0 {} {}", js_num(CHART_WIDTH), js_num(CHART_HEIGHT)),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "quadrantChart");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_quadrant_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    let main = append(&svg, "g");
    set_attr(&main, "class", "main");
    let quadrants_g = append(&main, "g");
    set_attr(&quadrants_g, "class", "quadrants");
    let border_g = append(&main, "g");
    set_attr(&border_g, "class", "border");
    let data_g = append(&main, "g");
    set_attr(&data_g, "class", "data-points");
    let label_g = append(&main, "g");
    set_attr(&label_g, "class", "labels");
    let title_g = append(&main, "g");
    set_attr(&title_g, "class", "title");

    let draw_text = |parent: &Element, t: &TextItem| {
        let text = append(parent, "text");
        set_attr(&text, "x", "0");
        set_attr(&text, "y", "0");
        set_attr(&text, "fill", t.fill.clone());
        set_attr(&text, "font-size", js_num(t.font_size));
        set_attr(
            &text,
            "dominant-baseline",
            if t.hanging { "hanging" } else { "middle" },
        );
        set_attr(
            &text,
            "text-anchor",
            if t.anchor_left { "start" } else { "middle" },
        );
        set_attr(
            &text,
            "transform",
            format!(
                "translate({}, {}) rotate({})",
                js_num(t.x),
                js_num(t.y),
                js_num(t.rotation)
            ),
        );
        set_text(&text, &t.text);
    };

    // Title.
    if show_title {
        draw_text(
            &title_g,
            &TextItem {
                text: db.title.clone(),
                fill: v("quadrantTitleFill"),
                x: CHART_WIDTH / 2.0,
                y: TITLE_PADDING,
                font_size: TITLE_FONT_SIZE,
                anchor_left: false,
                hanging: true,
                rotation: 0.0,
            },
        );
    }

    // Borders.
    let half_ext = EXTERNAL_BORDER_WIDTH / 2.0;
    let border = |x1: f64, y1: f64, x2: f64, y2: f64, fill: &str, w: f64| {
        let line = append(&border_g, "line");
        set_attr(&line, "x1", js_num(x1));
        set_attr(&line, "y1", js_num(y1));
        set_attr(&line, "x2", js_num(x2));
        set_attr(&line, "y2", js_num(y2));
        // d3 .style('stroke', color) serializes through CSSOM (hsl -> rgb).
        let stroke = crate::render::css::cssom_color_value("stroke", fill);
        set_attr(
            &line,
            "style",
            format!("stroke: {stroke}; stroke-width: {};", js_num(w)),
        );
    };
    let ext = v("quadrantExternalBorderStrokeFill");
    let int = v("quadrantInternalBorderStrokeFill");
    // top, right, bottom, left, vertical inner, horizontal inner.
    border(
        q_left - half_ext,
        q_top,
        q_left + q_width + half_ext,
        q_top,
        &ext,
        EXTERNAL_BORDER_WIDTH,
    );
    border(
        q_left + q_width,
        q_top + half_ext,
        q_left + q_width,
        q_top + q_height - half_ext,
        &ext,
        EXTERNAL_BORDER_WIDTH,
    );
    border(
        q_left - half_ext,
        q_top + q_height,
        q_left + q_width + half_ext,
        q_top + q_height,
        &ext,
        EXTERNAL_BORDER_WIDTH,
    );
    border(
        q_left,
        q_top + half_ext,
        q_left,
        q_top + q_height - half_ext,
        &ext,
        EXTERNAL_BORDER_WIDTH,
    );
    border(
        q_left + q_half_w,
        q_top + half_ext,
        q_left + q_half_w,
        q_top + q_height - half_ext,
        &int,
        INTERNAL_BORDER_WIDTH,
    );
    border(
        q_left + half_ext,
        q_top + q_half_h,
        q_left + q_width - half_ext,
        q_top + q_half_h,
        &int,
        INTERNAL_BORDER_WIDTH,
    );

    // Quadrants (rect + label). Order 1,2,3,4.
    let quads = [
        (
            &db.q1,
            "quadrant1TextFill",
            "quadrant1Fill",
            q_left + q_half_w,
            q_top,
        ),
        (&db.q2, "quadrant2TextFill", "quadrant2Fill", q_left, q_top),
        (
            &db.q3,
            "quadrant3TextFill",
            "quadrant3Fill",
            q_left,
            q_top + q_half_h,
        ),
        (
            &db.q4,
            "quadrant4TextFill",
            "quadrant4Fill",
            q_left + q_half_w,
            q_top + q_half_h,
        ),
    ];
    let has_points = !db.points.is_empty();
    for (text, text_fill, fill, qx, qy) in quads {
        let g = append(&quadrants_g, "g");
        set_attr(&g, "class", "quadrant");
        let rect = append(&g, "rect");
        set_attr(&rect, "x", js_num(qx));
        set_attr(&rect, "y", js_num(qy));
        set_attr(&rect, "width", js_num(q_half_w));
        set_attr(&rect, "height", js_num(q_half_h));
        set_attr(&rect, "fill", v(fill));
        if hand_drawn {
            crate::render::handdrawn::hd_overlay_rect(&g, qx, qy, q_half_w, q_half_h, &v(fill), "");
        }
        let tx = qx + q_half_w / 2.0;
        let (ty, hanging) = if has_points {
            (qy + QUADRANT_TEXT_TOP_PADDING, true)
        } else {
            (qy + q_half_h / 2.0, false)
        };
        draw_text(
            &g,
            &TextItem {
                text: text.clone(),
                fill: v(text_fill),
                x: tx,
                y: ty,
                font_size: QUADRANT_LABEL_FONT_SIZE,
                anchor_left: false,
                hanging,
                rotation: 0.0,
            },
        );
    }

    // Axis labels.
    let draw_x_middle = !db.x_right.is_empty();
    let draw_y_middle = !db.y_top.is_empty();
    let x_axis_text_fill = v("quadrantXAxisTextFill");
    let y_axis_text_fill = v("quadrantYAxisTextFill");
    let x_label_y = if x_axis_position == "top" {
        X_AXIS_LABEL_PADDING + title_top
    } else {
        X_AXIS_LABEL_PADDING + q_top + q_height + QUADRANT_PADDING
    };
    let push_axis_label = |item: &TextItem| {
        let g = append(&label_g, "g");
        set_attr(&g, "class", "label");
        draw_text(&g, item);
    };
    if !db.x_left.is_empty() && show_x {
        push_axis_label(&wrap_label(&TextItem {
            text: db.x_left.clone(),
            fill: x_axis_text_fill.clone(),
            x: q_left + if draw_x_middle { q_half_w / 2.0 } else { 0.0 },
            y: x_label_y,
            font_size: X_AXIS_LABEL_FONT_SIZE,
            anchor_left: !draw_x_middle,
            hanging: true,
            rotation: 0.0,
        }));
    }
    if !db.x_right.is_empty() && show_x {
        push_axis_label(&wrap_label(&TextItem {
            text: db.x_right.clone(),
            fill: x_axis_text_fill,
            x: q_left + q_half_w + if draw_x_middle { q_half_w / 2.0 } else { 0.0 },
            y: x_label_y,
            font_size: X_AXIS_LABEL_FONT_SIZE,
            anchor_left: !draw_x_middle,
            hanging: true,
            rotation: 0.0,
        }));
    }
    let y_label_x = Y_AXIS_LABEL_PADDING; // yAxisPosition left.
    if !db.y_bottom.is_empty() && show_y {
        push_axis_label(&wrap_label(&TextItem {
            text: db.y_bottom.clone(),
            fill: y_axis_text_fill.clone(),
            x: y_label_x,
            y: q_top + q_height - if draw_y_middle { q_half_h / 2.0 } else { 0.0 },
            font_size: Y_AXIS_LABEL_FONT_SIZE,
            anchor_left: !draw_y_middle,
            hanging: true,
            rotation: -90.0,
        }));
    }
    if !db.y_top.is_empty() && show_y {
        push_axis_label(&wrap_label(&TextItem {
            text: db.y_top.clone(),
            fill: y_axis_text_fill,
            x: y_label_x,
            y: q_top + q_half_h - if draw_y_middle { q_half_h / 2.0 } else { 0.0 },
            font_size: Y_AXIS_LABEL_FONT_SIZE,
            anchor_left: !draw_y_middle,
            hanging: true,
            rotation: -90.0,
        }));
    }

    // Data points (circle + label). Prepended in input order, so reversed.
    let point_fill = v("quadrantPointFill");
    let point_text_fill = v("quadrantPointTextFill");
    for (name, px, py) in db.points.iter().rev() {
        // d3 scaleLinear interpolate: a*(1-t) + b*t (not fma).
        let cx = q_left * (1.0 - px) + (q_width + q_left) * px;
        let cy = (q_height + q_top) * (1.0 - py) + q_top * py;
        let g = append(&data_g, "g");
        set_attr(&g, "class", "data-point");
        let circle = append(&g, "circle");
        set_attr(&circle, "cx", js_num(cx));
        set_attr(&circle, "cy", js_num(cy));
        set_attr(&circle, "r", js_num(POINT_RADIUS));
        set_attr(&circle, "fill", point_fill.clone());
        set_attr(&circle, "stroke", point_fill.clone());
        set_attr(&circle, "stroke-width", "0px");
        draw_text(
            &g,
            &TextItem {
                text: name.clone(),
                fill: point_text_fill.clone(),
                x: cx,
                y: cy + POINT_TEXT_PADDING,
                font_size: POINT_LABEL_FONT_SIZE,
                anchor_left: false,
                hanging: true,
                rotation: 0.0,
            },
        );
    }

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// The builder computes label positions with no text wrapping in the common
/// case; pass through unchanged.
fn wrap_label(t: &TextItem) -> TextItem {
    TextItem {
        text: t.text.clone(),
        fill: t.fill.clone(),
        x: t.x,
        y: t.y,
        font_size: t.font_size,
        anchor_left: t.anchor_left,
        hanging: t.hanging,
        rotation: t.rotation,
    }
}
