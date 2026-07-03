//! radar-beta support: a port of the langium radar grammar
//! (`packages/parser/src/language/radar`), `db.ts`, and the self-contained
//! polar `renderer.ts`. No text measurement — every coordinate comes from
//! `Math.cos`/`Math.sin` (via `core_math`, which matches V8) and constants.

#![allow(clippy::assigning_clones)]
use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};
use std::f64::consts::PI;
use std::fmt::Write as _;

/// A parse error for radar source.
#[derive(Debug)]
pub struct RadarParseError {
    pub message: String,
}

impl std::fmt::Display for RadarParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "radar parse error: {}", self.message)
    }
}

impl std::error::Error for RadarParseError {}

// Fixed defaults from `DEFAULT_CONFIG.radar` (config.schema.yaml).
const WIDTH: f64 = 600.0;
const HEIGHT: f64 = 600.0;
const MARGIN_TOP: f64 = 50.0;
const MARGIN_RIGHT: f64 = 50.0;
const MARGIN_BOTTOM: f64 = 50.0;
const MARGIN_LEFT: f64 = 50.0;
const AXIS_SCALE_FACTOR: f64 = 1.0;
const AXIS_LABEL_FACTOR: f64 = 1.05;
const CURVE_TENSION: f64 = 0.17;

struct Axis {
    name: String,
    label: String,
}

struct Curve {
    label: String,
    entries: Vec<f64>,
}

struct Options {
    show_legend: bool,
    ticks: f64,
    max: Option<f64>,
    min: f64,
    graticule: String,
}

struct Db {
    title: String,
    axes: Vec<Axis>,
    curves: Vec<Curve>,
    options: Options,
}

/// Parse `name` optionally followed by `["label"]`; returns (name, label).
fn parse_named(token: &str) -> (String, String) {
    let token = token.trim();
    if let Some(br) = token.find('[') {
        let name = token[..br].trim().to_owned();
        let inner = &token[br + 1..];
        let label = inner
            .rfind(']')
            .map_or(inner, |e| &inner[..e])
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_owned();
        let label = if label.is_empty() {
            name.clone()
        } else {
            label
        };
        (name, label)
    } else {
        (token.to_owned(), token.to_owned())
    }
}

