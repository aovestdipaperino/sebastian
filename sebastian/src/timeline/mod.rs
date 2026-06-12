//! timeline diagram support (parser + renderer port of
//! `timelineRenderer.ts` and its `svgDraw.js`).

use crate::svg::{Element, append, js_num, new_element, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

/// A parse error for timeline source.
#[derive(Debug)]
pub struct TimelineParseError {
    pub message: String,
}

impl std::fmt::Display for TimelineParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "timeline parse error: {}", self.message)
    }
}

impl std::error::Error for TimelineParseError {}

#[derive(Debug, Clone, Default)]
struct Task {
    section: String,
    label: String,
    events: Vec<String>,
}

#[derive(Debug, Default)]
struct TimelineDb {
    title: String,
    sections: Vec<String>,
    tasks: Vec<Task>,
}

fn parse(source: &str) -> Result<TimelineDb, TimelineParseError> {
    let mut db = TimelineDb::default();
    let mut found_header = false;
    let mut current_section = String::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with('#') {
            continue;
        }
        if !found_header {
            if line == "timeline" {
                found_header = true;
                continue;
            }
            return Err(TimelineParseError {
                message: format!("expected timeline header, got {line:?}"),
            });
        }
        if let Some(rest) = line.strip_prefix("title ") {
            rest.trim().clone_into(&mut db.title);
            continue;
        }
        if let Some(rest) = line.strip_prefix("section ") {
            rest.trim().clone_into(&mut current_section);
            db.sections.push(current_section.clone());
            continue;
        }
        if let Some(rest) = line.strip_prefix(':') {
            // continuation event for the previous task
            if let Some(task) = db.tasks.last_mut() {
                task.events.push(rest.trim().to_owned());
            }
            continue;
        }
        // period : event [: event ...]
        let mut parts = line.split(" : ");
        let period = parts.next().unwrap_or("").trim().to_owned();
        let events: Vec<String> = parts.map(|p| p.trim().to_owned()).collect();
        db.tasks.push(Task {
            section: current_section.clone(),
            label: period,
            events,
        });
    }
    if !found_header {
        return Err(TimelineParseError {
            message: "missing timeline header".to_owned(),
        });
    }
    Ok(db)
}

const LEFT_MARGIN: f64 = 150.0;
const FONT_SIZE: f64 = 16.0;

fn f32q(v: f64) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    f64::from(v as f32)
}

/// d3 `wrap`: greedy tspan filling, splitting on `/(\s+|<br>)/` with
/// separators kept (whence the multiplied spaces in the output).
fn wrap_tspans(descr: &str, width: f64, measurer: &TextMeasurer) -> Vec<String> {
    // Tokenize keeping whitespace runs and <br> tokens.
    let mut tokens: Vec<String> = Vec::new();
    let mut rest = descr;
    while !rest.is_empty() {
        if let Some(idx) = rest.find("<br>") {
            let before = &rest[..idx];
            push_ws_tokens(before, &mut tokens);
            tokens.push("<br>".to_owned());
            rest = &rest[idx + 4..];
        } else {
            push_ws_tokens(rest, &mut tokens);
            rest = "";
        }
    }

    let mut tspans: Vec<String> = Vec::new();
    let mut line: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in tokens {
        line.push(word.clone());
        current = join_trim(&line);
        // getComputedTextLength sees xml:space-collapsed whitespace.
        let collapsed = collapse_ws(&current);
        if measurer.measure_advance_svg(&collapsed, FONT_SIZE) > width || word == "<br>" {
            line.pop();
            current = join_trim(&line);
            tspans.push(current.clone());
            if word == "<br>" {
                line = vec![String::new()];
                current = String::new();
            } else {
                line = vec![word.clone()];
                current = word;
            }
        }
    }
    tspans.push(current);
    tspans
}

fn push_ws_tokens(text: &str, tokens: &mut Vec<String>) {
    let mut cur = String::new();
    let mut cur_ws = false;
    for c in text.chars() {
        let ws = c.is_whitespace();
        if !cur.is_empty() && ws != cur_ws {
            tokens.push(std::mem::take(&mut cur));
        }
        cur_ws = ws;
        cur.push(c);
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
}

fn join_trim(line: &[String]) -> String {
    line.join(" ").trim().to_owned()
}

/// Collapses whitespace runs to single spaces (SVG default xml:space).
fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prev_ws = false;
    for c in text.chars() {
        if c.is_whitespace() {
            if !prev_ws {
                out.push(' ');
            }
            prev_ws = true;
        } else {
            out.push(c);
            prev_ws = false;
        }
    }
    out
}

