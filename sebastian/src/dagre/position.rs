//! Port of dagre-d3-es `src/dagre/position/` (Brandes-Köpf coordinate
//! assignment plus the y-pass).

use std::collections::{HashMap, HashSet};

use crate::graphlib::{Graph, GraphOptions};

use super::types::{BorderType, LayoutGraph, js_gt, js_lt, js_math_max, js_math_min, js_str_gt};
use super::util;

type Conflicts = HashMap<String, HashSet<String>>;

pub fn position(g: &LayoutGraph) {
    let g = util::as_non_compound_graph(g);

    position_y(&g);
    for (v, x) in position_x(&g) {
        g.node(&v).expect("node").borrow_mut().x = Some(x);
    }
}

fn position_y(g: &LayoutGraph) {
    let layering = util::build_layer_matrix(g);
    let rank_sep = g.graph().borrow().ranksep;
    let mut prev_y = 0.0;
    for layer in layering {
        let max_height = layer
            .iter()
            .map(|v| g.node(v).expect("node").borrow().height)
            .fold(f64::NEG_INFINITY, f64::max);
        for v in &layer {
            g.node(v).expect("node").borrow_mut().y = Some(prev_y + max_height / 2.0);
        }
        prev_y += max_height + rank_sep;
    }
}

fn add_conflict(conflicts: &mut Conflicts, v: &str, w: &str) {
    let (v, w) = if js_str_gt(v, w) { (w, v) } else { (v, w) };
    conflicts
        .entry(v.to_owned())
        .or_default()
        .insert(w.to_owned());
}

fn has_conflict(conflicts: &Conflicts, v: &str, w: &str) -> bool {
    let (v, w) = if js_str_gt(v, w) { (w, v) } else { (v, w) };
    conflicts.get(v).is_some_and(|set| set.contains(w))
}

/// Type-1 conflicts: non-inner segments crossing inner segments.
fn find_type1_conflicts(g: &LayoutGraph, layering: &[Vec<String>]) -> Conflicts {
    let mut conflicts = Conflicts::new();

    let mut visit_layer = |prev_layer: &[String], layer: &[String]| {
        // Last visited node in the previous layer that is incident on an
        // inner segment.
        let mut k0: f64 = 0.0;
        // Last node in this layer scanned for crossings with a type-1 segment.
        let mut scan_pos: usize = 0;
        #[allow(clippy::cast_precision_loss)]
        let prev_layer_length = prev_layer.len() as f64;
        let last_node = layer.last();

        for (i, v) in layer.iter().enumerate() {
            let w = find_other_inner_segment_node(g, v);
            let k1 = w.as_ref().map_or(prev_layer_length, |w| {
                g.node(w).expect("node").borrow().order.expect("order")
            });

            if w.is_some() || Some(v) == last_node {
                for scan_node in &layer[scan_pos..=i] {
                    for u in g.predecessors(scan_node) {
                        let u_label = g.node(&u).expect("node");
                        let (u_pos, u_dummy) = {
                            let l = u_label.borrow();
                            (l.order.expect("order"), l.dummy.is_some())
                        };
                        let scan_dummy = g.node(scan_node).expect("node").borrow().dummy.is_some();
                        if (u_pos < k0 || k1 < u_pos) && !(u_dummy && scan_dummy) {
                            add_conflict(&mut conflicts, &u, scan_node);
                        }
                    }
                }
                scan_pos = i + 1;
                k0 = k1;
            }
        }
    };

    // _.reduce(layering, visitLayer) — pairwise over consecutive layers.
    for pair in layering.windows(2) {
        visit_layer(&pair[0], &pair[1]);
    }
    conflicts
}

