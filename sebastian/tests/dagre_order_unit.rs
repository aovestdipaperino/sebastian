//! Unit tests for `dagre::order`, the crossing-minimization pass that assigns
//! an `order` (position within rank) to every node. Deep in the layout
//! pipeline, so otherwise only covered end-to-end.

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::order::order;
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

fn ranked(g: &mut LayoutGraph, id: &str, rank: f64) {
    g.set_node_with(
        id,
        node_ref(NodeLabel {
            rank: Some(rank),
            width: 10.0,
            height: 10.0,
            ..NodeLabel::default()
        }),
    );
}

fn connect(g: &mut LayoutGraph, v: &str, w: &str) {
    g.set_edge(v, w, edge_ref(EdgeLabel::default()), None);
}

fn order_of(g: &LayoutGraph, id: &str) -> f64 {
    g.node(id).expect("node").borrow().order.expect("order")
}

#[test]
fn order_assigns_an_order_to_every_node() {
    let mut g = graph();
    ranked(&mut g, "a", 0.0);
    ranked(&mut g, "b", 1.0);
    ranked(&mut g, "c", 1.0);
    ranked(&mut g, "d", 2.0);
    connect(&mut g, "a", "b");
    connect(&mut g, "a", "c");
    connect(&mut g, "b", "d");
    connect(&mut g, "c", "d");

    order(&g, &UniqueId::new());

    for v in g.nodes() {
        assert!(
            g.node(&v).expect("node").borrow().order.is_some(),
            "{v} has no order"
        );
    }
}

#[test]
fn single_node_per_rank_orders_are_zero() {
    let mut g = graph();
    ranked(&mut g, "a", 0.0);
    ranked(&mut g, "b", 1.0);
    ranked(&mut g, "c", 2.0);
    connect(&mut g, "a", "b");
    connect(&mut g, "b", "c");

    order(&g, &UniqueId::new());

    assert_eq!(order_of(&g, "a"), 0.0);
    assert_eq!(order_of(&g, "b"), 0.0);
    assert_eq!(order_of(&g, "c"), 0.0);
}

#[test]
fn two_nodes_in_a_rank_get_distinct_orders() {
    let mut g = graph();
    ranked(&mut g, "root", 0.0);
    ranked(&mut g, "x", 1.0);
    ranked(&mut g, "y", 1.0);
    connect(&mut g, "root", "x");
    connect(&mut g, "root", "y");

    order(&g, &UniqueId::new());

    let ox = order_of(&g, "x");
    let oy = order_of(&g, "y");
    assert_ne!(ox, oy);
    // Two nodes in one rank occupy positions 0 and 1.
    let mut both = [ox, oy];
    both.sort_by(f64::total_cmp);
    assert_eq!(both, [0.0, 1.0]);
}
