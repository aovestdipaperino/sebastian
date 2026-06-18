//! Unit tests for `dagre::coordinate_system`, which rotates/reflects the graph
//! to implement non-top-down rank directions. `adjust` is applied before
//! layout (swap width/height for horizontal layouts) and `undo` after (swap
//! x/y back and reflect for bt/rl). These transforms are pure and must be
//! exact for layout parity.

use std::cell::RefCell;
use std::rc::Rc;

use sebastian::dagre::coordinate_system::{adjust, undo};
use sebastian::dagre::types::{GraphLabel, LayoutGraph, NodeLabel, node_ref};
use sebastian::graphlib::{Graph, GraphOptions};

fn graph_with_dir(dir: &str) -> LayoutGraph {
    let mut g: LayoutGraph = Graph::new(GraphOptions {
        multigraph: Some(true),
        compound: Some(true),
        ..Default::default()
    });
    let label = GraphLabel {
        rankdir: dir.to_owned(),
        ..GraphLabel::default()
    };
    g.set_graph(Rc::new(RefCell::new(label)));
    g
}

fn add_node(g: &mut LayoutGraph, id: &str, w: f64, h: f64, x: f64, y: f64) {
    g.set_node_with(
        id,
        node_ref(NodeLabel {
            width: w,
            height: h,
            x: Some(x),
            y: Some(y),
            ..NodeLabel::default()
        }),
    );
}

fn dims(g: &LayoutGraph, id: &str) -> (f64, f64) {
    let n = g.node(id).expect("node");
    let n = n.borrow();
    (n.width, n.height)
}

fn pos(g: &LayoutGraph, id: &str) -> (f64, f64) {
    let n = g.node(id).expect("node");
    let n = n.borrow();
    (n.x.expect("x"), n.y.expect("y"))
}

#[test]
fn adjust_tb_leaves_dimensions_unchanged() {
    let mut g = graph_with_dir("tb");
    add_node(&mut g, "a", 10.0, 20.0, 1.0, 2.0);
    adjust(&g);
    assert_eq!(dims(&g, "a"), (10.0, 20.0));
}

#[test]
fn adjust_lr_swaps_width_and_height() {
    let mut g = graph_with_dir("lr");
    add_node(&mut g, "a", 10.0, 20.0, 1.0, 2.0);
    adjust(&g);
    assert_eq!(dims(&g, "a"), (20.0, 10.0));
}

#[test]
fn adjust_rl_swaps_width_and_height() {
    let mut g = graph_with_dir("rl");
    add_node(&mut g, "a", 10.0, 20.0, 0.0, 0.0);
    adjust(&g);
    assert_eq!(dims(&g, "a"), (20.0, 10.0));
}

#[test]
fn undo_bt_reflects_y() {
    let mut g = graph_with_dir("bt");
    add_node(&mut g, "a", 10.0, 20.0, 3.0, 7.0);
    undo(&g);
    // bt reflects y but does not swap dimensions or x/y.
    assert_eq!(pos(&g, "a"), (3.0, -7.0));
    assert_eq!(dims(&g, "a"), (10.0, 20.0));
}

#[test]
fn undo_lr_swaps_xy_and_dimensions() {
    let mut g = graph_with_dir("lr");
    add_node(&mut g, "a", 10.0, 20.0, 3.0, 7.0);
    undo(&g);
    assert_eq!(pos(&g, "a"), (7.0, 3.0));
    assert_eq!(dims(&g, "a"), (20.0, 10.0));
}

#[test]
fn adjust_then_undo_lr_restores_dimensions_and_transposes_position() {
    let mut g = graph_with_dir("lr");
    add_node(&mut g, "a", 10.0, 20.0, 3.0, 7.0);
    adjust(&g); // w/h -> (20,10)
    undo(&g); // swaps x/y, swaps w/h back
    assert_eq!(dims(&g, "a"), (10.0, 20.0));
    assert_eq!(pos(&g, "a"), (7.0, 3.0));
}

#[test]
fn tb_adjust_and_undo_are_both_no_ops() {
    let mut g = graph_with_dir("tb");
    add_node(&mut g, "a", 10.0, 20.0, 3.0, 7.0);
    adjust(&g);
    undo(&g);
    assert_eq!(dims(&g, "a"), (10.0, 20.0));
    assert_eq!(pos(&g, "a"), (3.0, 7.0));
}