/// Type-2 conflicts: crossing inner segments (border-related).
fn find_type2_conflicts(g: &LayoutGraph, layering: &[Vec<String>]) -> Conflicts {
    let mut conflicts = Conflicts::new();

    fn scan(
        g: &LayoutGraph,
        conflicts: &mut Conflicts,
        south: &[String],
        south_pos: usize,
        south_end: usize,
        prev_north_border: Option<f64>,
        next_north_border: Option<f64>,
    ) {
        for v in &south[south_pos..south_end] {
            if g.node(v).expect("node").borrow().dummy.is_some() {
                for u in g.predecessors(v) {
                    let u_node = g.node(&u).expect("node");
                    let (order, dummy) = {
                        let l = u_node.borrow();
                        (l.order, l.dummy.is_some())
                    };
                    if dummy && (js_lt(order, prev_north_border) || js_gt(order, next_north_border))
                    {
                        add_conflict(conflicts, &u, v);
                    }
                }
            }
        }
    }

    let mut visit_layer = |north: &[String], south: &[String]| {
        let mut prev_north_pos: Option<f64> = Some(-1.0);
        let mut next_north_pos: Option<f64> = None;
        let mut south_pos: usize = 0;

        for (south_lookahead, v) in south.iter().enumerate() {
            let is_border = {
                let label = g.node(v).expect("node");
                let label = label.borrow();
                label.dummy == Some(super::types::Dummy::Border)
            };
            if is_border {
                let predecessors = g.predecessors(v);
                if !predecessors.is_empty() {
                    next_north_pos = g.node(&predecessors[0]).expect("node").borrow().order;
                    scan(
                        g,
                        &mut conflicts,
                        south,
                        south_pos,
                        south_lookahead,
                        prev_north_pos,
                        next_north_pos,
                    );
                    south_pos = south_lookahead;
                    prev_north_pos = next_north_pos;
                }
            }
            // JS calls this for every v in south (inside the forEach).
            #[allow(clippy::cast_precision_loss)]
            scan(
                g,
                &mut conflicts,
                south,
                south_pos,
                south.len(),
                next_north_pos,
                Some(north.len() as f64),
            );
        }
    };

    for pair in layering.windows(2) {
        visit_layer(&pair[0], &pair[1]);
    }
    conflicts
}

fn find_other_inner_segment_node(g: &LayoutGraph, v: &str) -> Option<String> {
    if g.node(v).expect("node").borrow().dummy.is_some() {
        return g
            .predecessors(v)
            .into_iter()
            .find(|u| g.node(u).expect("node").borrow().dummy.is_some());
    }
    None
}

struct Alignment {
    root: HashMap<String, String>,
    align: HashMap<String, String>,
}

/// Aligns nodes into vertical blocks with their median neighbors.
fn vertical_alignment(
    _g: &LayoutGraph,
    layering: &[Vec<String>],
    conflicts: &Conflicts,
    neighbor_fn: impl Fn(&str) -> Vec<String>,
) -> Alignment {
    let mut root: HashMap<String, String> = HashMap::new();
    let mut align: HashMap<String, String> = HashMap::new();
    let mut pos: HashMap<String, f64> = HashMap::new();

    // Cache position based on the layering, since the graph and layering may
    // be out of sync (the layering matrix is manipulated for the four
    // extreme alignments).
    for layer in layering {
        for (order, v) in layer.iter().enumerate() {
            root.insert(v.clone(), v.clone());
            align.insert(v.clone(), v.clone());
            #[allow(clippy::cast_precision_loss)]
            pos.insert(v.clone(), order as f64);
        }
    }

    for layer in layering {
        let mut prev_idx = -1.0;
        for v in layer {
            let mut ws = neighbor_fn(v);
            if !ws.is_empty() {
                ws.sort_by(|a, b| pos[a].partial_cmp(&pos[b]).expect("non-NaN position"));
                #[allow(clippy::cast_precision_loss)]
                let mp = (ws.len() as f64 - 1.0) / 2.0;
                let mut i = mp.floor() as usize;
                let il = mp.ceil() as usize;
                while i <= il {
                    let w = &ws[i];
                    if align[v] == *v && prev_idx < pos[w] && !has_conflict(conflicts, v, w) {
                        align.insert(w.clone(), v.clone());
                        let root_w = root[w].clone();
                        root.insert(v.clone(), root_w.clone());
                        align.insert(v.clone(), root_w);
                        prev_idx = pos[w];
                    }
                    i += 1;
                }
            }
        }
    }

    Alignment { root, align }
}

type BlockGraph = Graph<(), (), f64>;

