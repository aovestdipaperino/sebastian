//! Port of dagre-d3-es `src/dagre/rank/` (longest-path, feasible-tree,
//! network-simplex).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::graphlib::{EdgeObj, Graph, GraphOptions, alg};

use super::types::LayoutGraph;
use super::util::simplify;

/// Node label of the tight tree.
#[derive(Debug, Clone, Default)]
struct TreeNode {
    low: f64,
    lim: f64,
    parent: Option<String>,
}

/// Edge label of the tight tree.
#[derive(Debug, Clone, Default)]
struct TreeEdge {
    cutvalue: f64,
}

type TreeNodeRef = Rc<RefCell<TreeNode>>;
type TreeEdgeRef = Rc<RefCell<TreeEdge>>;
type Tree = Graph<(), TreeNodeRef, TreeEdgeRef>;

fn new_tree() -> Tree {
    Graph::new(GraphOptions {
        directed: Some(false),
        ..Default::default()
    })
}

/// Assigns a rank to each node respecting edge `minlen` constraints.
pub fn rank(g: &LayoutGraph) {
    let ranker = g.graph().borrow().ranker.clone();
    match ranker.as_deref() {
        Some("tight-tree") => {
            longest_path(g);
            feasible_tree(g);
        }
        Some("longest-path") => longest_path(g),
        _ => network_simplex(g),
    }
}

/// Initial ranking via longest path from the sources.
fn longest_path(g: &LayoutGraph) {
    let mut visited: HashSet<String> = HashSet::new();

    fn dfs(g: &LayoutGraph, v: &str, visited: &mut HashSet<String>) -> Option<f64> {
        let label = g.node(v).expect("node label");
        if visited.contains(v) {
            return label.borrow().rank;
        }
        visited.insert(v.to_owned());

        let rank = g
            .out_edges(v, None)
            .iter()
            .map(|e| {
                let child_rank = dfs(g, &e.w, visited);
                let minlen = g.edge_for(e).expect("edge label").borrow().minlen;
                // JS: undefined - minlen = NaN; cannot occur in a DAG.
                child_rank.expect("DAG rank") - minlen
            })
            .fold(None, |acc: Option<f64>, r| {
                Some(acc.map_or(r, |a| if r < a { r } else { a }))
            })
            .unwrap_or(0.0);

        label.borrow_mut().rank = Some(rank);
        Some(rank)
    }

    for v in g.sources() {
        dfs(g, &v, &mut visited);
    }
}

/// Slack: edge length minus its minimum length.
fn slack(g: &LayoutGraph, e: &EdgeObj) -> f64 {
    let w_rank = g.node(&e.w).expect("node").borrow().rank.expect("rank");
    let v_rank = g.node(&e.v).expect("node").borrow().rank.expect("rank");
    let minlen = g.edge_for(e).expect("edge label").borrow().minlen;
    w_rank - v_rank - minlen
}

/// Builds a spanning tree of tight edges, adjusting ranks until it spans.
fn feasible_tree(g: &LayoutGraph) -> Tree {
    let mut t = new_tree();

    // Choose arbitrary node from which to start our tree.
    let start = g.nodes()[0].clone();
    let size = g.node_count();
    t.set_node_with(&start, Rc::new(RefCell::new(TreeNode::default())));

    while tight_tree(&mut t, g) < size {
        let edge = find_min_slack_edge(&t, g).expect("incident edge with slack");
        let delta = if t.has_node(&edge.v) {
            slack(g, &edge)
        } else {
            -slack(g, &edge)
        };
        shift_ranks(&t, g, delta);
    }

    t
}

/// Grows `t` along zero-slack edges; returns the tree's node count.
fn tight_tree(t: &mut Tree, g: &LayoutGraph) -> usize {
    fn dfs(t: &mut Tree, g: &LayoutGraph, v: &str) {
        for e in g.node_edges(v, None) {
            let edge_v = &e.v;
            let w = if v == edge_v {
                e.w.clone()
            } else {
                edge_v.clone()
            };
            if !t.has_node(&w) && slack(g, &e) == 0.0 {
                t.set_node_with(&w, Rc::new(RefCell::new(TreeNode::default())));
                t.set_edge(v, &w, Rc::new(RefCell::new(TreeEdge::default())), None);
                dfs(t, g, &w);
            }
        }
    }

    for v in t.nodes() {
        dfs(t, g, &v);
    }
    t.node_count()
}

