//! pie chart support: parser (`pieParser.ts` subset), and a direct port of
//! `pieRenderer.ts` (d3 arcs with the d3-path `digits(3)` serializer).

use crate::render::themes;
use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

/// A parse error for pie source.
#[derive(Debug)]
pub struct PieParseError {
    pub message: String,
}

impl std::fmt::Display for PieParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pie parse error: {}", self.message)
    }
}

impl std::error::Error for PieParseError {}

#[derive(Debug, Default)]
struct PieDb {
    title: String,
    show_data: bool,
    /// Insertion-ordered (label, value); duplicate labels keep the first.
    sections: Vec<(String, f64)>,
}

fn parse(source: &str) -> Result<PieDb, PieParseError> {
    let mut db = PieDb::default();
    let mut found_header = false;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found_header {
            let Some(rest) = line.strip_prefix("pie") else {
                return Err(PieParseError {
                    message: format!("expected pie header, got {line:?}"),
                });
            };
            found_header = true;
            let mut rest = rest.trim();
            if let Some(r) = rest.strip_prefix("showData") {
                db.show_data = true;
                rest = r.trim();
            }
            if let Some(r) = rest.strip_prefix("title") {
                db.title = r.trim().to_owned();
            }
            continue;
        }
        if let Some(r) = line.strip_prefix("title") {
            db.title = r.trim().to_owned();
            continue;
        }
        if line == "showData" {
            db.show_data = true;
            continue;
        }
        if let Some(r) = line
            .strip_prefix("accTitle:")
            .or_else(|| line.strip_prefix("accDescr:"))
        {
            let _ = r;
            continue;
        }
        // "Label" : value
        if let Some(rest) = line.strip_prefix('"')
            && let Some(endq) = rest.find('"')
        {
            let label = rest[..endq].to_owned();
            let after = rest[endq + 1..].trim();
            if let Some(vs) = after.strip_prefix(':') {
                let value: f64 = vs.trim().parse().map_err(|_| PieParseError {
                    message: format!("bad pie value: {line}"),
                })?;
                if !db.sections.iter().any(|(l, _)| *l == label) {
                    db.sections.push((label, value));
                }
                continue;
            }
        }
        return Err(PieParseError {
            message: format!("unsupported pie statement: {line}"),
        });
    }
    if !found_header {
        return Err(PieParseError {
            message: "missing pie header".to_owned(),
        });
    }
    Ok(db)
}

/// JS `Math.round` (half toward +infinity).
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// d3-path `appendRound(3)` number formatting.
fn round3(x: f64) -> String {
    js_num(js_round(x * 1000.0) / 1000.0)
}

/// JS `Number.prototype.toFixed(0)`.
fn to_fixed0(x: f64) -> String {
    format!("{}", js_round(x))
}

const TAU: f64 = std::f64::consts::TAU;
const HALF_PI: f64 = std::f64::consts::FRAC_PI_2;
const EPSILON: f64 = 1e-12; // d3-shape epsilon
const PATH_EPSILON: f64 = 1e-6; // d3-path epsilon

fn cos(x: f64) -> f64 {
    core_math::cos(x)
}
fn sin(x: f64) -> f64 {
    core_math::sin(x)
}

