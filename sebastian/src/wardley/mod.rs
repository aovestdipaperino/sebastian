//! `wardley-beta`: a port of mermaid 11.15.0's Wardley map parser, builder
//! and renderer. The renderer draws everything with presentation attributes
//! (the diagram has no stylesheet), so the output is plain geometry; only the
//! annotation box needs text measurement.

use std::fmt::Write;

use indexmap::IndexMap;

use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};

/// A parse error for wardley source.
#[derive(Debug)]
pub struct WardleyParseError {
    pub message: String,
}

impl std::fmt::Display for WardleyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wardley parse error: {}", self.message)
    }
}

impl std::error::Error for WardleyParseError {}

fn err<T>(message: impl Into<String>) -> Result<T, WardleyParseError> {
    Err(WardleyParseError {
        message: message.into(),
    })
}

// ------------------------------------------------------------------ model --

#[derive(Debug, Clone, Default)]
struct Node {
    id: String,
    label: String,
    x: f64,
    y: f64,
    class_name: Option<String>,
    label_offset: (Option<f64>, Option<f64>),
    in_pipeline: bool,
    is_pipeline_parent: bool,
    inertia: bool,
    strategy: Option<String>,
}

#[derive(Debug, Clone)]
struct Link {
    source: String,
    target: String,
    dashed: bool,
    label: Option<String>,
    flow: Option<&'static str>,
}

#[derive(Debug, Clone)]
struct Pipeline {
    node_id: String,
    component_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct Annotation {
    number: i64,
    x: f64,
    y: f64,
    text: Option<String>,
}

#[derive(Debug, Default)]
struct Map {
    title: String,
    size: Option<(f64, f64)>,
    stages: Option<Vec<String>>,
    boundaries: Vec<f64>,
    nodes: IndexMap<String, Node>,
    links: Vec<Link>,
    trends: IndexMap<String, (f64, f64)>,
    pipelines: IndexMap<String, Pipeline>,
    annotations: Vec<Annotation>,
    notes: Vec<(String, f64, f64)>,
    accelerators: Vec<(String, f64, f64)>,
    deaccelerators: Vec<(String, f64, f64)>,
    annotations_box: Option<(f64, f64)>,
}

impl Map {
    /// `WardleyBuilder.addNode`: later fields replace earlier ones, except
    /// that class name and label offsets fall back to the existing values.
    fn add_node(&mut self, mut node: Node) {
        if let Some(old) = self.nodes.get(&node.id) {
            node.class_name = node.class_name.or_else(|| old.class_name.clone());
            node.label_offset.0 = node.label_offset.0.or(old.label_offset.0);
            node.label_offset.1 = node.label_offset.1.or(old.label_offset.1);
            node.is_pipeline_parent = old.is_pipeline_parent;
            node.in_pipeline = old.in_pipeline;
        }
        self.nodes.insert(node.id.clone(), node);
    }

    fn resolve_node_id(&self, name: &str) -> String {
        if self.nodes.contains_key(name) {
            return name.to_owned();
        }
        self.nodes
            .iter()
            .find(|(_, n)| n.label == name)
            .map_or_else(|| name.to_owned(), |(id, _)| id.clone())
    }
}

// ----------------------------------------------------------------- parser --

/// A statement line cursor.
struct Cur<'a> {
    s: &'a str,
}

impl Cur<'_> {
    fn ws(&mut self) {
        self.s = self.s.trim_start_matches([' ', '\t']);
    }
    fn eat(&mut self, t: &str) -> bool {
        if self.s.starts_with(t) {
            self.s = &self.s[t.len()..];
            true
        } else {
            false
        }
    }
    fn peek(&self) -> Option<char> {
        self.s.chars().next()
    }

    /// `STRING`: a quoted string with `\x` escapes kept verbatim.
    fn string(&mut self) -> Option<String> {
        let q = self.peek().filter(|c| *c == '"' || *c == '\'')?;
        let body = &self.s[1..];
        let mut end = None;
        let mut esc = false;
        for (i, c) in body.char_indices() {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == q {
                end = Some(i);
                break;
            }
        }
        let end = end?;
        let out = body[..end].to_owned();
        self.s = &body[end + 1..];
        Some(out)
    }

    /// `NAME_WITH_SPACES`: letters, digits, `_()&` and hyphens not followed
    /// by `>`, with single words separated by blanks.
    fn bare_name(&mut self) -> Option<String> {
        let first = self.peek()?;
        if !first.is_ascii_alphabetic() {
            return None;
        }
        let chars: Vec<(usize, char)> = self.s.char_indices().collect();
        let mut end = 0;
        let mut i = 0;
        while i < chars.len() {
            let (pos, c) = chars[i];
            let next = chars.get(i + 1).map(|x| x.1);
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '(' | ')' | '&') {
                end = pos + c.len_utf8();
                i += 1;
            } else if c == '-' && next != Some('>') {
                end = pos + 1;
                i += 1;
            } else if (c == ' ' || c == '\t') && end == pos {
                // A blank run continues the name only before a letter or '('.
                let mut j = i;
                while j < chars.len() && (chars[j].1 == ' ' || chars[j].1 == '\t') {
                    j += 1;
                }
                match chars.get(j).map(|x| x.1) {
                    Some(n) if n.is_ascii_alphabetic() || n == '(' => i = j,
                    _ => break,
                }
            } else {
                break;
            }
        }
        let name = self.s[..end].to_owned();
        self.s = &self.s[end..];
        Some(name)
    }

    fn name(&mut self) -> Option<String> {
        self.string().or_else(|| self.bare_name())
    }

    fn decimal(&mut self) -> Option<f64> {
        let digits = self.s.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let rest = &self.s[digits..];
        let frac = rest
            .strip_prefix('.')
            .map_or(0, |r| r.bytes().take_while(u8::is_ascii_digit).count());
        let len = if frac > 0 { digits + 1 + frac } else { digits };
        let v = self.s[..len].parse().ok()?;
        self.s = &self.s[len..];
        Some(v)
    }

    fn int(&mut self) -> Option<i64> {
        let digits = self.s.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        let v = self.s[..digits].parse().ok()?;
        self.s = &self.s[digits..];
        Some(v)
    }

    /// `[` a `,` b `]` of decimals.
    fn pair(&mut self) -> Option<(f64, f64)> {
        self.ws();
        if !self.eat("[") {
            return None;
        }
        self.ws();
        let a = self.decimal()?;
        self.ws();
        if !self.eat(",") {
            return None;
        }
        self.ws();
        let b = self.decimal()?;
        self.ws();
        self.eat("]").then_some((a, b))
    }

    /// `label [-x, -y]`
    fn label(&mut self) -> Option<(f64, f64)> {
        self.ws();
        let save = self.s;
        if !self.eat("label") {
            return None;
        }
        self.ws();
        let parsed = (|| {
            if !self.eat("[") {
                return None;
            }
            self.ws();
            let nx = self.eat("-");
            let x = self.int()? as f64;
            self.ws();
            if !self.eat(",") {
                return None;
            }
            self.ws();
            let ny = self.eat("-");
            let y = self.int()? as f64;
            self.ws();
            self.eat("]").then_some(())?;
            Some((if nx { -x } else { x }, if ny { -y } else { y }))
        })();
        if parsed.is_none() {
            self.s = save;
        }
        parsed
    }
}