/// First minimum-slack edge with exactly one endpoint in the tree.
fn find_min_slack_edge(t: &Tree, g: &LayoutGraph) -> Option<EdgeObj> {
    let mut best: Option<(f64, EdgeObj)> = None;
    for e in g.edges() {
        if t.has_node(&e.v) != t.has_node(&e.w) {
            let s = slack(g, &e);
            if best.as_ref().is_none_or(|(b, _)| s < *b) {
                best = Some((s, e));
            }
        }
    }
    best.map(|(_, e)| e)
}

fn shift_ranks(t: &Tree, g: &LayoutGraph, delta: f64) {
    for v in t.nodes() {
        let node = g.node(&v).expect("node label");
        *node.borrow_mut().rank.as_mut().expect("rank") += delta;
    }
}

/// The network simplex ranker.
fn network_simplex(g_in: &LayoutGraph) {
    let g = simplify(g_in);
    longest_path(&g);
    let mut t = feasible_tree(&g);
    init_low_lim_values(&t, None);
    init_cut_values(&t, &g);

    while let Some(e) = leave_edge(&t) {
        let f = enter_edge(&t, &g, &e).expect("entering edge");
        exchange_edges(&mut t, &g, &e, &f);
    }
}

/// Initializes cut values for all tree edges.
fn init_cut_values(t: &Tree, g: &LayoutGraph) {
    let mut vs = alg::postorder(t, &t.nodes());
    vs.pop();
    for v in vs {
        assign_cut_value(t, g, &v);
    }
}

fn assign_cut_value(t: &Tree, g: &LayoutGraph, child: &str) {
    let parent = t
        .node(child)
        .expect("tree node")
        .borrow()
        .parent
        .clone()
        .expect("tree parent");
    let cut = calc_cut_value(t, g, child);
    t.edge(child, &parent, None)
        .expect("tree edge")
        .borrow_mut()
        .cutvalue = cut;
}

/// Cut value of the tree edge between `child` and its tree parent.
fn calc_cut_value(t: &Tree, g: &LayoutGraph, child: &str) -> f64 {
    let parent = t
        .node(child)
        .expect("tree node")
        .borrow()
        .parent
        .clone()
        .expect("tree parent");
    // True if the child is on the tail end of the edge in the directed graph.
    let mut child_is_tail = true;
    let mut graph_edge = g.edge(child, &parent, None);
    if graph_edge.is_none() {
        child_is_tail = false;
        graph_edge = g.edge(&parent, child, None);
    }
    let mut cut_value = graph_edge.expect("graph edge").borrow().weight;

    for e in g.node_edges(child, None) {
        let is_out_edge = e.v == child;
        let other = if is_out_edge { &e.w } else { &e.v };
        if other != &parent {
            let points_to_head = is_out_edge == child_is_tail;
            let other_weight = g.edge_for(&e).expect("edge label").borrow().weight;
            cut_value += if points_to_head {
                other_weight
            } else {
                -other_weight
            };
            if t.has_edge(child, other, None) {
                let other_cut_value = t
                    .edge(child, other, None)
                    .expect("tree edge")
                    .borrow()
                    .cutvalue;
                cut_value += if points_to_head {
                    -other_cut_value
                } else {
                    other_cut_value
                };
            }
        }
    }

    cut_value
}

fn init_low_lim_values(tree: &Tree, root: Option<&str>) {
    let root = match root {
        Some(r) => r.to_owned(),
        None => tree.nodes()[0].clone(),
    };
    let mut visited = HashSet::new();
    dfs_assign_low_lim(tree, &mut visited, 1.0, &root, None);
}

