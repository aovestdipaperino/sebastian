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

/// Style overrides of a point or a `classDef` (`quadrantDb.parseStyles`).
#[derive(Debug, Default, Clone)]
struct PointStyle {
    radius: Option<i64>,
    color: Option<String>,
    stroke_color: Option<String>,
    stroke_width: Option<String>,
}

#[derive(Debug)]
struct Point {
    name: String,
    x: f64,
    y: f64,
    class: String,
    style: PointStyle,
}

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
    points: Vec<Point>,
    classes: Vec<(String, PointStyle)>,
}

fn err(message: String) -> QuadrantParseError {
    QuadrantParseError { message }
}

/// Case-insensitive `strip_prefix` for an ASCII keyword.
fn strip_keyword<'a>(line: &'a str, kw: &str) -> Option<&'a str> {
    let head = line.get(..kw.len())?;
    head.eq_ignore_ascii_case(kw).then(|| &line[kw.len()..])
}

/// The lexer's `text`: a quoted or markdown-quoted string, or the raw run.
fn parse_text(raw: &str) -> String {
    let t = raw.trim();
    if let Some(inner) = t.strip_prefix("\"`").and_then(|r| r.strip_suffix("`\"")) {
        return inner.to_owned();
    }
    if let Some(inner) = t.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return inner.to_owned();
    }
    t.to_owned()
}

/// Splits axis text on the `--+>` delimiter.
fn split_axis(rest: &str) -> (String, Option<String>) {
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() + 1 {
        if bytes.get(i) == Some(&b'-') && bytes.get(i + 1) == Some(&b'-') {
            let mut j = i;
            while bytes.get(j) == Some(&b'-') {
                j += 1;
            }
            if bytes.get(j) == Some(&b'>') {
                return (parse_text(&rest[..i]), Some(parse_text(&rest[j + 1..])));
            }
        }
        i += 1;
    }
    (parse_text(rest), None)
}

fn parse_styles(parts: &str) -> Result<PointStyle, QuadrantParseError> {
    let mut style = PointStyle::default();
    for part in parts.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part
            .split_once(':')
            .map_or((part, ""), |(k, v)| (k.trim(), v.trim()));
        let hex = |v: &str| {
            let h = v.strip_prefix('#').unwrap_or(v);
            matches!(h.len(), 3 | 6) && h.chars().all(|c| c.is_ascii_hexdigit())
        };
        match key {
            "radius" => {
                style.radius = Some(value.parse::<f64>().map_err(|_| {
                    err(format!(
                        "value for radius {value} is invalid, please use a valid number"
                    ))
                })? as i64);
            }
            "color" if hex(value) => style.color = Some(value.to_owned()),
            "stroke-color" if hex(value) => style.stroke_color = Some(value.to_owned()),
            "color" | "stroke-color" => {
                return Err(err(format!(
                    "value for {key} {value} is invalid, please use a valid hex code"
                )));
            }
            "stroke-width" => {
                let digits = value.strip_suffix("px").unwrap_or("");
                if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                    return Err(err(format!(
                        "value for stroke-width {value} is invalid, please use a valid number of pixels (eg. 10px)"
                    )));
                }
                style.stroke_width = Some(value.to_owned());
            }
            other => return Err(err(format!("style named {other} is not supported."))),
        }
    }
    Ok(style)
}

