//! Sequence diagram renderer (port of `sequenceRenderer.ts` + `svgDraw.js`).
//!
//! # Hand-drawn look (sebastian extension, not in upstream mermaid)
//!
//! Upstream mermaid's `look: handDrawn` (rough.js) is wired into the flowchart
//! and unified-renderer diagram types only; the legacy sequence renderer ported
//! here (`sequenceRenderer.ts` + `svgDraw.js`) never consults `look` and always
//! emits crisp `<rect>`/`<line>`. There is therefore no TypeScript reference for
//! a sketchy sequence diagram.
//!
//! As a sebastian-specific extension, when `config.is_hand_drawn()` is set we
//! route the box and straight-segment primitives through the same
//! [`crate::render::handdrawn`] helpers the flowchart uses
//! ([`hd_polygon`]/[`hd_edge_d`]). Scope, with deliberate exceptions:
//! - sketchy: actor boxes, footer boxes, note boxes, straight message lines, loop
//!   borders;
//! - left crisp (documented): self-message bezier curves, the loop label tab,
//!   thin lifelines, and arrowhead markers — keeping these clean reads better and
//!   avoids wobbling geometry that is resolved lazily (lifeline `y2`).
//!
//! Every hand-drawn branch below is marked `HAND-DRAWN EXTENSION`.

use super::{
    ACTIVE_END, ACTIVE_START, ALT_ELSE, ALT_END, ALT_START, AUTONUMBER, Actor,
    BIDIRECTIONAL_DOTTED, BIDIRECTIONAL_SOLID, BREAK_END, BREAK_START, CRITICAL_END,
    CRITICAL_OPTION, CRITICAL_START, DOTTED, DOTTED_CROSS, DOTTED_OPEN, DOTTED_POINT, LEFTOF,
    LOOP_END, LOOP_START, NOTE, OPT_END, OPT_START, OVER, PAR_AND, PAR_END, PAR_START, RECT_END,
    RECT_START, RIGHTOF, SOLID, SOLID_CROSS, SOLID_OPEN, SOLID_POINT, SeqMessage, SeqParseError,
    SequenceDb,
};
use crate::dagre::types::Point;
use crate::render::handdrawn::{hd_edge_d, hd_polygon, seed_from};
use crate::svg::{
    Element, append, insert_first, js_num, new_element, serialize, set_attr, set_text,
};
use crate::text::{SeqMeasurer, split_breaks};

// Sequence config defaults (after setConf applies the top-level fontSize).
const DIAGRAM_MARGIN_X: f64 = 50.0;
const DIAGRAM_MARGIN_Y: f64 = 10.0;
const ACTOR_MARGIN: f64 = 50.0;
const WIDTH: f64 = 150.0;
const HEIGHT: f64 = 65.0;
const BOX_MARGIN: f64 = 10.0;
const BOX_TEXT_MARGIN: f64 = 5.0;
const NOTE_MARGIN: f64 = 10.0;
const WRAP_PADDING: f64 = 10.0;
const LABEL_BOX_WIDTH: f64 = 50.0;
const LABEL_BOX_HEIGHT: f64 = 20.0;
const BOTTOM_MARGIN_ADJ: f64 = 1.0;
const ACTIVATION_WIDTH: f64 = 10.0;
const FONT_SIZE: f64 = 16.0;

/// JS `Math.round`.
fn round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Trebuchet bbox height for drawn text lines (the drawn SVG inherits the
/// stylesheet font), 16px → 19.
fn drawn_line_height() -> f64 {
    let ascent = (1923.0 * FONT_SIZE / 2048.0).round();
    let descent = (455.0 * FONT_SIZE / 2048.0).round();
    ascent + descent
}

#[derive(Debug, Clone, Default)]
struct MsgModel {
    width: f64,
    height: f64,
    startx: f64,
    stopx: f64,
    starty: f64,
    stopy: f64,
    message: String,
    ty: i32,
    from_bounds: f64,
    to_bounds: f64,
}

#[derive(Debug, Clone, Default)]
struct NoteModel {
    width: f64,
    height: f64,
    startx: f64,
    stopx: f64,
    starty: f64,
    stopy: f64,
    message: String,
}

#[derive(Debug, Clone, Default)]
struct LoopModel {
    startx: Option<f64>,
    starty: f64,
    starty_opt: Option<f64>,
    stopx: Option<f64>,
    stopy: Option<f64>,
    title: String,
    /// Background fill for `rect` blocks.
    fill: Option<String>,
    /// `(y, height)` per section (alt/else, par/and, critical/option).
    sections: Vec<f64>,
    section_titles: Vec<String>,
}

/// An activation in progress (`bounds.activations` entry).
#[derive(Debug, Clone)]
struct Activation {
    startx: f64,
    starty: f64,
    stopx: f64,
    actor: String,
    /// DOM anchor the activation rect is drawn into (z-order).
    anchored: Option<Element>,
}

#[derive(Debug, Default)]
struct Bounds {
    startx: Option<f64>,
    stopx: Option<f64>,
    starty: Option<f64>,
    stopy: Option<f64>,
    vertical_pos: f64,
    sequence_items: Vec<LoopModel>,
    activations: Vec<Activation>,
}

impl Bounds {
    fn update_min(v: &mut Option<f64>, val: f64) {
        *v = Some(v.map_or(val, |x| x.min(val)));
    }
    fn update_max(v: &mut Option<f64>, val: f64) {
        *v = Some(v.map_or(val, |x| x.max(val)));
    }

    fn update_bounds(&mut self, startx: f64, starty: f64, stopx: f64, stopy: f64) {
        let total = self.sequence_items.len();
        for (cnt, item) in self.sequence_items.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let n = (total - cnt) as f64;
            Self::update_min(item_starty(item), starty - n * BOX_MARGIN);
            Self::update_max(&mut item.stopy, stopy + n * BOX_MARGIN);
            Self::update_min(&mut self.startx, startx - n * BOX_MARGIN);
            Self::update_max(&mut self.stopx, stopx + n * BOX_MARGIN);
            Self::update_min(&mut item.startx, startx - n * BOX_MARGIN);
            Self::update_max(&mut item.stopx, stopx + n * BOX_MARGIN);
            Self::update_min(&mut self.starty, starty - n * BOX_MARGIN);
            Self::update_max(&mut self.stopy, stopy + n * BOX_MARGIN);
        }
        // updateBounds visits open activations with the same shared counter,
        // so n keeps decreasing past the loop stack (0, -1, ...). Activations
        // only take the item starty min plus the diagram startx/stopx.
        // The shared counter works out to n = -idx for the idx-th activation.
        for (idx, act) in self.activations.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let n = -(idx as f64);
            act.starty = act.starty.min(starty - n * BOX_MARGIN);
            Self::update_min(&mut self.startx, startx - n * BOX_MARGIN);
            Self::update_max(&mut self.stopx, stopx + n * BOX_MARGIN);
        }
    }

    fn insert(&mut self, startx: f64, starty: f64, stopx: f64, stopy: f64) {
        let sx = startx.min(stopx);
        let ex = startx.max(stopx);
        let sy = starty.min(stopy);
        let ey = starty.max(stopy);
        Self::update_min(&mut self.startx, sx);
        Self::update_min(&mut self.starty, sy);
        Self::update_max(&mut self.stopx, ex);
        Self::update_max(&mut self.stopy, ey);
        self.update_bounds(sx, sy, ex, ey);
    }

    fn bump_vertical_pos(&mut self, bump: f64) {
        self.vertical_pos += bump;
        Self::update_max(&mut self.stopy, self.vertical_pos);
    }
}

/// `item.starty` is plain f64 with a sentinel; wrap access so updateVal's
/// undefined-handling matches. We model starty as Option via stopy-like
/// helper on a parallel field.
fn item_starty(item: &mut LoopModel) -> &mut Option<f64> {
    // LoopModel.starty is set at creation (defined), so updateVal's min path
    // always applies; model it as Option that starts Some.
    &mut item.starty_opt
}

#[derive(Debug, Clone, Default)]
struct TextData<'a> {
    x: f64,
    y: f64,
    width: Option<f64>,
    anchor: Option<&'a str>,
    text: String,
    class: Option<&'a str>,
    dy: Option<&'a str>,
    tspan: bool,
    text_margin: Option<f64>,
    valign_center: bool,
}

