//! erDiagram support: parser subset (of `erDiagram.jison`/langium grammar),
//! `erDb.ts` semantics, and layout-data construction. Rendering goes through
//! the unified dagre pipeline with the `erBox` shape and crow's-foot markers.

#![allow(clippy::assigning_clones, clippy::match_same_arms)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::render::data::{LayoutData, RenderEdge, RenderNode};

/// A parse error for erDiagram source.
#[derive(Debug)]
pub struct ErParseError {
    pub message: String,
}

impl std::fmt::Display for ErParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "er diagram parse error: {}", self.message)
    }
}

impl std::error::Error for ErParseError {}

#[derive(Debug, Clone, Default)]
struct Attribute {
    ty: String,
    name: String,
    keys: Vec<String>,
    comment: String,
}

#[derive(Debug, Clone, Default)]
struct Entity {
    id: String,
    label: String,
    alias: String,
    attributes: Vec<Attribute>,
}

#[derive(Debug, Clone)]
struct Relationship {
    entity_a: String,
    entity_b: String,
    role_a: String,
    card_a: String,
    card_b: String,
    identifying: bool,
}

#[derive(Debug, Default)]
struct ErDb {
    entities: indexmap::IndexMap<String, Entity>,
    relationships: Vec<Relationship>,
    direction: String,
}

impl ErDb {
    fn add_entity(&mut self, name: &str) -> String {
        if !self.entities.contains_key(name) {
            let id = format!("entity-{name}-{}", self.entities.len());
            self.entities.insert(
                name.to_owned(),
                Entity {
                    id,
                    label: name.to_owned(),
                    ..Entity::default()
                },
            );
        }
        self.entities[name].id.clone()
    }
}

/// Cardinality token → erDb Cardinality name (lowercased in getData).
fn cardinality(token: &str, left: bool) -> Option<&'static str> {
    // Left tokens read outward: `||`, `|o`, `}o`, `}|`.
    // Right tokens: `||`, `o|`, `o{`, `|{`.
    match (token, left) {
        ("||", _) => Some("only_one"),
        ("|o", true) | ("o|", false) => Some("zero_or_one"),
        ("}o", true) | ("o{", false) => Some("zero_or_more"),
        ("}|", true) | ("|{", false) => Some("one_or_more"),
        ("one or zero" | "zero or one", _) => Some("zero_or_one"),
        ("one or more" | "one or many" | "many(1)" | "1+", _) => Some("one_or_more"),
        ("zero or more" | "zero or many" | "many(0)" | "0+", _) => Some("zero_or_more"),
        ("only one" | "1", _) => Some("only_one"),
        _ => None,
    }
}

/// Splits a relationship line `A <card><line><card> B : label`.
fn parse_relationship(line: &str) -> Option<(String, String, String, String, bool, String)> {
    // Find the line part: -- (identifying) or .. (non-identifying), also
    // `.-`/`-.` variants.
    let (idx, ident, line_len) = ["--", "..", ".-", "-."]
        .iter()
        .filter_map(|tok| line.find(tok).map(|i| (i, *tok == "--", tok.len())))
        .min_by_key(|(i, _, _)| *i)?;
    // Cardinality tokens are the 2 chars on each side.
    if idx < 2 || idx + line_len + 2 > line.len() {
        return None;
    }
    let card_b_tok = &line[idx - 2..idx];
    let card_a_tok = &line[idx + line_len..idx + line_len + 2];
    let card_b = cardinality(card_b_tok, true)?;
    let card_a = cardinality(card_a_tok, false)?;
    let entity_a = line[..idx - 2].trim();
    let rest = line[idx + line_len + 2..].trim();
    let (entity_b, label) = match rest.find(':') {
        Some(c) => (
            rest[..c].trim().to_owned(),
            rest[c + 1..].trim().trim_matches('"').to_owned(),
        ),
        None => (rest.to_owned(), String::new()),
    };
    if entity_a.is_empty() || entity_b.is_empty() {
        return None;
    }
    Some((
        entity_a.to_owned(),
        entity_b,
        label,
        card_a.to_owned(),
        ident,
        card_b.to_owned(),
    ))
}