/// Finds the `: [` that opens a point's coordinates (not a `:::` class marker).
fn point_open(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b':' {
            if b.get(i + 1) == Some(&b':') && b.get(i + 2) == Some(&b':') {
                i += 3;
                continue;
            }
            if line[i + 1..].trim_start().starts_with('[') {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn parse_point(line: &str, open: usize) -> Result<Point, QuadrantParseError> {
    let bad = || err(format!("bad quadrant point: {line}"));
    let (name_part, class) = match line[..open].split_once(":::") {
        Some((n, c)) => (n, c.trim().to_owned()),
        None => (&line[..open], String::new()),
    };
    let after = line[open + 1..]
        .trim_start()
        .strip_prefix('[')
        .ok_or_else(bad)?;
    let close = after.find(']').ok_or_else(bad)?;
    let (xs, ys) = after[..close].split_once(',').ok_or_else(bad)?;
    let x: f64 = xs.trim().parse().map_err(|_| bad())?;
    let y: f64 = ys.trim().parse().map_err(|_| bad())?;
    if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return Err(bad());
    }
    Ok(Point {
        name: parse_text(name_part),
        x,
        y,
        class,
        style: parse_styles(&after[close + 1..])?,
    })
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
            if strip_keyword(line, "quadrantChart").is_some() {
                found_header = true;
                continue;
            }
            return Err(err(format!("expected quadrantChart header, got {line:?}")));
        }
        if let Some(rest) = strip_keyword(line, "title") {
            db.title = rest.trim().to_owned();
            continue;
        }
        let axis = [("x-axis", true), ("y-axis", false)]
            .iter()
            .find_map(|(kw, is_x)| strip_keyword(line, kw).map(|rest| (rest, *is_x)));
        if let Some((rest, is_x)) = axis {
            let (first, second) = split_axis(rest);
            // A trailing delimiter without a second text appends " ⟶ ", which
            // mermaid's sanitizer then trims.
            let (first, second) = match second {
                Some(s) if s.is_empty() => (format!("{first} ⟶"), String::new()),
                Some(s) => (first, s),
                None => (first, String::new()),
            };
            if is_x {
                db.x_left = first;
                db.x_right = second;
            } else {
                db.y_bottom = first;
                db.y_top = second;
            }
            continue;
        }
        let mut handled = false;
        for n in 1..=4 {
            if let Some(rest) = strip_keyword(line, &format!("quadrant-{n}")) {
                let t = parse_text(rest);
                match n {
                    1 => db.q1 = t,
                    2 => db.q2 = t,
                    3 => db.q3 = t,
                    _ => db.q4 = t,
                }
                handled = true;
                break;
            }
        }
        if handled {
            continue;
        }
        if let Some(rest) = line.strip_prefix("classDef") {
            let rest = rest.trim_start();
            let (name, styles) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            db.classes.push((name.to_owned(), parse_styles(styles)?));
            continue;
        }
        if let Some(open) = point_open(line) {
            db.points.push(parse_point(line, open)?);
            continue;
        }
        return Err(err(format!("unsupported quadrantChart statement: {line}")));
    }
    if !found_header {
        return Err(err("missing quadrantChart header".to_owned()));
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
    let chart_width = config
        .section_f64("quadrantChart", "chartWidth")
        .unwrap_or(CHART_WIDTH);
    let chart_height = config
        .section_f64("quadrantChart", "chartHeight")
        .unwrap_or(CHART_HEIGHT);
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
    let q_width = chart_width - QUADRANT_PADDING * 2.0 - y_left;
    let q_height = chart_height - QUADRANT_PADDING * 2.0 - x_top - x_bottom - title_top;
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
            crate::render::css_length(chart_width)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!("0 0 {} {}", js_num(chart_width), js_num(chart_height)),
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
        // d3 `.text('')` leaves a text element with no child node.
        if !t.text.is_empty() {
            set_text(&text, &t.text);
        }
    };

    // Title.
    if show_title {
        draw_text(
            &title_g,
            &TextItem {
                text: db.title.clone(),
                fill: v("quadrantTitleFill"),
                x: chart_width / 2.0,
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
    for point in db.points.iter().rev() {
        let (px, py) = (point.x, point.y);
        // classDef styles under the point's own styles.
        let mut style = db
            .classes
            .iter()
            .rev()
            .find(|(n, _)| *n == point.class)
            .map(|(_, st)| st.clone())
            .unwrap_or_default();
        let own = &point.style;
        style.radius = own.radius.or(style.radius);
        style.color = own.color.clone().or(style.color);
        style.stroke_color = own.stroke_color.clone().or(style.stroke_color);
        style.stroke_width = own.stroke_width.clone().or(style.stroke_width);
        let fill = style.color.clone().unwrap_or_else(|| point_fill.clone());
        let name = &point.name;
        // d3 scaleLinear interpolate: a*(1-t) + b*t (not fma).
        let cx = q_left * (1.0 - px) + (q_width + q_left) * px;
        let cy = (q_height + q_top) * (1.0 - py) + q_top * py;
        let g = append(&data_g, "g");
        set_attr(&g, "class", "data-point");
        let circle = append(&g, "circle");
        set_attr(&circle, "cx", js_num(cx));
        set_attr(&circle, "cy", js_num(cy));
        #[allow(clippy::cast_precision_loss)]
        set_attr(
            &circle,
            "r",
            js_num(style.radius.map_or(POINT_RADIUS, |r| r as f64)),
        );
        set_attr(&circle, "fill", fill);
        set_attr(
            &circle,
            "stroke",
            style
                .stroke_color
                .clone()
                .unwrap_or_else(|| point_fill.clone()),
        );
        set_attr(
            &circle,
            "stroke-width",
            style
                .stroke_width
                .clone()
                .unwrap_or_else(|| "0px".to_owned()),
        );
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
