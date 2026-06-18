//! Port of dagre-d3-es `src/dagre/order/`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::graphlib::{Graph, GraphOptions};
use crate::jsmap::JsMap;

use super::types::{LayoutGraph, NodeLabelRef, UniqueId};
use super::util;

/// Node label in a layer graph: movable/fixed nodes alias the main graph's
/// labels; subgraph nodes carry per-rank border info.
#[derive(Debug, Clone)]
enum LayerNode {
    Alias(NodeLabelRef),
    Sub {
        border_left: Option<String>,
        border_right: Option<String>,
    },
}

#[derive(Debug, Clone)]
struct LayerEdge {
    weight: f64,
}

#[derive(Debug, Clone)]
struct LayerGraphLabel {
    root: String,
}

type LayerGraph = Graph<LayerGraphLabel, LayerNode, LayerEdge>;
/// The constraint graph: edges only, no labels.
type ConstraintGraph = Graph<(), (), ()>;

#[derive(Debug, Clone, Copy)]
enum Relationship {
    InEdges,
    OutEdges,
}

/// Applies crossing-minimization and assigns an `order` to every node.
pub fn order(g: &LayoutGraph, ids: &UniqueId) {
    let max_rank = util::max_rank(g).expect("ranked graph") as i64;
    let down_layer_graphs: Vec<LayerGraph> = (1..=max_rank)
        .map(|rank| build_layer_graph(g, rank, Relationship::InEdges, ids))
        .collect();
    let up_layer_graphs: Vec<LayerGraph> = (0..max_rank)
        .rev()
        .map(|rank| build_layer_graph(g, rank, Relationship::OutEdges, ids))
        .collect();

    let layering = init_order(g);
    assign_order(g, &layering);

    let mut best_cc = f64::INFINITY;
    let mut best: Option<Vec<Vec<String>>> = None;

    let mut i: u64 = 0;
    let mut last_best: u32 = 0;
    while last_best < 4 {
        sweep_layer_graphs(
            if i % 2 == 1 {
                &down_layer_graphs
            } else {
                &up_layer_graphs
            },
            i % 4 >= 2,
        );

        let layering = util::build_layer_matrix(g);
        let cc = cross_count(g, &layering);
        if cc < best_cc {
            last_best = 0;
            best = Some(layering);
            best_cc = cc;
        }
        i += 1;
        last_best += 1;
    }

    assign_order(g, &best.expect("at least one sweep"));
}

fn sweep_layer_graphs(layer_graphs: &[LayerGraph], bias_right: bool) {
    let mut cg: ConstraintGraph = Graph::new(GraphOptions::default());
    for lg in layer_graphs {
        let root = lg.graph().root;
        let sorted = sort_subgraph(lg, &root, &cg, bias_right);
        for (i, v) in sorted.vs.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            if let Some(LayerNode::Alias(label)) = lg.node(v) {
                label.borrow_mut().order = Some(i as f64);
            }
        }
        add_subgraph_constraints(lg, &mut cg, &sorted.vs);
    }
}

fn assign_order(g: &LayoutGraph, layering: &[Vec<String>]) {
    for layer in layering {
        for (i, v) in layer.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                g.node(v).expect("node").borrow_mut().order = Some(i as f64);
            }
        }
    }
}

/// Port of `init-order.js`: DFS from the lowest-rank simple nodes.
fn init_order(g: &LayoutGraph) -> Vec<Vec<String>> {
    let mut visited: HashSet<String> = HashSet::new();
    let simple_nodes: Vec<String> = g
        .nodes()
        .into_iter()
        .filter(|v| g.children(Some(v)).is_empty())
        .collect();
    let max_rank = simple_nodes
        .iter()
        .filter_map(|v| g.node(v).expect("node").borrow().rank)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_rank as usize + 1];

    fn dfs(g: &LayoutGraph, v: &str, visited: &mut HashSet<String>, layers: &mut [Vec<String>]) {
        if visited.contains(v) {
            return;
        }
        visited.insert(v.to_owned());
        let rank = g.node(v).expect("node").borrow().rank.expect("rank") as usize;
        layers[rank].push(v.to_owned());
        for w in g.successors(v) {
            dfs(g, &w, visited, layers);
        }
    }

    let mut ordered_vs = simple_nodes;
    ordered_vs.sort_by(|a, b| {
        let ra = g.node(a).expect("node").borrow().rank.expect("rank");
        let rb = g.node(b).expect("node").borrow().rank.expect("rank");
        ra.partial_cmp(&rb).expect("non-NaN rank")
    });
    for v in &ordered_vs {
        dfs(g, v, &mut visited, &mut layers);
    }

    layers
}

