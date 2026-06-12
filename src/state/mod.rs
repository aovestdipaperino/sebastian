//! stateDiagram-v2 support: parser (port of `stateDiagram.jison`), state
//! database (`stateDb.ts`), and layout-data construction (`dataFetcher.ts`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::render::data::{LayoutData, RenderEdge, RenderNode};

/// A parse error for state diagram source.
#[derive(Debug)]
pub struct StateParseError {
    pub message: String,
}

impl std::fmt::Display for StateParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "state diagram parse error: {}", self.message)
    }
}

impl std::error::Error for StateParseError {}

/// A note attached to a state.
#[derive(Debug, Clone)]
pub struct Note {
    pub position: String,
    pub text: String,
}

/// Parsed `stmt: 'state'` item.
#[derive(Debug, Clone, Default)]
pub struct StateStmt {
    pub id: String,
    pub ty: String,
    /// `description` may be a string or list (`STATE_DESCR` AS ID with colon).
    pub description: Vec<String>,
    pub doc: Option<Vec<Stmt>>,
    pub note: Option<Note>,
    pub classes: Vec<String>,
    /// Set by docTranslator for renamed `[*]` nodes.
    pub start: Option<bool>,
}

/// A parsed statement (subset of the jison grammar's productions).
#[derive(Debug, Clone)]
pub enum Stmt {
    State(StateStmt),
    Relation {
        state1: StateStmt,
        state2: StateStmt,
        description: Option<String>,
    },
    ClassDef {
        id: String,
        classes: String,
    },
    Style {
        id: String,
        style_class: String,
    },
    ApplyClass {
        id: String,
        style_class: String,
    },
    Dir {
        value: String,
    },
}

const START_NODE: &str = "[*]";
const DEFAULT_STATE_TYPE: &str = "default";

/// Parses stateDiagram(-v2) source into the root document statements.
///
/// # Errors
///
/// Returns [`StateParseError`] when the source doesn't match the state
/// diagram grammar.
pub fn parse(source: &str) -> Result<Vec<Stmt>, StateParseError> {
    let mut lines: Vec<&str> = Vec::new();
    let mut found_header = false;
    for raw in source.lines() {
        let t = raw.trim();
        if !found_header {
            if t.is_empty() || t.starts_with("%%") || t.starts_with('#') {
                continue;
            }
            if t == "stateDiagram" || t == "stateDiagram-v2" {
                found_header = true;
                continue;
            }
            if let Some(rest) = t
                .strip_prefix("stateDiagram-v2")
                .or_else(|| t.strip_prefix("stateDiagram"))
            {
                found_header = true;
                if !rest.trim().is_empty() {
                    lines.push(rest);
                }
                continue;
            }
            return Err(StateParseError {
                message: format!("expected stateDiagram header, got {t:?}"),
            });
        }
        lines.push(raw);
    }
    if !found_header {
        return Err(StateParseError {
            message: "missing stateDiagram header".to_owned(),
        });
    }
    let mut parser = Parser { lines, pos: 0 };
    parser.parse_document(false)
}

struct Parser<'a> {
    lines: Vec<&'a str>,
    pos: usize,
}

/// An `idStatement`: id with optional `:::class` suffix.
fn parse_id_statement(token: &str) -> StateStmt {
    let (id, classes) = if let Some((id, class)) = token.split_once(":::") {
        (id, vec![class.trim().to_owned()])
    } else {
        (token, vec![])
    };
    StateStmt {
        id: id.trim().to_owned(),
        ty: DEFAULT_STATE_TYPE.to_owned(),
        classes,
        ..StateStmt::default()
    }
}

