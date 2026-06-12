//! Port of dagre-d3-es `src/dagre/add-border-segments.js`.

use super::types::{
    BorderType, Dummy, EdgeLabel, LayoutGraph, NodeLabel, UniqueId, edge_ref, node_ref,
};
use super::util;

pub fn add_border_segments(g: &mut LayoutGraph, ids: &UniqueId) {
    fn dfs(g: &mut LayoutGraph, v: &str, ids: &UniqueId) {
        let children = g.children(Some(v));
        for child in &children {
            dfs(g, child, ids);
        }
        let node = g.node(v).expect("node label");
        let min_max = {
            let n = node.borrow();
            n.min_rank.map(|min| (min, n.max_rank.expect("maxRank")))
        };
        if let Some((min_rank, max_rank)) = min_max {
            {
                let mut n = node.borrow_mut();
                n.border_left.clear();
                n.border_right.clear();
            }
            let mut rank = min_rank as i64;
            let max = max_rank as i64 + 1;
            while rank < max {
                add_border_node(g, BorderType::BorderLeft, "_bl", v, rank, ids);
                add_border_node(g, BorderType::BorderRight, "_br", v, rank, ids);
                rank += 1;
            }
        }
    }

    for v in g.children(None) {
        dfs(g, &v, ids);
    }
}

fn add_border_node(
    g: &mut LayoutGraph,
    prop: BorderType,
    prefix: &str,
    sg: &str,
    rank: i64,
    ids: &UniqueId,
) {
    #[allow(clippy::cast_precision_loss)]
    let label = node_ref(NodeLabel {
        width: 0.0,
        height: 0.0,
        rank: Some(rank as f64),
        border_type: Some(prop),
        ..Default::default()
    });
    let sg_node = g.node(sg).expect("subgraph node");
    let prev = {
        let n = sg_node.borrow();
        let map = match prop {
            BorderType::BorderLeft => &n.border_left,
            BorderType::BorderRight => &n.border_right,
        };
        map.get(&(rank - 1)).cloned()
    };
    let curr = util::add_dummy_node(g, Dummy::Border, label, prefix, ids);
    {
        let mut n = sg_node.borrow_mut();
        let map = match prop {
            BorderType::BorderLeft => &mut n.border_left,
            BorderType::BorderRight => &mut n.border_right,
        };
        map.insert(rank, curr.clone());
    }
    g.set_parent(&curr, Some(sg));
    if let Some(prev) = prev {
        g.set_edge(
            &prev,
            &curr,
            edge_ref(EdgeLabel {
                weight: 1.0,
                ..Default::default()
            }),
            None,
        );
    }
}