fn parse(source: &str) -> Result<ErDb, ErParseError> {
    let mut db = ErDb {
        direction: "TB".to_owned(),
        ..ErDb::default()
    };
    let mut found_header = false;
    let mut lines = source.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if !found_header {
            if line.starts_with("erDiagram") {
                found_header = true;
                continue;
            }
            return Err(ErParseError {
                message: format!("expected erDiagram header, got {line:?}"),
            });
        }
        if let Some(rest) = line.strip_prefix("direction ") {
            rest.trim().clone_into(&mut db.direction);
            continue;
        }
        // Entity with attribute block: NAME { ... } (or NAME["alias"] {).
        if let Some(head) = line.strip_suffix('{') {
            let head = head.trim();
            let (name, alias) = split_alias(head);
            db.add_entity(&name);
            if !alias.is_empty() {
                db.entities[&name].alias = alias;
            }
            for attr_raw in lines.by_ref() {
                let a = attr_raw.trim();
                if a == "}" {
                    break;
                }
                if a.is_empty() || a.starts_with("%%") {
                    continue;
                }
                db.entities[&name].attributes.push(parse_attribute(a));
            }
            continue;
        }
        if let Some((a, b, label, card_a, ident, card_b)) = parse_relationship(line) {
            let ida = db.add_entity(&a);
            let idb = db.add_entity(&b);
            db.relationships.push(Relationship {
                entity_a: ida,
                entity_b: idb,
                role_a: label,
                card_a,
                card_b,
                identifying: ident,
            });
            continue;
        }
        // Standalone entity declaration.
        let (name, alias) = split_alias(line);
        if !name.is_empty() && !name.contains(char::is_whitespace) {
            db.add_entity(&name);
            if !alias.is_empty() {
                db.entities[&name].alias = alias;
            }
            continue;
        }
        return Err(ErParseError {
            message: format!("unsupported er statement: {line}"),
        });
    }
    if !found_header {
        return Err(ErParseError {
            message: "missing erDiagram header".to_owned(),
        });
    }
    Ok(db)
}

/// `NAME["alias"]` → (NAME, alias).
fn split_alias(head: &str) -> (String, String) {
    if let Some(open) = head.find('[')
        && head.ends_with(']')
    {
        let name = head[..open].trim().to_owned();
        let alias = head[open + 1..head.len() - 1].trim().trim_matches('"');
        return (name, alias.to_owned());
    }
    (head.trim().to_owned(), String::new())
}

/// `type name [PK|FK|UK[, ...]] ["comment"]`.
fn parse_attribute(raw: &str) -> Attribute {
    let mut attr = Attribute::default();
    let mut rest = raw.trim();
    // Trailing comment.
    if rest.ends_with('"')
        && let Some(open) = rest[..rest.len() - 1].rfind('"')
    {
        attr.comment = rest[open + 1..rest.len() - 1].to_owned();
        rest = rest[..open].trim_end();
    }
    let mut tokens = rest.split_whitespace();
    attr.ty = tokens.next().unwrap_or_default().to_owned();
    attr.name = tokens.next().unwrap_or_default().to_owned();
    // Remaining tokens are key lists (possibly comma-separated).
    for tok in tokens {
        for key in tok.split(',') {
            let key = key.trim();
            if !key.is_empty() {
                attr.keys.push(key.to_owned());
            }
        }
    }
    attr
}

/// Parses erDiagram source into layout data.
///
/// # Errors
///
/// Returns [`ErParseError`] for unparsable source.
pub fn get_layout_data(source: &str, id: &str) -> Result<LayoutData, ErParseError> {
    let db = parse(source)?;
    let mut nodes: Vec<Rc<RefCell<RenderNode>>> = Vec::new();
    let mut edges: Vec<Rc<RefCell<RenderEdge>>> = Vec::new();

    for entity in db.entities.values() {
        let node = RenderNode {
            id: entity.id.clone(),
            label: entity.label.clone(),
            label_raw: entity.label.clone(),
            label_type: "markdown".to_owned(),
            shape: "erBox".to_owned(),
            dom_id: entity.id.clone(),
            css_classes: "default".to_owned(),
            look: "classic".to_owned(),
            er_alias: entity.alias.clone(),
            er_attributes: entity
                .attributes
                .iter()
                .map(|a| {
                    (
                        a.ty.clone(),
                        a.name.clone(),
                        a.keys.join(","),
                        a.comment.clone(),
                    )
                })
                .collect(),
            ..RenderNode::default()
        };
        nodes.push(Rc::new(RefCell::new(node)));
    }

    for (count, rel) in db.relationships.iter().enumerate() {
        let edge = RenderEdge {
            id: format!("id_{}_{}_{count}", rel.entity_a, rel.entity_b),
            start: rel.entity_a.clone(),
            end: rel.entity_b.clone(),
            label: rel.role_a.clone(),
            label_raw: rel.role_a.clone(),
            labelpos: "c".to_owned(),
            thickness: "normal".to_owned(),
            classes: "relationshipLine".to_owned(),
            arrow_type_start: rel.card_b.clone(),
            arrow_type_end: rel.card_a.clone(),
            pattern: if rel.identifying {
                "solid".to_owned()
            } else {
                "dashed".to_owned()
            },
            look: "classic".to_owned(),
            label_type: "markdown".to_owned(),
            // edge.style is undefined upstream; the style-attr quirk turns
            // it into the literal string.
            style: vec!["undefined".to_owned()],
            label_style: Vec::new(),
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
