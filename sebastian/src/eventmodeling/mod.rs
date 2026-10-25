//! `eventmodeling`: a port of mermaid 11.15.0's event modeling parser, db
//! (the decider/evolver layout) and renderer.

#![allow(clippy::cast_possible_wrap)]

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

/// A parse error for eventmodeling source.
#[derive(Debug)]
pub struct EventModelingParseError {
    pub message: String,
}

impl std::fmt::Display for EventModelingParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "eventmodeling parse error: {}", self.message)
    }
}

impl std::error::Error for EventModelingParseError {}

fn err<T>(message: impl Into<String>) -> Result<T, EventModelingParseError> {
    Err(EventModelingParseError {
        message: message.into(),
    })
}

// ------------------------------------------------------------------ props --

const SWIMLANE_MIN_HEIGHT: f64 = 70.0;
const SWIMLANE_PADDING: f64 = 15.0;
const SWIMLANE_GAP: f64 = 10.0;
const BOX_PADDING: f64 = 10.0;
const BOX_OVERLAP: f64 = 90.0;
const BOX_MIN_WIDTH: f64 = 80.0;
const BOX_MAX_WIDTH: f64 = 450.0;
const BOX_MIN_HEIGHT: f64 = 80.0;
const BOX_MAX_HEIGHT: f64 = 750.0;
const CONTENT_START_X: f64 = 250.0;
const TEXT_MAX_WIDTH: f64 = 450.0 - 2.0 * 10.0;
const BOX_TEXT_PADDING: f64 = 10.0;

// ----------------------------------------------------------------- parser --

#[derive(Debug, Clone)]
struct Frame {
    name: String,
    reset: bool,
    entity_type: String,
    identifier: String,
    sources: Vec<String>,
    data_ref: Option<String>,
    inline_data: Option<String>,
}

#[derive(Debug, Default)]
struct Model {
    frames: Vec<Frame>,
    data: BTreeMap<String, String>,
}

fn is_entity_type(t: &str) -> bool {
    matches!(
        t,
        "rmo" | "readmodel" | "ui" | "cmd" | "command" | "evt" | "event" | "pcr" | "processor"
    )
}

fn parse(source: &str) -> Result<Model, EventModelingParseError> {
    let mut model = Model::default();
    let mut found = false;
    let mut lines = source.lines();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with("//") {
            continue;
        }
        if !found {
            if line == "eventmodeling" {
                found = true;
                continue;
            }
            return err(format!("expected eventmodeling header, got {line:?}"));
        }
        let mut words = line.splitn(2, char::is_whitespace);
        let keyword = words.next().unwrap_or("");
        let rest = words.next().unwrap_or("").trim();
        match keyword {
            "tf" | "timeframe" | "rf" | "resetframe" => {
                let mut toks = rest.split_whitespace();
                let name = toks.next().unwrap_or("");
                let entity_type = toks.next().unwrap_or("");
                let identifier = toks.next().unwrap_or("");
                if name.is_empty()
                    || name.len() > 3
                    || !name.bytes().all(|b| b.is_ascii_digit())
                    || !is_entity_type(entity_type)
                    || identifier.is_empty()
                {
                    return err(format!("bad frame: {line}"));
                }
                // The remainder after the identifier: sources, data ref, inline data.
                let after_ident = {
                    let at = rest.find(identifier).unwrap_or(0) + identifier.len();
                    rest[at..].trim()
                };
                let mut sources = Vec::new();
                let mut tail = after_ident;
                while let Some(r) = tail.strip_prefix("->>") {
                    let r = r.trim_start();
                    let n = r.bytes().take_while(u8::is_ascii_digit).count();
                    sources.push(r[..n].to_owned());
                    tail = r[n..].trim_start();
                }
                let mut data_ref = None;
                if let Some(r) = tail.strip_prefix("[[") {
                    let end = r.find("]]").unwrap_or(r.len());
                    data_ref = Some(r[..end].trim().to_owned());
                    tail = r.get(end + 2..).unwrap_or("").trim_start();
                }
                let inline_data = (!tail.is_empty()).then(|| tail.to_owned());
                model.frames.push(Frame {
                    name: name.to_owned(),
                    reset: keyword.starts_with('r'),
                    entity_type: entity_type.to_owned(),
                    identifier: identifier.to_owned(),
                    sources,
                    data_ref,
                    inline_data,
                });
            }
            "data" => {
                let name = rest.split(['{', ' ']).next().unwrap_or("").to_owned();
                // Block `{` ... `}` over following lines.
                let mut body = String::new();
                if !rest.contains('{') {
                    // The opening brace may sit on the next line.
                    let opened = lines.by_ref().find(|l| !l.trim().is_empty());
                    if !opened.is_some_and(|l| l.trim_start().starts_with('{')) {
                        return err("data needs a block");
                    }
                }
                body.push_str("{\n");
                for inner in lines.by_ref() {
                    if inner.trim_start().starts_with('}') {
                        break;
                    }
                    body.push_str(inner);
                    body.push('\n');
                }
                body.push_str("}\n");
                model.data.insert(name, body);
            }
            "note" => {
                if rest.contains('{') && !rest.contains('}') {
                    for inner in lines.by_ref() {
                        if inner.trim_start().starts_with('}') {
                            break;
                        }
                    }
                }
            }
            "entity" | "gwt" | "given" | "when" | "then" | "title" | "accTitle" | "accDescr" => {}
            _ => {
                // Continuation lines of a `gwt` statement are consumed here too.
                if !matches!(keyword, "ui" | "cmd" | "evt" | "rmo" | "pcr") {
                    return err(format!("unsupported statement: {line}"));
                }
            }
        }
    }
    if !found {
        return err("missing eventmodeling header");
    }
    Ok(model)
}