/// Values in 0..=1 are fractions of the axis; larger ones are percentages.
fn to_percent(value: f64, context: &str) -> Result<f64, WardleyParseError> {
    let n = if value <= 1.0 { value * 100.0 } else { value };
    if !(0.0..=100.0).contains(&n) {
        return err(format!(
            "{context} must be between 0-1 (decimal) or 0-100 (percentage). Received: {value}"
        ));
    }
    Ok(n)
}

fn flow_from_port(port: &str) -> Option<&'static str> {
    match port {
        "+<>" => Some("bidirectional"),
        "+<" => Some("backward"),
        "+>" => Some("forward"),
        _ => None,
    }
}

fn port(c: &mut Cur) -> Option<&'static str> {
    c.ws();
    for p in ["+<>", "+<", "+>"] {
        if c.s.starts_with(p) {
            c.s = &c.s[p.len()..];
            return Some(match p {
                "+<>" => "+<>",
                "+<" => "+<",
                _ => "+>",
            });
        }
    }
    None
}

/// Link arrow: returns the arrow text.
fn arrow(c: &mut Cur) -> Option<String> {
    c.ws();
    if c.s.starts_with("+'") {
        let close = c.s[2..].find('\'')? + 2;
        let tail = &c.s[close + 1..];
        for t in ["<>", "<", ">"] {
            if let Some(rest) = tail.strip_prefix(t) {
                let arrow = c.s[..close + 1 + t.len()].to_owned();
                c.s = rest;
                return Some(arrow);
            }
        }
        return None;
    }
    for a in ["-.->", "-->", "->", ">"] {
        if c.eat(a) {
            return Some(a.to_owned());
        }
    }
    None
}

