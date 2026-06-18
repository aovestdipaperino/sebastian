//! Unit tests for the rank-manipulation helpers in `dagre::util` that the
//! layout pipeline relies on: finding the max rank, normalizing ranks so the
//! minimum is zero, and projecting ranked/ordered nodes into a layer matrix.

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::types::{GraphLabel, LayoutGraph, NodeLabel, node_ref};
use sebastian::dagre::util::{build_layer_matrix, max_rank, normalize_ranks};
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

fn add(g: &mut LayoutGraph, id: &str, rank: f64, order: f64) {
    g.set_node_with(
        id,
        node_ref(NodeLabel {
            rank: Some(rank),
            order: Some(order),
            ..NodeLabel::default()
        }),
    );
}

fn rank_of(g: &LayoutGraph, id: &str) -> Option<f64> {
    g.node(id).expect("node").borrow().rank
}

#[test]
fn max_rank_is_none_for_unranked_graph() {
    assert_eq!(max_rank(&graph()), None);
}

#[test]
fn max_rank_returns_the_highest_rank() {
    let mut g = graph();
    add(&mut g, "a", 0.0, 0.0);
    add(&mut g, "b", 3.0, 0.0);
    add(&mut g, "c", 1.0, 1.0);
    assert_eq!(max_rank(&g), Some(3.0));
}

#[test]
fn normalize_ranks_shifts_minimum_to_zero() {
    let mut g = graph();
    add(&mut g, "a", 2.0, 0.0);
    add(&mut g, "b", 3.0, 0.0);
    add(&mut g, "c", 5.0, 1.0);
    normalize_ranks(&g);
    assert_eq!(rank_of(&g, "a"), Some(0.0));
    assert_eq!(rank_of(&g, "b"), Some(1.0));
    assert_eq!(rank_of(&g, "c"), Some(3.0));
}

#[test]
fn normalize_ranks_handles_negative_minimum() {
    let mut g = graph();
    add(&mut g, "a", -2.0, 0.0);
    add(&mut g, "b", 1.0, 0.0);
    normalize_ranks(&g);
    assert_eq!(rank_of(&g, "a"), Some(0.0));
    assert_eq!(rank_of(&g, "b"), Some(3.0));
}

#[test]
fn normalize_ranks_is_a_noop_on_unranked_graph() {
    let g = graph();
    normalize_ranks(&g); // must not panic when there is no minimum
    assert_eq!(max_rank(&g), None);
}

#[test]
fn build_layer_matrix_groups_nodes_by_rank() {
    let mut g = graph();
    add(&mut g, "a", 0.0, 0.0);
    add(&mut g, "b", 0.0, 1.0);
    add(&mut g, "c", 1.0, 0.0);
    let m = build_layer_matrix(&g);
    assert_eq!(m.len(), 2);
    assert_eq!(m[0], vec!["a".to_string(), "b".to_string()]);
    assert_eq!(m[1], vec!["c".to_string()]);
}

#[test]
fn build_layer_matrix_orders_within_a_rank_by_order() {
    let mut g = graph();
    // Insert out of order; the matrix must still be ordered by `order`.
    add(&mut g, "third", 0.0, 2.0);
    add(&mut g, "first", 0.0, 0.0);
    add(&mut g, "second", 0.0, 1.0);
    let m = build_layer_matrix(&g);
    assert_eq!(
        m[0],
        vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string()
        ]
    );
}