/// Port of `build-layer-graph.js`.
fn build_layer_graph(
    g: &LayoutGraph,
    rank: i64,
    relationship: Relationship,
    ids: &UniqueId,
) -> LayerGraph {
    let root = create_root_node(g, ids);
    let mut result: LayerGraph = Graph::new(GraphOptions {
        compound: Some(true),
        ..Default::default()
    });
    result.set_graph(LayerGraphLabel { root: root.clone() });
    // Default node labels alias the main graph's labels (a snapshot of Rc
    // handles; mutations flow through, as JS object references do).
    let labels: HashMap<String, NodeLabelRef> = g
        .nodes()
        .into_iter()
        .filter_map(|v| g.node(&v).map(|l| (v, l)))
        .collect();
    result.set_default_node_label(move |v| labels.get(v).cloned().map(LayerNode::Alias));

    #[allow(clippy::cast_precision_loss)]
    let rank_f = rank as f64;
    for v in g.nodes() {
        let node = g.node(&v).expect("node label");
        let (node_rank, min_rank, max_rank) = {
            let n = node.borrow();
            (n.rank, n.min_rank, n.max_rank)
        };
        let in_span = match (min_rank, max_rank) {
            (Some(min), Some(max)) => min <= rank_f && rank_f <= max,
            _ => false,
        };
        if node_rank == Some(rank_f) || in_span {
            result.set_node(&v);
            let parent = g.parent(&v);
            result.set_parent(&v, Some(parent.as_deref().unwrap_or(&root)));

            // This assumes we have only short edges!
            let edges = match relationship {
                Relationship::InEdges => g.in_edges(&v, None),
                Relationship::OutEdges => g.out_edges(&v, None),
            };
            for e in edges {
                let u = if e.v == v { e.w.clone() } else { e.v.clone() };
                let prev_weight = result.edge(&u, &v, None).map_or(0.0, |l| l.weight);
                let weight = g.edge_for(&e).expect("edge label").borrow().weight;
                result.set_edge(
                    &u,
                    &v,
                    LayerEdge {
                        weight: weight + prev_weight,
                    },
                    None,
                );
            }

            if min_rank.is_some() {
                let n = node.borrow();
                result.set_node_with(
                    &v,
                    LayerNode::Sub {
                        border_left: n.border_left.get(&rank).cloned(),
                        border_right: n.border_right.get(&rank).cloned(),
                    },
                );
            }
        }
    }

    result
}

fn create_root_node(g: &LayoutGraph, ids: &UniqueId) -> String {
    loop {
        let v = ids.next("_root");
        if !g.has_node(&v) {
            return v;
        }
    }
}

/// Port of `cross-count.js` (Barth et al. bilayer cross counting).
fn cross_count(g: &LayoutGraph, layering: &[Vec<String>]) -> f64 {
    let mut cc = 0.0;
    for i in 1..layering.len() {
        cc += two_layer_cross_count(g, &layering[i - 1], &layering[i]);
    }
    cc
}

