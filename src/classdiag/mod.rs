//! classDiagram support: parser subset (of `classDiagram.jison`), database
//! (`classDb.ts`), and layout-data construction. Rendering goes through the
//! unified dagre pipeline with the `classBox` shape.

use std::cell::RefCell;
use std::rc::Rc;

use crate::render::data::{LayoutData, RenderEdge, RenderNode};

/// A parse error for class diagram source.
#[derive(Debug)]
pub struct ClassParseError {
    pub message: String,
}

impl std::fmt::Display for ClassParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "class diagram parse error: {}", self.message)
    }
}

impl std::error::Error for ClassParseError {}

// relationType constants.
const AGGREGATION: i32 = 0;
const EXTENSION: i32 = 1;
const COMPOSITION: i32 = 2;
const DEPENDENCY: i32 = 3;
const LOLLIPOP: i32 = 4;
const NONE: i32 = -1;

#[derive(Debug, Clone, Default)]
struct ClassDef {
    id: String,
    label: String,
    dom_id: String,
    annotations: Vec<String>,
    members: Vec<(String, String)>,
    methods: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct Relation {
    id1: String,
    id2: String,
    title: String,
    title1: String,
    title2: String,
    type1: i32,
    type2: i32,
    line_type: i32,
}

#[derive(Debug, Default)]
struct ClassDb {
    classes: indexmap::IndexMap<String, ClassDef>,
    relations: Vec<Relation>,
    class_counter: usize,
    direction: String,
}

impl ClassDb {
    fn add_class(&mut self, id: &str) {
        if self.classes.contains_key(id) {
            return;
        }
        let def = ClassDef {
            id: id.to_owned(),
            label: id.to_owned(),
            dom_id: format!("classId-{id}-{}", self.class_counter),
            ..ClassDef::default()
        };
        self.class_counter += 1;
        self.classes.insert(id.to_owned(), def);
    }

