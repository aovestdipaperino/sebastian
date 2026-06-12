//! Port of dagre-d3-es `src/dagre/acyclic.js`.

use std::collections::HashSet;

use super::types::{LayoutGraph, UniqueId};
use crate::graphlib::EdgeObj;

pub fn run(g: &mut LayoutGraph, ids: &UniqueId) {
    let acyclicer = g.graph().borrow().acyclicer.clone();
    assert!(
        acyclicer.as_deref() != Some("greedy"),
        "greedy acyclicer is not used by mermaid and is not ported"
    );
    let fas = dfs_fas(g);
    for e in fas {
        let label = g.edge_for(&e).expect("edge label");
        g.remove_edge_obj(&e);
        {
            let mut label_mut = label.borrow_mut();
            label_mut.forward_name.clone_from(&e.name);
            label_mut.reversed = true;
        }
        let name = ids.next("rev");
        g.set_edge(&e.w, &e.v, label, Some(&name));
    }
}

fn dfs_fas(g: &LayoutGraph) -> Vec<EdgeObj> {
    let mut fas = Vec::new();
    let mut stack = HashSet::new();
    let mut visited = HashSet::new();

    fn dfs(
        g: &LayoutGraph,
        v: &str,
        fas: &mut Vec<EdgeObj>,
        stack: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) {
        if visited.contains(v) {
            return;
        }
        visited.insert(v.to_owned());
        stack.insert(v.to_owned());
        for e in g.out_edges(v, None) {
            if stack.contains(&e.w) {
                fas.push(e);
            } else {
                dfs(g, &e.w, fas, stack, visited);
            }
        }
        stack.remove(v);
    }

    for v in g.nodes() {
        dfs(g, &v, &mut fas, &mut stack, &mut visited);
    }
    fas
}

pub fn undo(g: &mut LayoutGraph) {
    for e in g.edges() {
        let label = g.edge_for(&e).expect("edge label");
        let reversed = label.borrow().reversed;
        if reversed {
            g.remove_edge_obj(&e);
            let forward_name = {
                let mut label_mut = label.borrow_mut();
                let name = label_mut.forward_name.take();
                label_mut.reversed = false;
                name
            };
            g.set_edge(&e.w, &e.v, label, forward_name.as_deref());
        }
    }
}