fn two_layer_cross_count(g: &LayoutGraph, north_layer: &[String], south_layer: &[String]) -> f64 {
    let south_pos: HashMap<&String, usize> = south_layer
        .iter()
        .enumerate()
        .map(|(i, v)| (v, i))
        .collect();
    let mut south_entries: Vec<(usize, f64)> = Vec::new();
    for v in north_layer {
        let mut entries: Vec<(usize, f64)> = g
            .out_edges(v, None)
            .iter()
            .map(|e| {
                let pos = *south_pos.get(&e.w).expect("short edge into south layer");
                let weight = g.edge_for(e).expect("edge label").borrow().weight;
                (pos, weight)
            })
            .collect();
        entries.sort_by_key(|&(pos, _)| pos);
        south_entries.extend(entries);
    }

    // Build the accumulator tree.
    let mut first_index: usize = 1;
    while first_index < south_layer.len() {
        first_index <<= 1;
    }
    let tree_size = 2 * first_index - 1;
    first_index -= 1;
    let mut tree = vec![0.0_f64; tree_size];

    // Calculate the weighted crossings.
    let mut cc = 0.0;
    for (pos, weight) in south_entries {
        let mut index = pos + first_index;
        tree[index] += weight;
        let mut weight_sum = 0.0;
        while index > 0 {
            if index % 2 == 1 {
                weight_sum += tree[index + 1];
            }
            index = (index - 1) >> 1;
            tree[index] += weight;
        }
        cc += weight * weight_sum;
    }

    cc
}

/// Barycenter entry prior to conflict resolution.
#[derive(Debug, Clone)]
struct BaryEntry {
    v: String,
    barycenter: Option<f64>,
    weight: Option<f64>,
}

/// Port of `barycenter.js`.
fn barycenter(g: &LayerGraph, movable: &[String]) -> Vec<BaryEntry> {
    movable
        .iter()
        .map(|v| {
            let in_v = g.in_edges(v, None);
            if in_v.is_empty() {
                BaryEntry {
                    v: v.clone(),
                    barycenter: None,
                    weight: None,
                }
            } else {
                let mut sum = 0.0;
                let mut weight = 0.0;
                for e in &in_v {
                    let edge_weight = g.edge_for(e).expect("edge label").weight;
                    let order = match g.node(&e.v).expect("fixed node") {
                        LayerNode::Alias(label) => label.borrow().order.expect("order"),
                        LayerNode::Sub { .. } => unreachable!("subgraph as edge endpoint"),
                    };
                    sum += edge_weight * order;
                    weight += edge_weight;
                }
                BaryEntry {
                    v: v.clone(),
                    barycenter: Some(sum / weight),
                    weight: Some(weight),
                }
            }
        })
        .collect()
}

#[derive(Debug)]
struct ConflictEntry {
    indegree: i64,
    in_: Vec<Rc<RefCell<ConflictEntry>>>,
    out: Vec<Rc<RefCell<ConflictEntry>>>,
    vs: Vec<String>,
    i: i64,
    barycenter: Option<f64>,
    weight: Option<f64>,
    merged: bool,
}

/// Entry produced by conflict resolution and consumed by `sort`.
#[derive(Debug, Clone)]
struct SortEntry {
    vs: Vec<String>,
    i: i64,
    barycenter: Option<f64>,
    weight: Option<f64>,
}

/// Port of `resolve-conflicts.js` (Forster constrained crossing reduction).
fn resolve_conflicts(entries: &[BaryEntry], cg: &ConstraintGraph) -> Vec<SortEntry> {
    let mut mapped_entries: JsMap<Rc<RefCell<ConflictEntry>>> = JsMap::new();
    for (i, entry) in entries.iter().enumerate() {
        mapped_entries.insert(
            entry.v.clone(),
            Rc::new(RefCell::new(ConflictEntry {
                indegree: 0,
                in_: Vec::new(),
                out: Vec::new(),
                vs: vec![entry.v.clone()],
                i: i64::try_from(i).expect("entry index"),
                barycenter: entry.barycenter,
                weight: entry.weight,
                merged: false,
            })),
        );
    }

    for e in cg.edges() {
        let entry_v = mapped_entries.get(&e.v).cloned();
        let entry_w = mapped_entries.get(&e.w).cloned();
        if let (Some(entry_v), Some(entry_w)) = (entry_v, entry_w) {
            entry_w.borrow_mut().indegree += 1;
            entry_v.borrow_mut().out.push(entry_w);
        }
    }

    let source_set: Vec<Rc<RefCell<ConflictEntry>>> = mapped_entries
        .values()
        .into_iter()
        .filter(|entry| entry.borrow().indegree == 0)
        .cloned()
        .collect();

    do_resolve_conflicts(source_set)
}

