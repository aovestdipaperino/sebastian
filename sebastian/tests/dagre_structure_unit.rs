//! Unit tests for dummy/border node creation (`dagre::util`) and the nesting
//! graph entry point (`dagre::nesting_graph`). These run deep inside layout and
//! are otherwise only exercised end-to-end by the SVG corpus.

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::nesting_graph;
use sebastian::dagre::types::{
    Dummy, EdgeLabel, GraphLabel, LayoutGraph, NodeLabel, UniqueId, edge_ref, node_ref,
};
use sebastian::dagre::util::{add_border_node, add_dummy_node};
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

#[test]
fn add_dummy_node_creates_prefixed_node_with_type() {
    let mut g = graph();
    let ids = UniqueId::new();
    let id = add_dummy_node(
        &mut g,
        Dummy::Edge,
        node_ref(NodeLabel::default()),
        "_d",
        &ids,
    );

    assert_eq!(id, "_d1");
    assert!(g.has_node("_d1"));
    assert_eq!(
        g.node("_d1").expect("node").borrow().dummy,
        Some(Dummy::Edge)
    );
    assert_eq!(g.node_count(), 1);
}

#[test]
fn add_dummy_node_uses_fresh_ids() {
    let mut g = graph();
    let ids = UniqueId::new();
    let a = add_dummy_node(
        &mut g,
        Dummy::Edge,
        node_ref(NodeLabel::default()),
        "_d",
        &ids,
    );
    let b = add_dummy_node(
        &mut g,
        Dummy::Edge,
        node_ref(NodeLabel::default()),
        "_d",
        &ids,
    );
    assert_ne!(a, b);
    assert_eq!(g.node_count(), 2);
}

#[test]
fn add_border_node_sets_rank_order_and_type() {
    let mut g = graph();
    let ids = UniqueId::new();
    let id = add_border_node(&mut g, "_bl", Some((2.0, 3.0)), &ids);

    let n = g.node(&id).expect("border node");
    let n = n.borrow();
    assert_eq!(n.rank, Some(2.0));
    assert_eq!(n.order, Some(3.0));
    assert_eq!(n.dummy, Some(Dummy::Border));
    assert_eq!(n.width, 0.0);
    assert_eq!(n.height, 0.0);
}

#[test]
fn add_border_node_without_rank_order() {
    let mut g = graph();
    let ids = UniqueId::new();
    let id = add_border_node(&mut g, "_b", None, &ids);
    let n = g.node(&id).expect("border node");
    let n = n.borrow();
    assert_eq!(n.rank, None);
    assert_eq!(n.order, None);
    assert_eq!(n.dummy, Some(Dummy::Border));
}

#[test]
fn nesting_graph_run_sets_root_and_rank_factor() {
    let mut g = graph();
    for v in ["p", "a", "b"] {
        g.set_node_with(v, node_ref(NodeLabel::default()));
    }
    g.set_parent("a", Some("p"));
    g.set_parent("b", Some("p"));
    g.set_edge("a", "b", edge_ref(EdgeLabel::default()), None);

    let before = g.node_count();
    nesting_graph::run(&mut g, &UniqueId::new());

    // A nesting root is created and recorded, plus border structure.
    assert!(g.graph().borrow().nesting_root.is_some());
    assert!(g.graph().borrow().node_rank_factor > 0.0);
    assert!(g.node_count() > before);
}

#[test]
fn nesting_graph_cleanup_clears_root() {
    let mut g = graph();
    for v in ["p", "a"] {
        g.set_node_with(v, node_ref(NodeLabel::default()));
    }
    g.set_parent("a", Some("p"));

    let ids = UniqueId::new();
    nesting_graph::run(&mut g, &ids);
    assert!(g.graph().borrow().nesting_root.is_some());

    nesting_graph::cleanup(&mut g);
    assert!(g.graph().borrow().nesting_root.is_none());
}