/// `drawText`: appends one `<text>` per line; returns the drawn line count.
#[allow(clippy::too_many_lines)]
fn draw_text(parent: &Element, td: &mut TextData<'_>) -> usize {
    let lines = split_breaks(&td.text);
    let mut prev_text_height = 0.0f64;
    let mut text_height = 0.0f64;
    let lh = drawn_line_height();

    let mut anchor = td.anchor;
    let mut baselines = false;
    if let (Some(_), Some(margin), Some(width)) = (anchor, td.text_margin, td.width) {
        match anchor {
            Some("left" | "start") => {
                td.x = round(td.x + margin);
                anchor = Some("start");
            }
            Some("middle" | "center") => {
                td.x = round(td.x + width / 2.0);
                anchor = Some("middle");
            }
            Some("right" | "end") => {
                td.x = round(td.x + width - margin);
                anchor = Some("end");
            }
            _ => {}
        }
        baselines = true;
    }

    for line in &lines {
        let y = if td.valign_center && td.text_margin.is_some_and(|m| m > 0.0) {
            round(td.y + (prev_text_height + text_height + td.text_margin.expect("checked")) / 2.0)
        } else {
            td.y
        };

        let text_el = append(parent, "text");
        set_attr(&text_el, "x", js_num(td.x));
        set_attr(&text_el, "y", js_num(y));
        if let Some(a) = anchor {
            set_attr(&text_el, "text-anchor", a);
            if baselines {
                set_attr(&text_el, "dominant-baseline", "middle");
                set_attr(&text_el, "alignment-baseline", "middle");
            }
        }
        if let Some(c) = td.class {
            set_attr(&text_el, "class", c);
        }
        if let Some(dy) = td.dy {
            set_attr(&text_el, "dy", dy);
        }
        // CSSOM-built style serializes last.
        set_attr(&text_el, "style", "font-size: 16px; font-weight: 400;");

        if td.tspan {
            let span = append(&text_el, "tspan");
            set_attr(&span, "x", js_num(td.x));
            set_text(&span, line);
        } else {
            set_text(&text_el, line);
        }

        if td.valign_center && td.text_margin.is_some_and(|m| m > 0.0) {
            text_height += lh;
            prev_text_height = text_height;
        }
    }
    lines.len()
}

/// `svgDrawCommon.drawRect`.
struct RectData<'a> {
    x: f64,
    y: f64,
    fill: &'a str,
    stroke: &'a str,
    width: f64,
    height: f64,
    name: Option<&'a str>,
    rx: f64,
    class: &'a str,
}

fn draw_rect(parent: &Element, r: &RectData<'_>, hand_drawn: bool) -> Element {
    // HAND-DRAWN EXTENSION: render the box as a sketchy filled polygon (rounded
    // corners are dropped — sketchy boxes are square). Returns the `<g>` wrapper;
    // `rx` and post-hoc `height` edits do not apply, so callers that resize after
    // text layout (notes) must compute the height before calling this.
    if hand_drawn {
        let pts = [
            Point { x: r.x, y: r.y },
            Point {
                x: r.x + r.width,
                y: r.y,
            },
            Point {
                x: r.x + r.width,
                y: r.y + r.height,
            },
            Point {
                x: r.x,
                y: r.y + r.height,
            },
        ];
        let g = hd_polygon(parent, &pts, r.fill, r.stroke, "1", "", seed_from(r.x, r.y));
        set_attr(&g, "class", r.class);
        if let Some(n) = r.name {
            set_attr(&g, "name", n);
        }
        return g;
    }
    let rect = append(parent, "rect");
    set_attr(&rect, "x", js_num(r.x));
    set_attr(&rect, "y", js_num(r.y));
    set_attr(&rect, "fill", r.fill);
    set_attr(&rect, "stroke", r.stroke);
    set_attr(&rect, "width", js_num(r.width));
    set_attr(&rect, "height", js_num(r.height));
    if let Some(n) = r.name {
        set_attr(&rect, "name", n);
    }
    if r.rx != 0.0 {
        set_attr(&rect, "rx", js_num(r.rx));
        set_attr(&rect, "ry", js_num(r.rx));
    }
    set_attr(&rect, "class", r.class);
    rect
}

/// A straight segment between two points. Crisp mode emits a `<line>`; the
/// HAND-DRAWN EXTENSION emits a sketchy `<path>` (a single rough pass via
/// [`hd_edge_d`]). Either way the returned element accepts the same `class`,
/// `stroke`, `marker-end`, and `style` attributes the callers set afterward.
fn draw_segment(parent: &Element, x1: f64, y1: f64, x2: f64, y2: f64, hand_drawn: bool) -> Element {
    if hand_drawn {
        let el = append(parent, "path");
        let pts = [Point { x: x1, y: y1 }, Point { x: x2, y: y2 }];
        set_attr(&el, "d", hd_edge_d(&pts, seed_from(x1, y1)));
        el
    } else {
        let el = append(parent, "line");
        set_attr(&el, "x1", js_num(x1));
        set_attr(&el, "y1", js_num(y1));
        set_attr(&el, "x2", js_num(x2));
        set_attr(&el, "y2", js_num(y2));
        el
    }
}

/// Inserts a defs>symbol icon.
fn insert_icon(svg: &Element, id: &str, suffix: &str, wh: bool, fill_rule: bool, d: &str) {
    let defs = append(svg, "defs");
    let symbol = append(&defs, "symbol");
    set_attr(&symbol, "id", format!("{id}-{suffix}"));
    if wh {
        set_attr(&symbol, "width", "24");
        set_attr(&symbol, "height", "24");
    }
    if fill_rule {
        set_attr(&symbol, "fill-rule", "evenodd");
        set_attr(&symbol, "clip-rule", "evenodd");
    }
    let path = append(&symbol, "path");
    set_attr(&path, "transform", "scale(.5)");
    set_attr(&path, "d", d);
}