    fn add_member(&mut self, id: &str, raw: &str) {
        self.add_class(id);
        let class = self.classes.get_mut(id).expect("just added");
        let member = parse_member(raw);
        if raw.contains('(') {
            class.methods.push(member);
        } else if let Some(annotation) = raw
            .trim()
            .strip_prefix("<<")
            .and_then(|r| r.strip_suffix(">>"))
        {
            class.annotations.push(annotation.to_owned());
        } else {
            class.members.push(member);
        }
    }
}

/// `ClassMember.getDisplayDetails()`: returns (displayText, cssStyle).
fn parse_member(raw: &str) -> (String, String) {
    let input = raw.trim();
    let visibilities = ['+', '-', '#', '~'];
    if input.contains('(') {
        // method: ([#+~-])?(.+)\((.*)\)([\s$*])?(.*)([$*])?
        let (visibility, rest) = match input.chars().next() {
            Some(c) if visibilities.contains(&c) => (c.to_string(), input[1..].to_owned()),
            _ => (String::new(), input.to_owned()),
        };
        let open = rest.find('(').unwrap_or(0);
        let close = rest.rfind(')').unwrap_or(rest.len() - 1);
        let name = rest[..open].trim_end().to_owned();
        let params = rest[open + 1..close].trim().to_owned();
        let after = rest[close + 1..].to_owned();
        // Optional classifier directly after ')' then return type.
        let (classifier, return_type) = {
            let t = after.trim_start();
            if let Some(r) = t.strip_prefix('*') {
                ("*".to_owned(), r.trim().to_owned())
            } else if let Some(r) = t.strip_prefix('$') {
                ("$".to_owned(), r.trim().to_owned())
            } else {
                let t = t.trim();
                if let Some(r) = t.strip_suffix('*') {
                    ("*".to_owned(), r.trim().to_owned())
                } else if let Some(r) = t.strip_suffix('$') {
                    ("$".to_owned(), r.trim().to_owned())
                } else {
                    (String::new(), t.to_owned())
                }
            }
        };
        let mut display = format!("{visibility}{name}({params})");
        if !return_type.is_empty() {
            let _ = std::fmt::Write::write_fmt(&mut display, format_args!(" : {return_type}"));
        }
        (display.trim().to_owned(), classifier_style(&classifier))
    } else {
        // attribute
        let (visibility, rest) = match input.chars().next() {
            Some(c) if visibilities.contains(&c) => (c.to_string(), input[1..].to_owned()),
            _ => (String::new(), input.to_owned()),
        };
        let t = rest.trim();
        let (text, classifier) = if let Some(r) = t.strip_suffix('*') {
            (r.trim_end(), "*".to_owned())
        } else if let Some(r) = t.strip_suffix('$') {
            (r.trim_end(), "$".to_owned())
        } else {
            (t, String::new())
        };
        (
            format!("{visibility}{text}").trim().to_owned(),
            classifier_style(&classifier),
        )
    }
}

fn classifier_style(classifier: &str) -> String {
    match classifier {
        "*" => "font-style:italic;".to_owned(),
        "$" => "text-decoration:underline;".to_owned(),
        _ => String::new(),
    }
}

/// Parses one relation arrow like `<|--`, `--|>`, `"1" --> "*"`.
fn parse_relation_arrow(arrow: &str) -> Option<(i32, i32, i32)> {
    // returns (type1, type2, lineType)
    let mut rest = arrow;
    let mut type1 = NONE;
    for (tok, ty) in [
        ("<|", EXTENSION),
        ("()", LOLLIPOP),
        ("o", AGGREGATION),
        ("*", COMPOSITION),
        ("<", DEPENDENCY),
    ] {
        if let Some(r) = rest.strip_prefix(tok) {
            type1 = ty;
            rest = r;
            break;
        }
    }
    let line_type = if rest.starts_with("--") {
        rest = &rest[2..];
        0
    } else if rest.starts_with("..") {
        rest = &rest[2..];
        1
    } else {
        return None;
    };
    let mut type2 = NONE;
    for (tok, ty) in [
        ("|>", EXTENSION),
        ("()", LOLLIPOP),
        ("o", AGGREGATION),
        ("*", COMPOSITION),
        (">", DEPENDENCY),
    ] {
        if let Some(r) = rest.strip_suffix(tok) {
            type2 = ty;
            rest = r;
            break;
        }
    }
    if !rest.is_empty() {
        return None;
    }
    Some((type1, type2, line_type))
}

fn arrow_marker(ty: i32) -> String {
    match ty {
        AGGREGATION => "aggregation".to_owned(),
        EXTENSION => "extension".to_owned(),
        COMPOSITION => "composition".to_owned(),
        DEPENDENCY => "dependency".to_owned(),
        LOLLIPOP => "lollipop".to_owned(),
        _ => "none".to_owned(),
    }
}

#[allow(clippy::too_many_lines)]
fn parse(source: &str) -> Result<ClassDb, ClassParseError> {
    let mut db = ClassDb {
        direction: "TB".to_owned(),
        ..ClassDb::default()
    };
    let mut found_header = false;
    let mut lines = source.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with('#') {
            continue;
        }
        if !found_header {
            if line.starts_with("classDiagram") {
                found_header = true;
                continue;
            }
            return Err(ClassParseError {
                message: format!("expected classDiagram header, got {line:?}"),
            });
        }
        if let Some(rest) = line.strip_prefix("direction ") {
            rest.trim().clone_into(&mut db.direction);
            continue;
        }
        if let Some(rest) = line.strip_prefix("class ") {
            let rest = rest.trim();
            if let Some(name) = rest.strip_suffix('{') {
                let name = name.trim();
                db.add_class(name);
                // read members until '}'
                for member_raw in lines.by_ref() {
                    let m = member_raw.trim();
                    if m == "}" {
                        break;
                    }
                    if m.is_empty() || m.starts_with("%%") {
                        continue;
                    }
                    db.add_member(name, m);
                }
            } else {
                db.add_class(rest);
            }
            continue;
        }
        // member via colon: `Class : +member`
        // or relation: `A <|-- B [: label]`
        if let Some((head, label)) = split_relation(line) {
            let (id1, arrow, id2, t1, t2) = head;
            db.add_class(&id1);
            db.add_class(&id2);
            let Some((type1, type2, line_type)) = parse_relation_arrow(&arrow) else {
                return Err(ClassParseError {
                    message: format!("bad relation arrow: {arrow}"),
                });
            };
            db.relations.push(Relation {
                id1,
                id2,
                title: label,
                title1: t1,
                title2: t2,
                type1,
                type2,
                line_type,
            });
            continue;
        }
        if let Some((id, member)) = line.split_once(':') {
            let id = id.trim();
            if !id.is_empty() && !id.contains(char::is_whitespace) {
                db.add_member(id, member.trim());
                continue;
            }
        }
        return Err(ClassParseError {
            message: format!("unsupported statement: {line}"),
        });
    }
    if !found_header {
        return Err(ClassParseError {
            message: "missing classDiagram header".to_owned(),
        });
    }
    Ok(db)
}

