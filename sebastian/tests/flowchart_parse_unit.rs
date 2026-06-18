//! Unit tests for the flowchart front-end: `parse` (source -> FlowDb) and the
//! pure text-normalization helpers it relies on. The parser is otherwise only
//! exercised end-to-end by the rendering corpus.

use sebastian::flowchart::db::{decode_html_entities, newlines_to_br, strip_html_tags};
use sebastian::flowchart::parser::parse;

#[test]
fn parses_a_simple_edge() {
    let db = parse("graph TD\nA-->B\n").expect("parse");
    assert!(db.vertices.contains_key("A"));
    assert!(db.vertices.contains_key("B"));
    assert_eq!(db.edges.len(), 1);
    assert_eq!(db.edges[0].start, "A");
    assert_eq!(db.edges[0].end, "B");
}

#[test]
fn parses_node_label_text() {
    let db = parse("graph LR\nA[Start]\n").expect("parse");
    assert_eq!(db.vertices["A"].text.as_deref(), Some("Start"));
}

#[test]
fn parses_edge_label() {
    let db = parse("graph TD\nA-->|yes|B\n").expect("parse");
    assert_eq!(db.edges.len(), 1);
    assert_eq!(db.edges[0].text, "yes");
}

#[test]
fn parses_a_chain_of_edges() {
    let db = parse("graph TD\nA-->B\nB-->C\n").expect("parse");
    assert_eq!(db.vertices.len(), 3);
    assert_eq!(db.edges.len(), 2);
}

#[test]
fn parses_classdef_and_class_assignment() {
    let db = parse("graph TD\nclassDef big fill:#f00\nA:::big\n").expect("parse");
    assert!(db.classes.contains_key("big"));
    assert!(db.vertices["A"].classes.iter().any(|c| c == "big"));
}

#[test]
fn newlines_to_br_replaces_literal_and_real_newlines() {
    assert_eq!(newlines_to_br("a\nb"), "a<br />b");
    assert_eq!(newlines_to_br("a\\nb"), "a<br />b");
}

#[test]
fn strip_html_tags_unwraps_non_br_tags() {
    assert_eq!(strip_html_tags("<b>bold</b>"), "bold");
    assert_eq!(strip_html_tags("a<span>b</span>c"), "abc");
}

#[test]
fn strip_html_tags_keeps_br() {
    assert_eq!(strip_html_tags("keep<br/>this"), "keep<br/>this");
}

#[test]
fn decode_html_entities_named() {
    assert_eq!(decode_html_entities("&amp;"), "&");
    assert_eq!(decode_html_entities("&lt;x&gt;"), "<x>");
}

#[test]
fn decode_html_entities_hex_numeric() {
    assert_eq!(decode_html_entities("&#x41;"), "A");
}