/// `utils.wrapLabel` (minimal: returns the label when it fits).
fn wrap_label(label: &str, max_width: f64, measurer: &SeqMeasurer) -> String {
    let (w, _) = measurer.text_dimensions(label, FONT_SIZE);
    if w <= max_width {
        return label.to_owned();
    }
    // Greedy word wrap joined with <br/> (utils.wrapLabel).
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in label.split(' ') {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        let (cw, _) = measurer.text_dimensions(&candidate, FONT_SIZE);
        if cw <= max_width || current.is_empty() {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            word.clone_into(&mut current);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.join("<br/>")
}

/// Renders mermaid sequenceDiagram source to a complete SVG document.
///
/// # Errors
/// Returns [`SeqParseError`] when the source is not a valid sequence diagram.
#[allow(clippy::too_many_lines)]
pub fn render_sequence(source: &str, id: &str) -> Result<String, SeqParseError> {
    let mut db = super::parse(source)?;
    let measurer = SeqMeasurer::new();
    let config = crate::render::config::detect_init(source);
    // HAND-DRAWN EXTENSION: sketchy shapes when `look: handDrawn` is set (see
    // module docs — this has no upstream sequence-renderer equivalent).
    let hand_drawn = config.is_hand_drawn();
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);

    // SVG scaffold.
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    set_attr(&svg, "style", "");
    set_attr(&svg, "viewBox", "");
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "sequence");

    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_sequence_css(id, &theme_vars),
    );
    let _empty_g = append(&svg, "g");

    insert_icon(&svg, id, "computer", true, false, COMPUTER_D);
    insert_icon(&svg, id, "database", false, true, DATABASE_D);
    insert_icon(&svg, id, "clock", true, false, CLOCK_D);

    // --- getMaxMessageWidthPerActor ---
    let mut max_message_width: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    {
        let mut bump = |k: &str, w: f64| {
            let e = max_message_width.entry(k.to_owned()).or_insert(0.0);
            if w > *e {
                *e = w;
            }
        };
        for msg in &db.messages {
            if !db.actors.contains_key(&msg.to) || !db.actors.contains_key(&msg.from) {
                continue;
            }
            let actor = &db.actors[&msg.to];
            if msg.placement == Some(LEFTOF) && actor.prev_actor.is_none() {
                continue;
            }
            if msg.placement == Some(RIGHTOF) && actor.next_actor.is_none() {
                continue;
            }
            let is_note = msg.placement.is_some();
            let (w, _) = measurer.text_dimensions(&msg.message, FONT_SIZE);
            let message_width = w + 2.0 * WRAP_PADDING;
            if !is_note && Some(&msg.from) == actor.next_actor.as_ref() {
                bump(&msg.to, message_width);
            } else if !is_note && Some(&msg.from) == actor.prev_actor.as_ref() {
                bump(&msg.from, message_width);
            } else if !is_note && msg.from == msg.to {
                bump(&msg.from, message_width / 2.0);
                bump(&msg.to, message_width / 2.0);
            } else if msg.placement == Some(RIGHTOF) {
                bump(&msg.from, message_width);
            } else if msg.placement == Some(LEFTOF) {
                if let Some(prev) = actor.prev_actor.clone() {
                    bump(&prev, message_width);
                }
            } else if msg.placement == Some(OVER) {
                if let Some(prev) = actor.prev_actor.clone() {
                    bump(&prev, message_width / 2.0);
                }
                if actor.next_actor.is_some() {
                    bump(&msg.from, message_width / 2.0);
                }
            }
        }
    }

    // --- calculateActorMargins ---
    let mut conf_height = HEIGHT;
    {
        let mut max_height = 0.0f64;
        let keys: Vec<String> = db.actors.keys().cloned().collect();
        for k in &keys {
            let (w, _) = measurer.text_dimensions(&db.actors[k].description, FONT_SIZE);
            let a = &mut db.actors[k];
            a.width = WIDTH.max(w + 2.0 * WRAP_PADDING);
            a.height = HEIGHT;
            max_height = max_height.max(a.height);
        }
        for k in &keys {
            db.actors[k].margin = ACTOR_MARGIN;
        }
        for (k, mw) in &max_message_width {
            if !db.actors.contains_key(k) {
                continue;
            }
            let next = db.actors[k].next_actor.clone();
            let self_width = db.actors[k].width;
            let margin = if let Some(n) = next.and_then(|n| db.actors.get(&n).map(|a| a.width)) {
                (mw + ACTOR_MARGIN - self_width / 2.0 - n / 2.0).max(ACTOR_MARGIN)
            } else {
                (mw + ACTOR_MARGIN - self_width / 2.0).max(ACTOR_MARGIN)
            };
            db.actors[k].margin = margin;
        }
        conf_height = conf_height.max(max_height);
    }

    // Box layout data, parallel to db.boxes.
    #[derive(Debug, Clone, Default)]
    struct BoxRender {
        x: f64,
        y: f64,
        width: f64,
        margin: f64,
    }
    let mut box_rd: Vec<BoxRender> = vec![BoxRender::default(); db.boxes.len()];
    let has_boxes = !db.boxes.is_empty();
    let has_box_titles = db.boxes.iter().any(|b| b.name.is_some());
    let mut box_text_max_height = 0.0f64;
    {
        for b in &db.boxes {
            let name = b.name.clone().unwrap_or_default();
            let (_, h) = measurer.text_dimensions(&name, FONT_SIZE);
            box_text_max_height = box_text_max_height.max(h);
        }
        for (i, b) in db.boxes.iter().enumerate() {
            let mut total_width: f64 = b
                .actor_keys
                .iter()
                .map(|k| db.actors[k].width + db.actors[k].margin)
                .sum();
            total_width += BOX_MARGIN * 8.0;
            total_width -= 2.0 * BOX_TEXT_MARGIN;
            let name = b.name.clone().unwrap_or_default();
            let (bw, _) = measurer.text_dimensions(&name, FONT_SIZE);
            let min_width = total_width.max(bw + 2.0 * WRAP_PADDING);
            box_rd[i].margin = BOX_TEXT_MARGIN;
            if total_width < min_width {
                box_rd[i].margin += (min_width - total_width) / 2.0;
            }
        }
    }

    let mut bounds = Bounds::default();
    if has_boxes {
        bounds.bump_vertical_pos(BOX_MARGIN);
        if has_box_titles {
            bounds.bump_vertical_pos(box_text_max_height);
        }
    }

    // --- addActorRenderingData ---
    // Boxes in the order they close (bounds.models.addBox).
    let mut box_order: Vec<usize> = Vec::new();
    {
        let mut prev_width = 0.0f64;
        let mut prev_margin = 0.0f64;
        let mut max_height = 0.0f64;
        let mut prev_box: Option<usize> = None;
        let keys: Vec<String> = db.actors.keys().cloned().collect();
        for k in &keys {
            let bi = db.actors[k].box_index;

            // end of box
            if let Some(pb) = prev_box {
                if prev_box != bi {
                    box_order.push(pb);
                    prev_margin += BOX_MARGIN + box_rd[pb].margin;
                }
            }
            // new box
            if let Some(i) = bi {
                if bi != prev_box {
                    box_rd[i].x = prev_width + prev_margin;
                    box_rd[i].y = 0.0;
                    prev_margin += box_rd[i].margin;
                }
            }

            let a = &mut db.actors[k];
            a.width = a.width.max(WIDTH);
            a.height = a.height.max(conf_height);
            if a.margin == 0.0 {
                a.margin = ACTOR_MARGIN;
            }
            max_height = max_height.max(a.height);
            a.x = prev_width + prev_margin;
            a.starty = bounds.vertical_pos;
            let (x, w, h) = (a.x, a.width, a.height);
            let m = a.margin;
            bounds.insert(x, 0.0, x + w, h);
            prev_width += w + prev_margin;
            if let Some(i) = bi {
                box_rd[i].width = prev_width + box_rd[i].margin - box_rd[i].x;
            }
            prev_margin = m;
            prev_box = bi;
        }
        if let Some(pb) = prev_box {
            box_order.push(pb);
        }
        bounds.bump_vertical_pos(max_height);
    }

    // --- calculateLoopBounds (loop widths) + per-message models ---
    let mut msg_models: std::collections::HashMap<String, MsgModel> =
        std::collections::HashMap::new();
    let mut note_models: std::collections::HashMap<String, NoteModel> =
        std::collections::HashMap::new();
    let mut loop_widths: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    {
        struct Stk {
            id: String,
            from: f64,
            to: f64,
            width: f64,
        }
        let mut stack: Vec<Stk> = Vec::new();
        let mut activations: Vec<Activation> = Vec::new();
        for msg in &db.messages {
            match msg.ty {
                LOOP_START | ALT_START | OPT_START | PAR_START | CRITICAL_START | BREAK_START => {
                    stack.push(Stk {
                        id: msg.id.clone(),
                        from: f64::MAX,
                        to: f64::MIN,
                        width: 0.0,
                    });
                }
                ALT_ELSE | PAR_AND | CRITICAL_OPTION => {
                    if !msg.message.is_empty()
                        && let Some(cur) = stack.pop()
                    {
                        loop_widths.insert(cur.id.clone(), cur.width);
                        loop_widths.insert(msg.id.clone(), cur.width);
                        stack.push(cur);
                    }
                }
                LOOP_END | ALT_END | OPT_END | PAR_END | CRITICAL_END | BREAK_END => {
                    if let Some(cur) = stack.pop() {
                        loop_widths.insert(cur.id.clone(), cur.width);
                    }
                }
                ACTIVE_START => {
                    let actor = &db.actors[&msg.from];
                    #[allow(clippy::cast_precision_loss)]
                    let stacked = activations.iter().filter(|a| a.actor == msg.from).count() as f64;
                    let x = actor.x + actor.width / 2.0 + (stacked - 1.0) * ACTIVATION_WIDTH / 2.0;
                    activations.push(Activation {
                        startx: x,
                        starty: 0.0,
                        stopx: x + ACTIVATION_WIDTH,
                        actor: msg.from.clone(),
                        anchored: None,
                    });
                }
                ACTIVE_END => {
                    if let Some(idx) = activations.iter().rposition(|a| a.actor == msg.from) {
                        activations.remove(idx);
                    }
                }
                _ => {}
            }
            let is_note = msg.placement.is_some();
            if is_note {
                let nm = build_note_model(msg, &db, &measurer);
                for stk in &mut stack {
                    stk.from = stk.from.min(nm.startx);
                    stk.to = stk.to.max(nm.startx + nm.width);
                    stk.width = stk.width.max((stk.from - stk.to).abs()) - LABEL_BOX_WIDTH;
                }
                note_models.insert(msg.id.clone(), nm);
            } else if is_line_type(msg.ty) {
                let mm = build_message_model(msg, &db, &measurer, &activations);
                if mm.startx != 0.0 && mm.stopx != 0.0 && !stack.is_empty() {
                    for stk in &mut stack {
                        if (mm.startx - mm.stopx).abs() < f64::EPSILON {
                            let from = &db.actors[&msg.from];
                            let to = &db.actors[&msg.to];
                            stk.from = (from.x - mm.width / 2.0)
                                .min(from.x - from.width / 2.0)
                                .min(stk.from);
                            stk.to = (to.x + mm.width / 2.0)
                                .max(to.x + from.width / 2.0)
                                .max(stk.to);
                            stk.width = stk.width.max((stk.to - stk.from).abs()) - LABEL_BOX_WIDTH;
                        } else {
                            stk.from = mm.startx.min(stk.from);
                            stk.to = mm.stopx.max(stk.to);
                            stk.width = stk.width.max(mm.width) - LABEL_BOX_WIDTH;
                        }
                    }
                }
                msg_models.insert(msg.id.clone(), mm);
            }
        }
    }

    // Arrowhead/marker defs.
    insert_markers(&svg, id);

    // --- main event loop ---
    struct QueuedMessage {
        model: MsgModel,
        line_start_y: f64,
        id: String,
        from: String,
        to: String,
        sequence_index: f64,
        sequence_visible: bool,
    }
    // adjustLoopHeightForWrap: bumps pre, runs f, bumps post (+ wrapped title
    // height when the block has a title).
    let adjust_block =
        |bounds: &mut Bounds, msg: &SeqMessage, pre: f64, post: f64| -> (String, f64) {
            bounds.bump_vertical_pos(pre);
            let mut height_adjust = post;
            let mut title = msg.message.clone();
            if !msg.message.is_empty()
                && let Some(lw) = loop_widths.get(&msg.id)
            {
                title = wrap_label(
                    &format!("[{}]", msg.message),
                    lw - 2.0 * WRAP_PADDING,
                    &measurer,
                );
                let (_, th) = measurer.text_dimensions(&title, FONT_SIZE);
                let total_offset = th.max(LABEL_BOX_HEIGHT);
                height_adjust = post + total_offset;
            }
            (title, height_adjust)
        };
    let mut to_draw: Vec<QueuedMessage> = Vec::new();
    let mut backgrounds: Vec<LoopModel> = Vec::new();
    let mut sequence_index = 1.0f64;
    let mut sequence_step = 1.0f64;
    let mut show_numbers = false;
    bounds.activations.clear();
    let messages = db.messages.clone();
    for msg in &messages {
        match msg.ty {
            NOTE => {
                let mut nm = note_models.get(&msg.id).cloned().unwrap_or_default();
                draw_note(&svg, &mut nm, &msg.id, &mut bounds, hand_drawn);
            }
            AUTONUMBER => {
                if let Some((start, step, visible)) = msg.autonumber {
                    if start != 0.0 {
                        sequence_index = start;
                    }
                    if step != 0.0 {
                        sequence_step = step;
                    }
                    show_numbers = visible;
                }
            }
            ACTIVE_START => {
                // bounds.newActivation
                let actor = &db.actors[&msg.from];
                #[allow(clippy::cast_precision_loss)]
                let stacked = bounds
                    .activations
                    .iter()
                    .filter(|a| a.actor == msg.from)
                    .count() as f64;
                let x = actor.x + actor.width / 2.0 + (stacked - 1.0) * ACTIVATION_WIDTH / 2.0;
                let anchored = append(&svg, "g");
                bounds.activations.push(Activation {
                    startx: x,
                    starty: bounds.vertical_pos + 2.0,
                    stopx: x + ACTIVATION_WIDTH,
                    actor: msg.from.clone(),
                    anchored: Some(anchored),
                });
            }
            ACTIVE_END => {
                if let Some(idx) = bounds.activations.iter().rposition(|a| a.actor == msg.from) {
                    let data = bounds.activations.remove(idx);
                    let mut vertical_pos = bounds.vertical_pos;
                    let mut starty = data.starty;
                    if starty + 18.0 > vertical_pos {
                        starty = vertical_pos - 6.0;
                        vertical_pos += 12.0;
                    }
                    let remaining = bounds
                        .activations
                        .iter()
                        .filter(|a| a.actor == msg.from)
                        .count();
                    if let Some(anchor) = &data.anchored {
                        draw_activation(anchor, &data, starty, vertical_pos, remaining);
                    }
                    bounds.insert(data.startx, vertical_pos - 10.0, data.stopx, vertical_pos);
                }
            }
            LOOP_START | ALT_START | OPT_START | PAR_START | CRITICAL_START | BREAK_START => {
                let (title, height_adjust) =
                    adjust_block(&mut bounds, msg, BOX_MARGIN, BOX_MARGIN + BOX_TEXT_MARGIN);
                bounds.sequence_items.push(LoopModel {
                    starty: bounds.vertical_pos,
                    starty_opt: Some(bounds.vertical_pos),
                    title,
                    ..LoopModel::default()
                });
                bounds.bump_vertical_pos(height_adjust);
            }
            ALT_ELSE | PAR_AND | CRITICAL_OPTION => {
                let (title, height_adjust) =
                    adjust_block(&mut bounds, msg, BOX_MARGIN + BOX_TEXT_MARGIN, BOX_MARGIN);
                if let Some(cur) = bounds.sequence_items.last_mut() {
                    cur.sections.push(bounds.vertical_pos);
                    cur.section_titles.push(title);
                }
                bounds.bump_vertical_pos(height_adjust);
            }
            RECT_START => {
                let (_, height_adjust) = adjust_block(&mut bounds, msg, BOX_MARGIN, BOX_MARGIN);
                bounds.sequence_items.push(LoopModel {
                    starty: bounds.vertical_pos,
                    starty_opt: Some(bounds.vertical_pos),
                    fill: Some(msg.message.clone()),
                    ..LoopModel::default()
                });
                bounds.bump_vertical_pos(height_adjust);
            }
            RECT_END => {
                if let Some(model) = bounds.sequence_items.pop() {
                    let stopy = model.stopy.unwrap_or(bounds.vertical_pos);
                    backgrounds.push(model);
                    bounds.bump_vertical_pos(stopy - bounds.vertical_pos);
                }
            }
            LOOP_END | ALT_END | OPT_END | PAR_END | CRITICAL_END | BREAK_END => {
                if let Some(model) = bounds.sequence_items.pop() {
                    let label = match msg.ty {
                        ALT_END => "alt",
                        OPT_END => "opt",
                        PAR_END => "par",
                        CRITICAL_END => "critical",
                        BREAK_END => "break",
                        _ => "loop",
                    };
                    draw_loop(&svg, &model, label, &msg.id, hand_drawn);
                    let stopy = model.stopy.unwrap_or(bounds.vertical_pos);
                    bounds.bump_vertical_pos(stopy - bounds.vertical_pos);
                }
            }
            t if is_line_type(t) => {
                let mut model = msg_models.get(&msg.id).cloned().unwrap_or_default();
                model.starty = bounds.vertical_pos;
                let line_start_y = bound_message(&mut model, &measurer, &mut bounds);
                to_draw.push(QueuedMessage {
                    model: model.clone(),
                    line_start_y,
                    id: msg.id.clone(),
                    from: msg.from.clone(),
                    to: msg.to.clone(),
                    sequence_index,
                    sequence_visible: show_numbers,
                });
                sequence_index = ((sequence_index + sequence_step) * 100.0).round() / 100.0;
            }
            _ => {}
        }
    }

    // --- drawActors (top) ---
    let mut actor_cnt: usize = 0;
    let keys: Vec<String> = db.actors.keys().cloned().collect();
    for k in &keys {
        let a = &mut db.actors[k];
        draw_actor_top(&svg, a, &mut actor_cnt, hand_drawn);
    }

    // --- draw queued messages ---
    for q in &to_draw {
        draw_message(
            &svg,
            &q.model,
            q.line_start_y,
            &q.id,
            &q.from,
            &q.to,
            id,
            hand_drawn,
            q.sequence_index,
            q.sequence_visible,
        );
    }

    // --- drawActors (footer, mirrorActors) ---
    bounds.bump_vertical_pos(BOX_MARGIN * 2.0);
    let mut max_footer_height = 0.0f64;
    for k in &keys {
        let a = &mut db.actors[k];
        if a.stopy.is_none() {
            a.stopy = Some(bounds.vertical_pos);
        }
        if a.is_actor_man {
            draw_actor_man(&svg, a, &mut actor_cnt, true);
        } else {
            draw_actor_footer(&svg, a, hand_drawn);
        }
        max_footer_height = max_footer_height.max(a.height);
    }
    bounds.bump_vertical_pos(max_footer_height + BOX_MARGIN);

    // --- backgrounds (rect blocks), prepended via .lower() ---
    for bg in &backgrounds {
        draw_background_rect(&svg, bg);
    }

    // --- fixLifeLineHeights ---
    for k in &keys {
        let a = &db.actors[k];
        if let (Some(cnt), Some(stopy)) = (a.actor_cnt, a.stopy) {
            // The line is inside the prepended top-actor group; find it by id.
            fix_lifeline(&svg, cnt, stopy);
        }
    }

    // --- draw boxes ---
    for &i in &box_order {
        let b = &box_rd[i];
        let height = bounds.vertical_pos - b.y;
        bounds.insert(b.x, b.y, b.x + b.width, height);
        let box_padding = BOX_MARGIN * 2.0;
        let startx = b.x - box_padding;
        let starty = b.y - box_padding * 0.25;
        let stopx = startx + b.width + 2.0 * box_padding;
        let stopy = starty + height + box_padding * 0.75;
        let g = insert_first(&svg, "g"); // append + g.lower()
        let rect = append(&g, "rect");
        set_attr(&rect, "x", js_num(startx));
        set_attr(&rect, "y", js_num(starty));
        set_attr(&rect, "fill", db.boxes[i].fill.clone());
        set_attr(&rect, "stroke", "rgb(0,0,0, 0.5)");
        set_attr(&rect, "width", js_num(stopx - startx));
        set_attr(&rect, "height", js_num(stopy - starty));
        set_attr(&rect, "class", "rect");
        if let Some(name) = &db.boxes[i].name {
            by_tspan(
                &g,
                name,
                b.x + b.width / 2.0,
                b.y + BOX_TEXT_MARGIN + box_text_max_height / 2.0,
                "text",
            );
        }
    }
    if has_boxes {
        bounds.bump_vertical_pos(BOX_MARGIN);
    }

    // --- viewport ---
    let startx = bounds.startx.unwrap_or(0.0);
    let stopx = bounds.stopx.unwrap_or(0.0);
    let starty = bounds.starty.unwrap_or(0.0);
    let stopy = bounds.stopy.unwrap_or(0.0);
    let box_height = stopy - starty;
    let mut height = box_height + 2.0 * DIAGRAM_MARGIN_Y;
    // mirrorActors:
    height = height - BOX_MARGIN + BOTTOM_MARGIN_ADJ;
    let width = stopx - startx + 2.0 * DIAGRAM_MARGIN_X;

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
            "{} -{} {} {}",
            js_num(startx - DIAGRAM_MARGIN_X),
            js_num(DIAGRAM_MARGIN_Y),
            js_num(width),
            js_num(height)
        ),
    );

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

