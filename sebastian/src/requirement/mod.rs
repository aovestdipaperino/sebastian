//! **Approximate** (non-byte-exact) renderer for mermaid `requirementDiagram`.
//!
//! Requirement diagrams drive the same unified dagre pipeline as flowchart/er,
//! but their `requirementBox` shape sizes each label with `calculateTextWidth`
//! (Blink `getBBox()` ink extents over sans-serif/Arial — the same font-metric
//! wall that blocks byte-exact C4) and draws the box with randomized roughjs
//! strokes. Reproducing those pixel values is out of scope, so this renderer
//! reuses the dagre layout + edge routing + markers unchanged and renders each
//! requirement/element as a plain multi-line `squareRect` node. The result is a
//! clean, stable, structurally faithful diagram but is *not* byte-identical to
//! mmdc — an opt-in approximation, the same stance as mindmap/architecture.

#![allow(clippy::needless_continue, clippy::assigning_clones)]

use std::cell::RefCell;
use std::rc::Rc;

use crate::render::data::{LayoutData, RenderEdge, RenderNode};

/// A parse error for requirementDiagram source.
#[derive(Debug)]
pub struct RequirementParseError {
    pub message: String,
}

impl std::fmt::Display for RequirementParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "requirement diagram parse error: {}", self.message)
    }
}

impl std::error::Error for RequirementParseError {}

#[derive(Debug, Clone)]
struct Requirement {
    name: String,
    ty: String,
    id: String,
    text: String,
    risk: String,
    verify: String,
}

#[derive(Debug, Clone)]
struct ElementDef {
    name: String,
    ty: String,
    doc_ref: String,
}

#[derive(Debug, Clone)]
struct Relation {
    ty: String,
    src: String,
    dst: String,
    contains: bool,
}

#[derive(Default)]
struct ReqDb {
    requirements: indexmap::IndexMap<String, Requirement>,
    elements: indexmap::IndexMap<String, ElementDef>,
    relations: Vec<Relation>,
    direction: String,
}

/// Maps a requirementType keyword to its display name.
fn req_type_name(kw: &str) -> Option<&'static str> {
    Some(match kw {
        "requirement" => "Requirement",
        "functionalRequirement" => "Functional Requirement",
        "interfaceRequirement" => "Interface Requirement",
        "performanceRequirement" => "Performance Requirement",
        "physicalRequirement" => "Physical Requirement",
        "designConstraint" => "Design Constraint",
        _ => return None,
    })
}

/// Maps a risk keyword to its display name (unknown values pass through).
fn risk_name(kw: &str) -> &str {
    match kw.to_ascii_lowercase().as_str() {
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        _ => kw,
    }
}

/// Maps a verifyMethod keyword to its display name.
fn verify_name(kw: &str) -> &str {
    match kw.to_ascii_lowercase().as_str() {
        "analysis" => "Analysis",
        "demonstration" => "Demonstration",
        "inspection" => "Inspection",
        "test" => "Test",
        _ => kw,
    }
}

fn strip_quotes(s: &str) -> String {
    s.trim().trim_matches('"').trim().to_owned()
}

/// Parses a `src <rel> dst` relationship line in either arrow direction.
/// `A - satisfies -> B` (A satisfies B) or `A <- satisfies - B` (B satisfies A).
fn parse_relation(line: &str) -> Option<Relation> {
    let contains_word = |w: &str| {
        matches!(
            w,
            "contains" | "copies" | "derives" | "satisfies" | "verifies" | "refines" | "traces"
        )
    };
    if let Some(arrow) = line.find("->") {
        // A - rel -> B
        let (left, right) = (&line[..arrow], &line[arrow + 2..]);
        let dash = left.rfind('-')?;
        let src = left[..dash].trim();
        let rel = left[dash + 1..].trim();
        let dst = right.trim();
        if !contains_word(rel) || src.is_empty() || dst.is_empty() {
            return None;
        }
        return Some(Relation {
            ty: rel.to_owned(),
            src: strip_quotes(src),
            dst: strip_quotes(dst),
            contains: rel == "contains",
        });
    }
    if let Some(arrow) = line.find("<-") {
        // A <- rel - B  ==>  B rel A
        let (left, right) = (&line[..arrow], &line[arrow + 2..]);
        let dash = right.find('-')?;
        let rel = right[..dash].trim();
        let dst = left.trim(); // A is the destination
        let src = right[dash + 1..].trim(); // B is the source
        if !contains_word(rel) || src.is_empty() || dst.is_empty() {
            return None;
        }
        return Some(Relation {
            ty: rel.to_owned(),
            src: strip_quotes(src),
            dst: strip_quotes(dst),
            contains: rel == "contains",
        });
    }
    None
}