/// `d3.arc().innerRadius(0).outerRadius(r1)` path for one sector, with the
/// d3-path digits(3) serializer.
fn arc_path(r1: f64, start_angle: f64, end_angle: f64) -> String {
    let a0 = start_angle - HALF_PI;
    let a1 = end_angle - HALF_PI;
    let da = (a1 - a0).abs();
    let mut d = String::new();
    let mut push = |s: String| d.push_str(&s);

    if r1 <= EPSILON {
        push("M0,0".to_owned());
    } else if da > TAU - EPSILON {
        // Full circle: move, then d3-path arc() emits two A segments.
        let x0 = r1 * cos(a0);
        let y0 = r1 * sin(a0);
        push(format!("M{},{}", round3(x0), round3(y0)));
        // context.arc(0,0,r1,a0,a1,false): da normalized to tau.
        push(format!(
            "A{},{},0,1,1,{},{}A{},{},0,1,1,{},{}",
            round3(r1),
            round3(r1),
            round3(-x0),
            round3(-y0),
            round3(r1),
            round3(r1),
            round3(x0),
            round3(y0)
        ));
        push("L0,0".to_owned());
    } else {
        let x01 = r1 * cos(a0);
        let y01 = r1 * sin(a0);
        push(format!("M{},{}", round3(x01), round3(y01)));
        // context.arc(0,0,r1,a0,a1,!cw) with cw (a1 > a0): start point
        // coincides with current point, so no extra L.
        let da_arc = a1 - a0; // cw
        if da_arc > PATH_EPSILON {
            push(format!(
                "A{},{},0,{},1,{},{}",
                round3(r1),
                round3(r1),
                i32::from(da_arc >= std::f64::consts::PI),
                round3(r1 * cos(a1)),
                round3(r1 * sin(a1))
            ));
        }
        // innerRadius 0: lineTo(0, 0)
        push("L0,0".to_owned());
    }
    push("Z".to_owned());
    d
}

/// `arc.centroid` for the label arc (inner == outer == r): full precision.
fn centroid(r: f64, start_angle: f64, end_angle: f64) -> (f64, f64) {
    let a = (start_angle + end_angle) / 2.0 - HALF_PI;
    (cos(a) * r, sin(a) * r)
}