fn is_line_type(t: i32) -> bool {
    matches!(
        t,
        SOLID
            | DOTTED
            | SOLID_OPEN
            | DOTTED_OPEN
            | SOLID_CROSS
            | DOTTED_CROSS
            | SOLID_POINT
            | DOTTED_POINT
            | BIDIRECTIONAL_SOLID
            | BIDIRECTIONAL_DOTTED
    )
}

/// `buildMessageModel`.
/// `activationBounds`.
fn activation_bounds(actor_name: &str, db: &SequenceDb, activations: &[Activation]) -> (f64, f64) {
    let actor = &db.actors[actor_name];
    let center = actor.x + actor.width / 2.0;
    let mut left = center - 1.0;
    let mut right = center + 1.0;
    for a in activations.iter().filter(|a| a.actor == actor_name) {
        left = left.min(a.startx);
        right = right.max(a.stopx);
    }
    (left, right)
}

fn build_message_model(
    msg: &SeqMessage,
    db: &SequenceDb,
    measurer: &SeqMeasurer,
    activations: &[Activation],
) -> MsgModel {
    let (from_left, from_right) = activation_bounds(&msg.from, db, activations);
    let (to_left, to_right) = activation_bounds(&msg.to, db, activations);
    let is_arrow_to_right = from_left <= to_left;
    let mut startx = if is_arrow_to_right {
        from_right
    } else {
        from_left
    };
    let mut stopx = if is_arrow_to_right { to_left } else { to_right };
    let adjust = |v: f64| if is_arrow_to_right { -v } else { v };

    let is_arrow_to_activation = (to_left - to_right).abs() > 2.0;
    if msg.from == msg.to {
        stopx = startx;
    } else {
        if msg.activate && !is_arrow_to_activation {
            stopx += adjust(ACTIVATION_WIDTH / 2.0 - 1.0);
        }
        if !matches!(msg.ty, SOLID_OPEN | DOTTED_OPEN) {
            stopx += adjust(3.0);
        }
        if matches!(msg.ty, BIDIRECTIONAL_SOLID | BIDIRECTIONAL_DOTTED) {
            startx -= adjust(3.0);
        }
    }

    let all_bounds = [from_left, from_right, to_left, to_right];
    let bounded_width = (startx - stopx).abs();
    let (msg_w, _) = measurer.text_dimensions(&msg.message, FONT_SIZE);
    let width = (msg_w + 2.0 * WRAP_PADDING)
        .max(bounded_width + 2.0 * WRAP_PADDING)
        .max(WIDTH);

    MsgModel {
        width,
        height: 0.0,
        startx,
        stopx,
        starty: 0.0,
        stopy: 0.0,
        message: msg.message.clone(),
        ty: msg.ty,
        from_bounds: all_bounds.iter().copied().fold(f64::MAX, f64::min),
        to_bounds: all_bounds.iter().copied().fold(f64::MIN, f64::max),
    }
}