fn parse(source: &str) -> Result<ReqDb, RequirementParseError> {
    let mut db = ReqDb {
        direction: "TB".to_owned(),
        ..ReqDb::default()
    };
    let mut found = false;
    let mut lines = source.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") || line.starts_with('#') {
            continue;
        }
        if !found {
            if line.starts_with("requirementDiagram") {
                found = true;
            } else {
                return Err(RequirementParseError {
                    message: format!("expected requirementDiagram header, got {line:?}"),
                });
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("direction ") {
            db.direction = rest.trim().to_owned();
            continue;
        }
        if line.starts_with("title ")
            || line.starts_with("accTitle")
            || line.starts_with("accDescr")
        {
            continue;
        }
        // Requirement / element block: `<kw> name {`.
        if let Some(head) = line.strip_suffix('{') {
            let mut parts = head.trim().splitn(2, char::is_whitespace);
            let kw = parts.next().unwrap_or("").trim();
            let name = strip_quotes(parts.next().unwrap_or("").trim());
            if kw == "element" {
                let mut el = ElementDef {
                    name: name.clone(),
                    ty: String::new(),
                    doc_ref: String::new(),
                };
                consume_body(&mut lines, |k, v| match k {
                    "type" => el.ty = strip_quotes(v),
                    "docref" | "docRef" => el.doc_ref = strip_quotes(v),
                    _ => {}
                });
                db.elements.entry(name).or_insert(el);
                continue;
            }
            if let Some(tyname) = req_type_name(kw) {
                let mut req = Requirement {
                    name: name.clone(),
                    ty: tyname.to_owned(),
                    id: String::new(),
                    text: String::new(),
                    risk: String::new(),
                    verify: String::new(),
                };
                consume_body(&mut lines, |k, v| match k {
                    "id" => req.id = strip_quotes(v),
                    "text" => req.text = strip_quotes(v),
                    "risk" => req.risk = risk_name(v).to_owned(),
                    "verifymethod" => req.verify = verify_name(v).to_owned(),
                    _ => {}
                });
                db.requirements.entry(name).or_insert(req);
                continue;
            }
            return Err(RequirementParseError {
                message: format!("unknown requirement statement: {line}"),
            });
        }
        if let Some(rel) = parse_relation(line) {
            db.relations.push(rel);
            continue;
        }
        // Ignore style/class/classDef and anything else (approximate).
    }
    if !found {
        return Err(RequirementParseError {
            message: "missing requirementDiagram header".to_owned(),
        });
    }
    Ok(db)
}

/// Consumes body lines up to the closing `}`, invoking `f(key_lowercased, value)`
/// for each `key: value` line.
fn consume_body<'a, I, F>(lines: &mut std::iter::Peekable<I>, mut f: F)
where
    I: Iterator<Item = &'a str>,
    F: FnMut(&str, &str),
{
    for raw in lines.by_ref() {
        let l = raw.trim();
        if l == "}" {
            break;
        }
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(colon) = l.find(':') {
            let key = l[..colon].trim().to_ascii_lowercase();
            let val = l[colon + 1..].trim();
            f(&key, val);
        }
    }
}