/// Split on commas that are not inside `[...]` or `{...}`.
fn split_top(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '[' | '{' => {
                depth += 1;
                cur.push(c);
            }
            ']' | '}' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn parse_entries(inner: &str, axes: &[Axis]) -> Result<Vec<f64>, RadarParseError> {
    let raw: Vec<String> = split_top(inner)
        .into_iter()
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .collect();
    let bad = |t: &str| RadarParseError {
        message: format!("bad radar entry: {t}"),
    };
    // Detailed entries reference an axis (`name: value` or `name value`).
    let is_detailed = raw.first().is_some_and(|t| t.parse::<f64>().is_err());
    if !is_detailed {
        return raw
            .iter()
            .map(|t| t.parse::<f64>().map_err(|_| bad(t)))
            .collect();
    }
    let mut map: Vec<(String, f64)> = Vec::new();
    for t in &raw {
        let (name, val) = match t.split_once(':') {
            Some(pair) => pair,
            None => t.split_once(char::is_whitespace).unwrap_or((t, "")),
        };
        let val: f64 = val.trim().parse().map_err(|_| bad(t))?;
        map.push((name.trim().to_owned(), val));
    }
    axes.iter()
        .map(|axis| {
            map.iter()
                .find(|(n, _)| *n == axis.name)
                .map(|(_, v)| *v)
                .ok_or_else(|| RadarParseError {
                    message: format!("missing radar entry for axis {}", axis.label),
                })
        })
        .collect()
}

fn parse_option(db: &mut Db, tok: &str) -> Result<(), RadarParseError> {
    let tok = tok.trim();
    let (name, val) = tok
        .split_once(char::is_whitespace)
        .map(|(a, b)| (a, b.trim()))
        .ok_or_else(|| RadarParseError {
            message: format!("bad radar option: {tok}"),
        })?;
    let numeric = |v: &str| {
        v.parse::<f64>().map_err(|_| RadarParseError {
            message: format!("bad radar option value: {tok}"),
        })
    };
    match name {
        "showLegend" => db.options.show_legend = val == "true",
        "ticks" => db.options.ticks = numeric(val)?,
        "max" => db.options.max = Some(numeric(val)?),
        "min" => db.options.min = numeric(val)?,
        "graticule" => db.options.graticule = val.to_owned(),
        _ => {
            return Err(RadarParseError {
                message: format!("unknown radar option: {name}"),
            });
        }
    }
    Ok(())
}

fn parse(source: &str) -> Result<Db, RadarParseError> {
    let mut db = Db {
        title: String::new(),
        axes: Vec::new(),
        curves: Vec::new(),
        options: Options {
            show_legend: true,
            ticks: 5.0,
            max: None,
            min: 0.0,
            graticule: "circle".to_owned(),
        },
    };
    let mut found_header = false;
    // Concatenate so that brace-delimited curve bodies may span lines.
    let mut pending = String::new();
    let mut depth = 0i32;
    let process = |db: &mut Db, stmt: &str| -> Result<(), RadarParseError> {
        let stmt = stmt.trim();
        if stmt.is_empty() {
            return Ok(());
        }
        if let Some(rest) = stmt.strip_prefix("axis ") {
            for t in split_top(rest) {
                let (name, label) = parse_named(&t);
                db.axes.push(Axis { name, label });
            }
        } else if let Some(rest) = stmt.strip_prefix("curve ") {
            for t in split_top(rest) {
                let t = t.trim();
                let br = t.find('{').ok_or_else(|| RadarParseError {
                    message: format!("radar curve missing entries: {t}"),
                })?;
                let (_, label) = parse_named(&t[..br]);
                let inner = t[br + 1..].trim_end().trim_end_matches('}');
                let entries = parse_entries(inner, &db.axes)?;
                db.curves.push(Curve { label, entries });
            }
        } else {
            for t in split_top(stmt) {
                parse_option(db, &t)?;
            }
        }
        Ok(())
    };

    for raw in source.lines() {
        let line = raw.trim();
        if !found_header {
            if line.is_empty() || line.starts_with("%%") {
                continue;
            }
            let head = line.trim_end_matches(':').trim();
            if head == "radar-beta" {
                found_header = true;
                continue;
            }
            return Err(RadarParseError {
                message: format!("expected radar-beta header, got {line:?}"),
            });
        }
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if depth == 0
            && let Some(rest) = line.strip_prefix("title")
            && (rest.is_empty() || rest.starts_with([' ', '\t']))
        {
            db.title = rest.trim().to_owned();
            continue;
        }
        for c in line.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        if !pending.is_empty() {
            pending.push(' ');
        }
        pending.push_str(line);
        if depth == 0 {
            let stmt = std::mem::take(&mut pending);
            process(&mut db, &stmt)?;
        }
    }
    if !pending.trim().is_empty() {
        process(&mut db, &pending.clone())?;
    }
    if !found_header {
        return Err(RadarParseError {
            message: "missing radar-beta header".to_owned(),
        });
    }
    Ok(db)
}

fn relative_radius(value: f64, min_value: f64, max_value: f64, radius: f64) -> f64 {
    let clipped = value.max(min_value).min(max_value);
    radius * (clipped - min_value) / (max_value - min_value)
}

/// Port of `closedRoundCurve` (Catmull-Rom → cubic Bézier).
fn closed_round_curve(points: &[(f64, f64)], tension: f64) -> String {
    let n = points.len();
    let mut d = format!("M{},{}", js_num(points[0].0), js_num(points[0].1));
    for i in 0..n {
        let p0 = points[(i + n - 1) % n];
        let p1 = points[i];
        let p2 = points[(i + 1) % n];
        let p3 = points[(i + 2) % n];
        let cp1 = (
            p1.0 + (p2.0 - p0.0) * tension,
            p1.1 + (p2.1 - p0.1) * tension,
        );
        let cp2 = (
            p2.0 - (p3.0 - p1.0) * tension,
            p2.1 - (p3.1 - p1.1) * tension,
        );
        let _ = write!(
            d,
            " C{},{} {},{} {},{}",
            js_num(cp1.0),
            js_num(cp1.1),
            js_num(cp2.0),
            js_num(cp2.1),
            js_num(p2.0),
            js_num(p2.1)
        );
    }
    d.push_str(" Z");
    d
}

/// Renders mermaid radar-beta source to a complete SVG document string.
///
/// # Errors
/// Returns a [`RadarParseError`] when the source is not a valid radar diagram.
pub fn render_radar(source: &str, id: &str) -> Result<String, RadarParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;

    let total_width = WIDTH + MARGIN_LEFT + MARGIN_RIGHT;
    let total_height = HEIGHT + MARGIN_TOP + MARGIN_BOTTOM;
    let center_x = MARGIN_LEFT + WIDTH / 2.0;
    let center_y = MARGIN_TOP + HEIGHT / 2.0;

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
            crate::render::css_length(total_width)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!("0 0 {} {}", js_num(total_width), js_num(total_height)),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "radar");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_radar_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    let g = append(&svg, "g");
    set_attr(
        &g,
        "transform",
        format!("translate({}, {})", js_num(center_x), js_num(center_y)),
    );

    let num_axes = db.axes.len();
    let radius = WIDTH.min(HEIGHT) / 2.0;
    let max_value = db.options.max.unwrap_or_else(|| {
        db.curves
            .iter()
            .flat_map(|c| c.entries.iter().copied())
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let min_value = db.options.min;

    // Graticule.
    if db.options.graticule == "circle" {
        let mut i = 0.0;
        while i < db.options.ticks {
            let r = radius * (i + 1.0) / db.options.ticks;
            let c = append(&g, "circle");
            set_attr(&c, "r", js_num(r));
            set_attr(&c, "class", "radarGraticule");
            i += 1.0;
        }
    } else if db.options.graticule == "polygon" {
        let mut i = 0.0;
        while i < db.options.ticks {
            let r = radius * (i + 1.0) / db.options.ticks;
            let points: Vec<String> = (0..num_axes)
                .map(|j| {
                    let angle = (2.0 * j as f64 * PI) / num_axes as f64 - PI / 2.0;
                    format!(
                        "{},{}",
                        js_num(r * core_math::cos(angle)),
                        js_num(r * core_math::sin(angle))
                    )
                })
                .collect();
            let poly = append(&g, "polygon");
            set_attr(&poly, "points", points.join(" "));
            set_attr(&poly, "class", "radarGraticule");
            i += 1.0;
        }
    }

    // Axes.
    for (i, axis) in db.axes.iter().enumerate() {
        let angle = (2.0 * i as f64 * PI) / num_axes as f64 - PI / 2.0;
        let line = append(&g, "line");
        set_attr(&line, "x1", "0");
        set_attr(&line, "y1", "0");
        set_attr(
            &line,
            "x2",
            js_num(radius * AXIS_SCALE_FACTOR * core_math::cos(angle)),
        );
        set_attr(
            &line,
            "y2",
            js_num(radius * AXIS_SCALE_FACTOR * core_math::sin(angle)),
        );
        set_attr(&line, "class", "radarAxisLine");
        let text = append(&g, "text");
        set_attr(
            &text,
            "x",
            js_num(radius * AXIS_LABEL_FACTOR * core_math::cos(angle)),
        );
        set_attr(
            &text,
            "y",
            js_num(radius * AXIS_LABEL_FACTOR * core_math::sin(angle)),
        );
        set_attr(&text, "class", "radarAxisLabel");
        set_text(&text, &axis.label);
    }

    // Curves.
    for (index, curve) in db.curves.iter().enumerate() {
        if curve.entries.len() != num_axes {
            continue;
        }
        let points: Vec<(f64, f64)> = curve
            .entries
            .iter()
            .enumerate()
            .map(|(i, &entry)| {
                let angle = (2.0 * PI * i as f64) / num_axes as f64 - PI / 2.0;
                let r = relative_radius(entry, min_value, max_value, radius);
                (r * core_math::cos(angle), r * core_math::sin(angle))
            })
            .collect();
        if db.options.graticule == "circle" {
            let path = append(&g, "path");
            set_attr(&path, "d", closed_round_curve(&points, CURVE_TENSION));
            set_attr(&path, "class", format!("radarCurve-{index}"));
        } else if db.options.graticule == "polygon" {
            let pts: Vec<String> = points
                .iter()
                .map(|(x, y)| format!("{},{}", js_num(*x), js_num(*y)))
                .collect();
            let poly = append(&g, "polygon");
            set_attr(&poly, "points", pts.join(" "));
            set_attr(&poly, "class", format!("radarCurve-{index}"));
        }
    }

    // Legend.
    if db.options.show_legend {
        let legend_x = (WIDTH / 2.0 + MARGIN_RIGHT) * 3.0 / 4.0;
        let legend_y = -(HEIGHT / 2.0 + MARGIN_TOP) * 3.0 / 4.0;
        let line_height = 20.0;
        for (index, curve) in db.curves.iter().enumerate() {
            let item: Element = append(&g, "g");
            set_attr(
                &item,
                "transform",
                format!(
                    "translate({}, {})",
                    js_num(legend_x),
                    js_num(legend_y + index as f64 * line_height)
                ),
            );
            let rect = append(&item, "rect");
            set_attr(&rect, "width", "12");
            set_attr(&rect, "height", "12");
            set_attr(&rect, "class", format!("radarLegendBox-{index}"));
            let text = append(&item, "text");
            set_attr(&text, "x", "16");
            set_attr(&text, "y", "0");
            set_attr(&text, "class", "radarLegendText");
            set_text(&text, &curve.label);
        }
    }

    // Title (class set first, then x/y; empty title stays self-closing).
    let title = append(&g, "text");
    set_attr(&title, "class", "radarTitle");
    if !db.title.is_empty() {
        set_text(&title, &db.title);
    }
    set_attr(&title, "x", "0");
    set_attr(&title, "y", js_num(-HEIGHT / 2.0 - MARGIN_TOP));

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
