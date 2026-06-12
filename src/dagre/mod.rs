//! Port of dagre-d3-es `src/dagre/layout.js` — the layout pipeline.

pub mod acyclic;
pub mod add_border_segments;
pub mod coordinate_system;
pub mod nesting_graph;
pub mod normalize;
pub mod order;
pub mod parent_dummy_chains;
pub mod position;
pub mod rank;
pub mod types;
pub mod util;

use std::cell::RefCell;
use std::rc::Rc;

use crate::graphlib::{Graph, GraphOptions};

use types::{
    Dummy, EdgeLabel, GraphLabel, LayoutGraph, NodeLabel, Point, SelfEdgeRec, UniqueId, edge_ref,
    node_ref,
};

/// Runs the dagre layout, writing results back into the input graph's labels.
pub fn layout(g: &LayoutGraph) {
    let ids = UniqueId::new();
    let mut layout_graph = build_layout_graph(g);
    run_layout(&mut layout_graph, &ids);
    update_input_graph(g, &layout_graph);
}

fn run_layout(g: &mut LayoutGraph, ids: &UniqueId) {
    make_space_for_edge_labels(g);
    remove_self_edges(g);
    acyclic::run(g, ids);
    nesting_graph::run(g, ids);
    rank::rank(&util::as_non_compound_graph(g));
    inject_edge_label_proxies(g, ids);
    util::remove_empty_ranks(g);
    nesting_graph::cleanup(g);
    util::normalize_ranks(g);
    assign_rank_min_max(g);
    remove_edge_label_proxies(g);
    normalize::run(g, ids);
    parent_dummy_chains::parent_dummy_chains(g);
    add_border_segments::add_border_segments(g, ids);
    order::order(g, ids);
    insert_self_edges(g, ids);
    coordinate_system::adjust(g);
    position::position(g);
    position_self_edges(g);
    remove_border_nodes(g);
    normalize::undo(g);
    fixup_edge_label_coords(g);
    coordinate_system::undo(g);
    translate_graph(g);
    assign_node_intersects(g);
    reverse_points_for_reversed_edges(g);
    acyclic::undo(g);
}

/// Copies layout results back to the input graph (whitelisted attrs only).
fn update_input_graph(input_graph: &LayoutGraph, layout_graph: &LayoutGraph) {
    for v in input_graph.nodes() {
        if let Some(input_label) = input_graph.node(&v) {
            let layout_label = layout_graph.node(&v).expect("layout node");
            let layout_label = layout_label.borrow();
            let mut input_label = input_label.borrow_mut();
            input_label.x = layout_label.x;
            input_label.y = layout_label.y;
            if !layout_graph.children(Some(&v)).is_empty() {
                input_label.width = layout_label.width;
                input_label.height = layout_label.height;
            }
        }
    }

    for e in input_graph.edges() {
        let input_label = input_graph.edge_for(&e).expect("input edge");
        let layout_label = layout_graph.edge_for(&e).expect("layout edge");
        let layout_label = layout_label.borrow();
        let mut input_label = input_label.borrow_mut();
        input_label.points.clone_from(&layout_label.points);
        if layout_label.x.is_some() {
            input_label.x = layout_label.x;
            input_label.y = layout_label.y;
        }
    }

    let layout_g = layout_graph.graph();
    let layout_g = layout_g.borrow();
    let input_g = input_graph.graph();
    let mut input_g = input_g.borrow_mut();
    input_g.width = layout_g.width;
    input_g.height = layout_g.height;
}

