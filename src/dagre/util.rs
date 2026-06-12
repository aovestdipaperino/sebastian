//! Port of dagre-d3-es `src/dagre/util.js`.

use std::collections::HashMap;

use crate::graphlib::{Graph, GraphOptions};

use super::types::{
    Dummy, EdgeLabel, LayoutGraph, NodeLabel, NodeLabelRef, Point, UniqueId, edge_ref,
};

/// Adds a dummy node to the graph and returns its generated name.
pub fn add_dummy_node(
    g: &mut LayoutGraph,
    dummy: Dummy,
    attrs: NodeLabelRef,
    name: &str,
    ids: &UniqueId,
) -> String {
    // JS uses do/while, always consuming at least one id.
    let mut v = ids.next(name);
    while g.has_node(&v) {
        v = ids.next(name);
    }
    attrs.borrow_mut().dummy = Some(dummy);
    g.set_node_with(&v, attrs);
    v
}

/// Returns a new graph with only simple edges, aggregating multi-edge weights.
#[must_use]
pub fn simplify(g: &LayoutGraph) -> LayoutGraph {
    let mut simplified: LayoutGraph = Graph::new(GraphOptions::default());
    simplified.set_graph(g.graph());
    for v in g.nodes() {
        if let Some(label) = g.node(&v) {
            simplified.set_node_with(&v, label);
        } else {
            simplified.set_node(&v);
        }
    }
    for e in g.edges() {
        let (prev_weight, prev_minlen) = simplified
            .edge(&e.v, &e.w, None)
            .map_or((0.0, 1.0), |l| (l.borrow().weight, l.borrow().minlen));
        let label = g.edge_for(&e).expect("edge label");
        let label = label.borrow();
        simplified.set_edge(
            &e.v,
            &e.w,
            edge_ref(EdgeLabel::rank_label(
                prev_weight + label.weight,
                prev_minlen.max(label.minlen),
            )),
            None,
        );
    }
    simplified
}

/// Copy of the graph without compound structure; labels are shared.
#[must_use]
pub fn as_non_compound_graph(g: &LayoutGraph) -> LayoutGraph {
    let mut simplified: LayoutGraph = Graph::new(GraphOptions {
        multigraph: Some(g.is_multigraph()),
        ..Default::default()
    });
    simplified.set_graph(g.graph());
    for v in g.nodes() {
        if g.children(Some(&v)).is_empty() {
            if let Some(label) = g.node(&v) {
                simplified.set_node_with(&v, label);
            } else {
                simplified.set_node(&v);
            }
        }
    }
    for e in g.edges() {
        let label = g.edge_for(&e).expect("edge label");
        simplified.set_edge_obj(&e, label);
    }
    simplified
}

/// Finds where a line from `point` toward the center of `rect` crosses the
/// rectangle boundary.
#[must_use]
pub fn intersect_rect(rect: &NodeLabel, point: Point) -> Point {
    let x = rect.x.expect("rect.x");
    let y = rect.y.expect("rect.y");
    let dx = point.x - x;
    let dy = point.y - y;
    let mut w = rect.width / 2.0;
    let mut h = rect.height / 2.0;
    assert!(
        dx != 0.0 || dy != 0.0,
        "not possible to find intersection inside of the rectangle"
    );
    let (sx, sy) = if dy.abs() * w > dx.abs() * h {
        // Intersection is top or bottom of rect.
        if dy < 0.0 {
            h = -h;
        }
        (h * dx / dy, h)
    } else {
        // Intersection is left or right of rect.
        if dx < 0.0 {
            w = -w;
        }
        (w, w * dy / dx)
    };
    Point {
        x: x + sx,
        y: y + sy,
    }
}

