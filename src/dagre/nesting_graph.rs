//! Port of dagre-d3-es `src/dagre/nesting-graph.js`.

use std::collections::HashMap;

use super::types::{Dummy, EdgeLabel, LayoutGraph, NodeLabel, UniqueId, edge_ref, node_ref};
use super::util;

pub fn run(g: &mut LayoutGraph, ids: &UniqueId) {
    let root = util::add_dummy_node(g, Dummy::Root, node_ref(NodeLabel::default()), "_root", ids);
    let depths = tree_depths(g);
    let height = depths.values().copied().fold(f64::NEG_INFINITY, f64::max) - 1.0;
    let node_sep = 2.0 * height + 1.0;

    g.graph().borrow_mut().nesting_root = Some(root.clone());

    // Multiply minlen by nodeSep to align nodes on non-border ranks.
    for e in g.edges() {
        g.edge_for(&e).expect("edge label").borrow_mut().minlen *= node_sep;
    }

    // Calculate a weight that is sufficient to keep subgraphs vertically compact.
    let weight = sum_weights(g) + 1.0;

    // Create border nodes and link them up.
    for child in g.children(None) {
        dfs(g, &root, node_sep, weight, height, &depths, &child, ids);
    }

    // Save the multiplier for node layers for later removal of empty border layers.
    g.graph().borrow_mut().node_rank_factor = node_sep;
}

#[allow(clippy::too_many_arguments)]
fn dfs(
    g: &mut LayoutGraph,
    root: &str,
    node_sep: f64,
    weight: f64,
    height: f64,
    depths: &HashMap<String, f64>,
    v: &str,
    ids: &UniqueId,
) {
    let children = g.children(Some(v));
    if children.is_empty() {
        if v != root {
            g.set_edge(
                root,
                v,
                edge_ref(EdgeLabel::rank_label(0.0, node_sep)),
                None,
            );
        }
        return;
    }

    let top = util::add_border_node(g, "_bt", None, ids);
    let bottom = util::add_border_node(g, "_bb", None, ids);
    let label = g.node(v).expect("subgraph node");

    g.set_parent(&top, Some(v));
    label.borrow_mut().border_top = Some(top.clone());
    g.set_parent(&bottom, Some(v));
    label.borrow_mut().border_bottom = Some(bottom.clone());

    for child in &children {
        dfs(g, root, node_sep, weight, height, depths, child, ids);

        let child_node = g.node(child).expect("child node");
        let (child_top, child_bottom, this_weight) = {
            let cn = child_node.borrow();
            (
                cn.border_top.clone().unwrap_or_else(|| child.clone()),
                cn.border_bottom.clone().unwrap_or_else(|| child.clone()),
                if cn.border_top.is_some() {
                    weight
                } else {
                    2.0 * weight
                },
            )
        };
        let minlen = if child_top == child_bottom {
            height - depths[v] + 1.0
        } else {
            1.0
        };

        let mut top_edge = EdgeLabel::rank_label(this_weight, minlen);
        top_edge.nesting_edge = true;
        g.set_edge(&top, &child_top, edge_ref(top_edge), None);

        let mut bottom_edge = EdgeLabel::rank_label(this_weight, minlen);
        bottom_edge.nesting_edge = true;
        g.set_edge(&child_bottom, &bottom, edge_ref(bottom_edge), None);
    }

    if g.parent(v).is_none() {
        g.set_edge(
            root,
            &top,
            edge_ref(EdgeLabel::rank_label(0.0, height + depths[v])),
            None,
        );
    }
}

fn tree_depths(g: &LayoutGraph) -> HashMap<String, f64> {
    let mut depths = HashMap::new();

    fn dfs(g: &LayoutGraph, depths: &mut HashMap<String, f64>, v: &str, depth: f64) {
        for child in g.children(Some(v)) {
            dfs(g, depths, &child, depth + 1.0);
        }
        depths.insert(v.to_owned(), depth);
    }

    for v in g.children(None) {
        dfs(g, &mut depths, &v, 1.0);
    }
    depths
}

fn sum_weights(g: &LayoutGraph) -> f64 {
    g.edges()
        .iter()
        .map(|e| g.edge_for(e).expect("edge label").borrow().weight)
        .sum()
}

pub fn cleanup(g: &mut LayoutGraph) {
    let nesting_root = {
        let graph = g.graph();
        let mut graph = graph.borrow_mut();
        graph.nesting_root.take()
    };
    if let Some(root) = nesting_root {
        g.remove_node(&root);
    }
    for e in g.edges() {
        if g.edge_for(&e).expect("edge label").borrow().nesting_edge {
            g.remove_edge_obj(&e);
        }
    }
}
