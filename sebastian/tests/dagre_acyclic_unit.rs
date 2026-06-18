//! Unit tests for `dagre::acyclic`, which makes the graph acyclic before
//! ranking by reversing a feedback-arc set of edges (marking them `reversed`),
//! and `undo`, which restores the original orientation afterwards.

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::acyclic;
use sebastian::dagre::types::{
    EdgeLabel, GraphLabel, LayoutGraph, NodeLabel, UniqueId, edge_ref, node_ref,
};
use sebastian::graphlib::{Graph, GraphOptions};

fn graph() -> LayoutGraph {
    let mut g: LayoutGraph = Graph::new(GraphOptions {
        multigraph: Some(true),
        compound: Some(true),
        ..Default::default()
    });
    g.set_graph(Rc::new(RefCell::new(GraphLabel::default())));
    g
}

fn node(g: &mut LayoutGraph, id: &str) {
    g.set_node_with(id, node_ref(NodeLabel::default()));
}

fn connect(g: &mut LayoutGraph, v: &str, w: &str) {
    g.set_edge(v, w, edge_ref(EdgeLabel::default()), None);
}

fn count_reversed(g: &LayoutGraph) -> usize {
    g.edges()
        .iter()
        .filter(|e| g.edge_for(e).expect("edge").borrow().reversed)
        .count()
}

#[test]
fn dag_keeps_all_edges_forward() {
    let mut g = graph();
    for id in ["a", "b", "c"] {
        node(&mut g, id);
    }
    connect(&mut g, "a", "b");
    connect(&mut g, "b", "c");

    acyclic::run(&mut g, &UniqueId::new());

    assert_eq!(count_reversed(&g), 0);
    assert_eq!(g.edge_count(), 2);
}

#[test]
fn two_cycle_reverses_exactly_one_edge() {
    let mut g = graph();
    node(&mut g, "a");
    node(&mut g, "b");
    connect(&mut g, "a", "b");
    connect(&mut g, "b", "a");

    acyclic::run(&mut g, &UniqueId::new());

    assert_eq!(count_reversed(&g), 1);
    assert_eq!(g.edge_count(), 2);
}

#[test]
fn three_cycle_reverses_exactly_one_edge() {
    let mut g = graph();
    for id in ["a", "b", "c"] {
        node(&mut g, id);
    }
    connect(&mut g, "a", "b");
    connect(&mut g, "b", "c");
    connect(&mut g, "c", "a");

    acyclic::run(&mut g, &UniqueId::new());

    assert_eq!(count_reversed(&g), 1);
}

#[test]
fn undo_restores_forward_orientation() {
    let mut g = graph();
    node(&mut g, "a");
    node(&mut g, "b");
    connect(&mut g, "a", "b");
    connect(&mut g, "b", "a");

    acyclic::run(&mut g, &UniqueId::new());
    assert_eq!(count_reversed(&g), 1);

    acyclic::undo(&mut g);
    assert_eq!(count_reversed(&g), 0);
    assert_eq!(g.edge_count(), 2);
}
