//! **sebastian extension** — a `pyramid` diagram type with no mermaid
//! equivalent, so it is an original renderer (not byte-exact against `mmdc`).
//!
//! Two shapes from one syntax:
//!
//! ```text
//! pyramid
//!   title Company
//!   CEO
//!   Directors
//!   Managers
//!   Staff
//! ```
//!
//! renders a **pyramid chart**: stacked trapezoid bands forming a triangle
//! (narrow apex on top, wide base at the bottom), one labelled band per level.
//!
//! Adding a `: a, b, c` component list to a level turns that band into a
//! **pyramid of components** — the named component boxes are laid out in a row
//! inside the band:
//!
//! ```text
//! pyramid
//!   title Architecture
//!   Presentation: Web, Mobile
//!   Business: Auth, Orders, Billing
//!   Data: Postgres, Redis
//! ```
//!
//! The two forms mix freely in one diagram. Deterministic layout, theme colours
//! from `cScale{n}`; validated by structural smoke tests.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::assigning_clones,
    clippy::manual_midpoint
)]

use std::fmt::Write as _;

use crate::svg::{append, js_num, new_element, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

const BAND_H: f64 = 78.0;
const FONT_SIZE: f64 = 17.0;
const MIN_FONT_SIZE: f64 = 9.0;
const COMP_FONT_SIZE: f64 = 13.0;
const COMP_H: f64 = 30.0;
const COMP_PAD_X: f64 = 12.0;
const COMP_GAP: f64 = 10.0;
const MIN_BASE: f64 = 220.0;
/// Base-width : total-height ratio for a plain (label-only) pyramid — keeps a
/// proper triangle instead of a wide, flat sliver. Component bands may widen the
/// base beyond this to fit their boxes.
const ASPECT: f64 = 1.35;
const PAD: f64 = 24.0;
const TITLE_H: f64 = 34.0;

/// A vivid, visually distinct palette (readable with black text), cycled per
/// level. Higher contrast than the washed theme pastels.
const PALETTE: [&str; 8] = [
    "#7EA8F0", // blue
    "#7BD389", // green
    "#F5CB5C", // gold
    "#F08C8C", // coral
    "#B991E6", // purple
    "#6FD0CC", // teal
    "#F2A75C", // orange
    "#A8D26A", // lime
];

/// Parse error for pyramid diagrams.
#[derive(Debug)]
pub struct PyramidParseError(pub String);

impl std::fmt::Display for PyramidParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pyramid diagram parse error: {}", self.0)
    }
}

impl std::error::Error for PyramidParseError {}

struct Level {
    label: String,
    components: Vec<String>,
}

#[derive(Default)]
struct PyramidDb {
    title: String,
    levels: Vec<Level>,
}

fn parse(source: &str) -> Result<PyramidDb, PyramidParseError> {
    let mut db = PyramidDb::default();
    let mut found = false;
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with('#') {
            continue;
        }
        if !found {
            if line == "pyramid" || line.starts_with("pyramid ") {
                found = true;
                continue;
            }
            return Err(PyramidParseError(format!(
                "expected pyramid header, got {line:?}"
            )));
        }
        if let Some(rest) = line.strip_prefix("title ") {
            db.title = rest.trim().to_owned();
            continue;
        }
        // `Label: a, b, c` (components) or bare `Label`.
        let (label, components) = match line.split_once(':') {
            Some((l, rest)) => (
                l.trim().to_owned(),
                rest.split(',')
                    .map(|c| c.trim().to_owned())
                    .filter(|c| !c.is_empty())
                    .collect(),
            ),
            None => (line.to_owned(), Vec::new()),
        };
        db.levels.push(Level { label, components });
    }
    if !found {
        return Err(PyramidParseError("missing pyramid header".to_owned()));
    }
    Ok(db)
}

