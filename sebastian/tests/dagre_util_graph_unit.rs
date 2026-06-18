//! Unit tests for the graph-transform helpers in `dagre::util`:
//! `simplify` (collapse a multigraph's parallel edges, summing weight and
//! taking the max minlen) and `as_non_compound_graph` (drop compound parents,
//! keeping only leaf nodes and edges).

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::types::{EdgeLabel, GraphLabel, LayoutGraph, NodeLabel, edge_ref, node_ref};
use sebastian::dagre::util::{as_non_compound_graph, simplify};
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

fn edge(g: &mut LayoutGraph, v: &str, w: &str, weight: f64, minlen: f64, name: &str) {
    g.set_edge(
        v,
        w,
        edge_ref(EdgeLabel::rank_label(weight, minlen)),
        Some(name),
    );
}

#[test]
fn simplify_merges_parallel_edges_summing_weight() {
    let mut g = graph();
    node(&mut g, "a");
    node(&mut g, "b");
    edge(&mut g, "a", "b", 1.0, 1.0, "e1");
    edge(&mut g, "a", "b", 1.0, 2.0, "e2");

    let s = simplify(&g);

    assert_eq!(s.edge_count(), 1);
    let l = s.edge("a", "b", None).expect("merged edge");
    assert_eq!(l.borrow().weight, 2.0); // 1 + 1
    assert_eq!(l.borrow().minlen, 2.0); // max(1, 2)
}

#[test]
fn simplify_preserves_a_single_edge() {
    let mut g = graph();
    node(&mut g, "a");
    node(&mut g, "b");
    edge(&mut g, "a", "b", 3.0, 1.0, "only");

    let s = simplify(&g);

    assert_eq!(s.edge_count(), 1);
    assert_eq!(s.node_count(), 2);
    assert_eq!(s.edge("a", "b", None).expect("edge").borrow().weight, 3.0);
}

#[test]
fn as_non_compound_graph_drops_parent_nodes() {
    let mut g = graph();
    node(&mut g, "p");
    node(&mut g, "a");
    node(&mut g, "b");
    g.set_parent("a", Some("p"));
    g.set_parent("b", Some("p"));
    edge(&mut g, "a", "b", 1.0, 1.0, "e");

    let nc = as_non_compound_graph(&g);
    let nodes = nc.nodes();

    assert!(nodes.contains(&"a".to_owned()));
    assert!(nodes.contains(&"b".to_owned()));
    // The compound parent has children, so it is excluded.
    assert!(!nodes.contains(&"p".to_owned()));
    assert_eq!(nc.edge_count(), 1);
}

#[test]
fn as_non_compound_graph_keeps_flat_graphs_intact() {
    let mut g = graph();
    node(&mut g, "a");
    node(&mut g, "b");
    edge(&mut g, "a", "b", 1.0, 1.0, "e");

    let nc = as_non_compound_graph(&g);

    assert_eq!(nc.node_count(), 2);
    assert_eq!(nc.edge_count(), 1);
}