fn dfs_assign_low_lim(
    tree: &Tree,
    visited: &mut HashSet<String>,
    mut next_lim: f64,
    v: &str,
    parent: Option<&str>,
) -> f64 {
    let low = next_lim;
    let label = tree.node(v).expect("tree node");

    visited.insert(v.to_owned());
    for w in tree.neighbors(v) {
        if !visited.contains(&w) {
            next_lim = dfs_assign_low_lim(tree, visited, next_lim, &w, Some(v));
        }
    }

    {
        let mut label = label.borrow_mut();
        label.low = low;
        label.lim = next_lim;
        label.parent = parent.map(str::to_owned);
    }
    next_lim + 1.0
}

/// First tree edge with negative cut value, if any.
fn leave_edge(tree: &Tree) -> Option<EdgeObj> {
    tree.edges()
        .into_iter()
        .find(|e| tree.edge_for(e).expect("tree edge").borrow().cutvalue < 0.0)
}

fn enter_edge(t: &Tree, g: &LayoutGraph, edge: &EdgeObj) -> Option<EdgeObj> {
    let mut v = edge.v.clone();
    let mut w = edge.w.clone();

    // For the rest of this function we assume that v is the tail and w is the
    // head, so if we don't have this edge in the graph we should flip it to
    // match the correct orientation.
    if !g.has_edge(&v, &w, None) {
        std::mem::swap(&mut v, &mut w);
    }

    let v_label = t.node(&v).expect("tree node");
    let w_label = t.node(&w).expect("tree node");
    let (mut tail_low, mut tail_lim) = {
        let l = v_label.borrow();
        (l.low, l.lim)
    };
    let mut flip = false;

    // If the root is in the tail of the edge then we need to flip the logic
    // that checks for the head and tail nodes in the candidates function.
    let w_lim = w_label.borrow().lim;
    if v_label.borrow().lim > w_lim {
        let l = w_label.borrow();
        tail_low = l.low;
        tail_lim = l.lim;
        flip = true;
    }

    let mut best: Option<(f64, EdgeObj)> = None;
    for candidate in g.edges() {
        let v_desc = {
            let l = t.node(&candidate.v).expect("tree node");
            let l = l.borrow();
            tail_low <= l.lim && l.lim <= tail_lim
        };
        let w_desc = {
            let l = t.node(&candidate.w).expect("tree node");
            let l = l.borrow();
            tail_low <= l.lim && l.lim <= tail_lim
        };
        if flip == v_desc && flip != w_desc {
            let s = slack(g, &candidate);
            if best.as_ref().is_none_or(|(b, _)| s < *b) {
                best = Some((s, candidate));
            }
        }
    }
    best.map(|(_, e)| e)
}

fn exchange_edges(t: &mut Tree, g: &LayoutGraph, e: &EdgeObj, f: &EdgeObj) {
    t.remove_edge(&e.v, &e.w, None);
    t.set_edge(&f.v, &f.w, Rc::new(RefCell::new(TreeEdge::default())), None);
    init_low_lim_values(t, None);
    init_cut_values(t, g);
    update_ranks(t, g);
}

fn update_ranks(t: &Tree, g: &LayoutGraph) {
    // JS looks for the first node whose *graph* label lacks a `parent`
    // property; dagre never sets one on graph labels, so this is always the
    // first tree node.
    let root = t.nodes()[0].clone();
    let mut vs = alg::preorder(t, &[root]);
    let rest = vs.split_off(1);
    for v in rest {
        let parent = t
            .node(&v)
            .expect("tree node")
            .borrow()
            .parent
            .clone()
            .expect("tree parent");
        let mut edge = g.edge(&v, &parent, None);
        let mut flipped = false;
        if edge.is_none() {
            edge = g.edge(&parent, &v, None);
            flipped = true;
        }
        let minlen = edge.expect("graph edge").borrow().minlen;
        let parent_rank = g.node(&parent).expect("node").borrow().rank.expect("rank");
        g.node(&v).expect("node").borrow_mut().rank =
            Some(parent_rank + if flipped { minlen } else { -minlen });
    }
}
