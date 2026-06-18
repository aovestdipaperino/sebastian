//! Unit tests for `dagre::normalize`, which splits edges that span more than
//! one rank into unit-length segments joined by dummy nodes (and records the
//! chain head in `dummy_chains`). Adjacent-rank edges are left untouched.

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::normalize;
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

fn add_ranked(g: &mut LayoutGraph, id: &str, rank: f64) {
    g.set_node_with(
        id,
        node_ref(NodeLabel {
            rank: Some(rank),
            ..NodeLabel::default()
        }),
    );
}

fn connect(g: &mut LayoutGraph, v: &str, w: &str) {
    g.set_edge(v, w, edge_ref(EdgeLabel::default()), None);
}

#[test]
fn adjacent_rank_edge_is_not_split() {
    let mut g = graph();
    add_ranked(&mut g, "a", 0.0);
    add_ranked(&mut g, "b", 1.0);
    connect(&mut g, "a", "b");

    normalize::run(&mut g, &UniqueId::new());

    assert_eq!(g.node_count(), 2);
    assert!(g.graph().borrow().dummy_chains.is_empty());
}

#[test]
fn two_rank_edge_inserts_one_dummy() {
    let mut g = graph();
    add_ranked(&mut g, "a", 0.0);
    add_ranked(&mut g, "b", 2.0);
    connect(&mut g, "a", "b");

    normalize::run(&mut g, &UniqueId::new());

    // One dummy at the intermediate rank; chain head recorded once.
    assert_eq!(g.node_count(), 3);
    assert_eq!(g.graph().borrow().dummy_chains.len(), 1);
}

#[test]
fn three_rank_edge_inserts_two_dummies() {
    let mut g = graph();
    add_ranked(&mut g, "a", 0.0);
    add_ranked(&mut g, "b", 3.0);
    connect(&mut g, "a", "b");

    normalize::run(&mut g, &UniqueId::new());

    assert_eq!(g.node_count(), 4);
    // Still a single chain head for the one split edge.
    assert_eq!(g.graph().borrow().dummy_chains.len(), 1);
}

#[test]
fn run_clears_stale_dummy_chains() {
    let mut g = graph();
    g.graph().borrow_mut().dummy_chains.push("stale".to_owned());
    add_ranked(&mut g, "a", 0.0);
    add_ranked(&mut g, "b", 1.0);
    connect(&mut g, "a", "b");

    normalize::run(&mut g, &UniqueId::new());

    // The pre-existing entry must be cleared; this edge adds none.
    assert!(g.graph().borrow().dummy_chains.is_empty());
}
