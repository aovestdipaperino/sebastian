//! Byte-exact port of mermaid 11.15.0 `block-beta` diagrams.
//!
//! Ports the jison grammar (`parser/block.jison`), `blockDB.ts`
//! (`populateBlockDatabase`/`setHierarchy`), `layout.ts`
//! (`setBlockSizes`/`layoutBlocks`/`findBounds`), and the legacy
//! `dagre-wrapper` node shapes used by `renderHelpers.ts` (rect, composite,
//! circle, diamond/question, …). Labels are HTML `foreignObject` spans built
//! through the shared `createText`/text-measurement code, matching the
//! deprecated `createLabel` path (`width: Infinity`, so labels never wrap).
//!
//! Stage 1 covers plain block grids: nodes, shapes, `columns`, `space`,
//! `space:N`, `:N` column spans, and nested `block:name … end` composites.
//! Edges are not yet rendered.

// These lints fire on faithful ports of mermaid's imperative layout code; the
// straightforward Rust rewrites either lose the 1:1 mapping to the source or do
// not help correctness here.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::assigning_clones,
    clippy::only_used_in_recursion,
    clippy::collapsible_if,
    clippy::if_not_else
)]

use std::collections::HashMap;

use crate::render::shapes::measure_label_sized;
use crate::svg::{
    Element, append, append_xhtml, insert_first, js_num, new_element, serialize, set_attr,
    set_text_append,
};
use crate::text::TextMeasurer;

const PADDING: f64 = 8.0;
const FONT_SIZE: f64 = 16.0;

/// Parse error for block diagrams.
#[derive(Debug)]
pub struct BlockParseError(pub String);

impl std::fmt::Display for BlockParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "block parse error: {}", self.0)
    }
}

impl std::error::Error for BlockParseError {}

#[derive(Debug, Clone, Default)]
struct Size {
    width: f64,
    height: f64,
    x: f64,
    y: f64,
}

#[derive(Debug, Clone)]
struct Block {
    id: String,
    /// Node kind: square/round/circle/diamond/hexagon/stadium/subroutine/
    /// cylinder/doublecircle/composite/space/na …
    typ: String,
    label: String,
    children: Vec<usize>,
    columns: i64,
    width_in_columns: f64,
    /// `space:N` width (number of blank columns).
    space_width: i64,
    styles: Vec<String>,
    classes: Vec<String>,
    size: Option<Size>,
    /// True once `label` has been assigned (mirrors JS truthiness checks).
    label_set: bool,
}