fn parse(source: &str) -> Result<Map, WardleyParseError> {
    let mut map = Map::default();
    let mut found = false;
    let mut lines = source.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found {
            if line == "wardley-beta" || line.starts_with("wardley-beta ") {
                found = true;
                continue;
            }
            return err(format!("expected wardley-beta header, got {line:?}"));
        }
        if let Some(t) = line.strip_prefix("title ") {
            t.trim().clone_into(&mut map.title);
            continue;
        }
        let mut c = Cur { s: line };
        let keyword = line.split([' ', '\t', '[']).next().unwrap_or("");
        match keyword {
            "size" => {
                c.eat("size");
                c.ws();
                if !c.eat("[") {
                    return err("bad size");
                }
                c.ws();
                let w = c.int();
                c.ws();
                c.eat(",");
                c.ws();
                let h = c.int();
                match (w, h) {
                    (Some(w), Some(h)) => map.size = Some((w as f64, h as f64)),
                    _ => return err("bad size"),
                }
            }
            "evolution" => {
                c.eat("evolution");
                let mut stages = Vec::new();
                let mut bounds = Vec::new();
                loop {
                    c.ws();
                    let Some(name) = c.name() else {
                        return err(format!("bad evolution stage in {line:?}"));
                    };
                    c.ws();
                    if c.eat("@") {
                        c.ws();
                        if let Some(b) = c.decimal() {
                            bounds.push(b);
                        }
                        c.ws();
                    }
                    let mut stage = name.trim().to_owned();
                    if c.eat("/") {
                        c.ws();
                        if let Some(second) = c.name() {
                            let _ = write!(stage, " / {}", second.trim());
                        }
                        c.ws();
                    }
                    stages.push(stage);
                    if !c.eat("->") {
                        break;
                    }
                }
                map.stages = Some(stages);
                map.boundaries = bounds;
            }
            "anchor" => {
                c.eat("anchor");
                c.ws();
                let name = c.name().ok_or_else(|| WardleyParseError {
                    message: "bad anchor".into(),
                })?;
                let Some((vis, evo)) = c.pair() else {
                    return err("bad anchor coordinates");
                };
                let x = to_percent(evo, &format!("Anchor \"{name}\" evolution"))?;
                let y = to_percent(vis, &format!("Anchor \"{name}\" visibility"))?;
                map.add_node(Node {
                    id: name.clone(),
                    label: name,
                    x,
                    y,
                    class_name: Some("anchor".into()),
                    ..Node::default()
                });
            }
            "component" => {
                c.eat("component");
                c.ws();
                let name = c.name().ok_or_else(|| WardleyParseError {
                    message: "bad component".into(),
                })?;
                let Some((vis, evo)) = c.pair() else {
                    return err("bad component coordinates");
                };
                let x = to_percent(evo, &format!("Component \"{name}\" evolution"))?;
                let y = to_percent(vis, &format!("Component \"{name}\" visibility"))?;
                let label = c.label();
                c.ws();
                let mut strategy = None;
                let mut inertia = false;
                if c.s.starts_with('(') {
                    for s in ["build", "buy", "outsource", "market"] {
                        if let Some(rest) = c.s.strip_prefix('(').and_then(|r| r.strip_prefix(s))
                            && rest.starts_with(')')
                        {
                            strategy = Some(s.to_owned());
                            c.s = &rest[1..];
                            break;
                        }
                    }
                    c.ws();
                }
                if c.eat("inertia") || c.s.starts_with("(inertia)") {
                    inertia = true;
                }
                map.add_node(Node {
                    id: name.clone(),
                    label: name,
                    x,
                    y,
                    class_name: Some("component".into()),
                    label_offset: (label.map(|l| l.0), label.map(|l| l.1)),
                    inertia,
                    strategy,
                    ..Node::default()
                });
            }
            "evolve" => {
                c.eat("evolve");
                c.ws();
                let name = c.name().ok_or_else(|| WardleyParseError {
                    message: "bad evolve".into(),
                })?;
                c.ws();
                let Some(target) = c.decimal() else {
                    return err("bad evolve target");
                };
                if let Some(node) = map.nodes.get(&name) {
                    let t = to_percent(target, &format!("Evolve target for \"{name}\""))?;
                    let y = node.y;
                    map.trends.insert(name, (t, y));
                }
            }
            "pipeline" => {
                c.eat("pipeline");
                c.ws();
                let parent = c.name().ok_or_else(|| WardleyParseError {
                    message: "bad pipeline".into(),
                })?;
                c.ws();
                if !c.eat("{") {
                    return err("pipeline needs '{'");
                }
                let Some(parent_node) = map.nodes.get_mut(&parent) else {
                    return err(format!(
                        "Pipeline \"{parent}\" must reference an existing component with coordinates."
                    ));
                };
                parent_node.is_pipeline_parent = true;
                let parent_y = parent_node.y;
                let mut pipeline = Pipeline {
                    node_id: parent.clone(),
                    component_ids: Vec::new(),
                };
                for inner in lines.by_ref() {
                    let inner = inner.trim();
                    if inner.is_empty() || inner.starts_with("%%") {
                        continue;
                    }
                    if inner.starts_with('}') {
                        break;
                    }
                    let mut ic = Cur { s: inner };
                    if !ic.eat("component") {
                        return err(format!("unexpected pipeline statement: {inner}"));
                    }
                    ic.ws();
                    let name = ic.name().ok_or_else(|| WardleyParseError {
                        message: "bad pipeline component".into(),
                    })?;
                    ic.ws();
                    if !ic.eat("[") {
                        return err("bad pipeline component");
                    }
                    ic.ws();
                    let evo = ic.decimal().ok_or_else(|| WardleyParseError {
                        message: "bad pipeline component evolution".into(),
                    })?;
                    ic.ws();
                    ic.eat("]");
                    let label = ic.label();
                    let id = format!("{parent}_{name}");
                    let x = to_percent(evo, &format!("Pipeline component \"{name}\" evolution"))?;
                    map.add_node(Node {
                        id: id.clone(),
                        label: name,
                        x,
                        y: parent_y,
                        class_name: Some("pipeline-component".into()),
                        label_offset: (label.map(|l| l.0), label.map(|l| l.1)),
                        in_pipeline: true,
                        ..Node::default()
                    });
                    pipeline.component_ids.push(id);
                }
                map.pipelines.insert(parent, pipeline);
            }
            "note" => {
                c.eat("note");
                c.ws();
                let text = c.string().ok_or_else(|| WardleyParseError {
                    message: "bad note".into(),
                })?;
                let Some((vis, evo)) = c.pair() else {
                    return err("bad note coordinates");
                };
                let x = to_percent(evo, &format!("Note \"{text}\" evolution"))?;
                let y = to_percent(vis, &format!("Note \"{text}\" visibility"))?;
                map.notes.push((text, x, y));
            }
            "annotations" => {
                c.eat("annotations");
                let Some((a, b)) = c.pair() else {
                    return err("bad annotations");
                };
                if map.annotations_box.is_none() {
                    map.annotations_box = Some((
                        to_percent(b, "Annotations box evolution")?,
                        to_percent(a, "Annotations box visibility")?,
                    ));
                }
            }
            "annotation" => {
                c.eat("annotation");
                c.ws();
                let Some(number) = c.int() else {
                    return err("bad annotation number");
                };
                c.ws();
                c.eat(",");
                c.ws();
                let Some((a, b)) = c.pair() else {
                    return err("bad annotation coordinates");
                };
                c.ws();
                let text = c.string();
                map.annotations.push(Annotation {
                    number,
                    x: to_percent(b, &format!("Annotation {number} evolution"))?,
                    y: to_percent(a, &format!("Annotation {number} visibility"))?,
                    text,
                });
            }
            "accelerator" | "deaccelerator" => {
                c.eat(keyword);
                c.ws();
                let name = c.name().ok_or_else(|| WardleyParseError {
                    message: format!("bad {keyword}"),
                })?;
                let Some((a, b)) = c.pair() else {
                    return err(format!("bad {keyword} coordinates"));
                };
                // toCoordinates(visibility = first, evolution = second).
                let x = to_percent(b, &format!("{keyword} \"{name}\" evolution"))?;
                let y = to_percent(a, &format!("{keyword} \"{name}\" visibility"))?;
                if keyword == "accelerator" {
                    map.accelerators.push((name, x, y));
                } else {
                    map.deaccelerators.push((name, x, y));
                }
            }
            _ => {
                // Link: from [port] [arrow] to [port] [;label]
                let Some(from) = c.name() else {
                    return err(format!("unsupported statement: {line}"));
                };
                let from_port = port(&mut c);
                let arrow_text = arrow(&mut c);
                c.ws();
                let Some(to) = c.name() else {
                    return err(format!("unsupported statement: {line}"));
                };
                let to_port = port(&mut c);
                c.ws();
                let ann =
                    c.s.strip_prefix(';')
                        .map(|l| l.trim().to_owned())
                        .filter(|l| !l.is_empty());
                if !c.s.is_empty() && ann.is_none() {
                    return err(format!("unsupported statement: {line}"));
                }
                let dashed = arrow_text
                    .as_deref()
                    .is_some_and(|a| a.contains("-.->") || a.contains(".-."));
                let mut flow = from_port
                    .and_then(flow_from_port)
                    .or_else(|| to_port.and_then(flow_from_port));
                let mut flow_label = None;
                if let Some(a) = arrow_text.as_deref().filter(|a| a.starts_with('+')) {
                    flow_label = a
                        .strip_prefix("+'")
                        .and_then(|r| r.split_once('\''))
                        .map(|(l, _)| l.to_owned());
                    let af = if a.contains("<>") {
                        Some("bidirectional")
                    } else if a.contains('<') {
                        Some("backward")
                    } else if a.contains('>') {
                        Some("forward")
                    } else {
                        None
                    };
                    if flow.is_none() {
                        flow = af;
                    }
                }
                let source = map.resolve_node_id(&from);
                let target = map.resolve_node_id(&to);
                map.links.push(Link {
                    source,
                    target,
                    dashed,
                    label: flow_label.or(ann),
                    flow,
                });
            }
        }
    }
    if !found {
        return err("missing wardley-beta header");
    }
    for n in map.nodes.values() {
        if !n.x.is_finite() || !n.y.is_finite() {
            return err(format!("Node \"{}\" is missing coordinates", n.label));
        }
    }
    Ok(map)
}