fn do_resolve_conflicts(mut source_set: Vec<Rc<RefCell<ConflictEntry>>>) -> Vec<SortEntry> {
    let mut entries: Vec<Rc<RefCell<ConflictEntry>>> = Vec::new();

    while let Some(entry) = source_set.pop() {
        entries.push(entry.clone());
        // JS reverses the `in` list in place before iterating.
        let in_list: Vec<Rc<RefCell<ConflictEntry>>> = {
            let mut e = entry.borrow_mut();
            e.in_.reverse();
            e.in_.clone()
        };
        for u_entry in in_list {
            handle_in(&entry, &u_entry);
        }
        let out_list: Vec<Rc<RefCell<ConflictEntry>>> = entry.borrow().out.clone();
        for w_entry in out_list {
            w_entry.borrow_mut().in_.push(entry.clone());
            let ready = {
                let mut w = w_entry.borrow_mut();
                w.indegree -= 1;
                w.indegree == 0
            };
            if ready {
                source_set.push(w_entry);
            }
        }
    }

    entries
        .iter()
        .filter(|entry| !entry.borrow().merged)
        .map(|entry| {
            let e = entry.borrow();
            SortEntry {
                vs: e.vs.clone(),
                i: e.i,
                barycenter: e.barycenter,
                weight: e.weight,
            }
        })
        .collect()
}

fn handle_in(v_entry: &Rc<RefCell<ConflictEntry>>, u_entry: &Rc<RefCell<ConflictEntry>>) {
    if u_entry.borrow().merged {
        return;
    }
    let merge = {
        let u = u_entry.borrow();
        let v = v_entry.borrow();
        u.barycenter.is_none()
            || v.barycenter.is_none()
            || u.barycenter.expect("checked") >= v.barycenter.expect("checked")
    };
    if merge {
        merge_entries(v_entry, u_entry);
    }
}

fn merge_entries(target: &Rc<RefCell<ConflictEntry>>, source: &Rc<RefCell<ConflictEntry>>) {
    let mut sum = 0.0;
    let mut weight = 0.0;

    {
        let t = target.borrow();
        // JS truthiness: a weight of 0 (or absent) contributes nothing.
        if let Some(w) = t.weight
            && w != 0.0
        {
            sum += t.barycenter.expect("barycenter with weight") * w;
            weight += w;
        }
    }
    {
        let s = source.borrow();
        if let Some(w) = s.weight
            && w != 0.0
        {
            sum += s.barycenter.expect("barycenter with weight") * w;
            weight += w;
        }
    }

    let mut t = target.borrow_mut();
    let mut s = source.borrow_mut();
    let mut vs = s.vs.clone();
    vs.extend(t.vs.iter().cloned());
    t.vs = vs;
    t.barycenter = Some(sum / weight);
    t.weight = Some(weight);
    t.i = t.i.min(s.i);
    s.merged = true;
}

/// Result of sorting a subgraph's movable nodes.
#[derive(Debug, Clone)]
struct SortResult {
    vs: Vec<String>,
    barycenter: Option<f64>,
    weight: Option<f64>,
}