// ------------------------------------------------------------------- text --

/// `utils.calculateTextDimensions` with bold Trebuchet at 16px: the width is
/// the widest rounded line, the height the sum of rounded line heights.
fn text_dimensions(measurer: &TextMeasurer, text: &str) -> (f64, f64) {
    let mut width = 0.0f64;
    let mut height = 0.0f64;
    for line in crate::text::split_breaks(text) {
        // SVG text collapses whitespace runs and drops the outer blanks.
        let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let (w, h) = if collapsed.is_empty() {
            (0.0, measurer.ink_height("\u{200b}", 16.0))
        } else {
            (
                measurer.measure_width(&collapsed, 16.0),
                measurer.ink_height(&collapsed, 16.0),
            )
        };
        width = width.max(js_round(w));
        height += js_round(h);
    }
    (width, height)
}

fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

fn text_width(measurer: &TextMeasurer, text: &str) -> f64 {
    text_dimensions(measurer, text).0
}

/// `utils.breakString`.
fn break_string(measurer: &TextMeasurer, word: &str, max_width: f64) -> (Vec<String>, String) {
    let chars: Vec<char> = word.chars().collect();
    let mut lines = Vec::new();
    let mut current = String::new();
    for (i, c) in chars.iter().enumerate() {
        let next = format!("{current}{c}");
        if text_width(measurer, &next) >= max_width {
            let last = chars.len() == i + 1;
            lines.push(if last { next } else { format!("{next}-") });
            current.clear();
        } else {
            current = next;
        }
    }
    (lines, current)
}