impl Parser<'_> {
    #[allow(clippy::too_many_lines)]
    fn parse_document(&mut self, in_struct: bool) -> Result<Vec<Stmt>, StateParseError> {
        let mut doc: Vec<Stmt> = Vec::new();
        while self.pos < self.lines.len() {
            let raw = self.lines[self.pos];
            self.pos += 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with("%%") && !line.starts_with("%%{") {
                continue;
            }
            if line.starts_with("%%{") {
                // Directives are handled by detect_init; the lexer treats the
                // remainder as a comment-free token stream we don't need.
                continue;
            }
            if in_struct && line == "}" {
                return Ok(doc);
            }
            if in_struct && (line == "--" || line.chars().all(|c| c == '-') && line.len() == 2) {
                doc.push(Stmt::State(StateStmt {
                    id: String::new(), // filled by the db's divider counter
                    ty: "divider".to_owned(),
                    ..StateStmt::default()
                }));
                continue;
            }
            // The jison lexer's `.*direction\s+XX[^\n]*` rules outrank
            // everything via longest-match.
            if let Some(dir) = ["TB", "BT", "RL", "LR"].iter().find(|d| {
                line.contains(&format!("direction {d}"))
                    || line.contains(&format!("direction\t{d}"))
            }) {
                doc.push(Stmt::Dir {
                    value: (*dir).to_owned(),
                });
                continue;
            }
            if line == "hide empty description" || line.starts_with("scale ") {
                continue;
            }
            if line.starts_with("accTitle") || line.starts_with("accDescr") {
                continue;
            }
            if line.starts_with("click ") {
                continue;
            }
            if let Some(rest) = line.strip_prefix("classDef ") {
                let rest = rest.trim();
                if let Some((id, styles)) = rest.split_once(char::is_whitespace) {
                    doc.push(Stmt::ClassDef {
                        id: id.trim().to_owned(),
                        classes: styles.trim().to_owned(),
                    });
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("style ") {
                let rest = rest.trim();
                if let Some((ids, styles)) = rest.split_once(char::is_whitespace) {
                    doc.push(Stmt::Style {
                        id: ids.trim().to_owned(),
                        style_class: styles.trim().to_owned(),
                    });
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("class ") {
                let rest = rest.trim();
                if let Some((ids, class)) = rest.split_once(char::is_whitespace) {
                    doc.push(Stmt::ApplyClass {
                        id: ids.trim().to_owned(),
                        style_class: class.trim().to_owned(),
                    });
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("note ") {
                doc.push(self.parse_note(rest.trim())?);
                continue;
            }
            if let Some(rest) = line.strip_prefix("state ") {
                doc.push(self.parse_state_stmt(rest.trim())?);
                continue;
            }
            // idStatement [DESCR] ['-->' idStatement [DESCR]]
            doc.push(Self::parse_id_line(line));
        }
        if in_struct {
            return Err(StateParseError {
                message: "unexpected EOF inside composite state".to_owned(),
            });
        }
        Ok(doc)
    }

    /// `note (left|right) of ID : text` or multi-line up to `end note`,
    /// or a floating `note "text" as id` (ignored, as in the grammar).
    fn parse_note(&mut self, rest: &str) -> Result<Stmt, StateParseError> {
        if rest.starts_with('"') {
            // Floating note: the grammar has no action for it.
            // Consume nothing extra; single-line form assumed.
            return Ok(Stmt::State(StateStmt {
                id: String::new(),
                ty: "ignore".to_owned(),
                ..StateStmt::default()
            }));
        }
        let position = if let Some(r) = rest.strip_prefix("left of") {
            ("left of", r)
        } else if let Some(r) = rest.strip_prefix("right of") {
            ("right of", r)
        } else {
            return Err(StateParseError {
                message: format!("bad note statement: note {rest}"),
            });
        };
        let (pos_str, r) = position;
        let r = r.trim_start();
        // NOTE_ID: up to `:`, whitespace, or `-`.
        let id_end = r
            .find(|c: char| c == ':' || c.is_whitespace() || c == '-')
            .unwrap_or(r.len());
        let id = &r[..id_end];
        let after = r[id_end..].trim_start();
        let text = if let Some(t) = after.strip_prefix(':') {
            t.trim().to_owned()
        } else {
            // Multi-line: read until a line ending with `end note`.
            let mut collected = String::new();
            loop {
                if self.pos >= self.lines.len() {
                    return Err(StateParseError {
                        message: "unterminated note".to_owned(),
                    });
                }
                let l = self.lines[self.pos];
                self.pos += 1;
                if l.trim() == "end note" {
                    break;
                }
                if !collected.is_empty() {
                    collected.push('\n');
                }
                collected.push_str(l);
            }
            // The lexer regex captures everything then strips `end note`
            // and trims the whole text.
            collected.trim().to_owned()
        };
        Ok(Stmt::State(StateStmt {
            id: id.trim().to_owned(),
            ty: DEFAULT_STATE_TYPE.to_owned(),
            note: Some(Note {
                position: pos_str.to_owned(),
                text,
            }),
            ..StateStmt::default()
        }))
    }

    /// `state ...` statement bodies.
    fn parse_state_stmt(&mut self, rest: &str) -> Result<Stmt, StateParseError> {
        // <<fork>>, <<join>>, <<choice>> (and [[..]] forms).
        for (marker, ty) in [
            ("<<fork>>", "fork"),
            ("<<join>>", "join"),
            ("<<choice>>", "choice"),
            ("[[fork]]", "fork"),
            ("[[join]]", "join"),
            ("[[choice]]", "choice"),
        ] {
            if let Some(idx) = rest.find(marker) {
                let id = rest[..idx].trim();
                return Ok(Stmt::State(StateStmt {
                    id: id.to_owned(),
                    ty: ty.to_owned(),
                    ..StateStmt::default()
                }));
            }
        }
        if let Some(r) = rest.strip_prefix('"') {
            // state "description" as id [{ doc }]
            let Some(endq) = r.find('"') else {
                return Err(StateParseError {
                    message: "unterminated state description".to_owned(),
                });
            };
            let description = r[..endq].trim().to_owned();
            let after = r[endq + 1..].trim();
            let Some(id_part) = after.strip_prefix("as ") else {
                return Err(StateParseError {
                    message: format!("expected `as` after state description: {rest}"),
                });
            };
            let id_part = id_part.trim();
            let (id_token, has_brace) = if let Some(stripped) = id_part.strip_suffix('{') {
                (stripped.trim(), true)
            } else {
                (id_part, false)
            };
            // The ID may embed `id:extra` which splits the description.
            let (id, description) = if let Some((id, extra)) = id_token.split_once(':') {
                (id.to_owned(), vec![description, extra.to_owned()])
            } else {
                (id_token.to_owned(), vec![description])
            };
            let doc = if has_brace {
                Some(self.parse_document(true)?)
            } else {
                None
            };
            return Ok(Stmt::State(StateStmt {
                id,
                ty: DEFAULT_STATE_TYPE.to_owned(),
                description,
                doc,
                ..StateStmt::default()
            }));
        }
        // COMPOSIT_STATE [{ doc }]
        let (id_token, has_brace) = if let Some(stripped) = rest.strip_suffix('{') {
            (stripped.trim(), true)
        } else {
            (rest, false)
        };
        if has_brace {
            let doc = self.parse_document(true)?;
            return Ok(Stmt::State(StateStmt {
                id: id_token.to_owned(),
                ty: DEFAULT_STATE_TYPE.to_owned(),
                description: vec![String::new()],
                doc: Some(doc),
                ..StateStmt::default()
            }));
        }
        Ok(Stmt::State(StateStmt {
            id: id_token.to_owned(),
            ty: DEFAULT_STATE_TYPE.to_owned(),
            ..StateStmt::default()
        }))
    }

    /// `idStatement [DESCR]` / `idStatement --> idStatement [DESCR]`.
    fn parse_id_line(line: &str) -> Stmt {
        // Split off the first description colon (`:::` is the class
        // separator, not a description).
        let mut descr_idx = None;
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b':' {
                if line[i..].starts_with(":::") {
                    i += 3;
                    continue;
                }
                descr_idx = Some(i);
                break;
            }
            i += 1;
        }
        let (head, descr) = match descr_idx {
            Some(i) => (line[..i].trim(), Some(trim_colon(line[i..].trim()))),
            None => (line, None),
        };
        if let Some((left, right)) = head.split_once("-->") {
            let state1 = parse_id_statement(left.trim());
            let state2 = parse_id_statement(right.trim());
            Stmt::Relation {
                state1,
                state2,
                description: descr,
            }
        } else if let Some(d) = descr {
            let mut s = parse_id_statement(head.trim());
            s.description = vec![d];
            Stmt::State(s)
        } else {
            Stmt::State(parse_id_statement(head.trim()))
        }
    }
}

/// `yy.trimColon`.
fn trim_colon(s: &str) -> String {
    s.strip_prefix(':')
        .map_or_else(|| s.trim().to_owned(), |r| r.trim().to_owned())
}

/// State record in the database (`StateStmt` of stateDb.ts).
#[derive(Debug, Clone, Default)]
struct DbState {
    descriptions: Vec<String>,
    classes: Vec<String>,
    styles: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct StyleClass {
    styles: Vec<String>,
    text_styles: Vec<String>,
}

/// The state database after `extract()`.
#[derive(Debug, Default)]
pub struct StateDb {
    root_doc: Vec<Stmt>,
    states: indexmap::IndexMap<String, DbState>,
    classes: indexmap::IndexMap<String, StyleClass>,
    divider_count: usize,
}

/// In-progress node info (`nodeDb` of dataFetcher.ts).
#[derive(Debug, Clone, Default)]
struct NodeInfo {
    shape: String,
    description: Vec<String>,
    css_classes: String,
    css_styles: Vec<String>,
    ty: String,
    dir: Option<String>,
}

/// Parses + extracts state diagram source into layout data.
///
/// # Errors
///
/// Returns [`StateParseError`] for unparsable source.
pub fn get_layout_data(
    source: &str,
    id: &str,
    config: &crate::render::config::RenderConfig,
) -> Result<LayoutData, StateParseError> {
    let mut db = StateDb {
        root_doc: parse(source)?,
        ..StateDb::default()
    };
    db.translate_and_extract(config);
    Ok(db.get_data(id, config))
}

impl StateDb {
    /// `docTranslator` + `extract` + `dataFetcher`.
    fn translate_and_extract(&mut self, _config: &crate::render::config::RenderConfig) {
        let mut doc = std::mem::take(&mut self.root_doc);
        // Assign divider ids (parser leaves them empty).
        for stmt in &mut doc {
            if let Stmt::State(s) = stmt
                && s.ty == "divider"
                && s.id.is_empty()
            {
                self.divider_count += 1;
                s.id = format!("divider-id-{}", self.divider_count);
            }
        }
        Self::doc_translator_root(&mut doc);
        self.root_doc = doc;

        // extract(): collect states/relations/classes at the root level.
        let doc = self.root_doc.clone();
        for item in &doc {
            match item {
                Stmt::State(s) => {
                    self.add_state(s);
                }
                Stmt::Relation { state1, state2, .. } => {
                    self.add_state(state1);
                    self.add_state(state2);
                }
                Stmt::ClassDef { id, classes } => self.add_style_class(id, classes),
                Stmt::Style { id, style_class } => {
                    for sid in id.split(',') {
                        let sid = sid.trim().to_owned();
                        let state = self.states.entry(sid).or_default();
                        state.styles = style_class
                            .split(',')
                            .map(|s| s.replace(';', "").trim().to_owned())
                            .collect();
                    }
                }
                Stmt::ApplyClass { id, style_class } => {
                    for sid in id.split(',') {
                        let state = self.states.entry(sid.trim().to_owned()).or_default();
                        state.classes.push(style_class.clone());
                    }
                }
                Stmt::Dir { .. } => {}
            }
        }
    }

    /// `docTranslator` applied to the root document: renames `[*]` states
    /// and groups divider sections.
    fn doc_translator_root(doc: &mut Vec<Stmt>) {
        Self::doc_translator("root", doc);
    }

    fn doc_translator(parent_id: &str, doc: &mut Vec<Stmt>) {
        for stmt in doc.iter_mut() {
            match stmt {
                Stmt::Relation { state1, state2, .. } => {
                    Self::translate_node(parent_id, state1, true);
                    Self::translate_node(parent_id, state2, false);
                }
                Stmt::State(s) => {
                    Self::translate_node(parent_id, s, true);
                }
                _ => {}
            }
        }
        // Divider grouping (only when dividers are present).
        let has_divider = doc
            .iter()
            .any(|s| matches!(s, Stmt::State(st) if st.ty == "divider"));
        if has_divider {
            let mut new_doc: Vec<Stmt> = Vec::new();
            let mut current: Vec<Stmt> = Vec::new();
            for stmt in doc.drain(..) {
                if let Stmt::State(mut st) = stmt {
                    if st.ty == "divider" {
                        st.doc = Some(std::mem::take(&mut current));
                        new_doc.push(Stmt::State(st));
                        continue;
                    }
                    current.push(Stmt::State(st));
                } else {
                    current.push(stmt);
                }
            }
            if !new_doc.is_empty() && !current.is_empty() {
                new_doc.push(Stmt::State(StateStmt {
                    id: generate_id(),
                    ty: "divider".to_owned(),
                    doc: Some(current),
                    ..StateStmt::default()
                }));
            }
            *doc = new_doc;
        }
        // Recurse into child documents.
        for stmt in doc.iter_mut() {
            let states: Vec<&mut StateStmt> = match stmt {
                Stmt::State(s) => vec![s],
                Stmt::Relation { state1, state2, .. } => vec![state1, state2],
                _ => vec![],
            };
            for s in states {
                let sid = s.id.clone();
                if let Some(d) = &mut s.doc {
                    Self::doc_translator(&sid, d);
                }
            }
        }
    }

    fn translate_node(parent_id: &str, node: &mut StateStmt, first: bool) {
        if node.id == START_NODE {
            node.id = format!("{parent_id}{}", if first { "_start" } else { "_end" });
            node.start = Some(first);
        } else {
            node.id = node.id.trim().to_owned();
        }
    }

    fn add_state(&mut self, s: &StateStmt) {
        let entry = self.states.entry(s.id.clone()).or_default();
        for d in &s.description {
            if !d.is_empty() {
                let trimmed = d.trim();
                let cleaned = trimmed.strip_prefix(':').map_or(trimmed, str::trim_start);
                entry
                    .descriptions
                    .push(crate::flowchart::db::sanitize_text(cleaned.trim()));
            }
        }
        for c in &s.classes {
            entry.classes.push(c.clone());
        }
        if let Some(doc) = &s.doc {
            for item in doc {
                match item {
                    Stmt::State(inner) => self.add_state(inner),
                    Stmt::Relation { state1, state2, .. } => {
                        self.add_state(state1);
                        self.add_state(state2);
                    }
                    _ => {}
                }
            }
        }
    }

    fn add_style_class(&mut self, id: &str, style_attributes: &str) {
        let entry = self.classes.entry(id.to_owned()).or_default();
        for attrib in style_attributes.split(',') {
            // Strip a trailing semicolon segment like the JS regex.
            let fixed = attrib.replacen(';', "", 1);
            let fixed = fixed.trim().to_owned();
            if attrib.contains("color") {
                let s1 = fixed.replace("fill", "bgFill");
                let s2 = s1.replace("color", "fill");
                entry.text_styles.push(s2);
            }
            entry.styles.push(fixed);
        }
    }

    /// `getData()`: runs dataFetcher over the translated document.
    fn get_data(&self, id: &str, config: &crate::render::config::RenderConfig) -> LayoutData {
        let mut fetcher = DataFetcher {
            db: self,
            node_db: HashMap::new(),
            nodes: Vec::new(),
            node_order: Vec::new(),
            edges: Vec::new(),
            graph_item_count: 0,
            config,
        };
        fetcher.setup_doc(None, &self.root_doc, true);

        let mut direction = "TB".to_owned();
        for stmt in &self.root_doc {
            if let Stmt::Dir { value } = stmt {
                direction.clone_from(value);
            }
        }

        LayoutData {
            nodes: fetcher
                .node_order
                .iter()
                .map(|id| fetcher.nodes[*id].clone())
                .collect(),
            edges: fetcher.edges,
            direction,
            diagram_id: id.to_owned(),
        }
    }
}

/// `generateId` from utils.ts (random in JS; only used for divider tails).
fn generate_id() -> String {
    "divider-tail".to_owned()
}

struct DataFetcher<'a> {
    db: &'a StateDb,
    /// `nodeDb` keyed by item id.
    node_db: HashMap<String, NodeInfo>,
    /// Inserted nodes (`nodes` array), in insertion order.
    nodes: Vec<Rc<RefCell<RenderNode>>>,
    node_order: Vec<usize>,
    edges: Vec<Rc<RefCell<RenderEdge>>>,
    graph_item_count: usize,
    config: &'a crate::render::config::RenderConfig,
}

impl DataFetcher<'_> {
    fn setup_doc(&mut self, parent: Option<&StateStmt>, doc: &[Stmt], alt_flag: bool) {
        for item in doc {
            match item {
                Stmt::State(s) => self.data_fetcher(parent, s, alt_flag),
                Stmt::Relation {
                    state1,
                    state2,
                    description,
                } => {
                    self.data_fetcher(parent, state1, alt_flag);
                    self.data_fetcher(parent, state2, alt_flag);
                    let edge = RenderEdge {
                        id: format!("edge{}", self.graph_item_count),
                        start: state1.id.clone(),
                        end: state2.id.clone(),
                        arrow_type_end: "arrow_barb".to_owned(),
                        style: vec!["fill:none".to_owned()],
                        label: crate::flowchart::db::sanitize_text(
                            description.as_deref().unwrap_or(""),
                        ),
                        label_raw: description.clone().unwrap_or_default(),
                        labelpos: "c".to_owned(),
                        label_type: "markdown".to_owned(),
                        thickness: "normal".to_owned(),
                        classes: "transition".to_owned(),
                        look: "classic".to_owned(),
                        pattern: "solid".to_owned(),
                        minlen: 1.0,
                        curve: "basis".to_owned(),
                        ..RenderEdge::default()
                    };
                    self.edges.push(Rc::new(RefCell::new(edge)));
                    self.graph_item_count += 1;
                }
                _ => {}
            }
        }
    }

    fn get_dir(parsed_item: &StateStmt) -> String {
        let mut dir = "TB".to_owned();
        if let Some(doc) = &parsed_item.doc {
            for stmt in doc {
                if let Stmt::Dir { value } = stmt {
                    dir.clone_from(value);
                }
            }
        }
        dir
    }

    #[allow(clippy::too_many_lines)]
    fn data_fetcher(
        &mut self,
        parent: Option<&StateStmt>,
        parsed_item: &StateStmt,
        alt_flag: bool,
    ) {
        if parsed_item.ty == "ignore" {
            return;
        }
        let item_id = parsed_item.id.clone();
        let db_state = self.db.states.get(&item_id);
        let class_str = db_state.map_or(String::new(), |s| s.classes.join(" "));
        let style: Vec<String> = db_state.map_or(Vec::new(), |s| s.styles.clone());

        if item_id != "root" {
            let mut shape = "rect";
            if parsed_item.start == Some(true) {
                shape = "stateStart";
            } else if parsed_item.start == Some(false) {
                shape = "stateEnd";
            }
            if parsed_item.ty != DEFAULT_STATE_TYPE {
                shape = &parsed_item.ty;
            }

            if !self.node_db.contains_key(&item_id) {
                self.node_db.insert(
                    item_id.clone(),
                    NodeInfo {
                        shape: shape.to_owned(),
                        description: vec![crate::flowchart::db::sanitize_text(&item_id)],
                        css_classes: format!("{class_str} statediagram-state"),
                        css_styles: style.clone(),
                        ty: String::new(),
                        dir: None,
                    },
                );
            }

            // Build up description list on the cached node.
            {
                let new_node = self.node_db.get_mut(&item_id).expect("inserted above");
                for desc in &parsed_item.description {
                    if desc.is_empty() {
                        continue;
                    }
                    match new_node.description.len() {
                        n if n > 1 => {
                            "rectWithTitle".clone_into(&mut new_node.shape);
                            new_node
                                .description
                                .push(crate::flowchart::db::sanitize_text(desc));
                        }
                        1 if new_node.description[0] == item_id => {
                            new_node.description = vec![crate::flowchart::db::sanitize_text(desc)];
                        }
                        1 => {
                            "rectWithTitle".clone_into(&mut new_node.shape);
                            new_node
                                .description
                                .push(crate::flowchart::db::sanitize_text(desc));
                        }
                        _ => {
                            "rect".clone_into(&mut new_node.shape);
                            new_node.description = vec![crate::flowchart::db::sanitize_text(desc)];
                        }
                    }
                }
                if new_node.description.len() == 1 && new_node.shape == "rectWithTitle" {
                    if new_node.ty == "group" {
                        "roundedWithTitle".clone_into(&mut new_node.shape);
                    } else {
                        "rect".clone_into(&mut new_node.shape);
                    }
                }

                // Group (composite) handling.
                if new_node.ty.is_empty() && parsed_item.doc.is_some() {
                    "group".clone_into(&mut new_node.ty);
                    new_node.dir = Some(Self::get_dir(parsed_item));
                    new_node.shape = if parsed_item.ty == "divider" {
                        "divider".to_owned()
                    } else {
                        "roundedWithTitle".to_owned()
                    };
                    new_node.css_classes = format!(
                        "{} statediagram-cluster {}",
                        new_node.css_classes,
                        if alt_flag {
                            "statediagram-cluster-alt"
                        } else {
                            ""
                        }
                    );
                }
            }

            let new_node = self.node_db.get(&item_id).expect("cached").clone();

            // Map mermaid's `rect`+rx/ry to the rounded shape handler the
            // unified renderer dispatches to (nodes.ts does this at insert
            // time; node rx/ry are 10).
            let shape_final = if new_node.shape == "rect" {
                "roundedRect".to_owned()
            } else {
                new_node.shape.clone()
            };

            let label = new_node.description.first().cloned().unwrap_or_default();
            let mut node_data = RenderNode {
                id: item_id.clone(),
                shape: shape_final,
                label: label.clone(),
                label_raw: label,
                css_classes: new_node.css_classes.clone(),
                css_styles: new_node.css_styles.clone(),
                css_compiled_styles: self.compiled_styles(&new_node.css_classes),
                dir: new_node.dir.clone(),
                dom_id: state_dom_id(&item_id, self.graph_item_count, None),
                is_group: new_node.ty == "group",
                padding: 8.0,
                look: "classic".to_owned(),
                label_type: "markdown".to_owned(),
                ..RenderNode::default()
            };
            if node_data.shape == "divider" {
                node_data.label = String::new();
                node_data.label_raw = String::new();
            }
            if let Some(p) = parent
                && p.id != "root"
            {
                node_data.parent_id = Some(p.id.clone());
            }

            if let Some(note) = &parsed_item.note {
                // The note itself. markdownToHTML turns `\n` + following
                // spaces into a line break.
                let note_text = markdown_breaks(&crate::flowchart::db::sanitize_text(&note.text));
                let note_data = RenderNode {
                    id: format!("{item_id}----note-{}", self.graph_item_count),
                    shape: "note".to_owned(),
                    label: note_text.clone(),
                    label_raw: note_text.clone(),
                    label_type: "markdown".to_owned(),
                    css_classes: "statediagram-note".to_owned(),
                    dom_id: state_dom_id(&item_id, self.graph_item_count, Some("note")),
                    is_group: new_node.ty == "group",
                    padding: self.config.padding,
                    look: "classic".to_owned(),
                    ..RenderNode::default()
                };
                let mut group_data = RenderNode {
                    id: format!("{item_id}----parent"),
                    shape: "noteGroup".to_owned(),
                    label: note_text.clone(),
                    label_raw: note_text,
                    css_classes: new_node.css_classes.clone(),
                    dom_id: state_dom_id(&item_id, self.graph_item_count, Some("parent")),
                    is_group: true,
                    padding: 16.0,
                    look: "classic".to_owned(),
                    ..RenderNode::default()
                };
                self.graph_item_count += 1;
                let parent_node_id = group_data.id.clone();
                group_data.id.clone_from(&parent_node_id);
                let mut note_data = note_data;
                note_data.parent_id = Some(parent_node_id.clone());

                self.insert_or_update_node(group_data);
                self.insert_or_update_node(note_data.clone());
                self.insert_or_update_node(node_data);

                let (from, to) = if note.position == "left of" {
                    (note_data.id.clone(), item_id.clone())
                } else {
                    (item_id.clone(), note_data.id.clone())
                };
                let edge = RenderEdge {
                    id: format!("{from}-{to}"),
                    start: from,
                    end: to,
                    arrow_type_end: String::new(),
                    style: vec!["fill:none".to_owned()],
                    classes: "transition note-edge".to_owned(),
                    labelpos: "c".to_owned(),
                    label_type: "markdown".to_owned(),
                    thickness: "normal".to_owned(),
                    look: "classic".to_owned(),
                    pattern: "solid".to_owned(),
                    minlen: 1.0,
                    curve: "basis".to_owned(),
                    ..RenderEdge::default()
                };
                self.edges.push(Rc::new(RefCell::new(edge)));
            } else {
                self.insert_or_update_node(node_data);
            }
        }
        if let Some(doc) = &parsed_item.doc {
            self.setup_doc(Some(parsed_item), doc, !alt_flag);
        }
    }

    /// Compiled styles from class definitions referenced by `cssClasses`.
    fn compiled_styles(&self, css_classes: &str) -> Vec<String> {
        let mut compiled = Vec::new();
        for class in css_classes.split(' ') {
            if let Some(def) = self.db.classes.get(class) {
                compiled.extend(def.styles.iter().cloned());
            }
        }
        compiled
    }

    /// `insertOrUpdateNode`: later encounters replace the node's data
    /// in place (JS `Object.assign`).
    fn insert_or_update_node(&mut self, node: RenderNode) {
        if node.id.is_empty() || node.id == "</join></fork>" || node.id == "</choice>" {
            return;
        }
        if let Some(existing) = self.nodes.iter().find(|n| n.borrow().id == node.id) {
            *existing.borrow_mut() = node;
        } else {
            let idx = self.nodes.len();
            self.nodes.push(Rc::new(RefCell::new(node)));
            self.node_order.push(idx);
        }
    }
}

/// `markdownToHTML`'s newline handling: `/\n */g` becomes a `<br />`.
fn markdown_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            out.push_str("<br />");
            while chars.peek() == Some(&' ') {
                chars.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `stateDomId`.
fn state_dom_id(item_id: &str, counter: usize, ty: Option<&str>) -> String {
    let type_str = ty.map_or(String::new(), |t| format!("----{t}"));
    format!("state-{item_id}{type_str}-{counter}")
}
