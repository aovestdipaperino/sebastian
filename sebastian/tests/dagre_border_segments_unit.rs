//! Unit tests for `dagre::add_border_segments`, which adds left/right border
//! dummy nodes for each rank a cluster (a node with `min_rank/max_rank`) spans.

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::add_border_segments::add_border_segments;
use sebastian::dagre::types::{GraphLabel, LayoutGraph, NodeLabel, UniqueId, node_ref};
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
fn cluster_gets_border_nodes_for_each_spanned_rank() {
    let mut g = graph();
    // Cluster `p` spanning ranks 0..=2, with two children.
    g.set_node_with(
        "p",
        node_ref(NodeLabel {
            min_rank: Some(0.0),
            max_rank: Some(2.0),
            ..NodeLabel::default()
        }),
    );
    g.set_node_with("a", node_ref(NodeLabel::default()));
    g.set_node_with("b", node_ref(NodeLabel::default()));
    g.set_parent("a", Some("p"));
    g.set_parent("b", Some("p"));

    let before = g.node_count();
    add_border_segments(&mut g, &UniqueId::new());

    let n = g.node("p").expect("cluster");
    let n = n.borrow();
    // One left + one right border node per rank in 0..=2 (3 ranks).
    assert_eq!(n.border_left.len(), 3);
    assert_eq!(n.border_right.len(), 3);
    // 3 left + 3 right border dummies were added to the graph.
    assert_eq!(g.node_count(), before + 6);
}

#[test]
fn non_cluster_nodes_get_no_border_segments() {
    let mut g = graph();
    g.set_node_with("a", node_ref(NodeLabel::default()));
    g.set_node_with("b", node_ref(NodeLabel::default()));

    let before = g.node_count();
    add_border_segments(&mut g, &UniqueId::new());

    // No node has min_rank, so nothing is added.
    assert_eq!(g.node_count(), before);
    assert!(g.node("a").expect("a").borrow().border_left.is_empty());
}

#[test]
fn single_rank_cluster_gets_one_border_pair() {
    let mut g = graph();
    g.set_node_with(
        "p",
        node_ref(NodeLabel {
            min_rank: Some(1.0),
            max_rank: Some(1.0),
            ..NodeLabel::default()
        }),
    );
    g.set_node_with("a", node_ref(NodeLabel::default()));
    g.set_parent("a", Some("p"));

    add_border_segments(&mut g, &UniqueId::new());

    let n = g.node("p").expect("cluster");
    let n = n.borrow();
    assert_eq!(n.border_left.len(), 1);
    assert_eq!(n.border_right.len(), 1);
}