/// `buildNoteModel`.
fn build_note_model(msg: &SeqMessage, db: &SequenceDb, measurer: &SeqMeasurer) -> NoteModel {
    let from = &db.actors[&msg.from];
    let to = &db.actors[&msg.to];
    let startx = from.x;
    let stopx = to.x;
    let (tw, _) = measurer.text_dimensions(&msg.message, FONT_SIZE);

    #[allow(unused_assignments)]
    let mut width = WIDTH.max(tw + 2.0 * NOTE_MARGIN);
    #[allow(unused_assignments)]
    let mut nstartx = from.x;
    if msg.placement == Some(RIGHTOF) {
        width = (from.width / 2.0 + to.width / 2.0).max(tw + 2.0 * NOTE_MARGIN);
        nstartx = startx + f64::midpoint(from.width, ACTOR_MARGIN);
    } else if msg.placement == Some(LEFTOF) {
        width = (from.width / 2.0 + to.width / 2.0).max(tw + 2.0 * NOTE_MARGIN);
        nstartx = startx - width + (from.width - ACTOR_MARGIN) / 2.0;
    } else if msg.to == msg.from {
        width = from.width.max(WIDTH).max(tw + 2.0 * NOTE_MARGIN);
        nstartx = startx + (from.width - width) / 2.0;
    } else {
        width = (startx + from.width / 2.0 - (stopx + to.width / 2.0)).abs() + ACTOR_MARGIN;
        nstartx = if startx < stopx {
            startx + from.width / 2.0 - ACTOR_MARGIN / 2.0
        } else {
            stopx + to.width / 2.0 - ACTOR_MARGIN / 2.0
        };
    }

    NoteModel {
        width,
        height: 0.0,
        startx: nstartx,
        stopx: 0.0,
        starty: 0.0,
        stopy: 0.0,
        message: msg.message.clone(),
    }
}

/// `drawNote`.
fn draw_note(svg: &Element, nm: &mut NoteModel, id: &str, bounds: &mut Bounds, hand_drawn: bool) {
    bounds.bump_vertical_pos(BOX_MARGIN);
    nm.height = BOX_MARGIN;
    nm.starty = bounds.vertical_pos;

    let g = append(svg, "g");
    set_attr(&g, "data-et", "note");
    set_attr(&g, "data-id", format!("i{id}"));

    let mut td = TextData {
        x: nm.startx,
        y: nm.starty,
        width: Some(nm.width),
        anchor: Some("center"),
        text: nm.message.clone(),
        class: Some("noteText"),
        dy: Some("1em"),
        tspan: true,
        text_margin: Some(NOTE_MARGIN),
        valign_center: true,
    };

    let note_rect = |g: &Element, height: f64| {
        draw_rect(
            g,
            &RectData {
                x: nm.startx,
                y: nm.starty,
                fill: "#EDF2AE",
                stroke: "#666",
                width: nm.width,
                height,
                name: None,
                rx: 0.0,
                class: "note",
            },
            hand_drawn,
        )
    };

    #[allow(clippy::cast_precision_loss)]
    let text_height = if hand_drawn {
        // HAND-DRAWN EXTENSION: a sketchy box can't be resized after creation, so
        // lay out the text first to learn its height, then draw the box at the
        // final size. `hd_polygon` inserts itself first, so it still sits behind
        // the text despite being added afterward.
        let lines = draw_text(&g, &mut td);
        let text_height = round(drawn_line_height() * lines as f64);
        note_rect(&g, text_height + 2.0 * NOTE_MARGIN);
        text_height
    } else {
        let rect = note_rect(&g, 100.0);
        let lines = draw_text(&g, &mut td);
        let text_height = round(drawn_line_height() * lines as f64);
        set_attr(&rect, "height", js_num(text_height + 2.0 * NOTE_MARGIN));
        text_height
    };
    nm.height += text_height + 2.0 * NOTE_MARGIN;
    bounds.bump_vertical_pos(text_height + 2.0 * NOTE_MARGIN);
    nm.stopy = nm.starty + text_height + 2.0 * NOTE_MARGIN;
    nm.stopx = nm.startx + nm.width;
    bounds.insert(nm.startx, nm.starty, nm.stopx, nm.stopy);
}

