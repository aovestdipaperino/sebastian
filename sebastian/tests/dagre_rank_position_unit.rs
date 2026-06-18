//! Unit tests for the ranking pass (`dagre::rank`) and the positioning pass
//! (`dagre::position`). Ranking assigns each node a layer respecting edge
//! minlen; positioning assigns x (Brandes-Köpf) and y (rank-based) coordinates.

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::order::order;
use sebastian::dagre::position::position;
use sebastian::dagre::rank::rank;
use sebastian::dagre::types::{
    EdgeLabel, GraphLabel, LayoutGraph, NodeLabel, UniqueId, edge_ref, node_ref,
};
use sebastian::dagre::util::normalize_ranks;
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
    g.set_node_with(
        id,
        node_ref(NodeLabel {
            width: 10.0,
            height: 10.0,
            ..NodeLabel::default()
        }),
    );
}

fn connect(g: &mut LayoutGraph, v: &str, w: &str) {
    g.set_edge(v, w, edge_ref(EdgeLabel::default()), None);
}

fn rank_of(g: &LayoutGraph, id: &str) -> f64 {
    g.node(id).expect("node").borrow().rank.expect("rank")
}

#[test]
fn rank_assigns_a_rank_to_every_node() {
    let mut g = graph();
    for v in ["a", "b", "c"] {
        node(&mut g, v);
    }
    connect(&mut g, "a", "b");
    connect(&mut g, "b", "c");

    rank(&g);

    for v in g.nodes() {
        assert!(g.node(&v).expect("node").borrow().rank.is_some(), "{v}");
    }
}

#[test]
fn rank_respects_edge_minlen() {
    let mut g = graph();
    node(&mut g, "a");
    node(&mut g, "b");
    connect(&mut g, "a", "b");

    rank(&g);

    // Default minlen is 1, so b is at least one rank below a.
    assert!(rank_of(&g, "b") - rank_of(&g, "a") >= 1.0);
}

#[test]
fn rank_diamond_spans_two_ranks() {
    let mut g = graph();
    for v in ["a", "b", "c", "d"] {
        node(&mut g, v);
    }
    connect(&mut g, "a", "b");
    connect(&mut g, "a", "c");
    connect(&mut g, "b", "d");
    connect(&mut g, "c", "d");

    rank(&g);

    assert!(rank_of(&g, "d") - rank_of(&g, "a") >= 2.0);
}

#[test]
fn position_assigns_x_and_y_to_every_node() {
    let mut g = graph();
    for v in ["a", "b", "c", "d"] {
        node(&mut g, v);
    }
    connect(&mut g, "a", "b");
    connect(&mut g, "a", "c");
    connect(&mut g, "b", "d");
    connect(&mut g, "c", "d");

    rank(&g);
    normalize_ranks(&g);
    order(&g, &UniqueId::new());
    position(&g);

    for v in g.nodes() {
        let n = g.node(&v).expect("node");
        let n = n.borrow();
        assert!(n.x.is_some(), "{v} has no x");
        assert!(n.y.is_some(), "{v} has no y");
    }
}

#[test]
fn position_y_increases_with_rank() {
    let mut g = graph();
    node(&mut g, "a");
    node(&mut g, "b");
    connect(&mut g, "a", "b");

    rank(&g);
    normalize_ranks(&g);
    order(&g, &UniqueId::new());
    position(&g);

    let ya = g.node("a").expect("a").borrow().y.expect("y");
    let yb = g.node("b").expect("b").borrow().y.expect("y");
    // b is ranked below a, so it gets a larger y.
    assert!(yb > ya);
}