/// Renders pie source to a complete SVG document string.
///
/// # Errors
/// Returns a [`PieParseError`] when the source is not a valid pie chart.
pub fn render_pie(source: &str, id: &str) -> Result<String, PieParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;
    let measurer = TextMeasurer::new();

    const MARGIN: f64 = 40.0;
    const LEGEND_RECT_SIZE: f64 = 18.0;
    const LEGEND_SPACING: f64 = 4.0;
    const HEIGHT: f64 = 450.0;
    let pie_width = HEIGHT;
    let text_position = 0.75;
    let radius = pie_width.min(HEIGHT) / 2.0 - MARGIN;
    let outer_stroke_width = 2.0;

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_pie_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    let group = append(&svg, "g");
    set_attr(
        &group,
        "transform",
        format!(
            "translate({},{})",
            js_num(pie_width / 2.0),
            js_num(HEIGHT / 2.0)
        ),
    );

    let circle = append(&group, "circle");
    set_attr(&circle, "cx", "0");
    set_attr(&circle, "cy", "0");
    set_attr(&circle, "r", js_num(radius + outer_stroke_width / 2.0));
    set_attr(&circle, "class", "pieOuterCircle");

    let total: f64 = db.sections.iter().map(|(_, v)| v).sum();

    // createPieArcs: filter < 1%, then d3.pie().sort(null).
    let pie_data: Vec<(String, f64)> = db
        .sections
        .iter()
        .filter(|(_, v)| (v / total) * 100.0 >= 1.0)
        .cloned()
        .collect();
    let pie_sum: f64 = pie_data.iter().map(|(_, v)| v).sum();
    struct ArcDatum {
        label: String,
        value: f64,
        start: f64,
        end: f64,
    }
    let k = TAU / pie_sum;
    let mut arcs: Vec<ArcDatum> = Vec::new();
    let mut a = 0.0f64;
    for (label, value) in &pie_data {
        let start = a;
        a += value * k;
        arcs.push(ArcDatum {
            label: label.clone(),
            value: *value,
            start,
            end: a,
        });
    }

    // Colors: theme pie1..pie12 cycling over section labels.
    let colors: Vec<String> = (1..=12)
        .map(|n| themes::get(&theme_vars, &format!("pie{n}")))
        .collect();
    let color_of = |label: &str| -> String {
        let idx = db
            .sections
            .iter()
            .position(|(l, _)| l == label)
            .unwrap_or(0);
        colors[idx % colors.len()].clone()
    };

    // Filter arcs that round to 0%.
    let filtered: Vec<&ArcDatum> = arcs
        .iter()
        .filter(|d| to_fixed0(d.value / total * 100.0) != "0")
        .collect();

    for d in &filtered {
        let path = append(&group, "path");
        set_attr(&path, "d", arc_path(radius, d.start, d.end));
        set_attr(&path, "fill", color_of(&d.label));
        set_attr(&path, "class", "pieCircle");
    }
    for d in &filtered {
        let text = append(&group, "text");
        set_text(&text, &format!("{}%", to_fixed0(d.value / total * 100.0)));
        let (cx, cy) = centroid(radius * text_position, d.start, d.end);
        set_attr(
            &text,
            "transform",
            format!("translate({},{})", js_num(cx), js_num(cy)),
        );
        set_attr(&text, "class", "slice");
        set_attr(&text, "style", "text-anchor: middle;");
    }

    let title_text = append(&group, "text");
    if !db.title.is_empty() {
        set_text(&title_text, &db.title);
    }
    set_attr(&title_text, "x", "0");
    set_attr(&title_text, "y", js_num(-(HEIGHT - 50.0) / 2.0));
    set_attr(&title_text, "class", "pieTitleText");

    // Legend.
    let n = db.sections.len();
    #[allow(clippy::cast_precision_loss)]
    let offset = ((LEGEND_RECT_SIZE + LEGEND_SPACING) * n as f64) / 2.0;
    let mut longest_text_width = f64::NEG_INFINITY;
    for (index, (label, value)) in db.sections.iter().enumerate() {
        let g = append(&group, "g");
        set_attr(&g, "class", "legend");
        #[allow(clippy::cast_precision_loss)]
        let vertical = index as f64 * (LEGEND_RECT_SIZE + LEGEND_SPACING) - offset;
        set_attr(
            &g,
            "transform",
            format!(
                "translate({},{})",
                js_num(12.0 * LEGEND_RECT_SIZE),
                js_num(vertical)
            ),
        );
        let rect = append(&g, "rect");
        set_attr(&rect, "width", js_num(LEGEND_RECT_SIZE));
        set_attr(&rect, "height", js_num(LEGEND_RECT_SIZE));
        let color = color_of(label);
        let rgb = crate::render::css::cssom_color_value("fill", &color);
        set_attr(&rect, "style", format!("fill: {rgb}; stroke: {rgb};"));
        let text = append(&g, "text");
        set_attr(&text, "x", js_num(LEGEND_RECT_SIZE + LEGEND_SPACING));
        set_attr(&text, "y", js_num(LEGEND_RECT_SIZE - LEGEND_SPACING));
        let label_text = if db.show_data {
            format!("{label} [{}]", js_num(*value))
        } else {
            label.clone()
        };
        set_text(&text, &label_text);
        longest_text_width = longest_text_width.max(measurer.measure_width(&label_text, 17.0));
    }

    let chart_and_legend_width =
        pie_width + MARGIN + LEGEND_RECT_SIZE + LEGEND_SPACING + longest_text_width;

    let title_width = if db.title.is_empty() {
        0.0
    } else {
        measurer.measure_width(&db.title, 25.0)
    };
    let title_left = pie_width / 2.0 - title_width / 2.0;
    let title_right = pie_width / 2.0 + title_width / 2.0;
    let view_box_x = 0.0f64.min(title_left);
    let view_box_right = chart_and_legend_width.max(title_right);
    let total_width = view_box_right - view_box_x;

    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} 0 {} {}",
            js_num(view_box_x),
            js_num(total_width),
            js_num(HEIGHT)
        ),
    );
    // configureSvgSize (useMaxWidth true).
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(total_width)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "pie");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