/// `boundMessage`.
fn bound_message(model: &mut MsgModel, measurer: &SeqMeasurer, bounds: &mut Bounds) -> f64 {
    bounds.bump_vertical_pos(10.0);
    let (startx, stopx) = (model.startx, model.stopx);
    #[allow(clippy::cast_precision_loss)]
    let lines = split_breaks(&model.message).len() as f64;
    let (tw, th) = measurer.text_dimensions(&model.message, FONT_SIZE);
    let line_height = th / lines;
    model.height += line_height;
    bounds.bump_vertical_pos(line_height);

    let line_start_y;
    let mut total_offset = th - 10.0;
    let text_width = tw;

    if (startx - stopx).abs() < f64::EPSILON {
        // self message (rightAngles false)
        total_offset += BOX_MARGIN;
        line_start_y = bounds.vertical_pos + total_offset;
        total_offset += 30.0;
        let dx = (text_width / 2.0).max(WIDTH / 2.0);
        bounds.insert(
            startx - dx,
            bounds.vertical_pos - 10.0 + total_offset,
            stopx + dx,
            bounds.vertical_pos + 30.0 + total_offset,
        );
    } else {
        total_offset += BOX_MARGIN;
        line_start_y = bounds.vertical_pos + total_offset;
        bounds.insert(startx, line_start_y - 10.0, stopx, line_start_y);
    }
    bounds.bump_vertical_pos(total_offset);
    model.height += total_offset;
    model.stopy = model.starty + model.height;
    bounds.insert(
        model.from_bounds,
        model.starty,
        model.to_bounds,
        model.stopy,
    );
    line_start_y
}

/// `drawMessage`.
#[allow(clippy::too_many_arguments)]
fn draw_message(
    svg: &Element,
    model: &MsgModel,
    line_start_y: f64,
    msg_id: &str,
    from: &str,
    to: &str,
    diagram_id: &str,
    hand_drawn: bool,
    sequence_index: f64,
    sequence_visible: bool,
) {
    let mut td = TextData {
        x: model.startx.min(model.stopx),
        y: model.starty + 10.0,
        width: Some((model.stopx - model.startx).abs()),
        anchor: Some("center"),
        text: model.message.clone(),
        class: Some("messageText"),
        dy: Some("1em"),
        tspan: false,
        text_margin: Some(WRAP_PADDING),
        valign_center: true,
    };
    draw_text(svg, &mut td);

    let line = if (model.startx - model.stopx).abs() < f64::EPSILON {
        let line = append(svg, "path");
        let x = model.startx;
        set_attr(
            &line,
            "d",
            format!(
                "M {},{} C {},{} {},{} {},{}",
                js_num(x),
                js_num(line_start_y),
                js_num(x + 60.0),
                js_num(line_start_y - 10.0),
                js_num(x + 60.0),
                js_num(line_start_y + 30.0),
                js_num(x),
                js_num(line_start_y + 20.0)
            ),
        );
        line
    } else {
        // HAND-DRAWN EXTENSION: straight messages become sketchy paths; the
        // self-message bezier above is left smooth on purpose.
        draw_segment(
            svg,
            model.startx,
            line_start_y,
            model.stopx,
            line_start_y,
            hand_drawn,
        )
    };

    let dotted = matches!(
        model.ty,
        DOTTED | DOTTED_CROSS | DOTTED_POINT | DOTTED_OPEN | BIDIRECTIONAL_DOTTED
    );
    if dotted {
        set_attr(&line, "class", "messageLine1");
    } else {
        set_attr(&line, "class", "messageLine0");
    }
    set_attr(&line, "data-et", "message");
    set_attr(&line, "data-id", format!("i{msg_id}"));
    set_attr(&line, "data-from", from);
    set_attr(&line, "data-to", to);
    set_attr(&line, "stroke-width", "2");
    set_attr(&line, "stroke", "none");

    match model.ty {
        SOLID | DOTTED => {
            set_attr(&line, "marker-end", format!("url(#{diagram_id}-arrowhead)"));
        }
        BIDIRECTIONAL_SOLID | BIDIRECTIONAL_DOTTED => {
            set_attr(
                &line,
                "marker-start",
                format!("url(#{diagram_id}-arrowhead)"),
            );
            set_attr(&line, "marker-end", format!("url(#{diagram_id}-arrowhead)"));
        }
        SOLID_POINT | DOTTED_POINT => {
            set_attr(
                &line,
                "marker-end",
                format!("url(#{diagram_id}-filled-head)"),
            );
        }
        SOLID_CROSS | DOTTED_CROSS => {
            set_attr(&line, "marker-end", format!("url(#{diagram_id}-crosshead)"));
        }
        _ => {}
    }
    if dotted {
        // d3 .style('stroke-dasharray', '3, 3') + fill none, CSSOM last.
        set_attr(&line, "style", "stroke-dasharray: 3, 3; fill: none;");
    } else {
        set_attr(&line, "style", "fill: none;");
    }

    // "add node number" — autonumber circle + index text.
    if sequence_visible {
        let radius = 6.0;
        let bidirectional = matches!(model.ty, BIDIRECTIONAL_SOLID | BIDIRECTIONAL_DOTTED);
        if bidirectional {
            let x1 = if model.startx < model.stopx {
                model.startx + radius * 2.0
            } else {
                model.startx - radius
            };
            set_attr(&line, "x1", js_num(x1));
        } else {
            // Applied even to self-message paths, matching upstream.
            set_attr(&line, "x1", js_num(model.startx + radius));
        }

        let self_message = (model.startx - model.stopx).abs() < f64::EPSILON;
        let autonumber_x = if self_message || model.startx <= model.stopx {
            model.from_bounds + 1.0
        } else {
            model.to_bounds - 1.0
        };

        let digits = js_num(sequence_index).len();
        let font_size = if digits > 5 {
            "7px"
        } else if digits > 3 {
            "9px"
        } else {
            "12px"
        };

        let nline = append(svg, "line");
        set_attr(&nline, "x1", js_num(autonumber_x));
        set_attr(&nline, "y1", js_num(line_start_y));
        set_attr(&nline, "x2", js_num(autonumber_x));
        set_attr(&nline, "y2", js_num(line_start_y));
        set_attr(&nline, "stroke-width", "0");
        set_attr(
            &nline,
            "marker-start",
            format!("url(#{diagram_id}-sequencenumber)"),
        );

        let ntext = append(svg, "text");
        set_attr(&ntext, "x", js_num(autonumber_x));
        set_attr(&ntext, "y", js_num(line_start_y + 4.0));
        set_attr(&ntext, "font-family", "sans-serif");
        set_attr(&ntext, "font-size", font_size);
        set_attr(&ntext, "text-anchor", "middle");
        set_attr(&ntext, "class", "sequenceNumber");
        crate::svg::set_text(&ntext, &js_num(sequence_index));
    }
}

/// `drawActivation`: rect into the DOM anchor created at ACTIVE_START.
fn draw_activation(
    anchor: &Element,
    data: &Activation,
    starty: f64,
    vertical_pos: f64,
    remaining_count: usize,
) {
    draw_rect(
        anchor,
        &RectData {
            x: data.startx,
            y: starty,
            fill: "#EDF2AE",
            stroke: "#666",
            width: data.stopx - data.startx,
            height: vertical_pos - starty,
            name: None,
            rx: 0.0,
            class: match remaining_count % 3 {
                1 => "activation1",
                2 => "activation2",
                _ => "activation0",
            },
        },
        false,
    );
}

/// `drawBackgroundRect` for `rect` blocks — prepended (d3 `.lower()`).
fn draw_background_rect(svg: &Element, model: &LoopModel) {
    let rect = crate::svg::insert_first(svg, "rect");
    set_attr(&rect, "x", js_num(model.startx.unwrap_or(0.0)));
    set_attr(&rect, "y", js_num(model.starty_opt.unwrap_or(model.starty)));
    set_attr(&rect, "fill", model.fill.clone().unwrap_or_default());
    set_attr(
        &rect,
        "width",
        js_num(model.stopx.unwrap_or(0.0) - model.startx.unwrap_or(0.0)),
    );
    set_attr(
        &rect,
        "height",
        js_num(model.stopy.unwrap_or(0.0) - model.starty_opt.unwrap_or(model.starty)),
    );
    set_attr(&rect, "class", "rect");
}