type RelationHead = (String, String, String, String, String);

/// Splits `A "1" <|-- "*" B : label` into ids, arrow, cardinalities, label.
fn split_relation(line: &str) -> Option<(RelationHead, String)> {
    // Find the arrow span: a run containing -- or .. with optional ends.
    let arrow_idx = line.find("--").or_else(|| line.find(".."))?;
    // expand left/right over arrow end tokens
    let bytes = line.as_bytes();
    let mut start = arrow_idx;
    for tok in ["<|", "()", "o", "*", "<"] {
        if start >= tok.len() && line[..start].ends_with(tok) {
            start -= tok.len();
            break;
        }
    }
    let mut end = arrow_idx + 2;
    for tok in ["|>", "()", "o", "*", ">"] {
        if line[end..].starts_with(tok) {
            end += tok.len();
            break;
        }
    }
    let _ = bytes;
    let arrow = line[start..end].to_owned();
    let left = line[..start].trim();
    let right_all = line[end..].trim();
    let (right_part, label) = match right_all.find(':') {
        Some(c) => (
            right_all[..c].trim().to_owned(),
            right_all[c + 1..].trim().to_owned(),
        ),
        None => (right_all.to_owned(), String::new()),
    };
    // cardinalities: quoted strings adjacent to the arrow
    let (id1, t1) = if let Some(q) = left.rfind('"') {
        let open = left[..q].rfind('"')?;
        (left[..open].trim().to_owned(), left[open + 1..q].to_owned())
    } else {
        (left.to_owned(), String::new())
    };
    let (id2, t2) = if let Some(stripped) = right_part.strip_prefix('"') {
        let close = stripped.find('"')?;
        (
            stripped[close + 1..].trim().to_owned(),
            stripped[..close].to_owned(),
        )
    } else {
        (right_part.clone(), String::new())
    };
    if id1.is_empty() || id2.is_empty() || id1.contains(char::is_whitespace) {
        return None;
    }
    Some(((id1, arrow, id2, t1, t2), label))
}

/// Parses class diagram source into layout data.
///
/// # Errors
///
/// Returns [`ClassParseError`] for unparsable source.
pub fn get_layout_data(source: &str, id: &str) -> Result<LayoutData, ClassParseError> {
    let db = parse(source)?;
    let mut nodes: Vec<Rc<RefCell<RenderNode>>> = Vec::new();
    let mut edges: Vec<Rc<RefCell<RenderEdge>>> = Vec::new();

    for class in db.classes.values() {
        let node = RenderNode {
            id: class.id.clone(),
            label: class.label.clone(),
            label_raw: class.label.clone(),
            label_type: "markdown".to_owned(),
            shape: "classBox".to_owned(),
            dom_id: class.dom_id.clone(),
            css_classes: "default".to_owned(),
            look: "classic".to_owned(),
            padding: 12.0,
            class_annotations: class.annotations.clone(),
            class_members: class.members.clone(),
            class_methods: class.methods.clone(),
            ..RenderNode::default()
        };
        nodes.push(Rc::new(RefCell::new(node)));
    }

    for (cnt, rel) in db.relations.iter().enumerate() {
        let edge = RenderEdge {
            id: format!("id_{}_{}_{}", rel.id1, rel.id2, cnt + 1),
            start: rel.id1.clone(),
            end: rel.id2.clone(),
            label: rel.title.clone(),
            label_raw: rel.title.clone(),
            labelpos: "c".to_owned(),
            thickness: "normal".to_owned(),
            classes: "relation".to_owned(),
            arrow_type_start: arrow_marker(rel.type1),
            arrow_type_end: arrow_marker(rel.type2),
            start_label_right: rel.title1.clone(),
            end_label_left: rel.title2.clone(),
            style: vec![String::new()],
            label_style: Vec::new(),
            pattern: if rel.line_type == 1 {
                "dashed".to_owned()
            } else {
                "solid".to_owned()
            },
            look: "classic".to_owned(),
            label_type: "markdown".to_owned(),
            minlen: 1.0,
            curve: "basis".to_owned(),
            ..RenderEdge::default()
        };
        edges.push(Rc::new(RefCell::new(edge)));
    }

    Ok(LayoutData {
        nodes,
        edges,
        direction: db.direction,
        diagram_id: id.to_owned(),
    })
}
