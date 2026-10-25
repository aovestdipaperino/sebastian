//! ishikawa (fishbone) diagrams: a port of mermaid's `ishikawaDb.ts` and
//! `ishikawaRenderer.ts` (mermaid 11.15.0). The parser follows the indentation
//! rules of the upstream Jison grammar (the same shape as obsidianmd/pele's
//! port, MIT).
//!
//! The renderer is pure arithmetic plus SVG `getBBox` of `<text>`, which the
//! shared Chrome text model reproduces: bounding boxes of text are not part of
//! `svg.getBBox()` in headless Chrome, so only lines, paths and label boxes
//! shape the viewBox. `look: handDrawn` is not specialised; it renders the
//! classic geometry.

use crate::render::bbox::{Rect, element_bbox};
use crate::svg::{Element, append, insert_first, js_num, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

/// A parse error for ishikawa source.
#[derive(Debug)]
pub struct IshikawaParseError {
    pub message: String,
}

impl std::fmt::Display for IshikawaParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ishikawa parse error: {}", self.message)
    }
}

impl std::error::Error for IshikawaParseError {}

const FONT_SIZE_DEFAULT: f64 = 14.0;
const SPINE_BASE_LENGTH: f64 = 250.0;
const BONE_STUB: f64 = 30.0;
const BONE_BASE: f64 = 60.0;
const BONE_PER_CHILD: f64 = 5.0;
const DIAGRAM_PADDING: f64 = 20.0;
const HEAD_FONT_SIZE: f64 = 14.0;

struct Node {
    text: String,
    children: Vec<Node>,
}

/// Mirror of `IshikawaDB.addNode`: the first line is the effect, later lines
/// nest by indentation relative to the first cause.
struct Db {
    root: Option<Node>,
    /// Path of child indices from the root with the level of each node.
    stack: Vec<(i64, Vec<usize>)>,
    base_level: Option<i64>,
}

impl Db {
    fn add_node(&mut self, raw_level: i64, text: String) {
        let node = Node {
            text,
            children: Vec::new(),
        };
        let Some(root) = self.root.as_mut() else {
            self.root = Some(node);
            self.stack = vec![(0, Vec::new())];
            return;
        };
        let base = *self.base_level.get_or_insert(raw_level);
        let level = (raw_level - base + 1).max(1);
        while self.stack.len() > 1 && self.stack[self.stack.len() - 1].0 >= level {
            self.stack.pop();
        }
        let mut path = self.stack[self.stack.len() - 1].1.clone();
        let mut parent = root;
        for &i in &path {
            parent = &mut parent.children[i];
        }
        parent.children.push(node);
        path.push(parent.children.len() - 1);
        self.stack.push((level, path));
    }
}

fn parse(source: &str) -> Result<Db, IshikawaParseError> {
    let mut db = Db {
        root: None,
        stack: Vec::new(),
        base_level: None,
    };
    let mut lines = source.lines().peekable();
    // Front matter.
    while let Some(first) = lines.peek() {
        if first.trim().is_empty() {
            lines.next();
        } else {
            if first.trim() == "---" {
                lines.next();
                for l in lines.by_ref() {
                    if l.trim() == "---" {
                        break;
                    }
                }
            }
            break;
        }
    }
    let mut found_header = false;
    for raw in lines {
        let t = raw.trim();
        if t.is_empty() || t.starts_with("%%") {
            continue;
        }
        if !found_header {
            let lower = t.to_ascii_lowercase();
            if lower == "ishikawa" || lower == "ishikawa-beta" {
                found_header = true;
                continue;
            }
            return Err(IshikawaParseError {
                message: format!("expected ishikawa header, got {t:?}"),
            });
        }
        let indent =
            i64::try_from(raw.chars().take_while(|c| c.is_whitespace()).count()).unwrap_or(0);
        db.add_node(indent, t.to_owned());
    }
    if !found_header {
        return Err(IshikawaParseError {
            message: "missing ishikawa header".to_owned(),
        });
    }
    if db.root.is_none() {
        return Err(IshikawaParseError {
            message: "ishikawa diagram has no effect line".to_owned(),
        });
    }
    Ok(db)
}