/// Greedy word-wrap: splits `text` on whitespace into lines that each fit
/// `avail` px at font size `fs`. A single word wider than `avail` is kept on its
/// own line (the caller shrinks the font to handle that case).
fn wrap_label(text: &str, avail: f64, fs: f64, m: &TextMeasurer) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let trial = if cur.is_empty() {
            word.to_owned()
        } else {
            format!("{cur} {word}")
        };
        if cur.is_empty() || m.measure_width(&trial, fs) <= avail {
            cur = trial;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_owned();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// The content width a level needs at its vertical midline.
fn level_content_width(level: &Level, measurer: &TextMeasurer) -> f64 {
    if level.components.is_empty() {
        measurer.measure_width(&level.label, FONT_SIZE)
    } else {
        let boxes: f64 = level
            .components
            .iter()
            .map(|c| measurer.measure_width(c, COMP_FONT_SIZE) + COMP_PAD_X * 2.0)
            .sum();
        boxes + COMP_GAP * (level.components.len() as f64 - 1.0)
    }
}

/// Renders a `pyramid` diagram to an SVG.
///
/// # Errors
/// Returns [`PyramidParseError`] when the source is not a valid pyramid diagram.
pub fn render_pyramid(source: &str, id: &str) -> Result<String, PyramidParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let tv = |k: &str| crate::render::themes::get(&theme_vars, k);
    let measurer = TextMeasurer::new();
    let db = parse(source)?;
    if db.levels.is_empty() {
        return Ok(empty_svg(id));
    }

    let n = db.levels.len();
    // Base width from a fixed aspect ratio → a proper triangle. Component bands
    // can't shrink their boxes, so those (only) may widen the base to fit; plain
    // labels instead shrink their font per band (below), so a long apex label no
    // longer blows the whole pyramid out to a wide, flat sliver.
    let height = n as f64 * BAND_H;
    let mut base = (height * ASPECT).max(MIN_BASE);
    for (i, level) in db.levels.iter().enumerate() {
        if !level.components.is_empty() {
            let mid_frac = (i as f64 + 0.5) / n as f64;
            base = base.max((level_content_width(level, &measurer) + 24.0) / mid_frac);
        }
    }

    let title_offset = if db.title.is_empty() { 0.0 } else { TITLE_H };
    let cx = PAD + base / 2.0;
    let total_w = base + PAD * 2.0;
    let total_h = title_offset + n as f64 * BAND_H + PAD * 2.0;

    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style_el = append(&svg, "style");
    set_text(&style_el, &pyramid_css(id, &tv));

    if !db.title.is_empty() {
        let t = append(&svg, "text");
        set_attr(&t, "x", js_num(cx));
        set_attr(&t, "y", js_num(PAD + FONT_SIZE));
        set_attr(&t, "text-anchor", "middle");
        set_attr(&t, "class", "pyramid-title");
        set_text(&t, &db.title);
    }

    let bands = append(&svg, "g");
    set_attr(&bands, "class", "pyramid-bands");
    for (i, level) in db.levels.iter().enumerate() {
        let sect = i % PALETTE.len();
        let top_y = PAD + title_offset + i as f64 * BAND_H;
        let bot_y = top_y + BAND_H;
        let top_w = base * i as f64 / n as f64;
        let bot_w = base * (i as f64 + 1.0) / n as f64;
        let g = append(&bands, "g");
        set_attr(&g, "class", format!("pyramid-level section-{sect}"));

        let poly = append(&g, "polygon");
        set_attr(&poly, "class", "pyramid-band");
        set_attr(
            &poly,
            "points",
            format!(
                "{},{} {},{} {},{} {},{}",
                js_num(cx - top_w / 2.0),
                js_num(top_y),
                js_num(cx + top_w / 2.0),
                js_num(top_y),
                js_num(cx + bot_w / 2.0),
                js_num(bot_y),
                js_num(cx - bot_w / 2.0),
                js_num(bot_y),
            ),
        );

        let mid_y = (top_y + bot_y) / 2.0;
        if level.components.is_empty() {
            // Fit the label into this band's midline width by wrapping onto
            // multiple lines; only shrink the font when even the wrapped lines
            // (by width, or by the number that fit the band height) won't fit.
            let avail = (top_w + bot_w) / 2.0 - 16.0;
            let mut fs = FONT_SIZE;
            let mut lines = wrap_label(&level.label, avail, fs, &measurer);
            loop {
                let lh = fs * 1.25;
                let widest = lines
                    .iter()
                    .map(|l| measurer.measure_width(l, fs))
                    .fold(0.0, f64::max);
                let fits = widest <= avail && lines.len() as f64 * lh <= BAND_H - 8.0;
                if fits || fs <= MIN_FONT_SIZE {
                    break;
                }
                fs -= 0.5;
                lines = wrap_label(&level.label, avail, fs, &measurer);
            }
            let lh = fs * 1.25;
            let label = append(&g, "text");
            set_attr(&label, "x", js_num(cx));
            set_attr(&label, "text-anchor", "middle");
            set_attr(&label, "class", "pyramid-label");
            set_attr(&label, "style", format!("font-size:{}px;", js_num(fs)));
            let first_y = mid_y - (lines.len() as f64 - 1.0) * lh / 2.0;
            for (k, line) in lines.iter().enumerate() {
                let ts = append(&label, "tspan");
                set_attr(&ts, "x", js_num(cx));
                set_attr(&ts, "y", js_num(first_y + k as f64 * lh));
                set_attr(&ts, "dominant-baseline", "central");
                set_text(&ts, line);
            }
        } else {
            draw_components(&g, level, cx, mid_y, &measurer);
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
    set_attr(&svg, "aria-roledescription", "pyramid");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// Draws a level's component boxes in a centered row on the midline.
fn draw_components(g: &crate::svg::Element, level: &Level, cx: f64, mid_y: f64, m: &TextMeasurer) {
    let widths: Vec<f64> = level
        .components
        .iter()
        .map(|c| m.measure_width(c, COMP_FONT_SIZE) + COMP_PAD_X * 2.0)
        .collect();
    let total: f64 = widths.iter().sum::<f64>() + COMP_GAP * (widths.len() as f64 - 1.0);
    let mut x = cx - total / 2.0;
    let top = mid_y - COMP_H / 2.0;
    for (comp, w) in level.components.iter().zip(&widths) {
        let cg = append(g, "g");
        set_attr(&cg, "class", "pyramid-component");
        let rect = append(&cg, "rect");
        set_attr(&rect, "x", js_num(x));
        set_attr(&rect, "y", js_num(top));
        set_attr(&rect, "width", js_num(*w));
        set_attr(&rect, "height", js_num(COMP_H));
        set_attr(&rect, "rx", "4");
        set_attr(&rect, "ry", "4");
        set_attr(&rect, "class", "pyramid-component-rect");
        let text = append(&cg, "text");
        set_attr(&text, "x", js_num(x + w / 2.0));
        set_attr(&text, "y", js_num(mid_y));
        set_attr(&text, "text-anchor", "middle");
        set_attr(&text, "dominant-baseline", "central");
        set_attr(&text, "class", "pyramid-component-label");
        set_text(&text, comp);
        x += w + COMP_GAP;
    }
}

fn pyramid_css(id: &str, tv: &dyn Fn(&str) -> String) -> String {
    let font = tv("fontFamily");
    let text_color = tv("textColor");
    let line = tv("lineColor");
    let title_stroke = crate::render::handdrawn::embolden_decls(&text_color);
    let label_stroke = crate::render::handdrawn::embolden_decls("#1a1a1a");
    let mut o = String::new();
    let _ = write!(
        o,
        "#{id}{{font-family:{font};}}\
         #{id} .pyramid-title{{font-size:20px;font-weight:bold;fill:{text_color};{title_stroke}}}\
         #{id} .pyramid-band{{stroke:{line};stroke-width:1px;}}\
         #{id} .pyramid-label{{font-weight:bold;fill:#1a1a1a;{label_stroke}}}\
         #{id} .pyramid-component-rect{{fill:rgba(255,255,255,0.92);stroke:{line};stroke-width:1px;}}\
         #{id} .pyramid-component-label{{font-size:{COMP_FONT_SIZE}px;fill:#1a1a1a;}}"
    );
    for (i, color) in PALETTE.iter().enumerate() {
        let _ = write!(o, "#{id} .section-{i} .pyramid-band{{fill:{color};}}");
    }
    o
}

fn empty_svg(id: &str) -> String {
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "aria-roledescription", "pyramid");
    let mut out = String::new();
    serialize(&svg, &mut out);
    out
}
