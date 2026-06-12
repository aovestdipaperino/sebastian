//! Port of `flowDb.ts` — the parse-time database for flowcharts.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::IndexMap;

use crate::render::data::{LayoutData, RenderEdge, RenderNode};

const MERMAID_DOM_ID_PREFIX: &str = "flowchart-";

#[derive(Debug, Clone, Default)]
pub struct FlowText {
    pub text: String,
    pub label_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct FlowVertex {
    pub id: String,
    pub text: Option<String>,
    pub label_type: String,
    pub vertex_type: Option<String>,
    pub styles: Vec<String>,
    pub classes: Vec<String>,
    pub dir: Option<String>,
    pub dom_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct FlowEdge {
    pub id: String,
    pub is_user_defined_id: bool,
    pub start: String,
    pub end: String,
    pub edge_type: Option<String>,
    pub stroke: Option<String>,
    pub length: f64,
    pub text: String,
    pub label_type: String,
    pub classes: Vec<String>,
    pub interpolate: Option<String>,
    pub style: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FlowClass {
    pub styles: Vec<String>,
    pub text_styles: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FlowSubGraph {
    pub id: String,
    pub nodes: Vec<String>,
    pub title: String,
    pub dir: Option<String>,
    pub label_type: String,
    pub classes: Vec<String>,
}

/// Link metadata produced by `destructLink`.
#[derive(Debug, Clone, Default)]
pub struct LinkInfo {
    pub link_type: String,
    pub stroke: String,
    pub length: f64,
    pub text: Option<FlowText>,
    pub id: Option<String>,
}

#[derive(Debug, Default)]
pub struct FlowDb {
    pub vertices: IndexMap<String, FlowVertex>,
    pub edges: Vec<FlowEdge>,
    pub default_interpolate: Option<String>,
    pub default_style: Option<Vec<String>>,
    pub classes: IndexMap<String, FlowClass>,
    pub sub_graphs: Vec<FlowSubGraph>,
    pub sub_graph_lookup: IndexMap<String, usize>,
    pub direction: Option<String>,
    pub vertex_counter: u64,
    pub sub_count: u64,
}

/// `common.sanitizeText` at securityLevel `strict`; plain labels pass
/// through unchanged.
#[must_use]
pub fn sanitize_text(text: &str) -> String {
    text.to_owned()
}

/// `nonMarkdownToHTML`: literal `\n` sequences and newlines become `<br />`.
#[must_use]
pub fn newlines_to_br(text: &str) -> String {
    text.replace("\\n", "<br />").replace('\n', "<br />")
}

/// innerHTML parses `<Tag>` markup as elements and `DOMPurify` unwraps the
/// non-whitelisted ones, keeping their content. `<br>` variants survive.
pub fn strip_html_tags(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<'
            && i + 1 < chars.len()
            && (chars[i + 1].is_ascii_alphabetic() || chars[i + 1] == '/')
            && let Some(close) = chars[i + 1..].iter().position(|&c| c == '>')
        {
            let tag: String = chars[i + 1..i + 1 + close].iter().collect();
            let name: String = tag
                .trim_start_matches('/')
                .chars()
                .take_while(char::is_ascii_alphanumeric)
                .collect::<String>()
                .to_lowercase();
            if name == "br" {
                out.push('<');
                out.push_str(&tag);
                out.push('>');
            }
            i += close + 2;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Decodes HTML entities the way `span.html(...)` (innerHTML) does in the
/// browser pipeline. Tags like `<br/>` pass through untouched.
pub fn decode_html_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&'
            && let Some(semi) = chars[i + 1..].iter().take(12).position(|&c| c == ';')
        {
            let entity: String = chars[i + 1..i + 1 + semi].iter().collect();
            let decoded: Option<String> = if let Some(num) = entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
            {
                u32::from_str_radix(num, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
            } else if let Some(num) = entity.strip_prefix('#') {
                num.parse::<u32>()
                    .ok()
                    .and_then(char::from_u32)
                    .map(String::from)
            } else {
                match entity.as_str() {
                    "amp" => Some("&".into()),
                    "lt" => Some("<".into()),
                    "gt" => Some(">".into()),
                    "quot" => Some("\"".into()),
                    "apos" => Some("'".into()),
                    "nbsp" => Some("\u{a0}".into()),
                    "rarr" => Some("\u{2192}".into()),
                    "larr" => Some("\u{2190}".into()),
                    "uarr" => Some("\u{2191}".into()),
                    "darr" => Some("\u{2193}".into()),
                    "harr" => Some("\u{2194}".into()),
                    "mdash" => Some("\u{2014}".into()),
                    "ndash" => Some("\u{2013}".into()),
                    "hellip" => Some("\u{2026}".into()),
                    "times" => Some("\u{d7}".into()),
                    "middot" => Some("\u{b7}".into()),
                    "bull" => Some("\u{2022}".into()),
                    "deg" => Some("\u{b0}".into()),
                    "plusmn" => Some("\u{b1}".into()),
                    "le" => Some("\u{2264}".into()),
                    "ge" => Some("\u{2265}".into()),
                    "ne" => Some("\u{2260}".into()),
                    "asymp" => Some("\u{2248}".into()),
                    "infin" => Some("\u{221e}".into()),
                    "copy" => Some("\u{a9}".into()),
                    "reg" => Some("\u{ae}".into()),
                    "trade" => Some("\u{2122}".into()),
                    "lpar" => Some("(".into()),
                    "rpar" => Some(")".into()),
                    "lbrack" => Some("[".into()),
                    "rbrack" => Some("]".into()),
                    "lbrace" => Some("{".into()),
                    "rbrace" => Some("}".into()),
                    "sol" => Some("/".into()),
                    "bsol" => Some("\\".into()),
                    "num" => Some("#".into()),
                    "percnt" => Some("%".into()),
                    "ast" => Some("*".into()),
                    "comma" => Some(",".into()),
                    "colon" => Some(":".into()),
                    "semi" => Some(";".into()),
                    "quest" => Some("?".into()),
                    "excl" => Some("!".into()),
                    "commat" => Some("@".into()),
                    "lowbar" => Some("_".into()),
                    "equals" => Some("=".into()),
                    "plus" => Some("+".into()),
                    "minus" => Some("\u{2212}".into()),
                    "dollar" => Some("$".into()),
                    _ => None,
                }
            };
            if let Some(d) = decoded {
                out.push_str(&d);
                i += semi + 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

impl FlowDb {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_direction(&mut self, dir: &str) {
        let mut direction = dir.trim().to_owned();
        if direction.contains('<') {
            "RL".clone_into(&mut direction);
        }
        if direction.contains('^') {
            "BT".clone_into(&mut direction);
        }
        if direction.contains('>') {
            "LR".clone_into(&mut direction);
        }
        if direction.contains('v') {
            "TB".clone_into(&mut direction);
        }
        if direction == "TD" {
            "TB".clone_into(&mut direction);
        }
        self.direction = Some(direction);
    }

    #[must_use]
    pub fn get_direction(&self) -> String {
        self.direction.clone().unwrap_or_else(|| "TB".to_owned())
    }

    /// `addVertex(id, textObj, type, style, classes, dir, props, metadata)`.
    pub fn add_vertex(
        &mut self,
        id: &str,
        text_obj: Option<&FlowText>,
        vertex_type: Option<&str>,
        style: &[String],
        classes: &[String],
        dir: Option<&str>,
    ) {
        if id.trim().is_empty() {
            return;
        }

        // Metadata on edges (id collision) is rare; supported minimally.
        if self.edges.iter().any(|e| e.id == id) {
            return;
        }

        if !self.vertices.contains_key(id) {
            self.vertices.insert(
                id.to_owned(),
                FlowVertex {
                    id: id.to_owned(),
                    label_type: "text".to_owned(),
                    dom_id: format!("{MERMAID_DOM_ID_PREFIX}{id}-{}", self.vertex_counter),
                    ..Default::default()
                },
            );
        }
        self.vertex_counter += 1;

        let vertex = self.vertices.get_mut(id).expect("vertex inserted");

        if let Some(text_obj) = text_obj {
            let mut txt = sanitize_text(text_obj.text.trim());
            vertex.label_type.clone_from(&text_obj.label_type);
            if txt.starts_with('"') && txt.ends_with('"') && txt.len() >= 2 {
                txt.pop();
                txt.remove(0);
            }
            vertex.text = Some(txt);
        } else if vertex.text.is_none() {
            vertex.text = Some(id.to_owned());
        }

        if let Some(t) = vertex_type {
            vertex.vertex_type = Some(t.to_owned());
        }
        for s in style {
            vertex.styles.push(s.clone());
        }
        for c in classes {
            vertex.classes.push(c.clone());
        }
        if let Some(d) = dir {
            vertex.dir = Some(d.to_owned());
        }
    }

    pub fn add_single_link(&mut self, start: &str, end: &str, link: &LinkInfo, id: Option<&str>) {
        let mut edge = FlowEdge {
            start: start.to_owned(),
            end: end.to_owned(),
            text: String::new(),
            label_type: "text".to_owned(),
            interpolate: self.default_interpolate.clone(),
            ..Default::default()
        };
        if let Some(text_obj) = &link.text {
            let mut text = sanitize_text(text_obj.text.trim());
            if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
                text.pop();
                text.remove(0);
            }
            edge.text = text;
            edge.label_type.clone_from(&text_obj.label_type);
        }
        edge.edge_type = Some(link.link_type.clone());
        edge.stroke = Some(link.stroke.clone());
        edge.length = if link.length > 10.0 {
            10.0
        } else {
            link.length
        };

        let user_id = id.filter(|id| !self.edges.iter().any(|e| e.id == *id));
        if let Some(id) = user_id {
            id.clone_into(&mut edge.id);
            edge.is_user_defined_id = true;
        } else {
            let existing = self
                .edges
                .iter()
                .filter(|e| e.start == edge.start && e.end == edge.end)
                .count();
            // Quirk preserved from flowDb: the second parallel edge gets
            // counter `existing + 1`, so ids go _0, _2, _3, ...
            let counter = if existing == 0 { 0 } else { existing + 1 };
            edge.id = format!("L_{}_{}_{counter}", edge.start, edge.end);
        }

        assert!(
            self.edges.len() < 500,
            "edge limit exceeded: more than maxEdges (500) edges"
        );
        self.edges.push(edge);
    }

    pub fn add_link(&mut self, starts: &[String], ends: &[String], link: &LinkInfo) {
        let id = link.id.as_ref().map(|id| id.trim_end_matches('@'));
        for start in starts {
            for end in ends {
                let is_last_start = Some(start) == starts.last();
                let is_first_end = Some(end) == ends.first();
                if is_last_start && is_first_end {
                    self.add_single_link(start, end, link, id);
                } else {
                    self.add_single_link(start, end, link, None);
                }
            }
        }
    }

    pub fn update_link_interpolate(&mut self, positions: &[Option<usize>], interpolate: &str) {
        for pos in positions {
            match pos {
                None => self.default_interpolate = Some(interpolate.to_owned()),
                Some(i) => {
                    if let Some(edge) = self.edges.get_mut(*i) {
                        edge.interpolate = Some(interpolate.to_owned());
                    }
                }
            }
        }
    }

    pub fn update_link(&mut self, positions: &[Option<usize>], styles: &[String]) {
        for pos in positions {
            match pos {
                None => self.default_style = Some(styles.to_vec()),
                Some(i) => {
                    if let Some(edge) = self.edges.get_mut(*i) {
                        edge.style = styles.to_vec();
                        let has_fill = edge
                            .style
                            .iter()
                            .any(|s| s.trim_start().starts_with("fill"));
                        if !has_fill {
                            edge.style.push("fill:none".to_owned());
                        }
                    }
                }
            }
        }
    }

    pub fn add_class(&mut self, ids: &str, styles: &[String]) {
        for id in ids.split(',') {
            let class = self.classes.entry(id.to_owned()).or_default();
            for style in styles {
                if style.contains("color") {
                    let new_style = style.replace("fill", "bgFill");
                    class.text_styles.push(new_style);
                }
                class.styles.push(style.clone());
            }
        }
    }

    pub fn set_class(&mut self, ids: &str, class_name: &str) {
        for id in ids.split(',') {
            if let Some(vertex) = self.vertices.get_mut(id) {
                vertex.classes.push(class_name.to_owned());
            }
            if let Some(&idx) = self.sub_graph_lookup.get(id) {
                self.sub_graphs[idx].classes.push(class_name.to_owned());
            }
        }
    }

    /// Items collected from a subgraph body document.
    pub fn add_subgraph(
        &mut self,
        id_obj: Option<&FlowText>,
        list: &[DocItem],
        title_obj: Option<&FlowText>,
    ) -> String {
        let mut id = id_obj.map(|t| t.text.trim().to_owned());
        let title_text = title_obj.map(|t| t.text.clone()).unwrap_or_default();
        if let (Some(idt), Some(tt)) = (id_obj, title_obj)
            && std::ptr::eq(idt, tt)
            && tt.text.contains(char::is_whitespace)
        {
            id = None;
        }

        let mut dir: Option<String> = None;
        let mut node_list: Vec<String> = Vec::new();
        for item in list {
            match item {
                DocItem::Dir(d) => dir = Some(d.clone()),
                DocItem::Node(n) => {
                    if !n.trim().is_empty() && !node_list.contains(n) {
                        node_list.push(n.clone());
                    }
                }
            }
        }

        let id = id
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| format!("subGraph{}", self.sub_count));
        let title = sanitize_text(title_text.trim());
        self.sub_count += 1;

        let subgraph = FlowSubGraph {
            id: id.clone(),
            nodes: node_list,
            title,
            dir,
            label_type: title_obj.map_or_else(|| "text".to_owned(), |t| t.label_type.clone()),
            classes: Vec::new(),
        };

        // flowDb replaces an existing subgraph with the same id.
        if let Some(&idx) = self.sub_graph_lookup.get(&id) {
            self.sub_graphs[idx] = subgraph;
        } else {
            self.sub_graphs.push(subgraph);
            self.sub_graph_lookup
                .insert(id.clone(), self.sub_graphs.len() - 1);
        }
        id
    }

    #[must_use]
    pub fn destruct_start_link(&self, s: &str) -> LinkInfo {
        let mut str = s.trim();
        let mut link_type = "arrow_open";
        match str.chars().next() {
            Some('<') => {
                link_type = "arrow_point";
                str = &str[1..];
            }
            Some('x') => {
                link_type = "arrow_cross";
                str = &str[1..];
            }
            Some('o') => {
                link_type = "arrow_circle";
                str = &str[1..];
            }
            _ => {}
        }
        let mut stroke = "normal";
        if str.contains('=') {
            stroke = "thick";
        }
        if str.contains('.') {
            stroke = "dotted";
        }
        LinkInfo {
            link_type: link_type.to_owned(),
            stroke: stroke.to_owned(),
            length: 1.0,
            text: None,
            id: None,
        }
    }

    #[must_use]
    pub fn destruct_end_link(&self, s: &str) -> LinkInfo {
        let str = s.trim();
        let mut line = &str[..str.len().saturating_sub(1)];
        let mut link_type = "arrow_open".to_owned();
        match str.chars().last() {
            Some('x') => {
                "arrow_cross".clone_into(&mut link_type);
                if str.starts_with('x') {
                    link_type = format!("double_{link_type}");
                    line = &line[1..];
                }
            }
            Some('>') => {
                "arrow_point".clone_into(&mut link_type);
                if str.starts_with('<') {
                    link_type = format!("double_{link_type}");
                    line = &line[1..];
                }
            }
            Some('o') => {
                "arrow_circle".clone_into(&mut link_type);
                if str.starts_with('o') {
                    link_type = format!("double_{link_type}");
                    line = &line[1..];
                }
            }
            _ => {
                // arrow_open: `line` keeps the default str.slice(0, -1).
            }
        }

        let mut stroke = "normal";
        #[allow(clippy::cast_precision_loss)]
        let mut length = line.chars().count().saturating_sub(1) as f64;
        if line.starts_with('=') {
            stroke = "thick";
        }
        if line.starts_with('~') {
            stroke = "invisible";
        }
        let dots = line.chars().filter(|&c| c == '.').count();
        if dots > 0 {
            stroke = "dotted";
            #[allow(clippy::cast_precision_loss)]
            {
                length = dots as f64;
            }
        }
        LinkInfo {
            link_type,
            stroke: stroke.to_owned(),
            length,
            text: None,
            id: None,
        }
    }

    #[must_use]
    pub fn destruct_link(&self, s: &str, start_str: Option<&str>) -> LinkInfo {
        let info = self.destruct_end_link(s);
        if let Some(start_str) = start_str.filter(|s| !s.is_empty()) {
            let mut start_info = self.destruct_start_link(start_str);
            if start_info.stroke != info.stroke {
                return LinkInfo {
                    link_type: "INVALID".to_owned(),
                    stroke: "INVALID".to_owned(),
                    ..Default::default()
                };
            }
            if start_info.link_type == "arrow_open" {
                start_info.link_type.clone_from(&info.link_type);
            } else {
                if start_info.link_type != info.link_type {
                    return LinkInfo {
                        link_type: "INVALID".to_owned(),
                        stroke: "INVALID".to_owned(),
                        ..Default::default()
                    };
                }
                start_info.link_type = format!("double_{}", start_info.link_type);
            }
            if start_info.link_type == "double_arrow" {
                "double_arrow_point".clone_into(&mut start_info.link_type);
            }
            start_info.length = info.length;
            return start_info;
        }
        info
    }

    fn get_compiled_styles(&self, class_defs: &[String]) -> Vec<String> {
        let mut compiled = Vec::new();
        for name in class_defs {
            if let Some(class) = self.classes.get(name) {
                for s in &class.styles {
                    compiled.push(s.trim().to_owned());
                }
                for s in &class.text_styles {
                    compiled.push(s.trim().to_owned());
                }
            }
        }
        compiled
    }

    fn vertex_shape(vertex: &FlowVertex) -> String {
        match vertex.vertex_type.as_deref() {
            None | Some("square") => "squareRect".to_owned(),
            Some("round") => "roundedRect".to_owned(),
            Some(other) => other.to_owned(),
        }
    }

    fn destruct_edge_type(edge_type: Option<&str>) -> (String, String) {
        let mut start = "none".to_owned();
        let mut end = "arrow_point".to_owned();
        match edge_type {
            Some(t @ ("arrow_point" | "arrow_circle" | "arrow_cross")) => {
                t.clone_into(&mut end);
            }
            Some(t @ ("double_arrow_point" | "double_arrow_circle" | "double_arrow_cross")) => {
                start = t.replace("double_", "");
                end.clone_from(&start);
            }
            _ => {}
        }
        (start, end)
    }

    /// Port of `flowDb.getData()`.
    #[must_use]
    pub fn get_data(
        &self,
        diagram_id: &str,
        config: &crate::render::config::RenderConfig,
    ) -> LayoutData {
        let mut nodes: Vec<RenderNode> = Vec::new();
        let mut edges: Vec<RenderEdge> = Vec::new();

        let mut parent_db: IndexMap<String, String> = IndexMap::new();
        let mut sub_graph_db: IndexMap<String, bool> = IndexMap::new();

        for sub_graph in self.sub_graphs.iter().rev() {
            if !sub_graph.nodes.is_empty() {
                sub_graph_db.insert(sub_graph.id.clone(), true);
            }
            for id in &sub_graph.nodes {
                parent_db.insert(id.clone(), sub_graph.id.clone());
            }
        }

        for sub_graph in self.sub_graphs.iter().rev() {
            nodes.push(RenderNode {
                id: sub_graph.id.clone(),
                label: decode_html_entities(&strip_html_tags(&newlines_to_br(&sub_graph.title))),
                label_raw: sub_graph.title.clone(),
                label_type: sub_graph.label_type.clone(),
                parent_id: parent_db.get(&sub_graph.id).cloned(),
                padding: 8.0,
                css_compiled_styles: self.get_compiled_styles(&sub_graph.classes),
                css_classes: sub_graph.classes.join(" "),
                shape: "rect".to_owned(),
                dir: sub_graph.dir.clone(),
                is_group: true,
                look: "classic".to_owned(),
                dom_id: sub_graph.id.clone(),
                ..Default::default()
            });
        }

        for vertex in self.vertices.values() {
            let parent_id = parent_db.get(&vertex.id).cloned();
            let is_group = sub_graph_db.get(&vertex.id).copied().unwrap_or(false);
            if let Some(node) = nodes.iter_mut().find(|n| n.id == vertex.id) {
                node.css_styles.clone_from(&vertex.styles);
                node.css_compiled_styles = self.get_compiled_styles(&vertex.classes);
                node.css_classes = vertex.classes.join(" ");
                continue;
            }
            let mut all_classes = vec!["default".to_owned(), "node".to_owned()];
            all_classes.extend(vertex.classes.iter().cloned());
            let base = RenderNode {
                id: vertex.id.clone(),
                label: decode_html_entities(&strip_html_tags(&newlines_to_br(
                    &vertex.text.clone().unwrap_or_default(),
                ))),
                label_raw: vertex.text.clone().unwrap_or_default(),
                label_type: vertex.label_type.clone(),
                parent_id,
                padding: config.padding,
                css_styles: vertex.styles.clone(),
                css_compiled_styles: self.get_compiled_styles(&all_classes),
                css_classes: format!("default {}", vertex.classes.join(" ")),
                dir: vertex.dir.clone(),
                dom_id: vertex.dom_id.clone(),
                look: "classic".to_owned(),
                ..Default::default()
            };
            if is_group {
                nodes.push(RenderNode {
                    is_group: true,
                    shape: "rect".to_owned(),
                    ..base
                });
            } else {
                nodes.push(RenderNode {
                    is_group: false,
                    shape: Self::vertex_shape(vertex),
                    ..base
                });
            }
        }

        for raw_edge in &self.edges {
            let (arrow_start, arrow_end) = Self::destruct_edge_type(raw_edge.edge_type.as_deref());
            let mut styles: Vec<String> = self.default_style.clone().unwrap_or_default();
            styles.extend(raw_edge.style.iter().cloned());
            let invisible = raw_edge.stroke.as_deref() == Some("invisible");
            let open = raw_edge.edge_type.as_deref() == Some("arrow_open");
            edges.push(RenderEdge {
                id: raw_edge.id.clone(),
                start: raw_edge.start.clone(),
                end: raw_edge.end.clone(),
                edge_type: raw_edge
                    .edge_type
                    .clone()
                    .unwrap_or_else(|| "normal".to_owned()),
                label: decode_html_entities(&strip_html_tags(&newlines_to_br(&raw_edge.text))),
                label_raw: raw_edge.text.clone(),
                label_type: raw_edge.label_type.clone(),
                labelpos: "c".to_owned(),
                thickness: raw_edge.stroke.clone().unwrap_or_default(),
                pattern: raw_edge.stroke.clone().unwrap_or_default(),
                minlen: raw_edge.length,
                classes: if invisible {
                    String::new()
                } else {
                    "edge-thickness-normal edge-pattern-solid flowchart-link".to_owned()
                },
                arrow_type_start: if invisible || open {
                    "none".to_owned()
                } else {
                    arrow_start
                },
                arrow_type_end: if invisible || open {
                    "none".to_owned()
                } else {
                    arrow_end
                },
                css_compiled_styles: self.get_compiled_styles(&raw_edge.classes),
                label_style: styles.clone(),
                style: styles,
                curve: raw_edge
                    .interpolate
                    .clone()
                    .or_else(|| config.curve.clone())
                    .unwrap_or_else(|| "basis".to_owned()),
                look: "classic".to_owned(),
                ..Default::default()
            });
        }

        LayoutData {
            nodes: nodes
                .into_iter()
                .map(|n| Rc::new(RefCell::new(n)))
                .collect(),
            edges: edges
                .into_iter()
                .map(|e| Rc::new(RefCell::new(e)))
                .collect(),
            direction: self.get_direction(),
            diagram_id: diagram_id.to_owned(),
        }
    }
}

/// Item collected while parsing a (sub)document.
#[derive(Debug, Clone)]
pub enum DocItem {
    Node(String),
    Dir(String),
}