/// `text.split(/<br\s*\/?>|\n/)` (case-sensitive, like the JS regex).
fn split_lines(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            out.push(text[start..i].to_owned());
            i += 1;
            start = i;
        } else if bytes[i..].starts_with(b"<br") {
            let mut j = i + 3;
            while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'/' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'>' {
                out.push(text[start..i].to_owned());
                i = j + 1;
                start = i;
                continue;
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    out.push(text[start..].to_owned());
    out
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `wrapText`: greedy word wrap on `\s+`, joined with `\n`.
fn wrap_text(text: &str, max_chars: usize) -> String {
    if utf16_len(text) <= max_chars {
        return text.to_owned();
    }
    let mut lines: Vec<String> = Vec::new();
    // JS `split(/\s+/)` keeps empty leading/trailing fields.
    let mut words: Vec<&str> = Vec::new();
    let mut last = 0;
    let mut in_ws = false;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if !in_ws {
                words.push(&text[last..i]);
                in_ws = true;
            }
        } else if in_ws {
            last = i;
            in_ws = false;
        }
    }
    words.push(if in_ws { "" } else { &text[last..] });
    for word in words {
        if let Some(prev) = lines.last_mut()
            && utf16_len(prev) + 1 + utf16_len(word) <= max_chars
        {
            prev.push(' ');
            prev.push_str(word);
        } else {
            lines.push(word.to_owned());
        }
    }
    lines.join("\n")
}

fn snap64(v: f64) -> f64 {
    (v * 64.0).round() / 64.0
}

fn f32r(v: f64) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    f64::from(v as f32)
}