/// Port of `sort-subgraph.js`.
fn sort_subgraph(g: &LayerGraph, v: &str, cg: &ConstraintGraph, bias_right: bool) -> SortResult {
    let mut movable = g.children(Some(v));
    let node = g.node(v);
    let (bl, br) = match node {
        Some(LayerNode::Sub {
            border_left,
            border_right,
        }) => (border_left, border_right),
        _ => (None, None),
    };
    let mut subgraphs: HashMap<String, SortResult> = HashMap::new();

    if let Some(bl) = &bl {
        movable.retain(|w| w != bl && Some(w) != br.as_ref());
    }

    let mut barycenters = barycenter(g, &movable);
    for entry in &mut barycenters {
        if !g.children(Some(&entry.v)).is_empty() {
            let subgraph_result = sort_subgraph(g, &entry.v, cg, bias_right);
            if subgraph_result.barycenter.is_some() {
                merge_barycenters(entry, &subgraph_result);
            }
            subgraphs.insert(entry.v.clone(), subgraph_result);
        }
    }

    let mut entries = resolve_conflicts(&barycenters, cg);
    expand_subgraphs(&mut entries, &subgraphs);

    let mut result = sort(entries, bias_right);

    if let Some(bl) = &bl {
        let br = br.as_ref().expect("borderRight with borderLeft");
        let mut vs = vec![bl.clone()];
        vs.extend(result.vs);
        vs.push(br.clone());
        result.vs = vs;

        let bl_preds = g.predecessors(bl);
        if !bl_preds.is_empty() {
            let order_of = |name: &str| -> f64 {
                match g.node(name).expect("border predecessor") {
                    LayerNode::Alias(label) => label.borrow().order.expect("order"),
                    LayerNode::Sub { .. } => unreachable!("subgraph as border predecessor"),
                }
            };
            let bl_pred_order = order_of(&bl_preds[0]);
            let br_pred_order = order_of(&g.predecessors(br)[0]);
            if result.barycenter.is_none() {
                result.barycenter = Some(0.0);
                result.weight = Some(0.0);
            }
            let bc = result.barycenter.expect("barycenter");
            let w = result.weight.expect("weight");
            result.barycenter = Some((bc * w + bl_pred_order + br_pred_order) / (w + 2.0));
            result.weight = Some(w + 2.0);
        }
    }

    result
}

fn expand_subgraphs(entries: &mut [SortEntry], subgraphs: &HashMap<String, SortResult>) {
    for entry in entries {
        let mut vs = Vec::new();
        for v in &entry.vs {
            if let Some(sub) = subgraphs.get(v) {
                vs.extend(sub.vs.iter().cloned());
            } else {
                vs.push(v.clone());
            }
        }
        entry.vs = vs;
    }
}

fn merge_barycenters(target: &mut BaryEntry, other: &SortResult) {
    let other_bc = other.barycenter.expect("caller checked");
    let other_w = other.weight.expect("weight with barycenter");
    if let Some(target_bc) = target.barycenter {
        let target_w = target.weight.expect("weight with barycenter");
        target.barycenter =
            Some((target_bc * target_w + other_bc * other_w) / (target_w + other_w));
        target.weight = Some(target_w + other_w);
    } else {
        target.barycenter = Some(other_bc);
        target.weight = Some(other_w);
    }
}

/// Port of `sort.js`.
fn sort(entries: Vec<SortEntry>, bias_right: bool) -> SortResult {
    let (mut sortable, mut unsortable): (Vec<SortEntry>, Vec<SortEntry>) = entries
        .into_iter()
        .partition(|entry| entry.barycenter.is_some());
    // lodash sortBy -entry.i ascending == by i descending (stable).
    unsortable.sort_by_key(|entry| std::cmp::Reverse(entry.i));

    let mut vs: Vec<Vec<String>> = Vec::new();
    let mut sum = 0.0;
    let mut weight = 0.0;
    let mut vs_index: i64 = 0;

    sortable.sort_by(|entry_v, entry_w| {
        let bv = entry_v.barycenter.expect("sortable");
        let bw = entry_w.barycenter.expect("sortable");
        if bv < bw {
            std::cmp::Ordering::Less
        } else if bv > bw {
            std::cmp::Ordering::Greater
        } else if bias_right {
            entry_w.i.cmp(&entry_v.i)
        } else {
            entry_v.i.cmp(&entry_w.i)
        }
    });

    vs_index = consume_unsortable(&mut vs, &mut unsortable, vs_index);

    for entry in sortable {
        vs_index += i64::try_from(entry.vs.len()).expect("vs length");
        vs.push(entry.vs.clone());
        sum += entry.barycenter.expect("sortable") * entry.weight.expect("weight");
        weight += entry.weight.expect("weight");
        vs_index = consume_unsortable(&mut vs, &mut unsortable, vs_index);
    }

    let mut result = SortResult {
        vs: vs.into_iter().flatten().collect(),
        barycenter: None,
        weight: None,
    };
    if weight != 0.0 {
        result.barycenter = Some(sum / weight);
        result.weight = Some(weight);
    }
    result
}