/// `drawLoop` (loop label box + title).
fn draw_loop(svg: &Element, model: &LoopModel, label_text: &str, msg_id: &str, hand_drawn: bool) {
    let g = append(svg, "g");
    set_attr(&g, "data-et", "control-structure");
    set_attr(&g, "data-id", format!("i{msg_id}"));
    let startx = model.startx.unwrap_or(0.0);
    let stopx = model.stopx.unwrap_or(0.0);
    let starty = model.starty_opt.unwrap_or(model.starty);
    let stopy = model.stopy.unwrap_or(0.0);
    // HAND-DRAWN EXTENSION: the four loop borders become sketchy segments; the
    // cut-corner label tab below stays crisp.
    let line = |x1: f64, y1: f64, x2: f64, y2: f64| {
        let l = draw_segment(&g, x1, y1, x2, y2, hand_drawn);
        set_attr(&l, "class", "loopLine");
    };
    line(startx, starty, stopx, starty);
    line(stopx, starty, stopx, stopy);
    line(startx, stopy, stopx, stopy);
    line(startx, starty, startx, stopy);
    for &section_y in &model.sections {
        let l = append(&g, "line");
        set_attr(&l, "x1", js_num(startx));
        set_attr(&l, "y1", js_num(section_y));
        set_attr(&l, "x2", js_num(stopx));
        set_attr(&l, "y2", js_num(section_y));
        set_attr(&l, "class", "loopLine");
        set_attr(&l, "style", "stroke-dasharray: 3, 3;");
    }

    // drawLabel polygon.
    let cut = 7.0;
    let (x, y, w, h) = (startx, starty, LABEL_BOX_WIDTH, LABEL_BOX_HEIGHT);
    let polygon = append(&g, "polygon");
    set_attr(
        &polygon,
        "points",
        format!(
            "{},{} {},{} {},{} {},{} {},{}",
            js_num(x),
            js_num(y),
            js_num(x + w),
            js_num(y),
            js_num(x + w),
            js_num(y + h - cut),
            js_num(x + w - cut * 1.2),
            js_num(y + h),
            js_num(x),
            js_num(y + h)
        ),
    );
    set_attr(&polygon, "class", "labelBox");

    let mut td = TextData {
        x: startx,
        y: starty + h / 2.0,
        width: Some(LABEL_BOX_WIDTH.max(50.0)),
        anchor: Some("middle"),
        text: label_text.to_owned(),
        class: Some("labelText"),
        dy: None,
        tspan: false,
        text_margin: Some(BOX_TEXT_MARGIN),
        valign_center: true,
    };
    draw_text(&g, &mut td);

    let mut title_td = TextData {
        x: startx + LABEL_BOX_WIDTH / 2.0 + (stopx - startx) / 2.0,
        y: starty + BOX_MARGIN + BOX_TEXT_MARGIN,
        width: None,
        anchor: Some("middle"),
        text: model.title.clone(),
        class: Some("loopText"),
        dy: None,
        tspan: true,
        text_margin: Some(BOX_TEXT_MARGIN),
        valign_center: true,
    };
    draw_text(&g, &mut title_td);

    // sectionTitles (alt/else, par/and, critical/option).
    for (idx, item) in model.section_titles.iter().enumerate() {
        if item.is_empty() {
            continue;
        }
        let mut td = TextData {
            x: startx + (stopx - startx) / 2.0,
            y: model.sections[idx] + BOX_MARGIN + BOX_TEXT_MARGIN,
            width: None,
            anchor: Some("middle"),
            text: item.clone(),
            class: Some("sectionTitle"),
            dy: None,
            tspan: false,
            text_margin: Some(BOX_TEXT_MARGIN),
            valign_center: true,
        };
        draw_text(&g, &mut td);
    }
}

/// Top actor: lifeline + box group, prepended to the svg.
fn draw_actor_top(svg: &Element, actor: &mut Actor, actor_cnt: &mut usize, hand_drawn: bool) {
    if actor.is_actor_man {
        draw_actor_man(svg, actor, actor_cnt, false);
        return;
    }
    let center = actor.x + actor.width / 2.0;
    let center_y = actor.starty + actor.height;

    let group = insert_first(svg, "g");
    let cnt = *actor_cnt;
    *actor_cnt += 1;
    actor.actor_cnt = Some(cnt);

    let line = append(&group, "line");
    set_attr(&line, "id", format!("actor{cnt}"));
    set_attr(&line, "x1", js_num(center));
    set_attr(&line, "y1", js_num(center_y));
    set_attr(&line, "x2", js_num(center));
    set_attr(&line, "y2", "2000");
    set_attr(&line, "class", "actor-line 200");
    set_attr(&line, "stroke-width", "0.5px");
    set_attr(&line, "stroke", "#999");
    set_attr(&line, "name", actor.name.clone());
    set_attr(&line, "data-et", "life-line");
    set_attr(&line, "data-id", actor.name.clone());

    let inner = append(&group, "g");
    set_attr(&inner, "id", format!("root-{cnt}"));

    draw_rect(
        &inner,
        &RectData {
            x: actor.x,
            y: actor.starty,
            fill: "#eaeaea",
            stroke: "#666",
            width: actor.width,
            height: actor.height,
            name: Some(&actor.name),
            rx: 3.0,
            class: "actor actor-top",
        },
        hand_drawn,
    );

    set_attr(&inner, "data-et", "participant");
    set_attr(&inner, "data-type", "participant");
    set_attr(&inner, "data-id", actor.name.clone());

    draw_actor_text(&inner, actor, actor.starty);
}

/// `drawActorTypeActor`: stick figure. The lifeline group is prepended
/// (empty for the footer); the figure group is appended.
const ACTOR_TYPE_WIDTH: f64 = 18.0 * 2.0;

fn draw_actor_man(svg: &Element, actor: &mut Actor, actor_cnt: &mut usize, is_footer: bool) {
    let actor_y = if is_footer {
        actor.stopy.unwrap_or(0.0)
    } else {
        actor.starty
    };
    let center = actor.x + actor.width / 2.0;
    let center_y = actor_y + 80.0;

    let line_g = insert_first(svg, "g");
    let cnt;
    if is_footer {
        // Footer ids reuse the counter as-is (post top pass), off by one.
        cnt = actor_cnt.saturating_sub(1);
    } else {
        cnt = *actor_cnt;
        *actor_cnt += 1;
        actor.actor_cnt = Some(cnt);
        let line = append(&line_g, "line");
        set_attr(&line, "id", format!("actor{cnt}"));
        set_attr(&line, "x1", js_num(center));
        set_attr(&line, "y1", js_num(center_y));
        set_attr(&line, "x2", js_num(center));
        set_attr(&line, "y2", "2000");
        set_attr(&line, "class", "actor-line 200");
        set_attr(&line, "stroke-width", "0.5px");
        set_attr(&line, "stroke", "#999");
        set_attr(&line, "name", actor.name.clone());
        set_attr(&line, "data-et", "life-line");
        set_attr(&line, "data-id", actor.name.clone());
    }

    let g = append(svg, "g");
    set_attr(
        &g,
        "class",
        if is_footer {
            "actor-man actor-bottom"
        } else {
            "actor-man actor-top"
        },
    );
    set_attr(&g, "name", actor.name.clone());
    if !is_footer {
        set_attr(&g, "data-et", "participant");
        set_attr(&g, "data-type", "actor");
        set_attr(&g, "data-id", actor.name.clone());
    }

    let seg = |x1: f64, y1: f64, x2: f64, y2: f64, id: Option<String>| {
        let l = append(&g, "line");
        if let Some(id) = id {
            set_attr(&l, "id", id);
        }
        set_attr(&l, "x1", js_num(x1));
        set_attr(&l, "y1", js_num(y1));
        set_attr(&l, "x2", js_num(x2));
        set_attr(&l, "y2", js_num(y2));
    };
    let half = ACTOR_TYPE_WIDTH / 2.0;
    seg(
        center,
        actor_y + 25.0,
        center,
        actor_y + 45.0,
        Some(format!("actor-man-torso{cnt}")),
    );
    seg(
        center - half,
        actor_y + 33.0,
        center + half,
        actor_y + 33.0,
        Some(format!("actor-man-arms{cnt}")),
    );
    seg(center - half, actor_y + 60.0, center, actor_y + 45.0, None);
    seg(
        center,
        actor_y + 45.0,
        center + (half - 2.0),
        actor_y + 60.0,
        None,
    );

    let circle = append(&g, "circle");
    set_attr(&circle, "cx", js_num(center));
    set_attr(&circle, "cy", js_num(actor_y + 10.0));
    set_attr(&circle, "r", "15");
    set_attr(&circle, "width", js_num(actor.width));
    set_attr(&circle, "height", js_num(actor.height));

    // getBBox of the stickman: circle top (y+10-15) to leg bottom (y+60).
    actor.height = 65.0;

    set_attr(&g, "style", "stroke: rgb(147, 112, 219);");

    for (i, line) in split_breaks(&actor.description).iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let dy = i as f64 * FONT_SIZE;
        let text = append(&g, "text");
        set_attr(&text, "x", js_num(center));
        set_attr(&text, "y", js_num(actor_y + 35.0 + actor.height / 2.0));
        let span = append(&text, "tspan");
        set_attr(&span, "x", js_num(center));
        set_attr(&span, "dy", js_num(dy));
        set_text(&span, line);
        set_attr(&text, "dominant-baseline", "central");
        set_attr(&text, "alignment-baseline", "central");
        set_attr(&text, "class", "actor actor-man");
        set_attr(
            &text,
            "style",
            "text-anchor: middle; font-size: 16px; font-weight: 400;",
        );
    }
}