/// Builds the stacked multi-line label for a requirement box.
fn requirement_label(r: &Requirement) -> String {
    let mut lines = vec![format!("<<{}>>", r.ty), r.name.clone()];
    if !r.id.is_empty() {
        lines.push(format!("ID: {}", r.id));
    }
    if !r.text.is_empty() {
        lines.push(format!("Text: {}", r.text));
    }
    if !r.risk.is_empty() {
        lines.push(format!("Risk: {}", r.risk));
    }
    if !r.verify.is_empty() {
        lines.push(format!("Verification: {}", r.verify));
    }
    lines.join("\n")
}

/// Builds the stacked multi-line label for an element box.
fn element_label(e: &ElementDef) -> String {
    let mut lines = vec!["<<Element>>".to_owned(), e.name.clone()];
    if !e.ty.is_empty() {
        lines.push(format!("Type: {}", e.ty));
    }
    if !e.doc_ref.is_empty() {
        lines.push(format!("Doc Ref: {}", e.doc_ref));
    }
    lines.join("\n")
}

/// Parses requirementDiagram source into layout data (approximate).
///
/// # Errors
///
/// Returns [`RequirementParseError`] for unparsable source.
pub fn get_layout_data(source: &str, id: &str) -> Result<LayoutData, RequirementParseError> {
    let db = parse(source)?;
    let mut nodes: Vec<Rc<RefCell<RenderNode>>> = Vec::new();
    let mut edges: Vec<Rc<RefCell<RenderEdge>>> = Vec::new();

    for req in db.requirements.values() {
        let label = requirement_label(req);
        nodes.push(Rc::new(RefCell::new(RenderNode {
            id: req.name.clone(),
            label: label.clone(),
            label_raw: label,
            label_type: "text".to_owned(),
            shape: "squareRect".to_owned(),
            dom_id: req.name.clone(),
            css_classes: "requirementBox default".to_owned(),
            look: "classic".to_owned(),
            padding: 20.0,
            ..RenderNode::default()
        })));
    }
    for el in db.elements.values() {
        let label = element_label(el);
        nodes.push(Rc::new(RefCell::new(RenderNode {
            id: el.name.clone(),
            label: label.clone(),
            label_raw: label,
            label_type: "text".to_owned(),
            shape: "squareRect".to_owned(),
            dom_id: el.name.clone(),
            css_classes: "requirementBox default".to_owned(),
            look: "classic".to_owned(),
            padding: 20.0,
            ..RenderNode::default()
        })));
    }

    for (count, rel) in db.relations.iter().enumerate() {
        edges.push(Rc::new(RefCell::new(RenderEdge {
            id: format!("{}-{}-{count}", rel.src, rel.dst),
            start: rel.src.clone(),
            end: rel.dst.clone(),
            label: format!("<<{}>>", rel.ty),
            label_raw: format!("<<{}>>", rel.ty),
            label_type: "text".to_owned(),
            labelpos: "c".to_owned(),
            thickness: "normal".to_owned(),
            classes: "relationshipLine".to_owned(),
            // Approximate marker mapping: contains draws a crosshair at the
            // source; every other relationship an open arrowhead at the target.
            arrow_type_start: if rel.contains {
                "arrow_cross".to_owned()
            } else {
                String::new()
            },
            arrow_type_end: if rel.contains {
                String::new()
            } else {
                "arrow_point".to_owned()
            },
            pattern: if rel.contains {
                "solid".to_owned()
            } else {
                "dashed".to_owned()
            },
            style: vec![
                "fill:none".to_owned(),
                if rel.contains {
                    String::new()
                } else {
                    "stroke-dasharray: 10,7".to_owned()
                },
            ],
            look: "classic".to_owned(),
            minlen: 1.0,
            curve: "basis".to_owned(),
            ..RenderEdge::default()
        })));
    }

    Ok(LayoutData {
        nodes,
        edges,
        direction: db.direction,
        diagram_id: id.to_owned(),
    })
}