fn horizontal_compaction(
    g: &LayoutGraph,
    layering: &[Vec<String>],
    root: &HashMap<String, String>,
    align: &HashMap<String, String>,
    reverse_sep: bool,
) -> HashMap<String, f64> {
    // We construct a block graph and do two sweeps: place blocks with the
    // smallest possible coordinates, then remove unused space by moving
    // blocks to the greatest coordinates without violating separation.
    let mut xs: HashMap<String, f64> = HashMap::new();
    let block_g = build_block_graph(g, layering, root, reverse_sep);
    let border_type = if reverse_sep {
        BorderType::BorderLeft
    } else {
        BorderType::BorderRight
    };

    fn iterate(
        block_g: &BlockGraph,
        set_xs: &mut impl FnMut(&str),
        next_nodes: impl Fn(&str) -> Vec<String>,
    ) {
        let mut stack = block_g.nodes();
        let mut visited: HashSet<String> = HashSet::new();
        while let Some(elem) = stack.pop() {
            if visited.contains(&elem) {
                set_xs(&elem);
            } else {
                visited.insert(elem.clone());
                stack.push(elem.clone());
                stack.extend(next_nodes(&elem));
            }
        }
    }

    // First pass, assign smallest coordinates.
    {
        let mut pass1 = |elem: &str| {
            let x = block_g.in_edges(elem, None).iter().fold(0.0, |acc, e| {
                let xv = xs.get(&e.v).copied().unwrap_or(f64::NAN);
                let sep = block_g.edge_for(e).expect("block edge");
                js_math_max(acc, xv + sep)
            });
            xs.insert(elem.to_owned(), x);
        };
        iterate(&block_g, &mut pass1, |v| block_g.predecessors(v));
    }

    // Second pass, assign greatest coordinates.
    {
        let mut pass2 = |elem: &str| {
            let min = block_g
                .out_edges(elem, None)
                .iter()
                .fold(f64::INFINITY, |acc, e| {
                    let xw = xs.get(&e.w).copied().unwrap_or(f64::NAN);
                    let sep = block_g.edge_for(e).expect("block edge");
                    js_math_min(acc, xw - sep)
                });
            let node = g.node(elem).expect("node");
            let node_border_type = node.borrow().border_type;
            if min != f64::INFINITY && node_border_type != Some(border_type) {
                let current = xs.get(elem).copied().unwrap_or(f64::NAN);
                xs.insert(elem.to_owned(), js_math_max(current, min));
            }
        };
        iterate(&block_g, &mut pass2, |v| block_g.successors(v));
    }

    // Assign x coordinates to all nodes.
    for v in align.values() {
        let x = xs[&root[v]];
        xs.insert(v.clone(), x);
    }

    xs
}

fn build_block_graph(
    g: &LayoutGraph,
    layering: &[Vec<String>],
    root: &HashMap<String, String>,
    reverse_sep: bool,
) -> BlockGraph {
    let mut block_graph: BlockGraph = Graph::new(GraphOptions::default());
    let (nodesep, edgesep) = {
        let label = g.graph();
        let label = label.borrow();
        (label.nodesep, label.edgesep)
    };

    for layer in layering {
        let mut u: Option<&String> = None;
        for v in layer {
            let v_root = &root[v];
            block_graph.set_node(v_root);
            if let Some(u) = u {
                let u_root = &root[u];
                let prev_max = block_graph.edge(u_root, v_root, None);
                block_graph.set_edge(
                    u_root,
                    v_root,
                    sep(g, nodesep, edgesep, reverse_sep, v, u).max(prev_max.unwrap_or(0.0)),
                    None,
                );
            }
            u = Some(v);
        }
    }

    block_graph
}

/// Separation between two adjacent nodes in a layer (`sep` in bk.js).
fn sep(g: &LayoutGraph, node_sep: f64, edge_sep: f64, reverse_sep: bool, v: &str, w: &str) -> f64 {
    let v_label = g.node(v).expect("node");
    let w_label = g.node(w).expect("node");
    let v_label = v_label.borrow();
    let w_label = w_label.borrow();
    let mut sum = 0.0;

    sum += v_label.width / 2.0;
    let mut delta = 0.0;
    if let Some(labelpos) = &v_label.labelpos {
        match labelpos.to_lowercase().as_str() {
            "l" => delta = -v_label.width / 2.0,
            "r" => delta = v_label.width / 2.0,
            _ => {}
        }
    }
    if delta != 0.0 {
        sum += if reverse_sep { delta } else { -delta };
    }

    sum += if v_label.dummy.is_some() {
        edge_sep
    } else {
        node_sep
    } / 2.0;
    sum += if w_label.dummy.is_some() {
        edge_sep
    } else {
        node_sep
    } / 2.0;

    sum += w_label.width / 2.0;
    delta = 0.0;
    if let Some(labelpos) = &w_label.labelpos {
        match labelpos.to_lowercase().as_str() {
            "l" => delta = w_label.width / 2.0,
            "r" => delta = -w_label.width / 2.0,
            _ => {}
        }
    }
    if delta != 0.0 {
        sum += if reverse_sep { delta } else { -delta };
    }

    sum
}

