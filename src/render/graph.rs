//! Port of `mermaid-graphlib.js` — cluster handling on top of graphlib.

use indexmap::IndexMap;

use crate::graphlib::{EdgeObj, Graph, GraphOptions};

use super::data::{EdgeRef, NodeRef};

/// Graph label for the render graph (mirrors the object passed to setGraph).
#[derive(Debug, Clone)]
pub struct RenderGraphLabel {
    pub rankdir: String,
    pub nodesep: f64,
    pub ranksep: f64,
    pub marginx: f64,
    pub marginy: f64,
}

pub type RenderGraph = Graph<RenderGraphLabel, NodeRef, EdgeRef>;

#[must_use]
pub fn new_render_graph(label: RenderGraphLabel) -> RenderGraph {
    let mut g: RenderGraph = Graph::new(GraphOptions {
        multigraph: Some(true),
        compound: Some(true),
        ..Default::default()
    });
    g.set_graph(label);
    g
}

/// `clusterDb` entry.
#[derive(Debug, Clone, Default)]
pub struct ClusterEntry {
    /// Replacement anchor id (a non-cluster descendant).
    pub id: String,
    pub cluster_data: Option<NodeRef>,
    pub external_connections: bool,
    /// The positioned cluster node, set during recursive render.
    pub node: Option<NodeRef>,
}

#[derive(Debug, Default)]
pub struct ClusterDb {
    pub map: IndexMap<String, ClusterEntry>,
}

#[derive(Debug, Default)]
pub struct GraphlibState {
    pub cluster_db: ClusterDb,
    descendants: IndexMap<String, Vec<String>>,
}

impl GraphlibState {
    fn is_descendant(&self, id: &str, ancestor_id: &str) -> bool {
        self.descendants
            .get(ancestor_id)
            .is_some_and(|d| d.iter().any(|x| x == id))
    }
}

fn extract_descendants(id: &str, graph: &RenderGraph) -> Vec<String> {
    let children = graph.children(Some(id));
    let mut res = children.clone();
    for child in &children {
        res.extend(extract_descendants(child, graph));
    }
    res
}

fn find_common_edges(graph: &RenderGraph, id1: &str, id2: &str) -> usize {
    let edges = graph.edges();
    let edges1: Vec<&EdgeObj> = edges.iter().filter(|e| e.v == id1 || e.w == id1).collect();
    let edges2: Vec<&EdgeObj> = edges.iter().filter(|e| e.v == id2 || e.w == id2).collect();
    let edges1_prim: Vec<(String, String)> = edges1
        .iter()
        .map(|e| {
            (
                if e.v == id1 {
                    id2.to_owned()
                } else {
                    e.v.clone()
                },
                if e.w == id1 {
                    id1.to_owned()
                } else {
                    e.w.clone()
                },
            )
        })
        .collect();
    edges1_prim
        .iter()
        .filter(|e1| edges2.iter().any(|e2| e1.0 == e2.v && e1.1 == e2.w))
        .count()
}

#[must_use]
pub fn find_non_cluster_child(id: &str, graph: &RenderGraph, cluster_id: &str) -> Option<String> {
    let children = graph.children(Some(id));
    if children.is_empty() {
        return Some(id.to_owned());
    }
    let mut reserve: Option<String> = None;
    for child in children {
        let found = find_non_cluster_child(&child, graph, cluster_id);
        if let Some(found) = found {
            if find_common_edges(graph, cluster_id, &found) > 0 {
                reserve = Some(found);
            } else {
                return Some(found);
            }
        }
    }
    reserve
}

fn get_anchor_id(state: &GraphlibState, id: &str) -> String {
    match state.cluster_db.map.get(id) {
        None => id.to_owned(),
        Some(entry) => {
            if entry.external_connections {
                entry.id.clone()
            } else {
                id.to_owned()
            }
        }
    }
}

/// Port of `adjustClustersAndEdges` (including `extractor`).
pub fn adjust_clusters_and_edges(graph: &mut RenderGraph, state: &mut GraphlibState) {
    // Identify clusters and their descendants.
    for id in graph.nodes() {
        let children = graph.children(Some(&id));
        if !children.is_empty() {
            state
                .descendants
                .insert(id.clone(), extract_descendants(&id, graph));
            state.cluster_db.map.insert(
                id.clone(),
                ClusterEntry {
                    id: find_non_cluster_child(&id, graph, &id).unwrap_or_else(|| id.clone()),
                    cluster_data: graph.node(&id),
                    external_connections: false,
                    node: None,
                },
            );
        }
    }

    // Mark clusters with edges crossing their boundary.
    for id in graph.nodes() {
        let children = graph.children(Some(&id));
        let edges = graph.edges();
        if !children.is_empty() {
            for edge in &edges {
                let d1 = state.is_descendant(&edge.v, &id);
                let d2 = state.is_descendant(&edge.w, &id);
                if d1 != d2 {
                    state
                        .cluster_db
                        .map
                        .get_mut(&id)
                        .expect("cluster entry")
                        .external_connections = true;
                }
            }
        }
    }

    // Anchor fixups.
    let keys: Vec<String> = state.cluster_db.map.keys().cloned().collect();
    for id in keys {
        let non_cluster_child = state.cluster_db.map[&id].id.clone();
        let parent = graph.parent(&non_cluster_child);
        if let Some(parent) = parent
            && parent != id
            && state.cluster_db.map.contains_key(&parent)
            && !state.cluster_db.map[&parent].external_connections
        {
            state.cluster_db.map.get_mut(&id).expect("entry").id = parent;
        }
    }

    // Rewire edges that touch clusters to anchor nodes.
    for e in graph.edges() {
        let edge = graph.edge_for(&e).expect("edge label");
        let touches_cluster =
            state.cluster_db.map.contains_key(&e.v) || state.cluster_db.map.contains_key(&e.w);
        if touches_cluster {
            let v = get_anchor_id(state, &e.v);
            let w = get_anchor_id(state, &e.w);
            graph.remove_edge(&e.v, &e.w, e.name.as_deref());
            if v != e.v {
                let parent = graph.parent(&v);
                if let Some(parent) = parent
                    && let Some(entry) = state.cluster_db.map.get_mut(&parent)
                {
                    entry.external_connections = true;
                }
                edge.borrow_mut().from_cluster = Some(e.v.clone());
            }
            if w != e.w {
                let parent = graph.parent(&w);
                if let Some(parent) = parent
                    && let Some(entry) = state.cluster_db.map.get_mut(&parent)
                {
                    entry.external_connections = true;
                }
                edge.borrow_mut().to_cluster = Some(e.w.clone());
            }
            graph.set_edge(&v, &w, edge, e.name.as_deref());
        }
    }

    extractor(graph, state, 0);
}