/// Builds the rank-by-order matrix of node ids.
///
/// Panics on a hole, which would mean ranks/orders went out of sync — a
/// porting bug.
#[must_use]
pub fn build_layer_matrix(g: &LayoutGraph) -> Vec<Vec<String>> {
    let max_rank = max_rank(g).expect("graph has ranked nodes");
    let len = usize::try_from(max_rank as i64 + 1).expect("non-negative max rank");
    let mut layering: Vec<Vec<Option<String>>> = vec![Vec::new(); len];
    for v in g.nodes() {
        let node = g.node(&v).expect("node label");
        let node = node.borrow();
        if let Some(rank) = node.rank {
            let rank = usize::try_from(rank as i64).expect("normalized rank");
            let order = node.order.expect("node order") as usize;
            let layer = &mut layering[rank];
            if layer.len() <= order {
                layer.resize(order + 1, None);
            }
            layer[order] = Some(v);
        }
    }
    layering
        .into_iter()
        .map(|layer| {
            layer
                .into_iter()
                .map(|v| v.expect("dense order assignment"))
                .collect()
        })
        .collect()
}

/// Shifts ranks so the minimum is zero.
pub fn normalize_ranks(g: &LayoutGraph) {
    let min = g
        .nodes()
        .iter()
        .filter_map(|v| g.node(v).expect("node label").borrow().rank)
        .fold(None, |acc: Option<f64>, r| {
            Some(acc.map_or(r, |a| if r < a { r } else { a }))
        });
    let Some(min) = min else { return };
    for v in g.nodes() {
        let node = g.node(&v).expect("node label");
        let mut node = node.borrow_mut();
        if let Some(rank) = node.rank.as_mut() {
            *rank -= min;
        }
    }
}

/// Removes empty ranks left behind by the nesting graph.
///
/// Replicates the JS sparse-array behavior: only integer `rank - offset`
/// indices participate; the iteration bound is the highest such index.
pub fn remove_empty_ranks(g: &LayoutGraph) {
    let offset = g
        .nodes()
        .iter()
        .filter_map(|v| g.node(v).expect("node label").borrow().rank)
        .fold(f64::INFINITY, f64::min);

    let mut layers: HashMap<i64, Vec<String>> = HashMap::new();
    let mut length: i64 = 0;
    for v in g.nodes() {
        // Unranked nodes (clusters) index as NaN in JS and are skipped.
        let Some(rank) = g.node(&v).expect("node label").borrow().rank else {
            continue;
        };
        let rank = rank - offset;
        if rank >= 0.0 && rank.fract() == 0.0 {
            let idx = rank as i64;
            layers.entry(idx).or_default().push(v);
            length = length.max(idx + 1);
        }
    }

    let mut delta: f64 = 0.0;
    let node_rank_factor = g.graph().borrow().node_rank_factor;
    for i in 0..length {
        let vs = layers.get(&i);
        #[allow(clippy::cast_precision_loss)]
        let i_mod = (i as f64) % node_rank_factor;
        if vs.is_none() && i_mod != 0.0 {
            delta -= 1.0;
        } else if delta != 0.0
            && let Some(vs) = vs
        {
            for v in vs {
                let node = g.node(v).expect("node label");
                let mut node = node.borrow_mut();
                *node.rank.as_mut().expect("rank") += delta;
            }
        }
    }
}

/// Adds a zero-size border dummy node.
pub fn add_border_node(
    g: &mut LayoutGraph,
    prefix: &str,
    rank_order: Option<(f64, f64)>,
    ids: &UniqueId,
) -> String {
    let mut label = NodeLabel {
        width: 0.0,
        height: 0.0,
        ..Default::default()
    };
    if let Some((rank, order)) = rank_order {
        label.rank = Some(rank);
        label.order = Some(order);
    }
    add_dummy_node(g, Dummy::Border, super::types::node_ref(label), prefix, ids)
}

/// Highest rank present in the graph, if any node is ranked.
#[must_use]
pub fn max_rank(g: &LayoutGraph) -> Option<f64> {
    g.nodes()
        .iter()
        .filter_map(|v| g.node(v).expect("node label").borrow().rank)
        .fold(None, |acc, r| {
            Some(acc.map_or(r, |a| if r > a { r } else { a }))
        })
}
