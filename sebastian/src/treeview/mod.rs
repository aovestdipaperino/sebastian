//! `treeView-beta`: a port of mermaid's `treeView` db and renderer
//! (mermaid 11.15.0). Rows are text labels joined by `treeView-node-line`
//! connectors; sizes come from `getBBox` of each label.

use crate::svg::{Element, append, js_num, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

/// A parse error for treeView source.
#[derive(Debug)]
pub struct TreeViewParseError {
    pub message: String,
}

impl std::fmt::Display for TreeViewParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "treeView parse error: {}", self.message)
    }
}

impl std::error::Error for TreeViewParseError {}

struct Node {
    level: i64,
    name: String,
    children: Vec<usize>,
    /// Bounding box assigned while drawing: x, y, width, height.
    bbox: (f64, f64, f64, f64),
}

/// Mermaid's imperative stack: an implicit root `/` at level -1.
fn build(names: &[(i64, String)]) -> Vec<Node> {
    let mut nodes = vec![Node {
        level: -1,
        name: "/".to_owned(),
        children: Vec::new(),
        bbox: (0.0, 0.0, 0.0, 0.0),
    }];
    let mut stack = vec![0usize];
    for (level, name) in names {
        while stack.last().is_some_and(|&top| *level <= nodes[top].level) {
            stack.pop();
        }
        let idx = nodes.len();
        nodes.push(Node {
            level: *level,
            name: name.clone(),
            children: Vec::new(),
            bbox: (0.0, 0.0, 0.0, 0.0),
        });
        if let Some(&parent) = stack.last() {
            nodes[parent].children.push(idx);
        }
        stack.push(idx);
    }
    nodes
}

fn parse(source: &str) -> Result<Vec<(i64, String)>, TreeViewParseError> {
    let mut found_header = false;
    let mut names = Vec::new();
    for raw in source.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        if !found_header {
            if trimmed == "treeView-beta" {
                found_header = true;
                continue;
            }
            return Err(TreeViewParseError {
                message: format!("expected treeView-beta header, got {trimmed:?}"),
            });
        }
        if trimmed == "title" || trimmed.starts_with("title ") || trimmed.starts_with("title\t") {
            continue;
        }
        let indent =
            i64::try_from(raw.len() - raw.trim_start_matches([' ', '\t']).len()).unwrap_or(0);
        let q = trimmed.chars().next().unwrap_or(' ');
        if (q == '"' || q == '\'') && trimmed.len() >= 2 && trimmed.ends_with(q) {
            names.push((indent, trimmed[1..trimmed.len() - 1].to_owned()));
        } else {
            return Err(TreeViewParseError {
                message: format!("expected a quoted node name, got {trimmed:?}"),
            });
        }
    }
    if !found_header {
        return Err(TreeViewParseError {
            message: "missing treeView-beta header".to_owned(),
        });
    }
    Ok(names)
}

struct Cfg {
    row_indent: f64,
    padding_x: f64,
    padding_y: f64,
    line_thickness: f64,
    font_size: f64,
    label_font_size: String,
    label_color: String,
    line_color: String,
}

fn num(config: &crate::render::config::RenderConfig, key: &str, default: f64) -> f64 {
    config.section_f64("treeView", key).unwrap_or(default)
}

fn theme_str(config: &crate::render::config::RenderConfig, key: &str, default: &str) -> String {
    config
        .theme_variables
        .get("treeView")
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .map_or_else(|| default.to_owned(), str::to_owned)
}

fn f32r(v: f64) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    f64::from(v as f32)
}

struct Draw<'a> {
    cfg: &'a Cfg,
    m: &'a TextMeasurer,
    nodes: Vec<Node>,
    total_height: f64,
    total_width: f64,
}