/// Footer actor: rect + text only, prepended.
fn draw_actor_footer(svg: &Element, actor: &Actor, hand_drawn: bool) {
    let group = insert_first(svg, "g");
    let y = actor.stopy.unwrap_or(0.0);
    draw_rect(
        &group,
        &RectData {
            x: actor.x,
            y,
            fill: "#eaeaea",
            stroke: "#666",
            width: actor.width,
            height: actor.height,
            name: Some(&actor.name),
            rx: 3.0,
            class: "actor actor-bottom",
        },
        hand_drawn,
    );
    draw_actor_text(&group, actor, y);
}

/// `byTspan` one-line label (box titles).
fn by_tspan(g: &Element, content: &str, x: f64, y: f64, class: &str) {
    for (i, line) in split_breaks(content).iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let dy = i as f64 * FONT_SIZE;
        let text = append(g, "text");
        set_attr(&text, "x", js_num(x));
        set_attr(&text, "y", js_num(y));
        let span = append(&text, "tspan");
        set_attr(&span, "x", js_num(x));
        set_attr(&span, "dy", js_num(dy));
        set_text(&span, line);
        set_attr(&text, "dominant-baseline", "central");
        set_attr(&text, "alignment-baseline", "central");
        set_attr(&text, "class", class);
        set_attr(
            &text,
            "style",
            "text-anchor: middle; font-size: 16px; font-weight: 400;",
        );
    }
}

/// `byTspan` actor label.
fn draw_actor_text(g: &Element, actor: &Actor, y: f64) {
    let lines = split_breaks(&actor.description);
    #[allow(clippy::cast_precision_loss)]
    let n = lines.len() as f64;
    for (i, line) in lines.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let dy = i as f64 * FONT_SIZE - (FONT_SIZE * (n - 1.0)) / 2.0;
        let text = append(g, "text");
        set_attr(&text, "x", js_num(actor.x + actor.width / 2.0));
        set_attr(&text, "y", js_num(y + actor.height / 2.0));
        let span = append(&text, "tspan");
        set_attr(&span, "x", js_num(actor.x + actor.width / 2.0));
        set_attr(&span, "dy", js_num(dy));
        set_text(&span, line);
        set_attr(&text, "dominant-baseline", "central");
        set_attr(&text, "alignment-baseline", "central");
        set_attr(&text, "class", "actor actor-box");
        set_attr(
            &text,
            "style",
            "text-anchor: middle; font-size: 16px; font-weight: 400;",
        );
    }
}

/// Finds `#actor{cnt}` and sets its `y2` (fixLifeLineHeights, mirrorActors).
fn fix_lifeline(svg: &Element, cnt: usize, stopy: f64) {
    fn walk(el: &Element, id: &str, stopy: f64) -> bool {
        let children: Vec<Element> = el
            .borrow()
            .children
            .iter()
            .filter_map(|c| match c {
                crate::svg::Node::Element(e) => Some(e.clone()),
                crate::svg::Node::Text(_) => None,
            })
            .collect();
        for child in children {
            if crate::svg::get_attr(&child, "id").as_deref() == Some(id) {
                set_attr(&child, "y2", js_num(stopy));
                return true;
            }
            if walk(&child, id, stopy) {
                return true;
            }
        }
        false
    }
    walk(svg, &format!("actor{cnt}"), stopy);
}

/// All sequence marker/arrowhead defs, in insertion order.
fn insert_markers(svg: &Element, id: &str) {
    // insertArrowHead
    let m = marker(svg, &format!("{id}-arrowhead"));
    set_attr(&m, "refX", "7.9");
    set_attr(&m, "refY", "5");
    set_attr(&m, "markerUnits", "userSpaceOnUse");
    set_attr(&m, "markerWidth", "12");
    set_attr(&m, "markerHeight", "12");
    set_attr(&m, "orient", "auto-start-reverse");
    let p = append(&m, "path");
    set_attr(&p, "d", "M -1 0 L 10 5 L 0 10 z");

    // insertArrowCrossHead
    let m = marker(svg, &format!("{id}-crosshead"));
    set_attr(&m, "markerWidth", "15");
    set_attr(&m, "markerHeight", "8");
    set_attr(&m, "orient", "auto");
    set_attr(&m, "refX", "4");
    set_attr(&m, "refY", "4.5");
    let p = append(&m, "path");
    set_attr(&p, "fill", "none");
    set_attr(&p, "stroke", "#000000");
    set_attr(&p, "stroke-width", "1pt");
    set_attr(&p, "d", "M 1,2 L 6,7 M 6,2 L 1,7");
    set_attr(&p, "style", "stroke-dasharray: 0, 0;");

    // insertArrowFilledHead
    let m = marker(svg, &format!("{id}-filled-head"));
    set_attr(&m, "refX", "15.5");
    set_attr(&m, "refY", "7");
    set_attr(&m, "markerWidth", "20");
    set_attr(&m, "markerHeight", "28");
    set_attr(&m, "orient", "auto");
    let p = append(&m, "path");
    set_attr(&p, "d", "M 18,7 L9,13 L14,7 L9,1 Z");

    // insertSequenceNumber
    let m = marker(svg, &format!("{id}-sequencenumber"));
    set_attr(&m, "refX", "15");
    set_attr(&m, "refY", "15");
    set_attr(&m, "markerWidth", "60");
    set_attr(&m, "markerHeight", "40");
    set_attr(&m, "orient", "auto");
    let c = append(&m, "circle");
    set_attr(&c, "cx", "15");
    set_attr(&c, "cy", "15");
    set_attr(&c, "r", "6");

    // insertSolidTopArrowHead
    let m = marker(svg, &format!("{id}-solidTopArrowHead"));
    set_attr(&m, "refX", "7.9");
    set_attr(&m, "refY", "7.25");
    set_attr(&m, "markerUnits", "userSpaceOnUse");
    set_attr(&m, "markerWidth", "12");
    set_attr(&m, "markerHeight", "12");
    set_attr(&m, "orient", "auto-start-reverse");
    let p = append(&m, "path");
    set_attr(&p, "d", "M 0 0 L 10 8 L 0 8 z");

    // insertSolidBottomArrowHead
    let m = marker(svg, &format!("{id}-solidBottomArrowHead"));
    set_attr(&m, "refX", "7.9");
    set_attr(&m, "refY", "0.75");
    set_attr(&m, "markerUnits", "userSpaceOnUse");
    set_attr(&m, "markerWidth", "12");
    set_attr(&m, "markerHeight", "12");
    set_attr(&m, "orient", "auto-start-reverse");
    let p = append(&m, "path");
    set_attr(&p, "d", "M 0 0 L 10 0 L 0 8 z");

    // insertStickTopArrowHead
    let m = marker(svg, &format!("{id}-stickTopArrowHead"));
    set_attr(&m, "refX", "7.5");
    set_attr(&m, "refY", "7");
    set_attr(&m, "markerUnits", "userSpaceOnUse");
    set_attr(&m, "markerWidth", "12");
    set_attr(&m, "markerHeight", "12");
    set_attr(&m, "orient", "auto-start-reverse");
    let p = append(&m, "path");
    set_attr(&p, "d", "M 0 0 L 7 7");
    set_attr(&p, "stroke", "black");
    set_attr(&p, "stroke-width", "1.5");
    set_attr(&p, "fill", "none");

    // insertStickBottomArrowHead
    let m = marker(svg, &format!("{id}-stickBottomArrowHead"));
    set_attr(&m, "refX", "7.5");
    set_attr(&m, "refY", "0");
    set_attr(&m, "markerUnits", "userSpaceOnUse");
    set_attr(&m, "markerWidth", "12");
    set_attr(&m, "markerHeight", "12");
    set_attr(&m, "orient", "auto-start-reverse");
    let p = append(&m, "path");
    set_attr(&p, "d", "M 0 7 L 7 0");
    set_attr(&p, "stroke", "black");
    set_attr(&p, "stroke-width", "1.5");
    set_attr(&p, "fill", "none");
}

fn marker(svg: &Element, id: &str) -> Element {
    let defs = append(svg, "defs");
    let m = append(&defs, "marker");
    set_attr(&m, "id", id);
    m
}

const COMPUTER_D: &str = "M2 2v13h20v-13h-20zm18 11h-16v-9h16v9zm-10.228 6l.466-1h3.524l.467 1h-4.457zm14.228 3h-24l2-6h2.104l-1.33 4h18.45l-1.297-4h2.073l2 6zm-5-10h-14v-7h14v7z";

const CLOCK_D: &str = "M12 2c5.514 0 10 4.486 10 10s-4.486 10-10 10-10-4.486-10-10 4.486-10 10-10zm0-2c-6.627 0-12 5.373-12 12s5.373 12 12 12 12-5.373 12-12-5.373-12-12-12zm5.848 12.459c.202.038.202.333.001.372-1.907.361-6.045 1.111-6.547 1.111-.719 0-1.301-.582-1.301-1.301 0-.512.77-5.447 1.125-7.445.034-.192.312-.181.343.014l.985 6.238 5.394 1.011z";

include!("database_icon.rs");