#[derive(Clone, Copy, PartialEq)]
enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn name(self) -> &'static str {
        match self {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
            Anchor::End => "end",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Baseline {
    Middle,
    Alphabetic,
    Hanging,
}

struct TextBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// `getBBox()` of the multi-line `<text>` that [`draw_multiline`] emits, in
/// the text's own coordinates (Chrome SVG text model, as in the timeline
/// title and ink-width helpers).
fn text_box(
    m: &TextMeasurer,
    lines: &[String],
    x: f64,
    first_y: f64,
    anchor: Anchor,
    baseline: Baseline,
    fs: f64,
) -> TextBox {
    // Chrome keeps text geometry in f32 (LayoutUnit-free SVG path): attribute
    // lengths parse to f32 and every sum rounds to f32.
    let lh = fs * 1.05;
    let asc = (1923.0 * fs / 2048.0).round();
    let shift = match baseline {
        Baseline::Middle => snap64(m.x_height_px(fs) / 2.0),
        Baseline::Alphabetic => 0.0,
        Baseline::Hanging => asc,
    };
    let (mut x0, mut x1) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY);
    for (i, line) in lines.iter().enumerate() {
        let w = m.measure_advance_svg(line, fs) as f32;
        let xf = x as f32;
        let l = match anchor {
            Anchor::Start => xf,
            Anchor::Middle => xf - w / 2.0,
            Anchor::End => xf - w,
        };
        let line_y = (first_y + lh * i as f64) as f32;
        let top = (line_y - asc as f32) + shift as f32;
        let bottom = top + m.ink_height(line, fs) as f32;
        x0 = x0.min(l);
        x1 = x1.max(l + w);
        y0 = y0.min(top);
        y1 = y1.max(bottom);
    }
    TextBox {
        x: f64::from(x0),
        y: f64::from(y0),
        width: f64::from(x1 - x0),
        height: f64::from(y1 - y0),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_multiline(
    parent: &Element,
    lines: &[String],
    x: f64,
    y: f64,
    class: &str,
    anchor: Anchor,
    fs: f64,
) -> Element {
    let lh = fs * 1.05;
    let el = append(parent, "text");
    set_attr(&el, "class", class);
    set_attr(&el, "text-anchor", anchor.name());
    set_attr(&el, "x", js_num(x));
    set_attr(&el, "y", js_num(y - (lines.len() as f64 - 1.0) * lh / 2.0));
    for (i, line) in lines.iter().enumerate() {
        let tspan = append(&el, "tspan");
        set_attr(&tspan, "x", js_num(x));
        set_attr(&tspan, "dy", js_num(if i == 0 { 0.0 } else { lh }));
        set_text(&tspan, line);
    }
    el
}

fn draw_line(parent: &Element, x1: f64, y1: f64, x2: f64, y2: f64, class: &str) -> Element {
    let line = append(parent, "line");
    set_attr(&line, "class", class);
    set_attr(&line, "x1", js_num(x1));
    set_attr(&line, "y1", js_num(y1));
    set_attr(&line, "x2", js_num(x2));
    set_attr(&line, "y2", js_num(y2));
    line
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

struct Entry {
    text: String,
    depth: usize,
    parent: Option<usize>,
    child_count: usize,
}

/// `flattenTree`: pre-order Y slots for even depths, post-order for odd.
fn flatten_tree(children: &[Node], direction: i32) -> (Vec<Entry>, Vec<usize>) {
    fn walk(
        nodes: &[Node],
        pid: Option<usize>,
        depth: usize,
        direction: i32,
        entries: &mut Vec<Entry>,
        y_order: &mut Vec<usize>,
    ) {
        let ordered: Vec<&Node> = if direction == -1 {
            nodes.iter().rev().collect()
        } else {
            nodes.iter().collect()
        };
        for child in ordered {
            let idx = entries.len();
            entries.push(Entry {
                depth,
                text: wrap_text(&child.text, 15),
                parent: pid,
                child_count: child.children.len(),
            });
            if depth.is_multiple_of(2) {
                y_order.push(idx);
                if !child.children.is_empty() {
                    walk(
                        &child.children,
                        Some(idx),
                        depth + 1,
                        direction,
                        entries,
                        y_order,
                    );
                }
            } else {
                if !child.children.is_empty() {
                    walk(
                        &child.children,
                        Some(idx),
                        depth + 1,
                        direction,
                        entries,
                        y_order,
                    );
                }
                y_order.push(idx);
            }
        }
    }
    let mut entries = Vec::new();
    let mut y_order = Vec::new();
    walk(children, None, 2, direction, &mut entries, &mut y_order);
    (entries, y_order)
}

struct Bone {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    child_count: usize,
    children_drawn: usize,
}

struct Ctx<'a> {
    m: &'a TextMeasurer,
    fs: f64,
    cos_a: f64,
    sin_a: f64,
    /// `x` of every `<text>` bbox drawn in the current pair group.
    text_xs: Vec<f64>,
}

impl Ctx<'_> {
    fn note_text(&mut self, lines: &[String], x: f64, y: f64, anchor: Anchor, baseline: Baseline) {
        let first_y = y - (lines.len() as f64 - 1.0) * self.fs * 1.05 / 2.0;
        let b = text_box(self.m, lines, x, first_y, anchor, baseline, self.fs);
        self.text_xs.push(b.x);
    }
}

fn draw_cause_label(ctx: &mut Ctx, parent: &Element, text: &str, x: f64, y: f64, direction: f64) {
    let group = append(parent, "g");
    set_attr(&group, "class", "ishikawa-label-group");
    let lines = split_lines(text);
    let ty = y + 11.0 * direction;
    draw_multiline(
        &group,
        &lines,
        x,
        ty,
        "ishikawa-label cause",
        Anchor::Middle,
        ctx.fs,
    );
    let first_y = ty - (lines.len() as f64 - 1.0) * ctx.fs * 1.05 / 2.0;
    let tb = text_box(
        ctx.m,
        &lines,
        x,
        first_y,
        Anchor::Middle,
        Baseline::Middle,
        ctx.fs,
    );
    ctx.text_xs.push(tb.x);
    let rect = insert_first(&group, "rect");
    set_attr(&rect, "class", "ishikawa-label-box");
    set_attr(&rect, "x", js_num(tb.x - 20.0));
    set_attr(&rect, "y", js_num(tb.y - 2.0));
    set_attr(&rect, "width", js_num(tb.width + 40.0));
    set_attr(&rect, "height", js_num(tb.height + 4.0));
}

#[allow(clippy::too_many_arguments)]
fn draw_branch(
    ctx: &mut Ctx,
    parent: &Element,
    node: &Node,
    start_x: f64,
    start_y: f64,
    direction: f64,
    length: f64,
) {
    let children = &node.children;
    let line_len = length * if children.is_empty() { 0.2 } else { 1.0 };
    let dx = -ctx.cos_a * line_len;
    let dy = ctx.sin_a * line_len * direction;
    let end_x = start_x + dx;
    let end_y = start_y + dy;

    draw_line(parent, start_x, start_y, end_x, end_y, "ishikawa-branch");
    draw_cause_label(ctx, parent, &node.text, end_x, end_y, direction);

    if children.is_empty() {
        return;
    }

    #[allow(clippy::cast_possible_truncation)]
    let (entries, y_order) = flatten_tree(children, direction as i32);
    let count = entries.len();
    let mut ys = vec![0.0f64; count];
    for (slot, &entry_idx) in y_order.iter().enumerate() {
        ys[entry_idx] = start_y + dy * ((slot + 1) as f64 / (count + 1) as f64);
    }

    let mut bones: std::collections::HashMap<Option<usize>, Bone> =
        std::collections::HashMap::new();
    bones.insert(
        None,
        Bone {
            x0: start_x,
            y0: start_y,
            x1: end_x,
            y1: end_y,
            child_count: children.len(),
            children_drawn: 0,
        },
    );

    let diagonal_x = -ctx.cos_a;
    let diagonal_y = ctx.sin_a * direction;
    let odd_label = if direction < 0.0 {
        "ishikawa-label up"
    } else {
        "ishikawa-label down"
    };

    for (i, e) in entries.iter().enumerate() {
        let y = ys[i];
        let group = append(parent, "g");
        set_attr(&group, "class", "ishikawa-sub-group");
        let lines = split_lines(&e.text);
        let (bx0, by0, bx1);
        if e.depth.is_multiple_of(2) {
            let par = &bones[&e.parent];
            let dy_p = par.y1 - par.y0;
            bx0 = lerp(
                par.x0,
                par.x1,
                if dy_p == 0.0 {
                    0.5
                } else {
                    (y - par.y0) / dy_p
                },
            );
            by0 = y;
            bx1 = bx0
                - if e.child_count > 0 {
                    BONE_BASE + e.child_count as f64 * BONE_PER_CHILD
                } else {
                    BONE_STUB
                };
            draw_line(&group, bx0, y, bx1, y, "ishikawa-sub-branch");
            draw_multiline(
                &group,
                &lines,
                bx1,
                y,
                "ishikawa-label align",
                Anchor::End,
                ctx.fs,
            );
            ctx.note_text(&lines, bx1, y, Anchor::End, Baseline::Middle);
        } else {
            let par = bones.get_mut(&e.parent).expect("parent bone");
            let k = par.children_drawn;
            par.children_drawn += 1;
            bx0 = lerp(
                par.x0,
                par.x1,
                (par.child_count - k) as f64 / (par.child_count + 1) as f64,
            );
            by0 = par.y0;
            bx1 = bx0 + diagonal_x * ((y - by0) / diagonal_y);
            draw_line(&group, bx0, by0, bx1, y, "ishikawa-sub-branch");
            draw_multiline(&group, &lines, bx1, y, odd_label, Anchor::End, ctx.fs);
            let baseline = if direction < 0.0 {
                Baseline::Alphabetic
            } else {
                Baseline::Hanging
            };
            ctx.note_text(&lines, bx1, y, Anchor::End, baseline);
        }
        if e.child_count > 0 {
            bones.insert(
                Some(i),
                Bone {
                    x0: bx0,
                    y0: by0,
                    x1: bx1,
                    y1: y,
                    child_count: e.child_count,
                    children_drawn: 0,
                },
            );
        }
    }
}

/// `sideStats`: total and maximum descendant counts of one side's causes.
fn side_stats(nodes: &[&Node]) -> (usize, usize) {
    fn count(n: &Node) -> usize {
        n.children.iter().map(|c| 1 + count(c)).sum()
    }
    let mut total = 0;
    let mut max = 0;
    for n in nodes {
        let d = count(n);
        total += d;
        max = max.max(d);
    }
    (total, max)
}

/// Renders mermaid ishikawa source to a complete SVG document string.
///
/// # Errors
/// Returns an [`IshikawaParseError`] when the source is not a valid ishikawa
/// diagram.
#[allow(clippy::too_many_lines)]
pub fn render_ishikawa(source: &str, id: &str) -> Result<String, IshikawaParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let db = parse(source)?;
    let root = db.root.expect("checked in parse");

    let m = TextMeasurer::new();
    let fs = crate::render::themes::get(&theme_vars, "fontSize")
        .trim_end_matches("px")
        .parse::<f64>()
        .unwrap_or(FONT_SIZE_DEFAULT);
    let angle = 82.0f64 * std::f64::consts::PI / 180.0;
    let mut ctx = Ctx {
        m: &m,
        fs,
        cos_a: angle.cos(),
        sin_a: angle.sin(),
        text_xs: Vec::new(),
    };

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style_el = append(&svg, "style");
    set_text(
        &style_el,
        &crate::render::css::themed_ishikawa_css(id, &theme_vars),
    );
    let _empty = append(&svg, "g");
    let g = append(&svg, "g");
    set_attr(&g, "class", "ishikawa");

    let marker_id = format!("ishikawa-arrow-{id}");
    let defs = append(&g, "defs");
    let marker = append(&defs, "marker");
    set_attr(&marker, "id", marker_id.as_str());
    set_attr(&marker, "viewBox", "0 0 10 10");
    set_attr(&marker, "refX", "0");
    set_attr(&marker, "refY", "5");
    set_attr(&marker, "markerWidth", "6");
    set_attr(&marker, "markerHeight", "6");
    set_attr(&marker, "orient", "auto");
    let arrow = append(&marker, "path");
    set_attr(&arrow, "d", "M 10 0 L 0 5 L 10 10 Z");
    set_attr(&arrow, "class", "ishikawa-arrow");

    let mut spine_x = 0.0f64;
    let mut spine_y = SPINE_BASE_LENGTH;
    let spine = draw_line(&g, spine_x, spine_y, spine_x, spine_y, "ishikawa-spine");

    // Head: wrapped, bold 14px label inside a curved nib.
    let max_chars = (6.0f64).max((110.0 / (fs * 0.6)).floor()) as usize;
    let head_group = append(&g, "g");
    set_attr(&head_group, "class", "ishikawa-head-group");
    set_attr(
        &head_group,
        "transform",
        format!("translate({},{})", js_num(spine_x), js_num(spine_y)),
    );
    let head_lines = split_lines(&wrap_text(&root.text, max_chars));
    let head_text = draw_multiline(
        &head_group,
        &head_lines,
        0.0,
        0.0,
        "ishikawa-head-label",
        Anchor::Start,
        fs,
    );
    // The label's CSS makes it middle-anchored, bold, 14px.
    let hlh = fs * 1.05;
    let mut head_w = 0.0f64;
    let (mut asc_b, mut desc_b) = (0.0, 0.0);
    // Bold run widths snap up to 1/64 px (verified against Chrome).
    let bold = TextMeasurer::new();
    bold.set_bold(true);
    for line in &head_lines {
        let (a, d, _) = m.bold_metrics(line, HEAD_FONT_SIZE);
        head_w = head_w.max(bold.measure_width(line, HEAD_FONT_SIZE));
        asc_b = a;
        desc_b = d;
    }
    let tb_x = f32r(-head_w / 2.0);
    let first_y = -(head_lines.len() as f64 - 1.0) * hlh / 2.0;
    let tb_y = f32r(first_y + snap64(m.bold_x_height_px(HEAD_FONT_SIZE) / 2.0) - asc_b);
    let tb_h = f32r((head_lines.len() as f64 - 1.0) * hlh + asc_b + desc_b);
    let tb_w = f32r(head_w);
    let w = 60.0f64.max(tb_w + 6.0);
    let h = 40.0f64.max(tb_h * 2.0 + 40.0);
    let head_path = insert_first(&head_group, "path");
    set_attr(&head_path, "class", "ishikawa-head");
    set_attr(
        &head_path,
        "d",
        format!(
            "M 0 {} L 0 {} Q {} 0 0 {} Z",
            js_num(-h / 2.0),
            js_num(h / 2.0),
            js_num(w * 2.4),
            js_num(-h / 2.0)
        ),
    );
    set_attr(
        &head_text,
        "transform",
        format!(
            "translate({},{})",
            js_num((w - tb_w) / 2.0 - tb_x + 3.0),
            js_num(-tb_y - tb_h / 2.0)
        ),
    );

    let causes = &root.children;
    if !causes.is_empty() {
        spine_x -= 20.0;
        let upper: Vec<&Node> = causes.iter().step_by(2).collect();
        let lower: Vec<&Node> = causes.iter().skip(1).step_by(2).collect();
        let (upper_total, upper_max) = side_stats(&upper);
        let (lower_total, lower_max) = side_stats(&lower);
        let descendant_total = upper_total + lower_total;

        let mut upper_len = SPINE_BASE_LENGTH;
        let mut lower_len = SPINE_BASE_LENGTH;
        if descendant_total > 0 {
            let pool = SPINE_BASE_LENGTH * 2.0;
            let min_len = SPINE_BASE_LENGTH * 0.3;
            upper_len = min_len.max(pool * (upper_total as f64 / descendant_total as f64));
            lower_len = min_len.max(pool * (lower_total as f64 / descendant_total as f64));
        }
        let min_spacing = fs * 2.0;
        upper_len = upper_len.max(upper_max as f64 * min_spacing);
        lower_len = lower_len.max(lower_max as f64 * min_spacing);

        spine_y = upper_len.max(SPINE_BASE_LENGTH);
        set_attr(&spine, "y1", js_num(spine_y));
        set_attr(&spine, "y2", js_num(spine_y));
        set_attr(
            &head_group,
            "transform",
            format!("translate(0,{})", js_num(spine_y)),
        );

        let pair_count = causes.len().div_ceil(2);
        for p in 0..pair_count {
            let pg = append(&g, "g");
            set_attr(&pg, "class", "ishikawa-pair");
            ctx.text_xs.clear();
            if let Some(cause) = causes.get(p * 2) {
                draw_branch(&mut ctx, &pg, cause, spine_x, spine_y, -1.0, upper_len);
            }
            if let Some(cause) = causes.get(p * 2 + 1) {
                draw_branch(&mut ctx, &pg, cause, spine_x, spine_y, 1.0, lower_len);
            }
            spine_x = ctx.text_xs.iter().copied().fold(f64::INFINITY, f64::min);
        }

        set_attr(&spine, "x1", js_num(spine_x));
        let marker_url = format!("url(#{marker_id})");
        mark_branches(&g, &marker_url);
    }

    // applyPaddedViewBox: svg.getBBox() (text excluded) plus padding.
    let bbox: Rect = element_bbox(&svg);
    let vw = bbox.width() + DIAGRAM_PADDING * 2.0;
    let vh = bbox.height() + DIAGRAM_PADDING * 2.0;
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
            js_num(bbox.min_x - DIAGRAM_PADDING),
            js_num(bbox.min_y - DIAGRAM_PADDING),
            js_num(vw),
            js_num(vh)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "ishikawa");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// Adds `marker-start` to every branch and sub-branch line (document order).
fn mark_branches(el: &Element, url: &str) {
    for child in crate::svg::child_elements(el) {
        let (tag, class) = {
            let b = child.borrow();
            (
                b.tag.clone(),
                crate::svg::get_attr(&child, "class").unwrap_or_default(),
            )
        };
        if tag == "line" && (class == "ishikawa-branch" || class == "ishikawa-sub-branch") {
            set_attr(&child, "marker-start", url);
        }
        mark_branches(&child, url);
    }
}