// --------------------------------------------------------------- renderer --

struct Theme {
    background: String,
    axis: String,
    axis_text: String,
    grid: String,
    component_fill: String,
    component_stroke: String,
    component_label: String,
    link_stroke: String,
    evolution_stroke: String,
    annotation_fill: String,
}

fn add(parent: &Element, tag: &str, attrs: &[(&str, String)]) -> Element {
    let e = append(parent, tag);
    for (k, v) in attrs {
        set_attr(&e, k, v.clone());
    }
    e
}

fn marker(defs: &Element, id: &str, refx: &str, w: &str, orient: &str, d: &str, fill: &str) {
    let m = add(
        defs,
        "marker",
        &[
            ("id", id.to_owned()),
            ("viewBox", "0 0 10 10".to_owned()),
            ("refX", refx.to_owned()),
            ("refY", "5".to_owned()),
            ("markerWidth", w.to_owned()),
            ("markerHeight", w.to_owned()),
            ("orient", orient.to_owned()),
        ],
    );
    add(
        &m,
        "path",
        &[
            ("d", d.to_owned()),
            ("fill", fill.to_owned()),
            ("stroke", "none".to_owned()),
        ],
    );
}

/// Renders mermaid `wardley-beta` source to a complete SVG document.
///
/// # Errors
/// Returns [`WardleyParseError`] when the source is not a valid map.
#[allow(clippy::too_many_lines)]
pub fn render_wardley(source: &str, id: &str) -> Result<String, WardleyParseError> {
    let config = crate::render::config::detect_init(source);
    let vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let map = parse(source)?;

    let wv = |key: &str, fallback: &str| -> String {
        config
            .theme_variables
            .get("wardley")
            .and_then(|w| w.get(key))
            .and_then(|v| v.as_str())
            .map_or_else(|| fallback.to_owned(), str::to_owned)
    };
    let tv = |key: &str| crate::render::themes::get(&vars, key);
    let theme = Theme {
        background: wv("backgroundColor", &tv("background")),
        axis: wv("axisColor", &tv("lineColor")),
        axis_text: wv("axisTextColor", &tv("primaryTextColor")),
        grid: wv("gridColor", &tv("gridColor")),
        component_fill: wv("componentFill", &tv("background")),
        component_stroke: wv("componentStroke", &tv("lineColor")),
        component_label: wv("componentLabelColor", &tv("primaryTextColor")),
        link_stroke: wv("linkStroke", &tv("lineColor")),
        evolution_stroke: wv("evolutionStroke", &{
            let c = tv("wardleyEvolutionColor");
            if c.is_empty() {
                "#dc3545".to_owned()
            } else {
                c
            }
        }),
        annotation_fill: wv("annotationFill", &tv("background")),
    };
    let annotation_stroke = wv("annotationStroke", &tv("lineColor"));
    let annotation_text = wv("annotationTextColor", &tv("primaryTextColor"));

    let cfg = |key: &str, d: f64| config.section_f64("wardley-beta", key).unwrap_or(d);
    let node_radius = cfg("nodeRadius", 6.0);
    let node_label_offset = cfg("nodeLabelOffset", 8.0);
    let axis_font = cfg("axisFontSize", 12.0);
    let label_font = cfg("labelFontSize", 10.0);
    let pad = cfg("padding", 48.0);
    let show_grid = config
        .raw
        .get("wardley-beta")
        .and_then(|c| c.get("showGrid"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let square_size = node_radius * 1.6;

    let (width, height) = map
        .size
        .unwrap_or_else(|| (cfg("width", 900.0), cfg("height", 600.0)));

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
            crate::render::css_length(width)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!("0 0 {} {}", js_num(width), js_num(height)),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "wardley-beta");

    let root = add(&svg, "g", &[("class", "wardley-map".to_owned())]);
    add(
        &root,
        "rect",
        &[
            ("class", "wardley-background".to_owned()),
            ("width", js_num(width)),
            ("height", js_num(height)),
            ("fill", theme.background.clone()),
        ],
    );

    let chart_w = width - pad * 2.0;
    let chart_h = height - pad * 2.0;
    let project_x = |v: f64| pad + (v / 100.0) * chart_w;
    let project_y = |v: f64| height - pad - (v / 100.0) * chart_h;

    if !map.title.is_empty() {
        let t = add(
            &root,
            "text",
            &[
                ("class", "wardley-title".to_owned()),
                ("x", js_num(width / 2.0)),
                ("y", js_num(pad / 2.0)),
                ("fill", theme.axis_text.clone()),
                ("font-size", js_num(axis_font * 1.05)),
                ("font-weight", "bold".to_owned()),
                ("text-anchor", "middle".to_owned()),
                ("dominant-baseline", "middle".to_owned()),
            ],
        );
        set_text(&t, &map.title);
    }

    // Axes.
    let axes = add(&root, "g", &[("class", "wardley-axes".to_owned())]);
    add(
        &axes,
        "line",
        &[
            ("x1", js_num(pad)),
            ("x2", js_num(width - pad)),
            ("y1", js_num(height - pad)),
            ("y2", js_num(height - pad)),
            ("stroke", theme.axis.clone()),
            ("stroke-width", "1".to_owned()),
        ],
    );
    add(
        &axes,
        "line",
        &[
            ("x1", js_num(pad)),
            ("x2", js_num(pad)),
            ("y1", js_num(pad)),
            ("y2", js_num(height - pad)),
            ("stroke", theme.axis.clone()),
            ("stroke-width", "1".to_owned()),
        ],
    );
    let xt = add(
        &axes,
        "text",
        &[
            (
                "class",
                "wardley-axis-label wardley-axis-label-x".to_owned(),
            ),
            ("x", js_num(pad + chart_w / 2.0)),
            ("y", js_num(height - pad / 4.0)),
            ("fill", theme.axis_text.clone()),
            ("font-size", js_num(axis_font)),
            ("font-weight", "bold".to_owned()),
            ("text-anchor", "middle".to_owned()),
        ],
    );
    set_text(&xt, "Evolution");
    let yx = pad / 3.0;
    let yy = pad + chart_h / 2.0;
    let yt = add(
        &axes,
        "text",
        &[
            (
                "class",
                "wardley-axis-label wardley-axis-label-y".to_owned(),
            ),
            ("x", js_num(yx)),
            ("y", js_num(yy)),
            ("fill", theme.axis_text.clone()),
            ("font-size", js_num(axis_font)),
            ("font-weight", "bold".to_owned()),
            ("text-anchor", "middle".to_owned()),
            (
                "transform",
                format!("rotate(-90 {} {})", js_num(yx), js_num(yy)),
            ),
        ],
    );
    set_text(&yt, "Visibility");

    // Stages.
    let default_stages: Vec<String> = ["Genesis", "Custom Built", "Product", "Commodity"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    let stages = map
        .stages
        .as_ref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&default_stages);
    let stage_g = add(&root, "g", &[("class", "wardley-stages".to_owned())]);
    let positions: Vec<(f64, f64)> = if map.boundaries.len() == stages.len() {
        let mut prev = 0.0;
        map.boundaries
            .iter()
            .map(|b| {
                let p = (prev, *b);
                prev = *b;
                p
            })
            .collect()
    } else {
        let w = 1.0 / stages.len() as f64;
        (0..stages.len())
            .map(|i| (i as f64 * w, (i + 1) as f64 * w))
            .collect()
    };
    for (i, stage) in stages.iter().enumerate() {
        let (s, e) = positions[i];
        let start_x = pad + s * chart_w;
        let end_x = pad + e * chart_w;
        let center = f64::midpoint(start_x, end_x);
        if i > 0 {
            add(
                &stage_g,
                "line",
                &[
                    ("x1", js_num(start_x)),
                    ("x2", js_num(start_x)),
                    ("y1", js_num(pad)),
                    ("y2", js_num(height - pad)),
                    ("stroke", "#000".to_owned()),
                    ("stroke-width", "1".to_owned()),
                    ("stroke-dasharray", "5 5".to_owned()),
                    ("opacity", "0.8".to_owned()),
                ],
            );
        }
        let t = add(
            &stage_g,
            "text",
            &[
                ("class", "wardley-stage-label".to_owned()),
                ("x", js_num(center)),
                ("y", js_num(height - pad / 1.5)),
                ("fill", theme.axis_text.clone()),
                ("font-size", js_num(axis_font - 2.0)),
                ("text-anchor", "middle".to_owned()),
            ],
        );
        set_text(&t, stage);
    }

    if show_grid {
        let g = add(&root, "g", &[("class", "wardley-grid".to_owned())]);
        for i in 1..4 {
            let ratio = f64::from(i) / 4.0;
            let x = pad + chart_w * ratio;
            add(
                &g,
                "line",
                &[
                    ("x1", js_num(x)),
                    ("x2", js_num(x)),
                    ("y1", js_num(pad)),
                    ("y2", js_num(height - pad)),
                    ("stroke", theme.grid.clone()),
                    ("stroke-dasharray", "2 6".to_owned()),
                ],
            );
            let y = height - pad - chart_h * ratio;
            add(
                &g,
                "line",
                &[
                    ("x1", js_num(pad)),
                    ("x2", js_num(width - pad)),
                    ("y1", js_num(y)),
                    ("y2", js_num(y)),
                    ("stroke", theme.grid.clone()),
                    ("stroke-dasharray", "2 6".to_owned()),
                ],
            );
        }
    }

    let mut pos: IndexMap<String, (f64, f64)> = map
        .nodes
        .iter()
        .map(|(id, n)| (id.clone(), (project_x(n.x), project_y(n.y))))
        .collect();

    // Pipelines.
    if !map.pipelines.is_empty() {
        let pg = add(&root, "g", &[("class", "wardley-pipelines".to_owned())]);
        let lg = add(
            &root,
            "g",
            &[("class", "wardley-pipeline-links".to_owned())],
        );
        for pipeline in map.pipelines.values() {
            if pipeline.component_ids.is_empty() {
                continue;
            }
            let mut sorted: Vec<&String> = pipeline
                .component_ids
                .iter()
                .filter(|c| map.nodes.contains_key(*c))
                .collect();
            sorted.sort_by(|a, b| {
                map.nodes[*a]
                    .x
                    .partial_cmp(&map.nodes[*b].x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for w in sorted.windows(2) {
                let (a, b) = (pos[w[0]], pos[w[1]]);
                add(
                    &lg,
                    "line",
                    &[
                        ("class", "wardley-pipeline-evolution-link".to_owned()),
                        ("x1", js_num(a.0)),
                        ("y1", js_num(a.1)),
                        ("x2", js_num(b.0)),
                        ("y2", js_num(b.1)),
                        ("stroke", theme.link_stroke.clone()),
                        ("stroke-width", "1".to_owned()),
                        ("stroke-dasharray", "4 4".to_owned()),
                    ],
                );
            }
            let (mut min_x, mut max_x, mut y) = (f64::INFINITY, f64::NEG_INFINITY, 0.0);
            for cid in &pipeline.component_ids {
                if let Some(&(px, py)) = pos.get(cid) {
                    min_x = min_x.min(px);
                    max_x = max_x.max(px);
                    y = py;
                }
            }
            if min_x != f64::INFINITY && max_x != f64::NEG_INFINITY {
                let padding = 15.0;
                let h = node_radius * 4.0;
                let top = y - h / 2.0;
                if let Some(p) = pos.get_mut(&pipeline.node_id) {
                    p.0 = f64::midpoint(min_x, max_x);
                    p.1 = top - square_size / 6.0;
                }
                add(
                    &pg,
                    "rect",
                    &[
                        ("class", "wardley-pipeline-box".to_owned()),
                        ("x", js_num(min_x - padding)),
                        ("y", js_num(top)),
                        ("width", js_num(max_x - min_x + padding * 2.0)),
                        ("height", js_num(h)),
                        ("fill", "none".to_owned()),
                        ("stroke", theme.axis.clone()),
                        ("stroke-width", "1.5".to_owned()),
                        ("rx", "4".to_owned()),
                        ("ry", "4".to_owned()),
                    ],
                );
            }
        }
    }

    // Links.
    let links_g = add(&root, "g", &[("class", "wardley-links".to_owned())]);
    let valid: Vec<&Link> = map
        .links
        .iter()
        .filter(|l| {
            pos.contains_key(&l.source)
                && pos.contains_key(&l.target)
                && !map
                    .pipelines
                    .get(&l.target)
                    .is_some_and(|p| p.component_ids.contains(&l.source))
        })
        .collect();
    let radius_of = |id: &str| {
        if map.nodes.get(id).is_some_and(|n| n.is_pipeline_parent) {
            square_size / 2.0_f64.sqrt()
        } else {
            node_radius
        }
    };
    for l in &valid {
        let (sp, tp) = (pos[&l.source], pos[&l.target]);
        let (rs, rt) = (radius_of(&l.source), radius_of(&l.target));
        let (dx, dy) = (tp.0 - sp.0, tp.1 - sp.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let (rdx, rdy) = (sp.0 - tp.0, sp.1 - tp.1);
        let rdist = (rdx * rdx + rdy * rdy).sqrt();
        let mut attrs = vec![
            (
                "class",
                format!(
                    "wardley-link{}",
                    if l.dashed {
                        " wardley-link--dashed"
                    } else {
                        ""
                    }
                ),
            ),
            ("x1", js_num(sp.0 + (dx / dist) * rs)),
            ("y1", js_num(sp.1 + (dy / dist) * rs)),
            ("x2", js_num(tp.0 + (rdx / rdist) * rt)),
            ("y2", js_num(tp.1 + (rdy / rdist) * rt)),
            ("stroke", theme.link_stroke.clone()),
            ("stroke-width", "1".to_owned()),
        ];
        if l.dashed {
            attrs.push(("stroke-dasharray", "6 6".to_owned()));
        }
        if matches!(l.flow, Some("forward" | "bidirectional")) {
            attrs.push(("marker-end", format!("url(#link-arrow-end-{id})")));
        }
        if matches!(l.flow, Some("backward" | "bidirectional")) {
            attrs.push(("marker-start", format!("url(#link-arrow-start-{id})")));
        }
        add(&links_g, "line", &attrs);
    }
    for l in valid.iter().filter(|l| l.label.is_some()) {
        let (sp, tp) = (pos[&l.source], pos[&l.target]);
        let mid_x = f64::midpoint(sp.0, tp.0);
        let mid_y = f64::midpoint(sp.1, tp.1);
        let dx = tp.0 - sp.0;
        let dy = tp.1 - sp.1;
        let dist = (dx * dx + dy * dy).sqrt();
        let offset = 8.0;
        let lx = mid_x + (dy / dist) * offset;
        let ly = mid_y + (-dx / dist) * offset;
        let mut angle = (crate::mathx::atan2(dy, dx) * 180.0) / std::f64::consts::PI;
        if !(-90.0..=90.0).contains(&angle) {
            angle += 180.0;
        }
        let t = add(
            &links_g,
            "text",
            &[
                ("class", "wardley-link-label".to_owned()),
                ("x", js_num(lx)),
                ("y", js_num(ly)),
                ("fill", theme.axis_text.clone()),
                ("font-size", js_num(label_font)),
                ("text-anchor", "middle".to_owned()),
                ("dominant-baseline", "middle".to_owned()),
                (
                    "transform",
                    format!("rotate({} {} {})", js_num(angle), js_num(lx), js_num(ly)),
                ),
            ],
        );
        set_text(&t, l.label.as_deref().unwrap_or(""));
    }

    // Trends.
    let trend_g = add(&root, "g", &[("class", "wardley-trends".to_owned())]);
    for (node_id, (tx, ty)) in &map.trends {
        let Some(&origin) = pos.get(node_id) else {
            continue;
        };
        let (target_x, target_y) = (project_x(*tx), project_y(*ty));
        let (dx, dy) = (target_x - origin.0, target_y - origin.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let shorten = node_radius + 2.0;
        let (ax, ay) = if dist > shorten {
            (
                target_x - (dx / dist) * shorten,
                target_y - (dy / dist) * shorten,
            )
        } else {
            (target_x, target_y)
        };
        add(
            &trend_g,
            "line",
            &[
                ("class", "wardley-trend".to_owned()),
                ("x1", js_num(origin.0)),
                ("y1", js_num(origin.1)),
                ("x2", js_num(ax)),
                ("y2", js_num(ay)),
                ("stroke", theme.evolution_stroke.clone()),
                ("stroke-width", "1".to_owned()),
                ("stroke-dasharray", "4 4".to_owned()),
                ("marker-end", format!("url(#arrow-{id})")),
            ],
        );
    }

    // Nodes.
    let nodes_g = add(&root, "g", &[("class", "wardley-nodes".to_owned())]);
    let cos30 = crate::mathx::cos(std::f64::consts::PI / 6.0);
    let sin30 = crate::mathx::sin(std::f64::consts::PI / 6.0);
    let small_r = node_radius * 0.7;
    let tri_r = node_radius * 1.2;
    for n in map.nodes.values() {
        let (px, py) = pos[&n.id];
        let class = ["wardley-node".to_owned()]
            .into_iter()
            .chain(n.class_name.as_ref().map(|c| format!("wardley-node--{c}")))
            .collect::<Vec<_>>()
            .join(" ");
        let g = add(&nodes_g, "g", &[("class", class)]);
        let overlay = |cls: &str, fill: &str, stroke: &str| {
            add(
                &g,
                "circle",
                &[
                    ("class", cls.to_owned()),
                    ("cx", js_num(px)),
                    ("cy", js_num(py)),
                    ("r", js_num(node_radius * 2.0)),
                    ("fill", fill.to_owned()),
                    ("stroke", stroke.to_owned()),
                    ("stroke-width", "1".to_owned()),
                ],
            );
        };
        let strategy = n.strategy.as_deref();
        match strategy {
            Some("outsource") => {
                overlay("wardley-outsource-overlay", "#666", &theme.component_stroke);
            }
            Some("buy") => overlay("wardley-buy-overlay", "#ccc", &theme.component_stroke),
            Some("build") => overlay("wardley-build-overlay", "#eee", "#000"),
            Some("market") => overlay("wardley-market-overlay", "white", &theme.component_stroke),
            _ => {}
        }
        let is_anchor = n.class_name.as_deref() == Some("anchor");
        if !n.is_pipeline_parent && strategy != Some("market") && !is_anchor {
            add(
                &g,
                "circle",
                &[
                    ("cx", js_num(px)),
                    ("cy", js_num(py)),
                    ("r", js_num(node_radius)),
                    ("fill", theme.component_fill.clone()),
                    ("stroke", theme.component_stroke.clone()),
                    ("stroke-width", "1".to_owned()),
                ],
            );
        }
        if strategy == Some("market") {
            let pts = [
                (px, py - tri_r),
                (px - tri_r * cos30, py + tri_r * sin30),
                (px + tri_r * cos30, py + tri_r * sin30),
            ];
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                add(
                    &g,
                    "line",
                    &[
                        ("class", "wardley-market-line".to_owned()),
                        ("x1", js_num(pts[a].0)),
                        ("y1", js_num(pts[a].1)),
                        ("x2", js_num(pts[b].0)),
                        ("y2", js_num(pts[b].1)),
                        ("stroke", theme.component_stroke.clone()),
                        ("stroke-width", "1".to_owned()),
                    ],
                );
            }
            for p in pts {
                add(
                    &g,
                    "circle",
                    &[
                        ("class", "wardley-market-dot".to_owned()),
                        ("cx", js_num(p.0)),
                        ("cy", js_num(p.1)),
                        ("r", js_num(small_r)),
                        ("fill", "white".to_owned()),
                        ("stroke", theme.component_stroke.clone()),
                        ("stroke-width", "2".to_owned()),
                    ],
                );
            }
        }
        if n.is_pipeline_parent {
            add(
                &g,
                "rect",
                &[
                    ("x", js_num(px - square_size / 2.0)),
                    ("y", js_num(py - square_size / 2.0)),
                    ("width", js_num(square_size)),
                    ("height", js_num(square_size)),
                    ("fill", theme.component_fill.clone()),
                    ("stroke", theme.component_stroke.clone()),
                    ("stroke-width", "1".to_owned()),
                ],
            );
        }
        if n.inertia {
            let mut offset = if n.is_pipeline_parent {
                square_size / 2.0 + 15.0
            } else {
                node_radius + 15.0
            };
            if strategy.is_some() {
                offset += node_radius + 10.0;
            }
            let line_h = if n.is_pipeline_parent {
                square_size
            } else {
                node_radius * 2.0
            };
            add(
                &g,
                "line",
                &[
                    ("class", "wardley-inertia".to_owned()),
                    ("x1", js_num(px + offset)),
                    ("y1", js_num(py - line_h / 2.0)),
                    ("x2", js_num(px + offset)),
                    ("y2", js_num(py + line_h / 2.0)),
                    ("stroke", theme.component_stroke.clone()),
                    ("stroke-width", "6".to_owned()),
                ],
            );
        }
        // Label.
        let (lx, ly) = if is_anchor {
            (
                n.label_offset.0.map_or(px, |o| px + o),
                n.label_offset.1.map_or(py - 3.0, |o| py + o),
            )
        } else {
            let mut dx = node_label_offset;
            let mut dy = -node_label_offset;
            if strategy.is_some() && n.label_offset.0.is_none() {
                dx += 10.0;
            }
            if strategy.is_some() && n.label_offset.1.is_none() {
                dy -= 10.0;
            }
            (
                px + n.label_offset.0.unwrap_or(dx),
                py + n.label_offset.1.unwrap_or(dy),
            )
        };
        let fill = match n.class_name.as_deref() {
            Some("evolved") => theme.evolution_stroke.clone(),
            Some("anchor") => "#000".to_owned(),
            _ => theme.component_label.clone(),
        };
        let t = add(
            &g,
            "text",
            &[
                ("x", js_num(lx)),
                ("y", js_num(ly)),
                ("class", "wardley-node-label".to_owned()),
                ("fill", fill),
                ("font-size", js_num(label_font)),
                (
                    "font-weight",
                    if is_anchor { "bold" } else { "normal" }.to_owned(),
                ),
                (
                    "text-anchor",
                    if is_anchor { "middle" } else { "start" }.to_owned(),
                ),
                (
                    "dominant-baseline",
                    if is_anchor { "middle" } else { "auto" }.to_owned(),
                ),
            ],
        );
        set_text(&t, &n.label);
    }

    // Annotations.
    if !map.annotations.is_empty() {
        let ag = add(&root, "g", &[("class", "wardley-annotations".to_owned())]);
        for a in &map.annotations {
            let (cx, cy) = (project_x(a.x), project_y(a.y));
            let node = add(&ag, "g", &[("class", "wardley-annotation".to_owned())]);
            add(
                &node,
                "circle",
                &[
                    ("cx", js_num(cx)),
                    ("cy", js_num(cy)),
                    ("r", "10".to_owned()),
                    ("fill", "white".to_owned()),
                    ("stroke", theme.axis.clone()),
                    ("stroke-width", "1.5".to_owned()),
                ],
            );
            let t = add(
                &node,
                "text",
                &[
                    ("x", js_num(cx)),
                    ("y", js_num(cy)),
                    ("text-anchor", "middle".to_owned()),
                    ("dominant-baseline", "central".to_owned()),
                    ("font-size", "10".to_owned()),
                    ("fill", theme.axis_text.clone()),
                    ("font-weight", "bold".to_owned()),
                ],
            );
            set_text(&t, &a.number.to_string());
        }
        if let Some((bx, by)) = map.annotations_box {
            let mut box_x = project_x(bx);
            let mut box_y = project_y(by);
            let padding = 10.0;
            let line_height = 16.0;
            let group = add(&ag, "g", &[("class", "wardley-annotations-box".to_owned())]);
            let mut sorted: Vec<&Annotation> = map
                .annotations
                .iter()
                .filter(|a| a.text.is_some())
                .collect();
            sorted.sort_by_key(|a| a.number);
            if !sorted.is_empty() {
                let tm = crate::text::TextMeasurer::new();
                let mut max_w: f64 = 0.0;
                let mut max_h: f64 = 0.0;
                let lines: Vec<String> = sorted
                    .iter()
                    .map(|a| format!("{}. {}", a.number, a.text.as_deref().unwrap_or("")))
                    .collect();
                for l in &lines {
                    max_w = max_w.max(tm.measure_advance(l, 11.0));
                    max_h = max_h.max(tm.ink_height(l, 11.0));
                }
                let box_w = max_w + padding * 2.0 + 105.0;
                let box_h = sorted.len() as f64 * line_height + padding * 2.0 + max_h / 2.0;
                let min_x = pad;
                let max_x = width - pad - box_w;
                let min_y = pad;
                let max_y = height - pad - box_h;
                box_x = box_x.min(max_x).max(min_x);
                box_y = box_y.min(max_y).max(min_y);
                add(
                    &group,
                    "rect",
                    &[
                        ("x", js_num(box_x)),
                        ("y", js_num(box_y)),
                        ("width", js_num(box_w)),
                        ("height", js_num(box_h)),
                        ("fill", "white".to_owned()),
                        ("stroke", theme.axis.clone()),
                        ("stroke-width", "1.5".to_owned()),
                        ("rx", "4".to_owned()),
                        ("ry", "4".to_owned()),
                    ],
                );
                for (idx, l) in lines.iter().enumerate() {
                    let t = add(
                        &group,
                        "text",
                        &[
                            ("x", js_num(box_x + padding)),
                            (
                                "y",
                                js_num(box_y + padding + (idx + 1) as f64 * line_height),
                            ),
                            ("font-size", "11".to_owned()),
                            ("fill", theme.axis_text.clone()),
                            ("text-anchor", "start".to_owned()),
                            ("dominant-baseline", "middle".to_owned()),
                        ],
                    );
                    set_text(&t, l);
                }
            }
        }
    }
    let _ = (&annotation_stroke, &annotation_text, &theme.annotation_fill);

    if !map.notes.is_empty() {
        let g = add(&root, "g", &[("class", "wardley-notes".to_owned())]);
        for (text, x, y) in &map.notes {
            let t = add(
                &g,
                "text",
                &[
                    ("x", js_num(project_x(*x))),
                    ("y", js_num(project_y(*y))),
                    ("text-anchor", "start".to_owned()),
                    ("font-size", "11".to_owned()),
                    ("fill", theme.axis_text.clone()),
                    ("font-weight", "bold".to_owned()),
                ],
            );
            set_text(&t, text);
        }
    }

    for (items, class, left) in [
        (&map.accelerators, "wardley-accelerators", false),
        (&map.deaccelerators, "wardley-deaccelerators", true),
    ] {
        if items.is_empty() {
            continue;
        }
        let g = add(&root, "g", &[("class", class.to_owned())]);
        for (name, x, y) in items {
            let (ax, ay) = (project_x(*x), project_y(*y));
            let (w, h, head) = (60.0, 30.0, 20.0);
            let d = if left {
                format!(
                    "\n        M {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        Z\n      ",
                    js_num(ax + w),
                    js_num(ay - h / 2.0),
                    js_num(ax + head),
                    js_num(ay - h / 2.0),
                    js_num(ax + head),
                    js_num(ay - h / 2.0 - 8.0),
                    js_num(ax),
                    js_num(ay),
                    js_num(ax + head),
                    js_num(ay + h / 2.0 + 8.0),
                    js_num(ax + head),
                    js_num(ay + h / 2.0),
                    js_num(ax + w),
                    js_num(ay + h / 2.0),
                )
            } else {
                format!(
                    "\n        M {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        L {} {}\n        Z\n      ",
                    js_num(ax),
                    js_num(ay - h / 2.0),
                    js_num(ax + w - head),
                    js_num(ay - h / 2.0),
                    js_num(ax + w - head),
                    js_num(ay - h / 2.0 - 8.0),
                    js_num(ax + w),
                    js_num(ay),
                    js_num(ax + w - head),
                    js_num(ay + h / 2.0 + 8.0),
                    js_num(ax + w - head),
                    js_num(ay + h / 2.0),
                    js_num(ax),
                    js_num(ay + h / 2.0),
                )
            };
            add(
                &g,
                "path",
                &[
                    ("d", d),
                    ("fill", "white".to_owned()),
                    ("stroke", theme.component_stroke.clone()),
                    ("stroke-width", "1".to_owned()),
                ],
            );
            let t = add(
                &g,
                "text",
                &[
                    ("x", js_num(ax + w / 2.0)),
                    ("y", js_num(ay + h / 2.0 + 15.0)),
                    ("text-anchor", "middle".to_owned()),
                    ("font-size", "10".to_owned()),
                    ("fill", theme.axis_text.clone()),
                    ("font-weight", "bold".to_owned()),
                ],
            );
            set_text(&t, name);
        }
    }

    // Markers.
    let defs = add(&svg, "defs", &[]);
    marker(
        &defs,
        &format!("arrow-{id}"),
        "9",
        "6",
        "auto-start-reverse",
        "M 0 0 L 10 5 L 0 10 z",
        &theme.evolution_stroke,
    );
    marker(
        &defs,
        &format!("link-arrow-end-{id}"),
        "9",
        "5",
        "auto",
        "M 0 0 L 10 5 L 0 10 z",
        &theme.link_stroke,
    );
    marker(
        &defs,
        &format!("link-arrow-start-{id}"),
        "1",
        "5",
        "auto",
        "M 10 0 L 0 5 L 10 10 z",
        &theme.link_stroke,
    );

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