/// Port of `copy` — moves a cluster's contents into `new_graph`.
fn copy(
    cluster_id: &str,
    graph: &mut RenderGraph,
    new_graph: &mut RenderGraph,
    root_id: &str,
    state: &GraphlibState,
) {
    let mut nodes = graph.children(Some(cluster_id));
    if cluster_id != root_id {
        nodes.push(cluster_id.to_owned());
    }

    for node in nodes {
        if graph.children(Some(&node)).is_empty() {
            let data = graph.node(&node);
            if let Some(data) = data {
                new_graph.set_node_with(&node, data);
            } else {
                new_graph.set_node(&node);
            }
            let parent = graph.parent(&node);
            if parent.as_deref() != Some(root_id)
                && let Some(parent) = &parent
            {
                new_graph.set_parent(&node, Some(parent));
            }

            if cluster_id != root_id && node != cluster_id {
                new_graph.set_parent(&node, Some(cluster_id));
            }

            // JS calls `graph.edges(node)`, but graphlib's edges() takes no
            // argument — every remaining edge is scanned for each node.
            for edge in graph.edges() {
                let data = graph.edge(&edge.v, &edge.w, edge.name.as_deref());
                let in_cluster = edge_in_cluster(state, &edge, root_id);
                if in_cluster && let Some(data) = data {
                    new_graph.set_edge(&edge.v, &edge.w, data, edge.name.as_deref());
                }
            }
        } else {
            copy(&node, graph, new_graph, root_id, state);
        }
        graph.remove_node(&node);
    }
}

fn edge_in_cluster(state: &GraphlibState, edge: &EdgeObj, cluster_id: &str) -> bool {
    if edge.v == cluster_id || edge.w == cluster_id {
        return false;
    }
    let descendants = state.descendants.get(cluster_id);
    let Some(descendants) = descendants else {
        return false;
    };
    descendants.iter().any(|d| d == &edge.v)
        || state.is_descendant(&edge.v, cluster_id)
        || state.is_descendant(&edge.w, cluster_id)
        || descendants.iter().any(|d| d == &edge.w)
}

/// Port of `extractor` — extracts isolated clusters into nested graphs.
pub fn extractor(graph: &mut RenderGraph, state: &mut GraphlibState, depth: u32) {
    if depth > 10 {
        return;
    }
    let nodes = graph.nodes();
    let mut has_children = false;
    for node in &nodes {
        has_children = has_children || !graph.children(Some(node)).is_empty();
    }
    if !has_children {
        return;
    }

    for node in &nodes {
        let Some(entry) = state.cluster_db.map.get(node) else {
            continue;
        };
        if !entry.external_connections && !graph.children(Some(node)).is_empty() {
            let graph_settings = graph.graph();
            let mut dir = if graph_settings.rankdir == "TB" {
                "LR".to_owned()
            } else {
                "TB".to_owned()
            };
            if let Some(cluster_data) = &entry.cluster_data
                && let Some(d) = &cluster_data.borrow().dir
            {
                dir.clone_from(d);
            }

            let mut cluster_graph = new_render_graph(RenderGraphLabel {
                rankdir: dir,
                nodesep: 50.0,
                ranksep: 50.0,
                marginx: 8.0,
                marginy: 8.0,
            });

            copy(node, graph, &mut cluster_graph, node, state);

            let entry = state.cluster_db.map.get(node).expect("entry");
            let new_node = std::rc::Rc::new(std::cell::RefCell::new(super::data::RenderNode {
                cluster_node: true,
                id: node.clone(),
                cluster_data: entry.cluster_data.clone(),
                cluster_graph: Some(cluster_graph),
                ..Default::default()
            }));
            graph.set_node_with(node, new_node);
        }
    }

    let nodes = graph.nodes();
    for node in nodes {
        let data = graph.node(&node);
        if let Some(data) = data {
            let is_cluster_node = data.borrow().cluster_node;
            if is_cluster_node {
                let mut sub = data.borrow_mut().cluster_graph.take();
                if let Some(sub_graph) = sub.as_mut() {
                    extractor(sub_graph, state, depth + 1);
                }
                data.borrow_mut().cluster_graph = sub;
            }
        }
    }
}

fn sorter(graph: &RenderGraph, nodes: &[String]) -> Vec<String> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let mut result: Vec<String> = nodes.to_vec();
    for node in nodes {
        let children = graph.children(Some(node));
        result.extend(sorter(graph, &children));
    }
    result
}

#[must_use]
pub fn sort_nodes_by_hierarchy(graph: &RenderGraph) -> Vec<String> {
    sorter(graph, &graph.children(None))
}
