//! Unit tests for the stateDiagram-v2 parser (`state::parse`): transitions,
//! classDef and apply-class statements.

use sebastian::state::Stmt;
use sebastian::state::parse;

fn count_relations(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .filter(|s| matches!(s, Stmt::Relation { .. }))
        .count()
}

#[test]
fn parses_start_and_end_transitions() {
    let stmts = parse("stateDiagram-v2\n[*] --> A\nA --> [*]\n").expect("parse");
    assert_eq!(count_relations(&stmts), 2);
}

#[test]
fn parses_transition_chain() {
    let stmts = parse("stateDiagram-v2\n[*] --> A\nA --> B\nB --> [*]\n").expect("parse");
    assert_eq!(count_relations(&stmts), 3);
}

#[test]
fn parses_transition_description() {
    let stmts = parse("stateDiagram-v2\nA --> B: go\n").expect("parse");
    let desc = stmts.iter().find_map(|s| match s {
        Stmt::Relation { description, .. } => Some(description.clone()),
        _ => None,
    });
    assert_eq!(desc, Some(Some("go".to_owned())));
}

#[test]
fn parses_classdef_statement() {
    let stmts = parse("stateDiagram-v2\nclassDef hot fill:red\nA --> B\n").expect("parse");
    let has_classdef = stmts.iter().any(|s| matches!(s, Stmt::ClassDef { .. }));
    assert!(has_classdef);
}