/// Builds the layout graph from whitelisted input attributes.
fn build_layout_graph(input_graph: &LayoutGraph) -> LayoutGraph {
    let mut g: LayoutGraph = Graph::new(GraphOptions {
        multigraph: Some(true),
        compound: Some(true),
        ..Default::default()
    });
    let input_label = input_graph.graph();
    let input_label = input_label.borrow();
    g.set_graph(Rc::new(RefCell::new(GraphLabel {
        nodesep: input_label.nodesep,
        edgesep: input_label.edgesep,
        ranksep: input_label.ranksep,
        marginx: input_label.marginx,
        marginy: input_label.marginy,
        rankdir: input_label.rankdir.clone(),
        align: input_label.align.clone(),
        ranker: input_label.ranker.clone(),
        acyclicer: input_label.acyclicer.clone(),
        ..Default::default()
    })));

    for v in input_graph.nodes() {
        let node = input_graph.node(&v).expect("input node");
        let node = node.borrow();
        g.set_node_with(
            &v,
            node_ref(NodeLabel {
                width: node.width,
                height: node.height,
                ..Default::default()
            }),
        );
        g.set_parent(&v, input_graph.parent(&v).as_deref());
    }

    for e in input_graph.edges() {
        let edge = input_graph.edge_for(&e).expect("input edge");
        let edge = edge.borrow();
        g.set_edge_obj(
            &e,
            edge_ref(EdgeLabel {
                minlen: edge.minlen,
                weight: edge.weight,
                width: edge.width,
                height: edge.height,
                labeloffset: edge.labeloffset,
                labelpos: edge.labelpos.clone(),
                ..Default::default()
            }),
        );
    }

    g
}

/// Halves ranksep and doubles minlen so edge labels get their own rank.
fn make_space_for_edge_labels(g: &LayoutGraph) {
    let graph = g.graph();
    let rankdir = {
        let mut graph = graph.borrow_mut();
        graph.ranksep /= 2.0;
        graph.rankdir.clone()
    };
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let mut edge = edge.borrow_mut();
        edge.minlen *= 2.0;
        if edge.labelpos.to_lowercase() != "c" {
            if rankdir == "TB" || rankdir == "BT" {
                edge.width += edge.labeloffset;
            } else {
                edge.height += edge.labeloffset;
            }
        }
    }
}

/// Adds dummy nodes capturing the rank each labeled edge's label will go to.
fn inject_edge_label_proxies(g: &mut LayoutGraph, ids: &UniqueId) {
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let (width, height) = {
            let edge = edge.borrow();
            (edge.width, edge.height)
        };
        if width != 0.0 && height != 0.0 {
            let v_rank = g.node(&e.v).expect("node").borrow().rank.expect("rank");
            let w_rank = g.node(&e.w).expect("node").borrow().rank.expect("rank");
            let label = node_ref(NodeLabel {
                rank: Some((w_rank - v_rank) / 2.0 + v_rank),
                e: Some(e.clone()),
                ..Default::default()
            });
            util::add_dummy_node(g, Dummy::EdgeProxy, label, "_ep", ids);
        }
    }
}

fn assign_rank_min_max(g: &LayoutGraph) {
    // JS calls `_.max(maxRank, node.maxRank)` which (mis)treats the first
    // argument as a collection: the result is 0 when no clusters exist and
    // `undefined` otherwise. Replicated faithfully.
    let mut any_cluster = false;
    for v in g.nodes() {
        let node = g.node(&v).expect("node");
        let border_top = node.borrow().border_top.clone();
        if let Some(border_top) = border_top {
            let border_bottom = node.borrow().border_bottom.clone().expect("borderBottom");
            let min_rank = g.node(&border_top).expect("border node").borrow().rank;
            let max_rank = g.node(&border_bottom).expect("border node").borrow().rank;
            let mut node = node.borrow_mut();
            node.min_rank = min_rank;
            node.max_rank = max_rank;
            any_cluster = true;
        }
    }
    g.graph().borrow_mut().max_rank = if any_cluster { None } else { Some(0.0) };
}

fn remove_edge_label_proxies(g: &mut LayoutGraph) {
    for v in g.nodes() {
        let node = g.node(&v).expect("node");
        let (dummy, rank, e) = {
            let n = node.borrow();
            (n.dummy, n.rank, n.e.clone())
        };
        if dummy == Some(Dummy::EdgeProxy) {
            let e = e.expect("edge-proxy edge");
            g.edge_for(&e).expect("edge label").borrow_mut().label_rank = rank;
            g.remove_node(&v);
        }
    }
}

