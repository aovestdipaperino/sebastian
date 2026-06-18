//! Unit tests for the `graphlib` graph API (port of graphlib used by dagre).
//! Exercises the topology accessors — sources/sinks, neighbors, edge queries,
//! node removal cascade, compound parent/children, and DFS orderings — using a
//! label-free `Graph<(), (), ()>`.

use sebastian::graphlib::alg::{postorder, preorder};
use sebastian::graphlib::{Graph, GraphOptions};

type G = Graph<(), (), ()>;

fn graph() -> G {
    Graph::new(GraphOptions {
        multigraph: Some(true),
        compound: Some(true),
        ..Default::default()
    })
}

fn line() -> G {
    // a -> b -> c
    let mut g = graph();
    for v in ["a", "b", "c"] {
        g.set_node(v);
    }
    g.set_edge("a", "b", (), None);
    g.set_edge("b", "c", (), None);
    g
}

#[test]
fn sources_and_sinks() {
    let g = line();
    assert_eq!(g.sources(), vec!["a".to_owned()]);
    assert_eq!(g.sinks(), vec!["c".to_owned()]);
}

#[test]
fn predecessors_successors_neighbors() {
    let g = line();
    assert_eq!(g.predecessors("b"), vec!["a".to_owned()]);
    assert_eq!(g.successors("b"), vec!["c".to_owned()]);
    let mut nb = g.neighbors("b");
    nb.sort();
    assert_eq!(nb, vec!["a".to_owned(), "c".to_owned()]);
}

#[test]
fn has_node_and_has_edge() {
    let g = line();
    assert!(g.has_node("a"));
    assert!(!g.has_node("z"));
    assert!(g.has_edge("a", "b", None));
    assert!(!g.has_edge("a", "c", None));
}

#[test]
fn counts_reflect_structure() {
    let g = line();
    assert_eq!(g.node_count(), 3);
    assert_eq!(g.edge_count(), 2);
}

#[test]
fn remove_node_removes_incident_edges() {
    let mut g = line();
    g.remove_node("b");
    assert_eq!(g.node_count(), 2);
    // Both edges touched b, so both are gone.
    assert_eq!(g.edge_count(), 0);
    assert!(g.has_node("a"));
    assert!(g.has_node("c"));
}

#[test]
fn in_out_and_node_edges() {
    let g = line();
    assert_eq!(g.in_edges("b", None).len(), 1);
    assert_eq!(g.out_edges("b", None).len(), 1);
    assert_eq!(g.node_edges("b", None).len(), 2);
    assert_eq!(g.in_edges("a", None).len(), 0);
    assert_eq!(g.out_edges("c", None).len(), 0);
}

#[test]
fn remove_edge_drops_only_that_edge() {
    let mut g = line();
    g.remove_edge("a", "b", None);
    assert_eq!(g.edge_count(), 1);
    assert!(!g.has_edge("a", "b", None));
    assert!(g.has_edge("b", "c", None));
}

#[test]
fn compound_parent_and_children() {
    let mut g = graph();
    g.set_node("p");
    g.set_node("a");
    g.set_node("b");
    g.set_parent("a", Some("p"));
    g.set_parent("b", Some("p"));

    assert_eq!(g.parent("a"), Some("p".to_owned()));
    let mut kids = g.children(Some("p"));
    kids.sort();
    assert_eq!(kids, vec!["a".to_owned(), "b".to_owned()]);
    // A leaf has no children.
    assert!(g.children(Some("a")).is_empty());
}

#[test]
fn set_parent_to_none_detaches() {
    let mut g = graph();
    g.set_node("p");
    g.set_node("a");
    g.set_parent("a", Some("p"));
    g.set_parent("a", None);
    assert_eq!(g.parent("a"), None);
}

#[test]
fn preorder_starts_at_root_postorder_ends_at_root() {
    let mut g = graph();
    for v in ["a", "b", "c"] {
        g.set_node(v);
    }
    g.set_edge("a", "b", (), None);
    g.set_edge("a", "c", (), None);

    let pre = preorder(&g, &["a".to_owned()]);
    let post = postorder(&g, &["a".to_owned()]);

    assert_eq!(pre.first(), Some(&"a".to_owned()));
    assert_eq!(post.last(), Some(&"a".to_owned()));
    assert_eq!(pre.len(), 3);
    assert_eq!(post.len(), 3);
}