/// Wrapped-text getBBox height: integer Trebuchet font box, first baseline
/// at 1em, subsequent baselines accumulated in f32 with a 1.1em step.
fn text_bbox_height(nlines: usize) -> f64 {
    let ascent = (1923.0 * FONT_SIZE / 2048.0).round();
    let descent = (455.0 * FONT_SIZE / 2048.0).round();
    let mut baseline = FONT_SIZE;
    for _ in 1..nlines {
        baseline = f32q(baseline + FONT_SIZE * 1.1);
    }
    baseline + descent - (FONT_SIZE - ascent)
}

struct NodeOut {
    height: f64,
}

/// `svgDraw.drawNode`.
#[allow(clippy::too_many_arguments)]
fn draw_node(
    wrapper: &Element,
    descr: &str,
    full_section: i64,
    node_width: f64,
    max_height: f64,
    node_count: &mut usize,
    diagram_id: &str,
    measurer: &TextMeasurer,
) -> NodeOut {
    let section = (full_section % 12) - 1;
    let node_elem = append(wrapper, "g");
    set_attr(
        &node_elem,
        "class",
        format!("timeline-node section-{section}"),
    );
    let bkg_elem = append(&node_elem, "g");
    let text_elem = append(&node_elem, "g");

    let text = append(&text_elem, "text");
    set_attr(&text, "dy", "1em");
    set_attr(&text, "alignment-baseline", "middle");
    set_attr(&text, "dominant-baseline", "middle");
    set_attr(&text, "text-anchor", "middle");
    let tspans = wrap_tspans(descr, node_width, measurer);
    for (i, t) in tspans.iter().enumerate() {
        let span = append(&text, "tspan");
        set_attr(&span, "x", "0");
        set_attr(&span, "dy", if i == 0 { "1em" } else { "1.1em" });
        set_text(&span, t);
    }

    let bbox_h = text_bbox_height(tspans.len());
    let padding = 20.0;
    let mut height = bbox_h + FONT_SIZE * 1.1 * 0.5 + padding;
    height = height.max(max_height);
    let width = node_width + 2.0 * padding;

    set_attr(
        &text_elem,
        "transform",
        format!(
            "translate({}, {})",
            js_num(width / 2.0),
            js_num(padding / 2.0)
        ),
    );

    // defaultBkg
    let rd = 5.0;
    let r = 5.0;
    let path = append(&bkg_elem, "path");
    set_attr(&path, "id", format!("{diagram_id}-node-{}", *node_count));
    *node_count += 1;
    set_attr(&path, "class", "node-bkg node-undefined");
    set_attr(
        &path,
        "d",
        format!(
            "M0 {} v{} q0,-{r},{r},-{r} h{} q{r},0,{r},{r} v{} H0 Z",
            js_num(height - rd),
            js_num(-height + 2.0 * rd),
            js_num(width - 2.0 * rd),
            js_num(height - rd)
        ),
    );
    let line = append(&bkg_elem, "line");
    set_attr(&line, "class", format!("node-line-{section}"));
    set_attr(&line, "x1", "0");
    set_attr(&line, "y1", js_num(height));
    set_attr(&line, "x2", js_num(width));
    set_attr(&line, "y2", js_num(height));

    NodeOut { height }
}

/// `getVirtualNodeHeight`.
fn virtual_node_height(descr: &str, measurer: &TextMeasurer) -> f64 {
    let tspans = wrap_tspans(descr, 150.0, measurer);
    text_bbox_height(tspans.len()) + FONT_SIZE * 1.1 * 0.5 + 20.0
}

