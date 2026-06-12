//! Port of dagre-d3-es `src/dagre/parent-dummy-chains.js`.
//!
//! Assigns dummy-chain nodes to the proper cluster parents along the path
//! between the original edge's endpoints.

use std::collections::HashMap;

use super::types::{LayoutGraph, js_lt};

#[derive(Debug, Clone, Copy)]
struct LowLim {
    low: f64,
    lim: f64,
}

pub fn parent_dummy_chains(g: &mut LayoutGraph) {
    let postorder_nums = postorder(g);

    let dummy_chains = g.graph().borrow().dummy_chains.clone();
    for chain_start in dummy_chains {
        let mut v = chain_start;
        let node = g.node(&v).expect("dummy node");
        let edge_obj = node.borrow().edge_obj.clone().expect("edgeObj");
        let path_data = find_path(g, &postorder_nums, &edge_obj.v, &edge_obj.w);
        let path = path_data.path;
        let lca = path_data.lca;
        let mut path_idx: usize = 0;
        let mut path_v: Option<String> = path.first().cloned().flatten();
        let mut ascending = true;

        while v != edge_obj.w {
            let node = g.node(&v).expect("dummy node");
            let rank = node.borrow().rank;

            if ascending {
                loop {
                    path_v = path.get(path_idx).cloned().flatten();
                    if path_v == lca {
                        break;
                    }
                    let max_rank = g
                        .node(path_v.as_deref().expect("path node before lca"))
                        .expect("path node")
                        .borrow()
                        .max_rank;
                    if !js_lt(max_rank, rank) {
                        break;
                    }
                    path_idx += 1;
                }

                if path_v == lca {
                    ascending = false;
                }
            }

            if !ascending {
                while path_idx < path.len() - 1 {
                    let next = path.get(path_idx + 1).cloned().flatten();
                    let min_rank = g
                        .node(next.as_deref().expect("descending path node"))
                        .expect("path node")
                        .borrow()
                        .min_rank;
                    // JS: g.node(path[pathIdx+1]).minRank <= node.rank
                    let proceed = match (min_rank, rank) {
                        (Some(min), Some(rank)) => min <= rank,
                        _ => false,
                    };
                    if !proceed {
                        break;
                    }
                    path_idx += 1;
                }
                path_v = path.get(path_idx).cloned().flatten();
            }

            g.set_parent(&v, path_v.as_deref());
            v.clone_from(&g.successors(&v)[0]);
        }
    }
}

struct PathData {
    path: Vec<Option<String>>,
    lca: Option<String>,
}

/// Path from `v` to `w` through their lowest common ancestor.
fn find_path(
    g: &LayoutGraph,
    postorder_nums: &HashMap<String, LowLim>,
    v: &str,
    w: &str,
) -> PathData {
    let mut v_path: Vec<Option<String>> = Vec::new();
    let mut w_path: Vec<String> = Vec::new();
    let low = postorder_nums[v].low.min(postorder_nums[w].low);
    let lim = postorder_nums[v].lim.max(postorder_nums[w].lim);

    // Traverse up from v to find the LCA.
    let mut parent = Some(v.to_owned());
    loop {
        parent = parent.and_then(|p| g.parent(&p));
        v_path.push(parent.clone());
        let keep_going = parent.as_ref().is_some_and(|p| {
            let nums = postorder_nums[p];
            nums.low > low || lim > nums.lim
        });
        if !keep_going {
            break;
        }
    }
    let lca = parent;

    // Traverse from w to LCA.
    let mut parent = g.parent(w);
    while parent != lca {
        w_path.push(parent.clone().expect("w path reaches lca"));
        parent = g.parent(parent.as_deref().expect("w path reaches lca"));
    }

    let mut path = v_path;
    path.extend(w_path.into_iter().rev().map(Some));
    PathData { path, lca }
}

fn postorder(g: &LayoutGraph) -> HashMap<String, LowLim> {
    let mut result = HashMap::new();
    let mut lim = 0.0;

    fn dfs(g: &LayoutGraph, result: &mut HashMap<String, LowLim>, lim: &mut f64, v: &str) {
        let low = *lim;
        for child in g.children(Some(v)) {
            dfs(g, result, lim, &child);
        }
        result.insert(v.to_owned(), LowLim { low, lim: *lim });
        *lim += 1.0;
    }

    for v in g.children(None) {
        dfs(g, &mut result, &mut lim, &v);
    }

    result
}