fn translate_graph(g: &LayoutGraph) {
    let mut min_x = f64::INFINITY;
    let mut max_x: f64 = 0.0;
    let mut min_y = f64::INFINITY;
    let mut max_y: f64 = 0.0;
    let graph_label = g.graph();
    let (margin_x, margin_y) = {
        let l = graph_label.borrow();
        (l.marginx, l.marginy)
    };

    let mut get_extremes = |x: f64, y: f64, w: f64, h: f64| {
        min_x = min_x.min(x - w / 2.0);
        max_x = max_x.max(x + w / 2.0);
        min_y = min_y.min(y - h / 2.0);
        max_y = max_y.max(y + h / 2.0);
    };

    for v in g.nodes() {
        let node = g.node(&v).expect("node");
        let n = node.borrow();
        get_extremes(n.x.expect("x"), n.y.expect("y"), n.width, n.height);
    }
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let l = edge.borrow();
        if let Some(x) = l.x {
            get_extremes(x, l.y.expect("y"), l.width, l.height);
        }
    }

    min_x -= margin_x;
    min_y -= margin_y;

    for v in g.nodes() {
        let node = g.node(&v).expect("node");
        let mut n = node.borrow_mut();
        *n.x.as_mut().expect("x") -= min_x;
        *n.y.as_mut().expect("y") -= min_y;
    }

    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let mut l = edge.borrow_mut();
        if let Some(points) = l.points.as_mut() {
            for p in points {
                p.x -= min_x;
                p.y -= min_y;
            }
        }
        if let Some(x) = l.x.as_mut() {
            *x -= min_x;
        }
        if let Some(y) = l.y.as_mut() {
            *y -= min_y;
        }
    }

    let mut graph_label = graph_label.borrow_mut();
    graph_label.width = max_x - min_x + margin_x;
    graph_label.height = max_y - min_y + margin_y;
}

fn assign_node_intersects(g: &LayoutGraph) {
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let node_v = g.node(&e.v).expect("node");
        let node_w = g.node(&e.w).expect("node");
        let (p1, p2) = {
            let mut l = edge.borrow_mut();
            if let Some(points) = &l.points {
                (points[0], points[points.len() - 1])
            } else {
                l.points = Some(Vec::new());
                let nw = node_w.borrow();
                let nv = node_v.borrow();
                (
                    Point {
                        x: nw.x.expect("x"),
                        y: nw.y.expect("y"),
                    },
                    Point {
                        x: nv.x.expect("x"),
                        y: nv.y.expect("y"),
                    },
                )
            }
        };
        let start = util::intersect_rect(&node_v.borrow(), p1);
        let end = util::intersect_rect(&node_w.borrow(), p2);
        let mut l = edge.borrow_mut();
        let points = l.points.as_mut().expect("points");
        points.insert(0, start);
        points.push(end);
    }
}

fn fixup_edge_label_coords(g: &LayoutGraph) {
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let mut l = edge.borrow_mut();
        if l.x.is_some() {
            if l.labelpos == "l" || l.labelpos == "r" {
                l.width -= l.labeloffset;
            }
            let (width, offset) = (l.width, l.labeloffset);
            match l.labelpos.as_str() {
                "l" => *l.x.as_mut().expect("x") -= width / 2.0 + offset,
                "r" => *l.x.as_mut().expect("x") += width / 2.0 + offset,
                _ => {}
            }
        }
    }
}

fn reverse_points_for_reversed_edges(g: &LayoutGraph) {
    for e in g.edges() {
        let edge = g.edge_for(&e).expect("edge label");
        let mut l = edge.borrow_mut();
        if l.reversed
            && let Some(points) = l.points.as_mut()
        {
            points.reverse();
        }
    }
}