impl Block {
    fn new(id: &str) -> Self {
        Block {
            id: id.to_owned(),
            typ: "na".to_owned(),
            label: String::new(),
            children: Vec::new(),
            columns: -1,
            width_in_columns: 1.0,
            space_width: 1,
            styles: Vec::new(),
            classes: Vec::new(),
            size: None,
            label_set: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Parser (a light hand-port of parser/block.jison, plain-block subset).
// ---------------------------------------------------------------------------

/// A parsed statement, as produced by the grammar's semantic actions.
#[derive(Debug, Clone)]
enum Stmt {
    /// A node (id, typeStr→type, label, widthInColumns).
    Node {
        id: String,
        typ: String,
        label: Option<String>,
        width_in_columns: f64,
    },
    Column(i64),
    Space(i64),
    /// A composite: (explicit id or None, children statements).
    Composite {
        id: Option<String>,
        children: Vec<Stmt>,
    },
    ClassDef {
        id: String,
        css: String,
    },
    ApplyClass {
        ids: String,
        class: String,
    },
    ApplyStyle {
        ids: String,
        styles: String,
    },
}

fn type_str_to_type(s: &str) -> &'static str {
    match s {
        "[]" => "square",
        "()" => "round",
        "(())" => "circle",
        ">]" => "rect_left_inv_arrow",
        "{}" => "diamond",
        "{{}}" => "hexagon",
        "([])" => "stadium",
        "[[]]" => "subroutine",
        "[()]" => "cylinder",
        "((()))" => "doublecircle",
        "[//]" => "lean_right",
        "[\\\\]" => "lean_left",
        "[/\\]" => "trapezoid",
        "[\\/]" => "inv_trapezoid",
        "<[]>" => "block_arrow",
        _ => "na",
    }
}

/// The ordered (start, end, typeStr) table for shape delimiters. Longest
/// starts first so `((` wins over `(`.
const SHAPE_STARTS: &[(&str, &str)] = &[
    ("(((", ")))"),
    ("(([", "]))"),
    ("[[", "]]"),
    ("[(", ")]"),
    ("([", "])"),
    ("((", "))"),
    ("{{", "}}"),
    ("[/", "/]"),
    ("[\\", "\\]"),
    ("[", "]"),
    ("(", ")"),
    ("{", "}"),
    (">", "]"),
];

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    src: &'a str,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Parser {
            s: src.as_bytes(),
            pos: 0,
            src,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.s.len() {
            let c = self.s[self.pos];
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else if c == b'%' && self.peek_str("%%") {
                // skip a %% comment to end of line
                while self.pos < self.s.len() && self.s[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek_str(&self, kw: &str) -> bool {
        self.src[self.pos..].starts_with(kw)
    }

    /// Matches a keyword only when followed by a word boundary.
    fn peek_word(&self, kw: &str) -> bool {
        if !self.peek_str(kw) {
            return false;
        }
        let after = self.pos + kw.len();
        after >= self.s.len() || {
            let c = self.s[after];
            !(c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
        }
    }

    fn eof(&mut self) -> bool {
        self.skip_ws();
        self.pos >= self.s.len()
    }

    /// Reads a quoted `"…"` string; assumes current char is `"`.
    fn read_string(&mut self) -> String {
        self.pos += 1; // opening quote
        let start = self.pos;
        while self.pos < self.s.len() && self.s[self.pos] != b'"' {
            self.pos += 1;
        }
        let out = self.src[start..self.pos].to_owned();
        if self.pos < self.s.len() {
            self.pos += 1; // closing quote
        }
        out
    }

    /// Reads a `NODE_ID`: run of chars excluding `( [ \n - ) { } ws < > : =`.
    fn read_node_id(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.s.len() {
            let c = self.s[self.pos];
            if matches!(
                c,
                b'(' | b'['
                    | b'\n'
                    | b'\r'
                    | b'-'
                    | b')'
                    | b'{'
                    | b'}'
                    | b' '
                    | b'\t'
                    | b'<'
                    | b'>'
                    | b':'
                    | b'='
            ) {
                break;
            }
            self.pos += 1;
        }
        self.src[start..self.pos].to_owned()
    }

    /// Attempts to read a shape `nodeShapeNLabel` at the current position,
    /// returning (typeStr, label).
    fn read_shape(&mut self) -> Option<(String, String)> {
        let rest = &self.src[self.pos..];
        for (start, end) in SHAPE_STARTS {
            if rest.starts_with(start) {
                let save = self.pos;
                self.pos += start.len();
                self.skip_ws_inline();
                if self.pos < self.s.len() && self.s[self.pos] == b'"' {
                    let label = self.read_string();
                    self.skip_ws_inline();
                    if self.src[self.pos..].starts_with(end) {
                        self.pos += end.len();
                        let type_str = format!("{start}{end}");
                        return Some((type_str, label));
                    }
                }
                self.pos = save;
            }
        }
        None
    }

    fn skip_ws_inline(&mut self) {
        while self.pos < self.s.len() {
            let c = self.s[self.pos];
            if c == b' ' || c == b'\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Reads to end of the current line (trimmed), for classDef/style bodies.
    fn read_to_eol(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.s.len() && self.s[self.pos] != b'\n' && self.s[self.pos] != b'\r' {
            self.pos += 1;
        }
        self.src[start..self.pos].trim().to_owned()
    }

    fn read_word(&mut self) -> String {
        let start = self.pos;
        while self.pos < self.s.len() {
            let c = self.s[self.pos];
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b',' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.src[start..self.pos].to_owned()
    }
}

fn parse(src: &str) -> Result<Vec<Stmt>, BlockParseError> {
    // prepareTextForParsing: trim each row, collapse blank lines, trim.
    let prepared = prepare_text(src);
    let mut p = Parser::new(&prepared);
    p.skip_ws();
    // BLOCK_DIAGRAM_KEY
    if p.peek_word("block-beta") {
        p.pos += "block-beta".len();
    } else if p.peek_word("block") {
        p.pos += "block".len();
    } else {
        return Err(BlockParseError("expected block-beta".into()));
    }
    let doc = parse_document(&mut p, false)?;
    Ok(doc)
}

fn prepare_text(text: &str) -> String {
    let trimmed: Vec<&str> = text.lines().map(str::trim).collect();
    // collapse consecutive blank lines
    let mut out: Vec<&str> = Vec::new();
    let mut prev_blank = false;
    for line in trimmed {
        let blank = line.is_empty();
        if blank && prev_blank {
            continue;
        }
        out.push(line);
        prev_blank = blank;
    }
    out.join("\n").trim().to_owned()
}

fn parse_document(p: &mut Parser, nested: bool) -> Result<Vec<Stmt>, BlockParseError> {
    let mut stmts = Vec::new();
    loop {
        if p.eof() {
            break;
        }
        if nested && p.peek_word("end") {
            p.pos += "end".len();
            break;
        }
        stmts.push(parse_statement(p)?);
    }
    Ok(stmts)
}

fn parse_statement(p: &mut Parser) -> Result<Stmt, BlockParseError> {
    p.skip_ws();
    // columns
    if p.peek_word("columns") {
        p.pos += "columns".len();
        p.skip_ws_inline();
        if p.peek_word("auto") {
            p.pos += "auto".len();
            return Ok(Stmt::Column(-1));
        }
        let num = p.read_word();
        let n = num
            .parse::<i64>()
            .map_err(|_| BlockParseError(format!("bad columns value: {num}")))?;
        return Ok(Stmt::Column(n));
    }
    // space / space:N
    if p.peek_word("space") {
        p.pos += "space".len();
        if p.pos < p.s.len() && p.s[p.pos] == b':' {
            p.pos += 1;
            let num = p.read_word();
            let n = num.parse::<i64>().unwrap_or(1);
            return Ok(Stmt::Space(n));
        }
        return Ok(Stmt::Space(1));
    }
    // classDef
    if p.peek_word("classDef") {
        p.pos += "classDef".len();
        p.skip_ws_inline();
        let id = p.read_word();
        p.skip_ws_inline();
        let css = p.read_to_eol();
        return Ok(Stmt::ClassDef { id, css });
    }
    // class
    if p.peek_word("class") {
        p.pos += "class".len();
        p.skip_ws_inline();
        let ids = p.read_word();
        p.skip_ws_inline();
        let class = p.read_to_eol();
        return Ok(Stmt::ApplyClass { ids, class });
    }
    // style
    if p.peek_word("style") {
        p.pos += "style".len();
        p.skip_ws_inline();
        let ids = p.read_word();
        p.skip_ws_inline();
        let styles = p.read_to_eol();
        return Ok(Stmt::ApplyStyle { ids, styles });
    }
    // nested block:  "block:name … end"  or  "block … end"
    if p.peek_word("block") {
        // Distinguish `block:` (id-block) from bare `block` composite.
        let after = p.pos + "block".len();
        if after < p.s.len() && p.s[after] == b':' {
            p.pos = after + 1;
            p.skip_ws_inline();
            let id = p.read_node_id();
            let children = parse_document(p, true)?;
            return Ok(Stmt::Composite {
                id: Some(id),
                children,
            });
        }
        p.pos = after;
        let children = parse_document(p, true)?;
        return Ok(Stmt::Composite { id: None, children });
    }
    // node statement
    parse_node_statement(p)
}

fn parse_node_statement(p: &mut Parser) -> Result<Stmt, BlockParseError> {
    p.skip_ws();
    let id = p.read_node_id();
    if id.is_empty() {
        return Err(BlockParseError(format!(
            "unexpected token at byte {}",
            p.pos
        )));
    }
    let (typ, label) = if let Some((type_str, label)) = p.read_shape() {
        (type_str_to_type(&type_str).to_owned(), Some(label))
    } else {
        ("na".to_owned(), None)
    };
    // optional :N size
    let mut width_in_columns = 1.0;
    if p.pos < p.s.len() && p.s[p.pos] == b':' {
        p.pos += 1;
        let num = p.read_word();
        if let Ok(n) = num.parse::<f64>() {
            width_in_columns = n;
        }
    }
    Ok(Stmt::Node {
        id,
        typ,
        label,
        width_in_columns,
    })
}

// ---------------------------------------------------------------------------
// DB: populateBlockDatabase + setHierarchy (blockDB.ts)
// ---------------------------------------------------------------------------

struct Db {
    blocks: Vec<Block>,
    /// id → index into `blocks`.
    index: HashMap<String, usize>,
    root: usize,
    id_counter: usize,
    /// classDef definitions in insertion order: (name, styles, textStyles).
    classes: Vec<(String, Vec<String>, Vec<String>)>,
    class_index: HashMap<String, usize>,
}

impl Db {
    fn new() -> Self {
        let root = Block {
            typ: "composite".to_owned(),
            ..Block::new("root")
        };
        let mut index = HashMap::new();
        index.insert("root".to_owned(), 0usize);
        Db {
            blocks: vec![root],
            index,
            root: 0,
            id_counter: 0,
            classes: Vec::new(),
            class_index: HashMap::new(),
        }
    }

    /// Port of `addStyleClass`.
    fn add_style_class(&mut self, id: &str, style_attributes: &str) {
        let entry = if let Some(&i) = self.class_index.get(id) {
            i
        } else {
            self.classes.push((id.to_owned(), Vec::new(), Vec::new()));
            let i = self.classes.len() - 1;
            self.class_index.insert(id.to_owned(), i);
            i
        };
        for attrib in style_attributes.split(',') {
            // remove a single trailing `;`: /([^;]*);/ → $1
            let fixed = attrib
                .find(';')
                .map_or(attrib, |p| &attrib[..p])
                .trim()
                .to_owned();
            if attrib.contains("color") {
                let s = fixed.replacen("fill", "bgFill", 1);
                let s = s.replacen("color", "fill", 1);
                self.classes[entry].2.push(s);
            }
            self.classes[entry].1.push(fixed);
        }
    }

    fn generate_id(&mut self) -> String {
        self.id_counter += 1;
        format!("id-{}", self.id_counter)
    }
}

/// Converts parsed statements into `Block` records under `parent`, mirroring
/// `populateBlockDatabase`. Returns the ordered children indices.
fn populate(db: &mut Db, stmts: &[Stmt], parent: usize) -> Vec<usize> {
    // column-setting detection: first Column stmt sets parent.columns
    let mut children: Vec<usize> = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Column(c) => {
                db.blocks[parent].columns = *c;
            }
            Stmt::ClassDef { id, css } => {
                db.add_style_class(id, css);
            }
            Stmt::ApplyStyle { ids, styles } => {
                // addStyle2Node: foundBlock.styles = styles.split(',')
                if let Some(&idx) = db.index.get(ids.trim()) {
                    db.blocks[idx].styles = styles.split(',').map(str::to_owned).collect();
                }
            }
            Stmt::ApplyClass { ids, class } => {
                for id in ids.split(',') {
                    let id = id.trim();
                    let idx = *db.index.entry(id.to_owned()).or_insert_with(|| {
                        db.blocks.push(Block::new(id));
                        db.blocks.len() - 1
                    });
                    db.blocks[idx].classes.push(class.clone());
                }
            }
            Stmt::Space(w) => {
                let space_id = db.generate_id();
                let mut b = Block::new(&space_id);
                b.typ = "space".to_owned();
                b.label = String::new();
                b.label_set = true;
                b.space_width = *w;
                let base = add_block(db, b);
                // expand into `width` copies
                for j in 0..*w {
                    let mut nb = db.blocks[base].clone();
                    nb.id = format!("{}-{j}", nb.id);
                    let idx = add_block(db, nb);
                    children.push(idx);
                }
                // the base space block itself is stored but not a child
                let _ = base;
            }
            Stmt::Node {
                id,
                typ,
                label,
                width_in_columns,
            } => {
                let mut b = Block::new(id);
                b.typ = typ.clone();
                b.width_in_columns = *width_in_columns;
                if let Some(l) = label {
                    b.label = l.clone();
                    b.label_set = true;
                } else {
                    b.label = id.clone();
                    b.label_set = true;
                }
                let idx = insert_or_merge(db, b);
                if let Some(i) = idx {
                    children.push(i);
                }
            }
            Stmt::Composite {
                id: comp_id,
                children: comp_children,
            } => {
                let id = comp_id.clone().unwrap_or_else(|| db.generate_id());
                let mut b = Block::new(&id);
                b.typ = "composite".to_owned();
                b.label = String::new();
                b.label_set = true;
                let idx = insert_or_merge(db, b);
                if let Some(i) = idx {
                    let grand = populate(db, comp_children, i);
                    db.blocks[i].children = grand;
                    children.push(i);
                }
            }
        }
    }
    children
}

fn add_block(db: &mut Db, b: Block) -> usize {
    let id = b.id.clone();
    db.blocks.push(b);
    let idx = db.blocks.len() - 1;
    db.index.insert(id, idx);
    idx
}

/// insert-or-merge like `populateBlockDatabase`'s existingBlock handling.
/// Returns Some(idx) only for newly-inserted blocks (existing merges push no
/// new child, matching JS `else if (existingBlock === undefined)`).
fn insert_or_merge(db: &mut Db, b: Block) -> Option<usize> {
    if let Some(&existing) = db.index.get(&b.id) {
        if b.typ != "na" {
            db.blocks[existing].typ = b.typ.clone();
        }
        if b.label != b.id {
            db.blocks[existing].label = b.label.clone();
            db.blocks[existing].label_set = true;
        }
        None
    } else {
        Some(add_block(db, b))
    }
}

fn set_hierarchy(db: &mut Db, stmts: &[Stmt]) {
    let root = db.root;
    let children = populate(db, stmts, root);
    db.blocks[root].children = children;
}

fn get_columns(db: &Db, idx: usize) -> i64 {
    let b = &db.blocks[idx];
    if b.columns != 0 && b.columns != -1 {
        return b.columns;
    }
    if b.columns == -1 {
        // JS: `if (block.columns) return block.columns;` -1 is truthy
        return -1;
    }
    if b.children.is_empty() {
        return -1;
    }
    b.children.len() as i64
}

// ---------------------------------------------------------------------------
// Layout (layout.ts): setBlockSizes / layoutBlocks / findBounds
// ---------------------------------------------------------------------------

fn calc_block_position(columns: i64, position: i64) -> (i64, i64) {
    if columns < 0 {
        return (position, 0);
    }
    if columns == 1 {
        return (0, position);
    }
    (position % columns, position / columns)
}

fn get_max_child_size(db: &Db, idx: usize) -> (f64, f64) {
    let mut max_w = 0.0f64;
    let mut max_h = 0.0f64;
    for &c in &db.blocks[idx].children.clone() {
        let child = &db.blocks[c];
        if child.typ == "space" {
            continue;
        }
        let (w, h) = child
            .size
            .as_ref()
            .map_or((0.0, 0.0), |s| (s.width, s.height));
        // normalizedWidth = width / (widthInColumns ?? 1)
        let wic = if child.width_in_columns == 0.0 {
            1.0
        } else {
            child.width_in_columns
        };
        let normalized_width = w / wic;
        if normalized_width > max_w {
            max_w = normalized_width;
        }
        if h > max_h {
            max_h = h;
        }
    }
    (max_w, max_h)
}

/// The label bbox for a node (createLabel path: width = Infinity → no wrap).
fn label_bbox(measurer: &TextMeasurer, label: &str) -> (f64, f64) {
    let b = measure_label_sized(measurer, label, f64::INFINITY, FONT_SIZE);
    (b.width, b.height)
}

/// The rendered node's `getBBox` size for a given shape (sizing pass).
fn node_bbox_size(typ: &str, lw: f64, lh: f64) -> (f64, f64) {
    match typ {
        "circle" => {
            let d = lw + PADDING;
            (d, d)
        }
        "doublecircle" => {
            let d = lw + PADDING + 10.0;
            (d, d)
        }
        "diamond" => {
            let s = (lw + PADDING) + (lh + PADDING);
            (s, s)
        }
        "stadium" => {
            let h = lh + PADDING;
            let w = lw + h / 4.0 + PADDING;
            (w, h)
        }
        "subroutine" => (lw + PADDING + 16.0, lh + PADDING),
        // rect / composite / square / round / na …
        _ => (lw + PADDING, lh + PADDING),
    }
}

/// calculateBlockSizes: measures every block (DFS over the hierarchy) and sets
/// its `size` from the label + shape `getBBox`. `group`-typed blocks are
/// skipped (never sized). Runs before `set_block_sizes`.
fn calculate_block_sizes(db: &mut Db, children: &[usize], measurer: &TextMeasurer) {
    for &c in children {
        if db.blocks[c].typ != "group" {
            let (lw, lh) = label_bbox(measurer, &db.blocks[c].label);
            let (w, h) = node_bbox_size(&db.blocks[c].typ, lw, lh);
            db.blocks[c].size = Some(Size {
                width: w,
                height: h,
                x: 0.0,
                y: 0.0,
            });
        }
        let grand = db.blocks[c].children.clone();
        if !grand.is_empty() {
            calculate_block_sizes(db, &grand, measurer);
        }
    }
}

fn set_block_sizes(
    db: &mut Db,
    idx: usize,
    measurer: &TextMeasurer,
    sibling_width: f64,
    sibling_height: f64,
) {
    if db.blocks[idx].size.as_ref().is_none_or(|s| s.width == 0.0) {
        db.blocks[idx].size = Some(Size {
            width: sibling_width,
            height: sibling_height,
            x: 0.0,
            y: 0.0,
        });
    }

    let children = db.blocks[idx].children.clone();
    if children.is_empty() {
        // Leaf sizes are pre-computed by calculate_block_sizes; nothing to do.
        return;
    }

    for &c in &children {
        set_block_sizes(db, c, measurer, 0.0, 0.0);
    }
    let (max_width, max_height) = get_max_child_size(db, idx);

    for &c in &children {
        let wic = db.blocks[c].width_in_columns;
        if let Some(s) = db.blocks[c].size.as_mut() {
            s.width = max_width * wic + PADDING * (wic - 1.0);
            s.height = max_height;
            s.x = 0.0;
            s.y = 0.0;
        }
    }
    for &c in &children {
        set_block_sizes(db, c, measurer, max_width, max_height);
    }

    let columns = db.blocks[idx].columns;
    let mut num_items = 0.0f64;
    for &c in &children {
        num_items += db.blocks[c].width_in_columns;
    }
    let mut x_size = children.len() as f64;
    if columns > 0 && (columns as f64) < num_items {
        x_size = columns as f64;
    }
    let y_size = (num_items / x_size).ceil();

    let mut width = x_size * (max_width + PADDING) + PADDING;
    let mut height = y_size * (max_height + PADDING) + PADDING;

    if width < sibling_width {
        width = sibling_width;
        height = sibling_height;
        let child_width = (sibling_width - x_size * PADDING - PADDING) / x_size;
        let child_height = (sibling_height - y_size * PADDING - PADDING) / y_size;
        for &c in &children {
            if let Some(s) = db.blocks[c].size.as_mut() {
                s.width = child_width;
                s.height = child_height;
                s.x = 0.0;
                s.y = 0.0;
            }
        }
    }

    let cur = db.blocks[idx].size.as_ref().map_or(0.0, |s| s.width);
    if width < cur {
        width = cur;
        let num = if columns > 0 {
            (children.len() as i64).min(columns) as f64
        } else {
            children.len() as f64
        };
        if num > 0.0 {
            let child_width = (width - num * PADDING - PADDING) / num;
            for &c in &children {
                if let Some(s) = db.blocks[c].size.as_mut() {
                    s.width = child_width;
                }
            }
        }
    }
    db.blocks[idx].size = Some(Size {
        width,
        height,
        x: 0.0,
        y: 0.0,
    });
}

fn layout_blocks(db: &mut Db, idx: usize) {
    let columns = db.blocks[idx].columns;
    let children = db.blocks[idx].children.clone();
    if children.is_empty() {
        return;
    }
    let (block_x, block_y, block_w, block_h) = {
        let s = db.blocks[idx].size.clone().unwrap_or_default();
        (s.x, s.y, s.width, s.height)
    };

    // Pre-compute per-row max heights.
    let mut row_heights: HashMap<i64, f64> = HashMap::new();
    {
        let mut col_pos = 0i64;
        for &c in &children {
            let Some(s) = db.blocks[c].size.as_ref() else {
                continue;
            };
            let (_px, py) = calc_block_position(columns, col_pos);
            let cur = row_heights.get(&py).copied().unwrap_or(0.0);
            if s.height > cur {
                row_heights.insert(py, s.height);
            }
            let mut filled = db.blocks[c].width_in_columns;
            if columns > 0 {
                filled = filled.min((columns - (col_pos % columns)) as f64);
            }
            col_pos += filled as i64;
        }
    }
    let mut row_y_offsets: HashMap<i64, f64> = HashMap::new();
    {
        let mut offset = 0.0f64;
        let mut rows: Vec<i64> = row_heights.keys().copied().collect();
        rows.sort_unstable();
        for row in rows {
            row_y_offsets.insert(row, offset);
            offset += row_heights.get(&row).copied().unwrap_or(0.0) + PADDING;
        }
    }

    let mut column_pos = 0i64;
    let mut starting_pos_x = if block_x != 0.0 {
        block_x + (-block_w / 2.0)
    } else {
        -PADDING
    };
    let mut row_pos = 0i64;
    for &c in &children {
        let (width, height) = {
            let Some(s) = db.blocks[c].size.as_ref() else {
                continue;
            };
            (s.width, s.height)
        };
        let (_px, py) = calc_block_position(columns, column_pos);
        if py != row_pos {
            row_pos = py;
            starting_pos_x = if block_x != 0.0 {
                block_x + (-block_w / 2.0)
            } else {
                -PADDING
            };
        }
        let half_width = width / 2.0;
        let cx = starting_pos_x + PADDING + half_width;
        starting_pos_x = cx + half_width;
        let row_y_offset = row_y_offsets.get(&py).copied().unwrap_or(0.0);
        let row_height = row_heights.get(&py).copied().unwrap_or(height);
        let cy = block_y - block_h / 2.0 + row_y_offset + row_height / 2.0 + PADDING;
        if let Some(s) = db.blocks[c].size.as_mut() {
            s.x = cx;
            s.y = cy;
        }
        if !db.blocks[c].children.is_empty() {
            layout_blocks(db, c);
        }
        let mut columns_filled = db.blocks[c].width_in_columns;
        if columns > 0 {
            columns_filled = columns_filled.min((columns - (column_pos % columns)) as f64);
        }
        column_pos += columns_filled as i64;
    }
}

fn find_bounds(db: &Db, idx: usize, bounds: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = bounds;
    let b = &db.blocks[idx];
    if let Some(s) = b.size.as_ref() {
        if b.id != "root" {
            if s.x - s.width / 2.0 < min_x {
                min_x = s.x - s.width / 2.0;
            }
            if s.y - s.height / 2.0 < min_y {
                min_y = s.y - s.height / 2.0;
            }
            if s.x + s.width / 2.0 > max_x {
                max_x = s.x + s.width / 2.0;
            }
            if s.y + s.height / 2.0 > max_y {
                max_y = s.y + s.height / 2.0;
            }
        }
    }
    for &c in &b.children {
        (min_x, min_y, max_x, max_y) = find_bounds(db, c, (min_x, min_y, max_x, max_y));
    }
    (min_x, min_y, max_x, max_y)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Builds the label `<g class="label">` with the block createLabel div style
/// (`display: table-cell; white-space: nowrap; line-height: 1.5;`).
/// Port of `getStylesFromArray`: returns (style, labelStyle). `color:` and
/// `text-align:` declarations go to labelStyle; the rest to style. Each keeps a
/// trailing `;`.
fn styles_from_array(arr: &[String]) -> (String, String) {
    let mut style = String::new();
    let mut label_style = String::new();
    for el in arr {
        if el.starts_with("color:") || el.starts_with("text-align:") {
            label_style.push_str(el);
            label_style.push(';');
        } else {
            style.push_str(el);
            style.push(';');
        }
    }
    (style, label_style)
}

fn build_label(shape_svg: &Element, label: &str, lw: f64, lh: f64, label_style: &str) {
    let label_el = append(shape_svg, "g");
    set_attr(&label_el, "class", "label");
    set_attr(&label_el, "style", label_style);
    // foreignObject
    let fo = append(&label_el, "foreignObject");
    set_attr(&fo, "width", js_num(lw));
    set_attr(&fo, "height", js_num(lh));
    let div = append_xhtml(&fo, "div");
    set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
    set_attr(
        &div,
        "style",
        "display: table-cell; white-space: nowrap; line-height: 1.5;",
    );
    let span = append_xhtml(&div, "span");
    set_attr(&span, "class", "nodeLabel");
    if !label.is_empty() {
        let p = append_xhtml(&span, "p");
        set_text_append(&p, label);
    }
    // label.attr('transform', translate(-lw/2, -lh/2))
    set_attr(
        &label_el,
        "transform",
        format!("translate({}, {})", js_num(-lw / 2.0), js_num(-lh / 2.0)),
    );
    // label.insert('rect', ':first-child')
    insert_first(&label_el, "rect");
}

/// Renders one positioned block (insertBlockPositioned + shape function).
fn insert_block(parent: &Element, db: &Db, idx: usize, diagram_id: &str, measurer: &TextMeasurer) {
    let b = &db.blocks[idx];
    if b.typ == "space" {
        return;
    }
    let (lw, lh) = label_bbox(measurer, &b.label);
    let size = b.size.clone().unwrap_or_default();

    // shapeSvg <g> — class is overwritten by insertNode to "node default {class}".
    let class_str = if b.classes.is_empty() {
        "default flowchart-label".to_owned()
    } else {
        format!("{} flowchart-label", b.classes.join(" "))
    };
    let g = append(parent, "g");
    set_attr(&g, "class", format!("node default {class_str}"));
    let dom_id = if diagram_id.is_empty() {
        b.id.clone()
    } else {
        format!("{diagram_id}-{}", b.id)
    };
    set_attr(&g, "id", dom_id);

    // getStylesFromArray: split into shape style and label style.
    let (style, label_style) = styles_from_array(&b.styles);
    build_label(&g, &b.label, lw, lh, &label_style);
    insert_shape(&g, &b.typ, lw, lh, &size, &style);

    // positionNode: translate(x, y)
    set_attr(
        &g,
        "transform",
        format!("translate({}, {})", js_num(size.x), js_num(size.y)),
    );
}

/// Inserts the shape element as first-child of the shapeSvg group.
fn insert_shape(g: &Element, typ: &str, lw: f64, lh: f64, size: &Size, style: &str) {
    let half_padding = PADDING / 2.0;
    match typ {
        "circle" => {
            let c = insert_first(g, "circle");
            set_attr(&c, "style", style);
            set_attr(&c, "rx", "0");
            set_attr(&c, "ry", "0");
            set_attr(&c, "r", js_num(lw / 2.0 + half_padding));
            set_attr(&c, "width", js_num(lw + PADDING));
            set_attr(&c, "height", js_num(lh + PADDING));
        }
        "doublecircle" => {
            let group = insert_first(g, "g");
            let outer = append(&group, "circle");
            let inner = append(&group, "circle");
            for (circ, extra) in [(&outer, 5.0f64), (&inner, 0.0)] {
                set_attr(circ, "style", style);
                set_attr(circ, "rx", "0");
                set_attr(circ, "ry", "0");
                set_attr(circ, "r", js_num(lw / 2.0 + half_padding + extra));
                set_attr(circ, "width", js_num(lw + PADDING + extra * 2.0));
                set_attr(circ, "height", js_num(lh + PADDING + extra * 2.0));
            }
        }
        "diamond" => {
            let w = lw + PADDING;
            let h = lh + PADDING;
            let s = w + h;
            let points = [
                (s / 2.0, 0.0),
                (s, -s / 2.0),
                (s / 2.0, -s),
                (0.0, -s / 2.0),
            ];
            insert_polygon(g, s, s, &points, style);
        }
        "stadium" => {
            let h = lh + PADDING;
            let w = lw + h / 4.0 + PADDING;
            let r = insert_first(g, "rect");
            set_attr(&r, "style", style);
            set_attr(&r, "rx", js_num(h / 2.0));
            set_attr(&r, "ry", js_num(h / 2.0));
            set_attr(&r, "x", js_num(-w / 2.0));
            set_attr(&r, "y", js_num(-h / 2.0));
            set_attr(&r, "width", js_num(w));
            set_attr(&r, "height", js_num(h));
        }
        "subroutine" => {
            let w = lw + PADDING;
            let h = lh + PADDING;
            let points = [
                (0.0, 0.0),
                (w, 0.0),
                (w, -h),
                (0.0, -h),
                (0.0, 0.0),
                (-8.0, 0.0),
                (w + 8.0, 0.0),
                (w + 8.0, -h),
                (-8.0, -h),
                (-8.0, 0.0),
            ];
            insert_polygon(g, w, h, &points, style);
        }
        "composite" => {
            let r = insert_first(g, "rect");
            set_attr(&r, "class", "basic cluster composite label-container");
            set_attr(&r, "style", style);
            set_attr(&r, "rx", "0");
            set_attr(&r, "ry", "0");
            let tw = size.width;
            let th = size.height;
            set_attr(&r, "x", js_num(-tw / 2.0));
            set_attr(&r, "y", js_num(-th / 2.0));
            set_attr(&r, "width", js_num(tw));
            set_attr(&r, "height", js_num(th));
        }
        // rect / square / round / na …
        _ => {
            let r = insert_first(g, "rect");
            set_attr(&r, "class", "basic label-container");
            set_attr(&r, "style", style);
            let radius = if typ == "round" { "5" } else { "0" };
            set_attr(&r, "rx", radius);
            set_attr(&r, "ry", radius);
            let tw = size.width;
            let th = size.height;
            set_attr(&r, "x", js_num(-tw / 2.0));
            set_attr(&r, "y", js_num(-th / 2.0));
            set_attr(&r, "width", js_num(tw));
            set_attr(&r, "height", js_num(th));
        }
    }
}

fn insert_polygon(g: &Element, w: f64, h: f64, points: &[(f64, f64)], style: &str) {
    let poly = insert_first(g, "polygon");
    let pts = points
        .iter()
        .map(|(x, y)| format!("{},{}", js_num(*x), js_num(*y)))
        .collect::<Vec<_>>()
        .join(" ");
    set_attr(&poly, "points", pts);
    set_attr(&poly, "class", "label-container");
    set_attr(
        &poly,
        "transform",
        format!("translate({},{})", js_num(-w / 2.0), js_num(h / 2.0)),
    );
    set_attr(&poly, "style", style);
}

/// Recursively renders all blocks (performOperations order: block then its
/// children).
fn insert_blocks(
    parent: &Element,
    db: &Db,
    children: &[usize],
    diagram_id: &str,
    measurer: &TextMeasurer,
) {
    for &c in children {
        insert_block(parent, db, c, diagram_id, measurer);
        let grand = db.blocks[c].children.clone();
        if !grand.is_empty() {
            insert_blocks(parent, db, &grand, diagram_id, measurer);
        }
    }
}

/// Renders mermaid `block-beta` source to a complete SVG document string.
///
/// # Errors
/// Returns [`BlockParseError`] when the source is not a valid block diagram.
pub fn render_block(source: &str, id: &str) -> Result<String, BlockParseError> {
    let config = crate::render::config::detect_init(source);
    let theme_vars = crate::render::themes::theme_variables(&config.theme, &config.theme_variables);
    let measurer = TextMeasurer::new();

    let stmts = parse(source)?;
    let mut db = Db::new();
    set_hierarchy(&mut db, &stmts);

    // calculateBlockSizes (measure) then layout(db): setBlockSizes + layoutBlocks
    let root = db.root;
    let root_children_for_calc = db.blocks[root].children.clone();
    calculate_block_sizes(&mut db, &root_children_for_calc, &measurer);
    set_block_sizes(&mut db, root, &measurer, 0.0, 0.0);
    layout_blocks(&mut db, root);
    let (min_x, min_y, max_x, max_y) = find_bounds(&db, root, (0.0, 0.0, 0.0, 0.0));
    let width = max_x - min_x;
    let height = max_y - min_y;

    // SVG scaffold.
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    let style_el = append(&svg, "style");
    crate::svg::set_text(
        &style_el,
        &format!(
            "{}{}",
            crate::render::css::themed_block_css(id, &theme_vars),
            crate::render::css::class_defs_css(id, true, &db.classes),
        ),
    );
    let _ = get_columns; // referenced for parity with blockDB (unused directly)

    // markers group placeholder (mermaid inserts an empty <g/> before markers)
    let _empty = append(&svg, "g");
    insert_markers(&svg, id);

    // block container group
    let nodes = append(&svg, "g");
    set_attr(&nodes, "class", "block");
    let root_children = db.blocks[db.root].children.clone();
    insert_blocks(&nodes, &db, &root_children, id, &measurer);

    // viewBox: `${x-5} ${y-5} ${width+10} ${height+10}`
    let vb_w = width + 10.0;
    let vb_h = height + 10.0;
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            crate::render::css_length(vb_w)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} {} {} {}",
            js_num(min_x - 5.0),
            js_num(min_y - 5.0),
            js_num(vb_w),
            js_num(vb_h)
        ),
    );
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", "block");

    let mut out = String::new();
    serialize(&svg, &mut out);
    Ok(out)
}

/// Inserts the point/circle/cross markers (dagre `insertMarkers`, block flavor).
fn insert_markers(svg: &Element, id: &str) {
    let mut buf = String::new();
    let _ = &mut buf;
    // point
    marker_point(svg, id);
    marker_circle(svg, id);
    marker_cross(svg, id);
}

fn marker_point(svg: &Element, id: &str) {
    for (suffix, refx, path) in [
        ("End", "6", "M 0 0 L 10 5 L 0 10 z"),
        ("Start", "4.5", "M 0 5 L 10 10 L 10 0 z"),
    ] {
        let m = append(svg, "marker");
        set_attr(&m, "id", format!("{id}_block-point{suffix}"));
        set_attr(&m, "class", "marker block");
        set_attr(&m, "viewBox", "0 0 10 10");
        set_attr(&m, "refX", refx);
        set_attr(&m, "refY", "5");
        set_attr(&m, "markerUnits", "userSpaceOnUse");
        set_attr(&m, "markerWidth", "12");
        set_attr(&m, "markerHeight", "12");
        set_attr(&m, "orient", "auto");
        let p = append(&m, "path");
        set_attr(&p, "d", path);
        set_attr(&p, "class", "arrowMarkerPath");
        set_attr(&p, "style", "stroke-width: 1; stroke-dasharray: 1, 0;");
    }
}

fn marker_circle(svg: &Element, id: &str) {
    for (suffix, refx) in [("End", "11"), ("Start", "-1")] {
        let m = append(svg, "marker");
        set_attr(&m, "id", format!("{id}_block-circle{suffix}"));
        set_attr(&m, "class", "marker block");
        set_attr(&m, "viewBox", "0 0 10 10");
        set_attr(&m, "refX", refx);
        set_attr(&m, "refY", "5");
        set_attr(&m, "markerUnits", "userSpaceOnUse");
        set_attr(&m, "markerWidth", "11");
        set_attr(&m, "markerHeight", "11");
        set_attr(&m, "orient", "auto");
        let c = append(&m, "circle");
        set_attr(&c, "cx", "5");
        set_attr(&c, "cy", "5");
        set_attr(&c, "r", "5");
        set_attr(&c, "class", "arrowMarkerPath");
        set_attr(&c, "style", "stroke-width: 1; stroke-dasharray: 1, 0;");
    }
}

fn marker_cross(svg: &Element, id: &str) {
    for (suffix, refx) in [("End", "12"), ("Start", "-1")] {
        let m = append(svg, "marker");
        set_attr(&m, "id", format!("{id}_block-cross{suffix}"));
        set_attr(&m, "class", "marker cross block");
        set_attr(&m, "viewBox", "0 0 11 11");
        set_attr(&m, "refX", refx);
        set_attr(&m, "refY", "5.2");
        set_attr(&m, "markerUnits", "userSpaceOnUse");
        set_attr(&m, "markerWidth", "11");
        set_attr(&m, "markerHeight", "11");
        set_attr(&m, "orient", "auto");
        let p = append(&m, "path");
        set_attr(&p, "d", "M 1,1 l 9,9 M 10,1 l -9,9");
        set_attr(&p, "class", "arrowMarkerPath");
        set_attr(&p, "style", "stroke-width: 2; stroke-dasharray: 1, 0;");
    }
}