/// `utils.wrapLabel` joining wrapped lines with `<br/>`.
fn wrap_label(measurer: &TextMeasurer, label: &str, max_width: f64) -> String {
    if label.is_empty() {
        return String::new();
    }
    if crate::text::split_breaks(label).len() > 1 {
        return label.to_owned();
    }
    let words: Vec<&str> = label.split(' ').filter(|w| !w.is_empty()).collect();
    let mut completed: Vec<String> = Vec::new();
    let mut next_line = String::new();
    for (i, word) in words.iter().enumerate() {
        let word_len = text_width(measurer, &format!("{word} "));
        let next_len = text_width(measurer, &next_line);
        if word_len > max_width {
            let (hyph, rest) = break_string(measurer, word, max_width);
            completed.push(std::mem::take(&mut next_line));
            completed.extend(hyph);
            next_line = rest;
        } else if next_len + word_len >= max_width {
            completed.push(std::mem::replace(&mut next_line, (*word).to_owned()));
        } else {
            next_line = [next_line.as_str(), word]
                .iter()
                .filter(|s| !s.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
        }
        if i + 1 == words.len() {
            completed.push(next_line.clone());
        }
    }
    completed
        .into_iter()
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("<br/>")
}

fn sanitize(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// JS `substring(start, end)` on byte offsets of an ASCII-safe split.
fn js_substring(s: &str, start: i64, end: i64) -> String {
    let len = s.chars().count() as i64;
    let clamp = |v: i64| v.clamp(0, len);
    let (a, b) = (clamp(start), clamp(end));
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    s.chars().skip(a as usize).take((b - a) as usize).collect()
}

fn char_index(s: &str, pat: &str, last: bool) -> i64 {
    let byte = if last { s.rfind(pat) } else { s.find(pat) };
    byte.map_or(-1, |b| s[..b].chars().count() as i64)
}

struct TextProps {
    /// Wrapped name and optional data html, as drawn inside the box.
    name_html: String,
    data_html: Option<String>,
    width: f64,
    height: f64,
}

fn text_props(measurer: &TextMeasurer, frame: &Frame, model: &Model) -> TextProps {
    let name = frame
        .identifier
        .rsplit_once('.')
        .map_or(frame.identifier.as_str(), |(_, n)| n);
    let name = if frame.identifier.matches('.').count() == 1 {
        name
    } else {
        &frame.identifier
    };
    let wrapped = wrap_label(measurer, &sanitize(name), TEXT_MAX_WIDTH);
    let mut content = format!("<b>{wrapped}</b>");
    let mut html: Option<String> = None;
    if let Some(inline) = &frame.inline_data {
        let after = js_substring(inline, char_index(inline, "{", false) + 1, i64::MAX);
        let upto = js_substring(&after, 0, char_index(&after, "}", true) - 1);
        let wrapped = wrap_label(measurer, &sanitize(&upto), TEXT_MAX_WIDTH);
        html = Some(wrapped.replace(' ', "&nbsp;"));
    }
    if let Some(r) = &frame.data_ref
        && let Some(block) = model.data.get(r)
    {
        let after = js_substring(block, char_index(block, "{\n", false) + 2, i64::MAX);
        let upto = js_substring(&after, 0, char_index(&after, "}", true) - 1);
        let wrapped = wrap_label(measurer, &sanitize(&upto), TEXT_MAX_WIDTH);
        html = Some(format!("{}<br/>", wrapped.replace(' ', "&nbsp;")));
    }
    let has_data = html.is_some();
    if let Some(h) = &html {
        let _ = write!(
            content,
            "<br/><br/><code style=\"text-align: left; display: block;max-width:{}px\">{h}</code>",
            js_num(TEXT_MAX_WIDTH)
        );
    }
    let (w, h) = text_dimensions(measurer, &content);
    TextProps {
        name_html: wrapped,
        data_html: html,
        width: if has_data { w / 3.0 } else { w },
        height: h,
    }
}

// ----------------------------------------------------------------- layout --

#[derive(Debug, Clone)]
struct Swimlane {
    index: i64,
    label: String,
    r: f64,
    y: f64,
    height: f64,
    max_height: f64,
}

#[derive(Debug, Clone)]
struct BoxItem {
    x: f64,
    r: f64,
    width: f64,
    height: f64,
    swimlane: i64,
    fill: String,
    stroke: String,
    name_html: String,
    data_html: Option<String>,
    frame: usize,
}

#[derive(Debug, Clone, Copy)]
struct Relation {
    source: usize,
    target: usize,
}

struct State {
    boxes: Vec<BoxItem>,
    swimlanes: BTreeMap<i64, Swimlane>,
    relations: Vec<Relation>,
    previous_swimlane: Option<i64>,
    max_r: f64,
}

fn namespace_of(identifier: &str) -> Option<&str> {
    let parts: Vec<&str> = identifier.split('.').collect();
    (parts.len() == 2).then(|| parts[0])
}

fn next_index(sw: &BTreeMap<i64, Swimlane>, min: i64, max: i64) -> i64 {
    sw.keys()
        .copied()
        .filter(|k| *k > min && *k < max)
        .fold(min, i64::max)
        + 1
}

fn swimlane_props(frame: &Frame, sw: &BTreeMap<i64, Swimlane>) -> (i64, String) {
    let ns = namespace_of(&frame.identifier).filter(|n| !n.is_empty());
    // Swimlanes remember no namespace here (the namespace is never stored),
    // so the lookup by namespace never matches.
    let (base, lo, hi, label, prefix) = match frame.entity_type.as_str() {
        "ui" | "pcr" | "processor" => (0, 0, 100, "UI/Automation", "UI/A: "),
        "rmo" | "readmodel" | "cmd" | "command" => (100, 100, 200, "Command/Read Model", "C/RM: "),
        _ => (200, 200, 300, "Events", "Stream: "),
    };
    match ns {
        Some(n) => (next_index(sw, lo, hi), format!("{prefix}{n}")),
        None => (base, label.to_owned()),
    }
}

fn visual(frame: &Frame, vars: &serde_json::Map<String, serde_json::Value>) -> (String, String) {
    let pick = |key: &str, d: &str| {
        vars.get(key)
            .and_then(|v| v.as_str())
            .map_or_else(|| d.to_owned(), str::to_owned)
    };
    match frame.entity_type.as_str() {
        "ui" => (pick("emUiFill", "white"), pick("emUiStroke", "#dbdada")),
        "pcr" | "processor" => (
            pick("emProcessorFill", "#edb3f6"),
            pick("emProcessorStroke", "#b88cbf"),
        ),
        "rmo" | "readmodel" => (
            pick("emReadModelFill", "#d3f1a2"),
            pick("emReadModelStroke", "#a3b732"),
        ),
        "cmd" | "command" => (
            pick("emCommandFill", "#bcd6fe"),
            pick("emCommandStroke", "#679ac3"),
        ),
        "evt" | "event" => (
            pick("emEventFill", "#ffb778"),
            pick("emEventStroke", "#c19a0f"),
        ),
        _ => ("red".to_owned(), "black".to_owned()),
    }
}

fn layout(
    model: &Model,
    measurer: &TextMeasurer,
    vars: &serde_json::Map<String, serde_json::Value>,
) -> Result<State, EventModelingParseError> {
    let mut st = State {
        boxes: Vec::new(),
        swimlanes: BTreeMap::new(),
        relations: Vec::new(),
        previous_swimlane: None,
        max_r: 0.0,
    };
    for (index, frame) in model.frames.iter().enumerate() {
        let props = text_props(measurer, frame, model);
        let (fill, stroke) = visual(frame, vars);
        // evolveFramePositioned
        let (sw_index, sw_label) = swimlane_props(frame, &st.swimlanes);
        let mut swimlane = st.swimlanes.get(&sw_index).cloned().unwrap_or(Swimlane {
            index: sw_index,
            label: sw_label,
            r: 0.0,
            y: sw_index as f64 * SWIMLANE_MIN_HEIGHT + SWIMLANE_GAP,
            height: SWIMLANE_MIN_HEIGHT,
            max_height: SWIMLANE_MIN_HEIGHT,
        });
        let last_box_r = st.boxes.last().map(|b| b.r);
        let previous = st
            .previous_swimlane
            .and_then(|i| st.swimlanes.get(&i))
            .map(|s| s.index);
        let width = BOX_MIN_WIDTH.max(BOX_MAX_WIDTH.min(props.width + 2.0 * BOX_TEXT_PADDING))
            + 2.0 * BOX_PADDING;
        let height = BOX_MIN_HEIGHT.max(BOX_MAX_HEIGHT.min(props.height + 2.0 * BOX_TEXT_PADDING))
            + 2.0 * BOX_PADDING;
        let x = match (previous, last_box_r) {
            (Some(p), _) if p == swimlane.index && swimlane.r != 0.0 => swimlane.r + BOX_PADDING,
            (None, _) | (_, None) => CONTENT_START_X,
            (_, Some(r)) => r - BOX_OVERLAP + BOX_PADDING,
        };
        let r = x + width + BOX_PADDING;
        let max_r = st
            .swimlanes
            .values()
            .map(|s| s.r)
            .chain(std::iter::once(r))
            .fold(f64::NEG_INFINITY, f64::max);
        swimlane.r = x + width;
        swimlane.max_height = swimlane.max_height.max(height);
        swimlane.height = SWIMLANE_MIN_HEIGHT.max(swimlane.max_height) + 2.0 * SWIMLANE_PADDING;
        st.swimlanes.insert(sw_index, swimlane);
        st.boxes.push(BoxItem {
            x,
            r,
            width,
            height,
            swimlane: sw_index,
            fill,
            stroke,
            name_html: props.name_html,
            data_html: props.data_html,
            frame: index,
        });
        st.previous_swimlane = Some(sw_index);
        st.max_r = max_r;
        // Swimlane y recalculation.
        let keys: Vec<i64> = st.swimlanes.keys().copied().collect();
        let mut prev: Option<(f64, f64)> = None;
        for k in keys {
            let s = st.swimlanes.get_mut(&k).expect("swimlane");
            s.y = match prev {
                None => 0.0,
                Some((py, ph)) => py + ph + SWIMLANE_GAP,
            };
            prev = Some((s.y, s.height));
        }

        // Relations.
        let box_of = |boxes: &[BoxItem], f: usize| boxes.iter().position(|b| b.frame == f);
        let pos_relation =
            |st: &mut State, source: Option<usize>| -> Result<(), EventModelingParseError> {
                if frame.reset || (index == 0 && frame.sources.is_empty()) {
                    return Ok(());
                }
                let Some(target) = box_of(&st.boxes, index) else {
                    return err(format!("Target box not found for frame {}", frame.name));
                };
                let source_box = if let Some(f) = source {
                    box_of(&st.boxes, f)
                } else {
                    let target_lane = st.boxes[target].swimlane;
                    (0..index)
                        .rev()
                        .find(|&i| st.boxes[i].swimlane != target_lane)
                };
                if let Some(s) = source_box {
                    st.relations.push(Relation { source: s, target });
                }
                Ok(())
            };
        if frame.sources.is_empty() {
            pos_relation(&mut st, None)?;
        } else {
            let sources: Vec<usize> = model
                .frames
                .iter()
                .enumerate()
                .filter(|(_, f)| frame.sources.contains(&f.name))
                .map(|(i, _)| i)
                .collect();
            for s in sources {
                pos_relation(&mut st, Some(s))?;
            }
        }
    }
    Ok(st)
}

// --------------------------------------------------------------- renderer --

fn add(parent: &Element, tag: &str, attrs: &[(&str, String)]) -> Element {
    let e = append(parent, tag);
    for (k, v) in attrs {
        set_attr(&e, k, v.clone());
    }
    e
}

/// Renders mermaid `eventmodeling` source to a complete SVG document.
///
/// # Errors
/// Returns [`EventModelingParseError`] when the source is not valid.
#[allow(clippy::too_many_lines)]
pub fn render_eventmodeling(source: &str, id: &str) -> Result<String, EventModelingParseError> {
    let config = crate::render::config::detect_init(source);
    let vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let model = parse(source)?;
    let measurer = TextMeasurer::new();
    measurer.set_bold(true);
    let st = layout(&model, &measurer, &vars)?;

    let tv = |key: &str, fallback: &str| {
        let v = crate::render::themes::get(&vars, key);
        if v.is_empty() { fallback.to_owned() } else { v }
    };
    let line_color = tv("lineColor", "#333333");
    let odd_bg = tv("emSwimlaneBackgroundOdd", "rgb(250,250,250)");
    let bg_stroke = tv("emSwimlaneBackgroundStroke", "rgb(240,240,240)");
    let relation_stroke = {
        let v = tv("emRelationStroke", "");
        if v.is_empty() { line_color.clone() } else { v }
    };
    let arrowhead = {
        let v = tv("emArrowhead", "");
        if v.is_empty() { line_color.clone() } else { v }
    };

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style = append(&svg, "style");
    set_text(&style, &crate::render::css::themed_info_css(id, &vars));
    append(&svg, "g");

    for sw in st.swimlanes.values() {
        let g = add(&svg, "g", &[("class", "em-swimlane".to_owned())]);
        add(
            &g,
            "rect",
            &[
                ("x", "0".to_owned()),
                ("y", js_num(sw.y)),
                ("rx", "3".to_owned()),
                ("width", js_num(st.max_r + SWIMLANE_PADDING)),
                ("height", js_num(sw.height)),
                ("fill", odd_bg.clone()),
                ("stroke", bg_stroke.clone()),
            ],
        );
        let t = add(
            &g,
            "text",
            &[
                ("font-weight", "bold".to_owned()),
                ("x", "30".to_owned()),
                ("y", js_num(sw.y + 30.0)),
            ],
        );
        set_text(&t, &sw.label);
    }
    for b in &st.boxes {
        let y = st.swimlanes[&b.swimlane].y + SWIMLANE_PADDING;
        let g = add(&svg, "g", &[("class", "em-box".to_owned())]);
        add(
            &g,
            "rect",
            &[
                ("x", js_num(b.x)),
                ("y", js_num(y)),
                ("rx", "3".to_owned()),
                ("width", js_num(b.width)),
                ("height", js_num(b.height)),
                ("stroke", b.stroke.clone()),
                ("fill", b.fill.clone()),
            ],
        );
        let fo = add(
            &g,
            "foreignObject",
            &[
                ("x", js_num(b.x + BOX_PADDING)),
                ("y", js_num(y + 10.0)),
                ("width", js_num(b.width - 2.0 * BOX_PADDING)),
                ("height", js_num(b.height - 2.0 * BOX_PADDING)),
            ],
        );
        let div = crate::svg::append_xhtml(&fo, "div");
        set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
        set_attr(&div, "style", "display: table; height: 100%; width: 100%;");
        let span = crate::svg::append_xhtml(&div, "span");
        set_attr(
            &span,
            "style",
            "display: table-cell; text-align: center; vertical-align: middle;",
        );
        let bold = crate::svg::append_xhtml(&span, "b");
        append_html(&bold, &b.name_html);
        if let Some(data) = &b.data_html {
            for _ in 0..2 {
                crate::svg::append_xhtml(&span, "br");
            }
            let code = crate::svg::append_xhtml(&span, "code");
            set_attr(
                &code,
                "style",
                format!(
                    "text-align: left; display: block;max-width:{}px",
                    js_num(TEXT_MAX_WIDTH)
                ),
            );
            append_html(&code, data);
        }
    }
    for rel in &st.relations {
        let (s, t) = (&st.boxes[rel.source], &st.boxes[rel.target]);
        let sy = st.swimlanes[&s.swimlane].y + SWIMLANE_PADDING;
        let ty = st.swimlanes[&t.swimlane].y + SWIMLANE_PADDING;
        let sx = s.x + (s.width * 2.0) / 3.0;
        let tx = t.x + t.width / 3.0;
        let (y1, y2) = if sy > ty {
            (sy, ty + t.height)
        } else {
            (sy + s.height, ty)
        };
        add(
            &svg,
            "path",
            &[
                ("class", "em-relation".to_owned()),
                ("fill", "none".to_owned()),
                ("stroke", relation_stroke.clone()),
                ("stroke-width", "1".to_owned()),
                ("marker-end", format!("url(#em-arrowhead-{id})")),
                (
                    "d",
                    format!(
                        "M{} {} L{} {}",
                        js_num(sx),
                        js_num(y1),
                        js_num(tx),
                        js_num(y2)
                    ),
                ),
            ],
        );
    }
    let defs = add(&svg, "defs", &[]);
    let marker = add(
        &defs,
        "marker",
        &[
            ("id", format!("em-arrowhead-{id}")),
            ("markerWidth", "10".to_owned()),
            ("markerHeight", "7".to_owned()),
            ("refX", "10".to_owned()),
            ("refY", "3.5".to_owned()),
            ("orient", "auto".to_owned()),
        ],
    );
    add(
        &marker,
        "polygon",
        &[
            ("points", "0 0, 10 3.5, 0 7".to_owned()),
            ("fill", arrowhead),
        ],
    );

    // setupGraphViewbox(padding 30): bounding box of the drawn geometry.
    let padding = config
        .section_f64("eventmodeling", "padding")
        .unwrap_or(30.0);
    let bounds = crate::render::bbox::element_bbox(&svg);
    let f32q = |v: f64| f64::from(v as f32);
    let (bx, by, bw, bh) = if bounds.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (
            f32q(bounds.min_x),
            f32q(bounds.min_y),
            f32q(bounds.width()),
            f32q(bounds.height()),
        )
    };
    let vw = bw + 2.0 * padding;
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
            js_num(bx - padding),
            js_num(by - padding),
            js_num(vw),
            js_num(bh + 2.0 * padding)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "eventmodeling");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// Appends text containing `<br/>` separators and HTML entities.
fn append_html(parent: &Element, html: &str) {
    for (i, part) in html.split("<br/>").enumerate() {
        if i > 0 {
            crate::svg::append_xhtml(parent, "br");
        }
        if !part.is_empty() {
            crate::svg::set_text_append(parent, &crate::flowchart::db::decode_html_entities(part));
        }
    }
}
