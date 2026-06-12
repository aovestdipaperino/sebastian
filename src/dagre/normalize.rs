//! Port of dagre-d3-es `src/dagre/normalize.js`.
//!
//! Splits multi-rank edges into unit-length segments joined by dummy nodes,
//! and undoes the split after positioning.

use super::types::{Dummy, EdgeLabel, LayoutGraph, NodeLabel, Point, UniqueId, edge_ref, node_ref};
use super::util;
use crate::graphlib::EdgeObj;

pub fn run(g: &mut LayoutGraph, ids: &UniqueId) {
    g.graph().borrow_mut().dummy_chains.clear();
    for e in g.edges() {
        normalize_edge(g, &e, ids);
    }
}

fn normalize_edge(g: &mut LayoutGraph, e: &EdgeObj, ids: &UniqueId) {
    let mut v = e.v.clone();
    let mut v_rank = g.node(&v).expect("node").borrow().rank.expect("rank");
    let w = &e.w;
    let w_rank = g.node(w).expect("node").borrow().rank.expect("rank");
    let name = e.name.clone();
    let edge_label = g.edge_for(e).expect("edge label");
    let label_rank = edge_label.borrow().label_rank;

    if w_rank == v_rank + 1.0 {
        return;
    }

    g.remove_edge_obj(e);

    let mut i = 0u64;
    v_rank += 1.0;
    while v_rank < w_rank {
        edge_label.borrow_mut().points = Some(Vec::new());
        let attrs = node_ref(NodeLabel {
            width: 0.0,
            height: 0.0,
            edge_label: Some(edge_label.clone()),
            edge_obj: Some(e.clone()),
            rank: Some(v_rank),
            ..Default::default()
        });
        let dummy = util::add_dummy_node(g, Dummy::Edge, attrs.clone(), "_d", ids);
        if Some(v_rank) == label_rank {
            let mut attrs_mut = attrs.borrow_mut();
            let label = edge_label.borrow();
            attrs_mut.width = label.width;
            attrs_mut.height = label.height;
            attrs_mut.dummy = Some(Dummy::EdgeLabel);
            attrs_mut.labelpos = Some(label.labelpos.clone());
        }
        let weight = edge_label.borrow().weight;
        g.set_edge(
            &v,
            &dummy,
            edge_ref(EdgeLabel {
                weight,
                ..Default::default()
            }),
            name.as_deref(),
        );
        if i == 0 {
            g.graph().borrow_mut().dummy_chains.push(dummy.clone());
        }
        v = dummy;
        i += 1;
        v_rank += 1.0;
    }

    let weight = edge_label.borrow().weight;
    g.set_edge(
        &v,
        w,
        edge_ref(EdgeLabel {
            weight,
            ..Default::default()
        }),
        name.as_deref(),
    );
}

pub fn undo(g: &mut LayoutGraph) {
    let dummy_chains = g.graph().borrow().dummy_chains.clone();
    for chain_start in dummy_chains {
        let mut v = chain_start;
        let mut node = g.node(&v).expect("dummy node");
        let orig_label = node.borrow().edge_label.clone().expect("edge label");
        let edge_obj = node.borrow().edge_obj.clone().expect("edge obj");
        g.set_edge_obj(&edge_obj, orig_label.clone());
        while node.borrow().dummy.is_some() {
            let w = g.successors(&v)[0].clone();
            g.remove_node(&v);
            {
                let n = node.borrow();
                let mut label = orig_label.borrow_mut();
                label.points.get_or_insert_with(Vec::new).push(Point {
                    x: n.x.expect("x"),
                    y: n.y.expect("y"),
                });
                if n.dummy == Some(Dummy::EdgeLabel) {
                    label.x = n.x;
                    label.y = n.y;
                    label.width = n.width;
                    label.height = n.height;
                }
            }
            v = w;
            node = g.node(&v).expect("node");
        }
    }
}
