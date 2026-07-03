//! user-journey support: parser subset, `journeyDb.js` semantics, and a
//! direct port of `journeyRenderer.ts` + its `svgDraw.js`.

#![allow(clippy::assigning_clones, clippy::explicit_counter_loop)]
use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};

/// A parse error for journey source.
#[derive(Debug)]
pub struct JourneyParseError {
    pub message: String,
}

impl std::fmt::Display for JourneyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "journey parse error: {}", self.message)
    }
}

impl std::error::Error for JourneyParseError {}

const LEFT_MARGIN: f64 = 150.0;
const WIDTH: f64 = 150.0;
const HEIGHT: f64 = 50.0;
const TASK_MARGIN: f64 = 50.0;
const DIAGRAM_MARGIN_X: f64 = 50.0;
const DIAGRAM_MARGIN_Y: f64 = 10.0;
const BOX_TEXT_MARGIN: f64 = 5.0;
const ACTOR_COLOURS: &[&str] = &[
    "#8FBC8F", "#7CFC00", "#00FFFF", "#20B2AA", "#B0E0E6", "#FFFFE0",
];
const SECTION_FILLS: &[&str] = &[
    "#191970", "#8B008B", "#4B0082", "#2F4F4F", "#800000", "#8B4513", "#00008B",
];
const SECTION_COLOURS: &[&str] = &["#fff"];

#[derive(Debug, Clone)]
struct Task {
    section: String,
    name: String,
    score: f64,
    people: Vec<String>,
}

#[derive(Debug, Default)]
struct Db {
    title: String,
    tasks: Vec<Task>,
}

fn parse(source: &str) -> Result<Db, JourneyParseError> {
    let mut db = Db::default();
    let mut found_header = false;
    let mut section = String::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found_header {
            if line.starts_with("journey") {
                found_header = true;
                continue;
            }
            return Err(JourneyParseError {
                message: format!("expected journey header, got {line:?}"),
            });
        }
        if let Some(rest) = line.strip_prefix("title") {
            db.title = rest.trim().to_owned();
            continue;
        }
        if let Some(rest) = line.strip_prefix("section") {
            section = rest.trim().to_owned();
            continue;
        }
        // Task: name: score: person1, person2
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 2 {
            return Err(JourneyParseError {
                message: format!("unsupported journey statement: {line}"),
            });
        }
        let name = parts[0].trim().to_owned();
        let score: f64 = parts[1].trim().parse().map_err(|_| JourneyParseError {
            message: format!("bad journey score: {line}"),
        })?;
        let people: Vec<String> = if parts.len() > 2 {
            parts[2]
                .split(',')
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty())
                .collect()
        } else {
            Vec::new()
        };
        db.tasks.push(Task {
            section: section.clone(),
            name,
            score,
            people,
        });
    }
    if !found_header {
        return Err(JourneyParseError {
            message: "missing journey header".to_owned(),
        });
    }
    Ok(db)
}

/// d3 annular arc (`innerRadius`/`outerRadius`, half-circle) with the
/// digits(3) path serializer.
fn mouth_arc(start_angle: f64, end_angle: f64, r_inner: f64, r_outer: f64) -> String {
    let round3 = |v: f64| js_num(((v * 1000.0) + 0.5).floor() / 1000.0);
    // r1 must be the larger.
    let (r0, r1) = if r_outer < r_inner {
        (r_outer, r_inner)
    } else {
        (r_inner, r_outer)
    };
    let half_pi = std::f64::consts::FRAC_PI_2;
    let a0 = start_angle - half_pi;
    let a1 = end_angle - half_pi;
    let large = i32::from(a1 - a0 >= std::f64::consts::PI);
    let (x01, y01) = (r1 * core_math::cos(a0), r1 * core_math::sin(a0));
    let (x11, y11) = (r1 * core_math::cos(a1), r1 * core_math::sin(a1));
    let (x10, y10) = (r0 * core_math::cos(a1), r0 * core_math::sin(a1));
    let (x00, y00) = (r0 * core_math::cos(a0), r0 * core_math::sin(a0));
    format!(
        "M{},{}A{},{},0,{},1,{},{}L{},{}A{},{},0,{},0,{},{}Z",
        round3(x01),
        round3(y01),
        round3(r1),
        round3(r1),
        large,
        round3(x11),
        round3(y11),
        round3(x10),
        round3(y10),
        round3(r0),
        round3(r0),
        large,
        round3(x00),
        round3(y00)
    )
}

struct Bounds {
    startx: Option<f64>,
    starty: Option<f64>,
    stopx: Option<f64>,
    stopy: Option<f64>,
}