fn consume_unsortable(
    vs: &mut Vec<Vec<String>>,
    unsortable: &mut Vec<SortEntry>,
    mut index: i64,
) -> i64 {
    while let Some(last) = unsortable.last() {
        if last.i > index {
            break;
        }
        let last = unsortable.pop().expect("checked last");
        vs.push(last.vs);
        index += 1;
    }
    index
}

/// Port of `add-subgraph-constraints.js`.
fn add_subgraph_constraints(g: &LayerGraph, cg: &mut ConstraintGraph, vs: &[String]) {
    let mut prev: HashMap<String, String> = HashMap::new();
    let mut root_prev: Option<String> = None;

    for v in vs {
        let mut child = g.parent(v);
        while let Some(c) = child {
            let parent = g.parent(&c);
            let prev_child = if let Some(p) = &parent {
                prev.insert(p.clone(), c.clone())
            } else {
                root_prev.replace(c.clone())
            };
            if let Some(pc) = prev_child
                && pc != c
            {
                cg.set_edge_default(&pc, &c);
                break;
            }
            child = parent;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dagre::types::{EdgeLabel, GraphLabel, NodeLabel, edge_ref, node_ref};
    use crate::graphlib::GraphOptions;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn graph() -> LayoutGraph {
        let mut g: LayoutGraph = Graph::new(GraphOptions {
            multigraph: Some(true),
            compound: Some(true),
            ..Default::default()
        });
        g.set_graph(Rc::new(RefCell::new(GraphLabel::default())));
        g
    }

    fn n(g: &mut LayoutGraph, id: &str) {
        g.set_node_with(id, node_ref(NodeLabel::default()));
    }

    fn e(g: &mut LayoutGraph, v: &str, w: &str) {
        g.set_edge(v, w, edge_ref(EdgeLabel::default()), None);
    }

    fn layer(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parallel_edges_have_no_crossings() {
        let mut g = graph();
        for id in ["a", "b", "c", "d"] {
            n(&mut g, id);
        }
        e(&mut g, "a", "c");
        e(&mut g, "b", "d");
        let cc = two_layer_cross_count(&g, &layer(&["a", "b"]), &layer(&["c", "d"]));
        assert_eq!(cc, 0.0);
    }

    #[test]
    fn inverted_edges_cross_once() {
        let mut g = graph();
        for id in ["a", "b", "c", "d"] {
            n(&mut g, id);
        }
        e(&mut g, "a", "d");
        e(&mut g, "b", "c");
        let cc = two_layer_cross_count(&g, &layer(&["a", "b"]), &layer(&["c", "d"]));
        assert_eq!(cc, 1.0);
    }

    #[test]
    fn full_bipartite_counts_one_crossing() {
        // a,b -> c,d (all four edges): exactly one inversion (a-d x b-c).
        let mut g = graph();
        for id in ["a", "b", "c", "d"] {
            n(&mut g, id);
        }
        e(&mut g, "a", "c");
        e(&mut g, "a", "d");
        e(&mut g, "b", "c");
        e(&mut g, "b", "d");
        let cc = two_layer_cross_count(&g, &layer(&["a", "b"]), &layer(&["c", "d"]));
        assert_eq!(cc, 1.0);
    }

    #[test]
    fn cross_count_sums_over_adjacent_layers() {
        let mut g = graph();
        for id in ["a", "b", "c", "d", "e", "f"] {
            n(&mut g, id);
        }
        // Layer0 [a,b] -> Layer1 [c,d]: crossing. Layer1 -> Layer2 [e,f]: none.
        e(&mut g, "a", "d");
        e(&mut g, "b", "c");
        e(&mut g, "c", "e");
        e(&mut g, "d", "f");
        let layering = [layer(&["a", "b"]), layer(&["c", "d"]), layer(&["e", "f"])];
        assert_eq!(cross_count(&g, &layering), 1.0);
    }
}