const ALIGNMENTS: [&str; 4] = ["ul", "ur", "dl", "dr"];

/// Width of each alignment; returns the key of the narrowest (first wins ties).
fn find_smallest_width_alignment<'a>(
    g: &LayoutGraph,
    xss: &HashMap<&'a str, HashMap<String, f64>>,
) -> &'a str {
    let mut best: Option<(f64, &str)> = None;
    for key in ALIGNMENTS {
        let xs = &xss[key];
        let mut max = f64::NEG_INFINITY;
        let mut min = f64::INFINITY;
        for (v, x) in xs {
            let half_width = g.node(v).expect("node").borrow().width / 2.0;
            max = f64::max(x + half_width, max);
            min = f64::min(x - half_width, min);
        }
        let width = max - min;
        if best.is_none_or(|(b, _)| width < b) {
            best = Some((width, key));
        }
    }
    best.expect("four alignments").1
}

/// Shifts the other alignments so their extreme coordinates line up with the
/// narrowest alignment.
fn align_coordinates(xss: &mut HashMap<&str, HashMap<String, f64>>, align_to: &str) {
    let align_to_vals: Vec<f64> = xss[align_to].values().copied().collect();
    let align_to_min = align_to_vals.iter().copied().fold(f64::INFINITY, f64::min);
    let align_to_max = align_to_vals
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);

    for vert in ["u", "d"] {
        for horiz in ["l", "r"] {
            let alignment = format!("{vert}{horiz}");
            if alignment == align_to {
                continue;
            }
            let xs = xss.get_mut(alignment.as_str()).expect("alignment");
            let xs_vals: Vec<f64> = xs.values().copied().collect();
            let delta = if horiz == "l" {
                align_to_min - xs_vals.iter().copied().fold(f64::INFINITY, f64::min)
            } else {
                align_to_max - xs_vals.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            };
            // JS truthiness: 0 and NaN skip the shift.
            if delta != 0.0 && !delta.is_nan() {
                for x in xs.values_mut() {
                    *x += delta;
                }
            }
        }
    }
}

fn balance(xss: &HashMap<&str, HashMap<String, f64>>, align: Option<&str>) -> HashMap<String, f64> {
    let mut result = HashMap::new();
    for v in xss["ul"].keys() {
        let x = if let Some(align) = align {
            xss[align.to_lowercase().as_str()][v]
        } else {
            let mut vals: Vec<f64> = ALIGNMENTS.iter().map(|key| xss[*key][v]).collect();
            vals.sort_by(|a, b| a.partial_cmp(b).expect("non-NaN x"));
            f64::midpoint(vals[1], vals[2])
        };
        result.insert(v.clone(), x);
    }
    result
}

fn position_x(g: &LayoutGraph) -> HashMap<String, f64> {
    let layering = util::build_layer_matrix(g);
    let mut conflicts = find_type1_conflicts(g, &layering);
    for (v, ws) in find_type2_conflicts(g, &layering) {
        conflicts.entry(v).or_default().extend(ws);
    }

    let mut xss: HashMap<&str, HashMap<String, f64>> = HashMap::new();
    for vert in ["u", "d"] {
        let mut adjusted_layering: Vec<Vec<String>> = if vert == "u" {
            layering.clone()
        } else {
            layering.iter().rev().cloned().collect()
        };
        for horiz in ["l", "r"] {
            if horiz == "r" {
                adjusted_layering = adjusted_layering
                    .iter()
                    .map(|inner| inner.iter().rev().cloned().collect())
                    .collect();
            }

            let align = if vert == "u" {
                vertical_alignment(g, &adjusted_layering, &conflicts, |v| g.predecessors(v))
            } else {
                vertical_alignment(g, &adjusted_layering, &conflicts, |v| g.successors(v))
            };
            let mut xs = horizontal_compaction(
                g,
                &adjusted_layering,
                &align.root,
                &align.align,
                horiz == "r",
            );
            if horiz == "r" {
                for x in xs.values_mut() {
                    *x = -*x;
                }
            }
            let key = match (vert, horiz) {
                ("u", "l") => "ul",
                ("u", "r") => "ur",
                ("d", "l") => "dl",
                _ => "dr",
            };
            xss.insert(key, xs);
        }
    }

    let smallest_width = find_smallest_width_alignment(g, &xss);
    align_coordinates(&mut xss, smallest_width);
    let align = g.graph().borrow().align.clone();
    balance(&xss, align.as_deref())
}