/// Renders mermaid timeline source to a complete SVG document.
///
/// # Errors
/// Returns [`TimelineParseError`] for unparsable source.
#[allow(clippy::too_many_lines)]
pub fn render_timeline(source: &str, id: &str) -> Result<String, TimelineParseError> {
    let db = parse(source)?;
    let measurer = TextMeasurer::new();
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);

    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    set_attr(&svg, "style", "");
    set_attr(&svg, "viewBox", "");
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "timeline");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_timeline_css(id, &theme_vars),
    );
    let _scaffold_g = append(&svg, "g");
    let _draw_g = append(&svg, "g");

    // initGraphics: arrowhead marker.
    let defs = append(&svg, "defs");
    let marker = append(&defs, "marker");
    set_attr(&marker, "id", format!("{id}-arrowhead"));
    set_attr(&marker, "refX", "5");
    set_attr(&marker, "refY", "2");
    set_attr(&marker, "markerWidth", "6");
    set_attr(&marker, "markerHeight", "4");
    set_attr(&marker, "orient", "auto");
    let mpath = append(&marker, "path");
    set_attr(&mpath, "d", "M 0,0 V 4 L6,2 Z");

    // Section/task sizing.
    let mut max_section_height = 0.0f64;
    for section in &db.sections {
        let h = virtual_node_height(section, &measurer);
        max_section_height = max_section_height.max(h + 20.0);
    }
    let mut max_task_height = 0.0f64;
    let mut max_event_line_length = 0.0f64;
    for task in &db.tasks {
        let h = virtual_node_height(&task.label, &measurer);
        max_task_height = max_task_height.max(h + 20.0);
        let mut event_len = 0.0;
        for event in &task.events {
            event_len += virtual_node_height(event, &measurer);
        }
        if !task.events.is_empty() {
            #[allow(clippy::cast_precision_loss)]
            let spacing = (task.events.len() - 1) as f64 * 10.0;
            event_len += spacing;
        }
        max_event_line_length = max_event_line_length.max(event_len);
    }

    let mut node_count = 0usize;
    let mut master_x = 50.0 + LEFT_MARGIN;
    let master_y = 50.0;
    let has_sections = !db.sections.is_empty();

    let mut section_number: i64 = 0;
    if has_sections {
        let mut master_y_local = master_y;
        for section in &db.sections {
            let tasks_for_section: Vec<&Task> =
                db.tasks.iter().filter(|t| t.section == *section).collect();
            let wrapper = append(&svg, "g");
            // drawNode with width 200*n-50: the width affects only the bkg —
            // our draw_node hardcodes 150; extend if sectioned corpora appear.
            #[allow(clippy::cast_precision_loss)]
            let section_width = 200.0 * (tasks_for_section.len().max(1)) as f64 - 50.0;
            let node = draw_node(
                &wrapper,
                section,
                section_number,
                section_width,
                max_section_height,
                &mut node_count,
                id,
                &measurer,
            );
            let _ = node;
            set_attr(
                &wrapper,
                "transform",
                format!("translate({}, {})", js_num(master_x), js_num(master_y)),
            );
            master_y_local += max_section_height + 50.0;
            if !tasks_for_section.is_empty() {
                draw_tasks(
                    &svg,
                    &tasks_for_section,
                    section_number,
                    master_x,
                    master_y_local,
                    max_task_height,
                    max_event_line_length,
                    false,
                    id,
                    &measurer,
                    &mut node_count,
                );
            }
            #[allow(clippy::cast_precision_loss)]
            let advance = 200.0 * (tasks_for_section.len().max(1)) as f64;
            master_x += advance;
            master_y_local = master_y;
            section_number += 1;
        }
    } else {
        let tasks: Vec<&Task> = db.tasks.iter().collect();
        draw_tasks(
            &svg,
            &tasks,
            section_number,
            master_x,
            master_y,
            max_task_height,
            max_event_line_length,
            true,
            id,
            &measurer,
            &mut node_count,
        );
    }

    // box = svg.getBBox() before title/activity line.
    let pre_box = crate::render::bbox::element_bbox(&svg);
    let box_width = pre_box.width();

    let mut title_box: Option<crate::render::bbox::Rect> = None;
    if !db.title.is_empty() {
        let title = append(&svg, "text");
        set_text(&title, &db.title);
        set_attr(&title, "x", js_num(box_width / 2.0 - LEFT_MARGIN));
        set_attr(&title, "font-size", "4ex");
        set_attr(&title, "font-weight", "bold");
        set_attr(&title, "y", "20");
        // Title bbox: 4ex of the inherited 16px Trebuchet, bold face.
        let ex = measurer.x_height_px(FONT_SIZE);
        let fs = 4.0 * ex;
        let (asc, desc, adv) = measurer.bold_metrics(&db.title, fs);
        title_box = Some(crate::render::bbox::Rect::from_geometry(
            box_width / 2.0 - LEFT_MARGIN,
            20.0 - asc,
            adv,
            asc + desc,
        ));
    }

    let depth_y = if has_sections {
        max_section_height + max_task_height + 150.0
    } else {
        max_task_height + 100.0
    };

    let line_wrapper = append(&svg, "g");
    set_attr(&line_wrapper, "class", "lineWrapper");
    let line = append(&line_wrapper, "line");
    set_attr(&line, "x1", js_num(LEFT_MARGIN));
    set_attr(&line, "y1", js_num(depth_y));
    set_attr(&line, "x2", js_num(box_width + 3.0 * LEFT_MARGIN));
    set_attr(&line, "y2", js_num(depth_y));
    set_attr(&line, "stroke-width", "4");
    set_attr(&line, "stroke", "black");
    set_attr(&line, "marker-end", format!("url(#{id}-arrowhead)"));

    // setupGraphViewbox(padding 50).
    let mut bounds = crate::render::bbox::element_bbox(&svg);
    if let Some(t) = title_box {
        bounds.union_with(&t);
    }
    let padding = 50.0;
    let width = bounds.width() + padding * 2.0;
    let height = bounds.height() + padding * 2.0;
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
            "{} {} {} {}",
            js_num(bounds.min_x - padding),
            js_num(bounds.min_y - padding),
            js_num(width),
            js_num(height)
        ),
    );

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// `drawTasks` (+ `drawEvents`).
#[allow(clippy::too_many_arguments)]
fn draw_tasks(
    svg: &Element,
    tasks: &[&Task],
    section_color_start: i64,
    master_x_start: f64,
    master_y: f64,
    max_task_height: f64,
    max_event_line_length: f64,
    is_without_sections: bool,
    diagram_id: &str,
    measurer: &TextMeasurer,
    node_count: &mut usize,
) {
    let mut master_x = master_x_start;
    let mut section_color = section_color_start;
    for task in tasks {
        let task_wrapper = append(svg, "g");
        set_attr(&task_wrapper, "class", "taskWrapper");
        let node = draw_node(
            &task_wrapper,
            &task.label,
            section_color,
            150.0,
            max_task_height,
            node_count,
            diagram_id,
            measurer,
        );
        set_attr(
            &task_wrapper,
            "transform",
            format!("translate({}, {})", js_num(master_x), js_num(master_y)),
        );
        let task_height = node.height.max(max_task_height);

        if !task.events.is_empty() {
            let line_wrapper = append(svg, "g");
            set_attr(&line_wrapper, "class", "lineWrapper");
            let events_y = master_y + 100.0;
            draw_events(
                svg,
                &task.events,
                section_color,
                master_x,
                events_y,
                diagram_id,
                measurer,
                node_count,
            );
            let line = append(&line_wrapper, "line");
            set_attr(&line, "x1", js_num(master_x + 190.0 / 2.0));
            set_attr(&line, "y1", js_num(master_y + task_height));
            set_attr(&line, "x2", js_num(master_x + 190.0 / 2.0));
            set_attr(
                &line,
                "y2",
                js_num(master_y + task_height + 100.0 + max_event_line_length + 100.0),
            );
            set_attr(&line, "stroke-width", "2");
            set_attr(&line, "stroke", "black");
            set_attr(&line, "marker-end", format!("url(#{diagram_id}-arrowhead)"));
            set_attr(&line, "stroke-dasharray", "5,5");
        }

        master_x += 200.0;
        if is_without_sections {
            section_color += 1;
        }
    }
}

/// `drawEvents`.
#[allow(clippy::too_many_arguments)]
fn draw_events(
    svg: &Element,
    events: &[String],
    section_color: i64,
    master_x: f64,
    master_y_in: f64,
    diagram_id: &str,
    measurer: &TextMeasurer,
    node_count: &mut usize,
) -> f64 {
    let mut master_y = master_y_in + 100.0;
    let mut max_event_height = 0.0;
    for event in events {
        let wrapper = append(svg, "g");
        set_attr(&wrapper, "class", "eventWrapper");
        let node = draw_node(
            &wrapper,
            event,
            section_color,
            150.0,
            50.0,
            node_count,
            diagram_id,
            measurer,
        );
        max_event_height += node.height;
        set_attr(
            &wrapper,
            "transform",
            format!("translate({}, {})", js_num(master_x), js_num(master_y)),
        );
        master_y = master_y + 10.0 + node.height;
    }
    max_event_height
}