impl Draw<'_> {
    fn line(&self, parent: &Element, x1: f64, y1: f64, x2: f64, y2: f64) {
        let l = append(parent, "line");
        set_attr(&l, "x1", js_num(x1));
        set_attr(&l, "y1", js_num(y1));
        set_attr(&l, "x2", js_num(x2));
        set_attr(&l, "y2", js_num(y2));
        set_attr(&l, "stroke-width", js_num(self.cfg.line_thickness));
        set_attr(&l, "class", "treeView-node-line");
    }

    fn process(&mut self, parent: &Element, idx: usize, depth: usize) {
        let cfg = self.cfg;
        let indent = depth as f64 * (cfg.row_indent + cfg.padding_x);
        // positionLabel: the label is appended first, then measured.
        let name = self.nodes[idx].name.clone();
        let text = append(parent, "text");
        set_text(&text, &name);
        set_attr(&text, "dominant-baseline", "middle");
        set_attr(&text, "class", "treeView-node-label");
        let label_h = f32r(self.m.ink_height(&name, cfg.font_size));
        let label_w = f32r(self.m.measure_width(&name, cfg.font_size));
        let height = label_h + cfg.padding_y * 2.0;
        let width = label_w + cfg.padding_x * 2.0;
        set_attr(&text, "x", js_num(indent + cfg.padding_x));
        set_attr(&text, "y", js_num(self.total_height + height / 2.0));
        self.nodes[idx].bbox = (indent, self.total_height, width, height);

        self.line(
            parent,
            indent - cfg.row_indent,
            self.total_height + height / 2.0,
            indent,
            self.total_height + height / 2.0,
        );
        self.total_width = self.total_width.max(indent + width);
        self.total_height += height;

        let children = self.nodes[idx].children.clone();
        for &c in &children {
            self.process(parent, c, depth + 1);
        }
        let (x, y, _, h) = self.nodes[idx].bbox;
        if let Some(&last) = children.last() {
            let (_, end_y, _, end_h) = self.nodes[last].bbox;
            self.line(
                parent,
                x + cfg.padding_x,
                y + h,
                x + cfg.padding_x,
                end_y + end_h / 2.0 + cfg.line_thickness / 2.0,
            );
        }
    }
}

/// Renders mermaid treeView source to a complete SVG document string.
///
/// # Errors
/// Returns a [`TreeViewParseError`] when the source is not a valid treeView.
pub fn render_treeview(source: &str, id: &str) -> Result<String, TreeViewParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let names = parse(source)?;

    let label_font_size = theme_str(&config, "labelFontSize", "16px");
    let cfg = Cfg {
        row_indent: num(&config, "rowIndent", 10.0),
        padding_x: num(&config, "paddingX", 5.0),
        padding_y: num(&config, "paddingY", 5.0),
        line_thickness: num(&config, "lineThickness", 1.0),
        font_size: label_font_size
            .trim_end_matches("px")
            .trim()
            .parse()
            .unwrap_or(16.0),
        label_font_size,
        label_color: theme_str(&config, "labelColor", "black"),
        line_color: theme_str(&config, "lineColor", "black"),
    };

    let svg = crate::svg::new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style = append(&svg, "style");
    set_text(
        &style,
        &crate::render::css::themed_treeview_css(
            id,
            &theme_vars,
            &cfg.label_font_size,
            &cfg.label_color,
            &cfg.line_color,
        ),
    );
    let _empty = append(&svg, "g");
    let tree = append(&svg, "g");
    set_attr(&tree, "class", "tree-view");

    let m = TextMeasurer::new();
    let mut draw = Draw {
        cfg: &cfg,
        m: &m,
        nodes: build(&names),
        total_height: 0.0,
        total_width: 0.0,
    };
    draw.process(&tree, 0, 0);

    set_attr(
        &svg,
        "viewBox",
        format!(
            "-{} 0 {} {}",
            js_num(cfg.line_thickness / 2.0),
            js_num(draw.total_width),
            js_num(draw.total_height)
        ),
    );
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(draw.total_width)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "treeView");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}