fn remove_border_nodes(g: &mut LayoutGraph) {
    for v in g.nodes() {
        if !g.children(Some(&v)).is_empty() {
            let node = g.node(&v).expect("node");
            let (top, bottom, left, right) = {
                let n = node.borrow();
                (
                    n.border_top.clone().expect("borderTop"),
                    n.border_bottom.clone().expect("borderBottom"),
                    n.border_left
                        .values()
                        .next_back()
                        .cloned()
                        .expect("borderLeft"),
                    n.border_right
                        .values()
                        .next_back()
                        .cloned()
                        .expect("borderRight"),
                )
            };
            let t = g.node(&top).expect("border node");
            let b = g.node(&bottom).expect("border node");
            let l = g.node(&left).expect("border node");
            let r = g.node(&right).expect("border node");

            let mut n = node.borrow_mut();
            n.width = (r.borrow().x.expect("x") - l.borrow().x.expect("x")).abs();
            n.height = (b.borrow().y.expect("y") - t.borrow().y.expect("y")).abs();
            n.x = Some(l.borrow().x.expect("x") + n.width / 2.0);
            n.y = Some(t.borrow().y.expect("y") + n.height / 2.0);
        }
    }

    for v in g.nodes() {
        if g.node(&v).expect("node").borrow().dummy == Some(Dummy::Border) {
            g.remove_node(&v);
        }
    }
}

fn remove_self_edges(g: &mut LayoutGraph) {
    for e in g.edges() {
        if e.v == e.w {
            let node = g.node(&e.v).expect("node");
            let label = g.edge_for(&e).expect("edge label");
            node.borrow_mut().self_edges.push(SelfEdgeRec {
                e: e.clone(),
                label,
            });
            g.remove_edge_obj(&e);
        }
    }
}

fn insert_self_edges(g: &mut LayoutGraph, ids: &UniqueId) {
    let layers = util::build_layer_matrix(g);
    for layer in layers {
        let mut order_shift: f64 = 0.0;
        for (i, v) in layer.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let i = i as f64;
            let node = g.node(v).expect("node");
            let (rank, self_edges) = {
                let mut n = node.borrow_mut();
                n.order = Some(i + order_shift);
                (n.rank, std::mem::take(&mut n.self_edges))
            };
            for self_edge in self_edges {
                let (width, height) = {
                    let l = self_edge.label.borrow();
                    (l.width, l.height)
                };
                order_shift += 1.0;
                let label = node_ref(NodeLabel {
                    width,
                    height,
                    rank,
                    order: Some(i + order_shift),
                    e: Some(self_edge.e.clone()),
                    se_label: Some(self_edge.label.clone()),
                    ..Default::default()
                });
                util::add_dummy_node(g, Dummy::SelfEdge, label, "_se", ids);
            }
        }
    }
}

fn position_self_edges(g: &mut LayoutGraph) {
    for v in g.nodes() {
        let node = g.node(&v).expect("node");
        let is_self_edge = node.borrow().dummy == Some(Dummy::SelfEdge);
        if is_self_edge {
            let (e, se_label, node_x, node_y) = {
                let n = node.borrow();
                (
                    n.e.clone().expect("selfedge e"),
                    n.se_label.clone().expect("selfedge label"),
                    n.x.expect("x"),
                    n.y.expect("y"),
                )
            };
            let self_node = g.node(&e.v).expect("self node");
            let (x, y, dy) = {
                let sn = self_node.borrow();
                (
                    sn.x.expect("x") + sn.width / 2.0,
                    sn.y.expect("y"),
                    sn.height / 2.0,
                )
            };
            let dx = node_x - x;
            g.set_edge_obj(&e, se_label.clone());
            g.remove_node(&v);
            let mut label = se_label.borrow_mut();
            label.points = Some(vec![
                Point {
                    x: x + 2.0 * dx / 3.0,
                    y: y - dy,
                },
                Point {
                    x: x + 5.0 * dx / 6.0,
                    y: y - dy,
                },
                Point { x: x + dx, y },
                Point {
                    x: x + 5.0 * dx / 6.0,
                    y: y + dy,
                },
                Point {
                    x: x + 2.0 * dx / 3.0,
                    y: y + dy,
                },
            ]);
            label.x = Some(node_x);
            label.y = Some(node_y);
        }
    }
}