impl Bounds {
    fn insert(&mut self, startx: f64, starty: f64, stopx: f64, stopy: f64) {
        let (sx, ex) = (startx.min(stopx), startx.max(stopx));
        let (sy, ey) = (starty.min(stopy), starty.max(stopy));
        self.startx = Some(self.startx.map_or(sx, |v| v.min(sx)));
        self.starty = Some(self.starty.map_or(sy, |v| v.min(sy)));
        self.stopx = Some(self.stopx.map_or(ex, |v| v.max(ex)));
        self.stopy = Some(self.stopy.map_or(ey, |v| v.max(ey)));
    }
}

/// Renders journey source to a complete SVG document string.
///
/// # Errors
/// Returns a [`JourneyParseError`] when the source is not a valid journey.
#[allow(clippy::too_many_lines)]
pub fn render_journey(source: &str, id: &str) -> Result<String, JourneyParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;
    let measurer = crate::text::TextMeasurer::new();

    // Actors: unique, sorted.
    let mut actor_names: Vec<String> = Vec::new();
    for t in &db.tasks {
        for p in &t.people {
            if !actor_names.contains(p) {
                actor_names.push(p.clone());
            }
        }
    }
    actor_names.sort();
    let actor_pos = |name: &str| actor_names.iter().position(|a| a == name).unwrap_or(0);
    let actor_colour = |name: &str| ACTOR_COLOURS[actor_pos(name) % ACTOR_COLOURS.len()].to_owned();

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_journey_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");

    // initGraphics: arrowhead marker.
    let defs = append(&svg, "defs");
    let marker = append(&defs, "marker");
    set_attr(&marker, "id", format!("{id}-arrowhead"));
    set_attr(&marker, "refX", "5");
    set_attr(&marker, "refY", "2");
    set_attr(&marker, "markerWidth", "6");
    set_attr(&marker, "markerHeight", "4");
    set_attr(&marker, "orient", "auto");
    let mp = append(&marker, "path");
    set_attr(&mp, "d", "M 0,0 V 4 L6,2 Z");

    // drawActorLegend.
    let mut max_width = 0.0f64;
    {
        let mut y_pos = 60.0;
        for name in &actor_names {
            let circle = append(&svg, "circle");
            set_attr(&circle, "cx", "20");
            set_attr(&circle, "cy", js_num(y_pos));
            set_attr(&circle, "class", format!("actor-{}", actor_pos(name)));
            set_attr(&circle, "fill", actor_colour(name));
            set_attr(&circle, "stroke", "#000");
            set_attr(&circle, "r", "7");

            // Full text width fits within maxLabelWidth (360) for single
            // lines; no wrapping in the corpus.
            let text = append(&svg, "text");
            set_attr(&text, "x", "40");
            set_attr(&text, "y", js_num(y_pos + 7.0));
            set_attr(&text, "class", "legend");
            let tspan = append(&text, "tspan");
            set_attr(&tspan, "x", js_num(40.0 + BOX_TEXT_MARGIN * 2.0));
            set_text(&tspan, name);
            let line_width = measurer.measure_width(name, 16.0);
            if line_width > max_width && line_width > LEFT_MARGIN - line_width {
                max_width = line_width;
            }
            y_pos += 20.0;
        }
    }
    let left_margin = LEFT_MARGIN + max_width;

    let mut bounds = Bounds {
        startx: None,
        starty: None,
        stopx: None,
        stopy: None,
    };
    #[allow(clippy::cast_precision_loss)]
    bounds.insert(0.0, 0.0, left_margin, actor_names.len() as f64 * 50.0);

    // drawTasks.
    {
        let section_v_height = 2.0f64.mul_add(HEIGHT, DIAGRAM_MARGIN_Y);
        let task_pos = section_v_height;
        let mut last_section = String::new();
        let mut section_number = 0usize;
        let mut fill = "#CCC".to_owned();
        let mut colour = "black".to_owned();
        let mut num = 0usize;

        // The `switch` foreignObject + fallback-text label writer.
        let draw_label = |g: &Element,
                          content: &str,
                          x: f64,
                          y: f64,
                          w: f64,
                          h: f64,
                          class: &str,
                          colour: &str| {
            let _ = colour;
            let switch = append(g, "switch");
            let fo = append(&switch, "foreignObject");
            set_attr(&fo, "x", js_num(x));
            set_attr(&fo, "y", js_num(y));
            set_attr(&fo, "width", js_num(w));
            set_attr(&fo, "height", js_num(h));
            let div = crate::svg::append_xhtml(&fo, "div");
            set_attr(&div, "class", class);
            set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
            set_attr(&div, "style", "display: table; height: 100%; width: 100%;");
            let inner = crate::svg::append_xhtml(&div, "div");
            set_attr(&inner, "class", "label");
            set_attr(
                &inner,
                "style",
                "display: table-cell; text-align: center; vertical-align: middle;",
            );
            crate::svg::set_text(&inner, content);
            let text = append(&switch, "text");
            set_attr(&text, "x", js_num(x + w / 2.0));
            set_attr(&text, "y", js_num(y + h / 2.0));
            let tspan = append(&text, "tspan");
            set_attr(&tspan, "x", js_num(x + w / 2.0));
            set_attr(&tspan, "dy", "0");
            set_text(&tspan, content);
            set_attr(&text, "dominant-baseline", "central");
            set_attr(&text, "alignment-baseline", "central");
            set_attr(&text, "class", class);
            set_attr(
                &text,
                "style",
                "text-anchor: middle; font-size: 14px; font-family: &quot;Open Sans&quot;, sans-serif;"
                    .replace("&quot;", "\""),
            );
        };

        let mut task_count = 0usize;
        for (i, task) in db.tasks.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let fi = i as f64;
            if last_section != task.section {
                fill = SECTION_FILLS[section_number % SECTION_FILLS.len()].to_owned();
                num = section_number % SECTION_FILLS.len();
                colour = SECTION_COLOURS[section_number % SECTION_COLOURS.len()].to_owned();
                let mut task_in_section = 0usize;
                for t in &db.tasks[i..] {
                    if t.section == task.section {
                        task_in_section += 1;
                    } else {
                        break;
                    }
                }
                let sx = fi.mul_add(TASK_MARGIN + WIDTH, left_margin);
                let g = append(&svg, "g");
                let rect = append(&g, "rect");
                set_attr(&rect, "x", js_num(sx));
                set_attr(&rect, "y", "50");
                set_attr(&rect, "fill", fill.clone());
                set_attr(&rect, "stroke", "#666");
                #[allow(clippy::cast_precision_loss)]
                let sw = (task_in_section as f64)
                    .mul_add(WIDTH, DIAGRAM_MARGIN_X * (task_in_section as f64 - 1.0));
                set_attr(&rect, "width", js_num(sw));
                set_attr(&rect, "height", js_num(HEIGHT));
                set_attr(&rect, "rx", "3");
                set_attr(&rect, "ry", "3");
                set_attr(
                    &rect,
                    "class",
                    format!("journey-section section-type-{num}"),
                );
                draw_label(
                    &g,
                    &task.section,
                    sx,
                    50.0,
                    sw,
                    HEIGHT,
                    &format!("journey-section section-type-{num}"),
                    &colour,
                );
                last_section.clone_from(&task.section);
                section_number += 1;
            }

            let x = fi.mul_add(TASK_MARGIN + WIDTH, left_margin);
            let y = task_pos;
            let center = x + WIDTH / 2.0;
            let g = append(&svg, "g");
            let line = append(&g, "line");
            set_attr(&line, "id", format!("{id}-task{task_count}"));
            task_count += 1;
            set_attr(&line, "x1", js_num(center));
            set_attr(&line, "y1", js_num(y));
            set_attr(&line, "x2", js_num(center));
            set_attr(&line, "y2", "450");
            set_attr(&line, "class", "task-line");
            set_attr(&line, "stroke-width", "1px");
            set_attr(&line, "stroke-dasharray", "4 2");
            set_attr(&line, "stroke", "#666");

            // Face.
            let cy = (5.0 - task.score).mul_add(30.0, 300.0);
            let face = append(&g, "circle");
            set_attr(&face, "cx", js_num(center));
            set_attr(&face, "cy", js_num(cy));
            set_attr(&face, "class", "face");
            set_attr(&face, "r", "15");
            set_attr(&face, "stroke-width", "2");
            set_attr(&face, "overflow", "visible");
            let fg = append(&g, "g");
            for dx in [-5.0, 5.0] {
                let eye = append(&fg, "circle");
                set_attr(&eye, "cx", js_num(center + dx));
                set_attr(&eye, "cy", js_num(cy - 5.0));
                set_attr(&eye, "r", "1.5");
                set_attr(&eye, "stroke-width", "2");
                set_attr(&eye, "fill", "#666");
                set_attr(&eye, "stroke", "#666");
            }
            let pi = std::f64::consts::PI;
            if task.score > 3.0 {
                let mouth = append(&fg, "path");
                set_attr(&mouth, "class", "mouth");
                set_attr(
                    &mouth,
                    "d",
                    mouth_arc(pi / 2.0, 3.0 * (pi / 2.0), 15.0 / 2.0, 15.0 / 2.2),
                );
                set_attr(
                    &mouth,
                    "transform",
                    format!("translate({},{})", js_num(center), js_num(cy + 2.0)),
                );
            } else if task.score < 3.0 {
                let mouth = append(&fg, "path");
                set_attr(&mouth, "class", "mouth");
                set_attr(
                    &mouth,
                    "d",
                    mouth_arc(3.0 * pi / 2.0, 5.0 * (pi / 2.0), 15.0 / 2.0, 15.0 / 2.2),
                );
                set_attr(
                    &mouth,
                    "transform",
                    format!("translate({},{})", js_num(center), js_num(cy + 7.0)),
                );
            } else {
                let mouth = append(&fg, "line");
                set_attr(&mouth, "class", "mouth");
                set_attr(&mouth, "stroke", "2");
                set_attr(&mouth, "x1", js_num(center - 5.0));
                set_attr(&mouth, "y1", js_num(cy + 7.0));
                set_attr(&mouth, "x2", js_num(center + 5.0));
                set_attr(&mouth, "y2", js_num(cy + 7.0));
                set_attr(&mouth, "class", "mouth");
                set_attr(&mouth, "stroke-width", "1px");
                set_attr(&mouth, "stroke", "#666");
            }

            let rect = append(&g, "rect");
            set_attr(&rect, "x", js_num(x));
            set_attr(&rect, "y", js_num(y));
            set_attr(&rect, "fill", fill.clone());
            set_attr(&rect, "stroke", "#666");
            set_attr(&rect, "width", js_num(WIDTH));
            set_attr(&rect, "height", js_num(HEIGHT));
            set_attr(&rect, "rx", "3");
            set_attr(&rect, "ry", "3");
            set_attr(&rect, "class", format!("task task-type-{num}"));

            let mut x_pos = x + 14.0;
            for person in &task.people {
                let circle = append(&g, "circle");
                set_attr(&circle, "cx", js_num(x_pos));
                set_attr(&circle, "cy", js_num(y));
                set_attr(&circle, "class", format!("actor-{}", actor_pos(person)));
                set_attr(&circle, "fill", actor_colour(person));
                set_attr(&circle, "stroke", "#000");
                set_attr(&circle, "r", "7");
                let title = append(&circle, "title");
                set_text(&title, person);
                x_pos += 10.0;
            }

            draw_label(&g, &task.name, x, y, WIDTH, HEIGHT, "task", &colour);
            bounds.insert(x, y, x + DIAGRAM_MARGIN_X + TASK_MARGIN, 300.0 + 5.0 * 30.0);
        }
    }

    let box_stopy = bounds.stopy.unwrap_or(0.0);
    let box_starty = bounds.starty.unwrap_or(0.0);
    let box_stopx = bounds.stopx.unwrap_or(0.0);
    let box_startx = bounds.startx.unwrap_or(0.0);

    if !db.title.is_empty() {
        let title = append(&svg, "text");
        set_text(&title, &db.title);
        set_attr(&title, "x", js_num(left_margin));
        set_attr(&title, "font-size", "4ex");
        set_attr(&title, "font-weight", "bold");
        set_attr(&title, "y", "25");
        set_attr(&title, "fill", "");
        set_attr(
            &title,
            "font-family",
            "\"trebuchet ms\", verdana, arial, sans-serif",
        );
    }

    let height = box_stopy - box_starty + 2.0 * DIAGRAM_MARGIN_Y;
    let width = left_margin + box_stopx + 2.0 * DIAGRAM_MARGIN_X;

    // Activity line.
    let line = append(&svg, "line");
    set_attr(&line, "x1", js_num(left_margin));
    set_attr(&line, "y1", js_num(HEIGHT * 4.0));
    set_attr(&line, "x2", js_num(width - left_margin - 4.0));
    set_attr(&line, "y2", js_num(HEIGHT * 4.0));
    set_attr(&line, "stroke-width", "4");
    set_attr(&line, "stroke", "black");
    set_attr(&line, "marker-end", format!("url(#{id}-arrowhead)"));

    let extra_vert = if db.title.is_empty() { 0.0 } else { 70.0 };
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(width)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} -25 {} {}",
            js_num(box_startx),
            js_num(width),
            js_num(height + extra_vert)
        ),
    );
    set_attr(&svg, "preserveAspectRatio", "xMinYMin meet");
    set_attr(&svg, "height", js_num(height + extra_vert + 25.0));
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "journey");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
