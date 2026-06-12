//! Port of `layout-algorithms/dagre/index.js` — graph assembly, recursive
//! render, dagre invocation, and final positioning.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::dagre;
use crate::dagre::types::{EdgeLabel, GraphLabel, NodeLabel, edge_ref, node_ref};
use crate::graphlib::{Graph, GraphOptions};
use crate::svg::{Element, append, js_num, set_attr};
use crate::text::TextMeasurer;

use super::clusters::insert_cluster;
use super::data::{EdgeRef, LayoutData, NodeRef, RenderNode};
use super::edges::{calc_label_position, insert_edge, insert_edge_label};
use super::graph::{
    GraphlibState, RenderGraph, RenderGraphLabel, adjust_clusters_and_edges,
    find_non_cluster_child, new_render_graph, sort_nodes_by_hierarchy,
};
use super::markers::insert_markers;
use super::shapes::insert_node_shape;

const SUB_GRAPH_TITLE_TOTAL_MARGIN: f64 = 0.0;

impl std::fmt::Debug for RenderCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderCtx")
            .field("diagram_id", &self.diagram_id)
            .finish_non_exhaustive()
    }
}

pub struct RenderCtx {
    pub measurer: TextMeasurer,
    pub config: super::config::RenderConfig,
    pub markers: super::edges::MarkerState,
    pub state: GraphlibState,
    pub diagram_id: String,
    pub diagram_type: String,
    pub node_elems: HashMap<String, Element>,
    pub edge_label_elems: HashMap<String, Element>,
}

/// Port of the top-level `render(data4Layout, svg)`.
pub fn render(data: &LayoutData, svg_root_g: &Element, ctx: &mut RenderCtx) {
    let mut graph = new_render_graph(RenderGraphLabel {
        rankdir: data.direction.clone(),
        nodesep: ctx.config.node_spacing,
        ranksep: ctx.config.rank_spacing,
        marginx: 8.0,
        marginy: 8.0,
    });

    insert_markers(svg_root_g, &ctx.diagram_type, &ctx.diagram_id);

    for node in &data.nodes {
        // `graph.setNode(node.id, { ...node })` — a fresh copy.
        let copy = Rc::new(RefCell::new(node.borrow().clone()));
        let id = copy.borrow().id.clone();
        graph.set_node_with(&id, copy.clone());
        if let Some(parent_id) = copy.borrow().parent_id.clone() {
            graph.set_parent(&id, Some(&parent_id));
        }
    }

    for edge in &data.edges {
        let e = edge.borrow();
        if e.start == e.end {
            // Self-loop: split through two labelRect helper nodes.
            let node_id = e.start.clone();
            let special_id1 = format!("{node_id}---{node_id}---1");
            let special_id2 = format!("{node_id}---{node_id}---2");
            let node = graph.node(&node_id).expect("self-loop node");
            let parent_id = node.borrow().parent_id.clone();
            for special_id in [&special_id1, &special_id2] {
                let special = Rc::new(RefCell::new(RenderNode {
                    id: special_id.clone(),
                    dom_id: special_id.clone(),
                    parent_id: parent_id.clone(),
                    label: String::new(),
                    padding: 0.0,
                    shape: "labelRect".to_owned(),
                    width: 10.0,
                    height: 10.0,
                    ..Default::default()
                }));
                graph.set_node_with(special_id, special);
                graph.set_parent(special_id, parent_id.as_deref());
            }

            let mut edge1 = e.clone();
            let mut edge_mid = e.clone();
            let mut edge2 = e.clone();
            edge1.label = String::new();
            "none".clone_into(&mut edge1.arrow_type_end);
            edge1.id = format!("{node_id}-cyclic-special-1");
            "none".clone_into(&mut edge_mid.arrow_type_start);
            "none".clone_into(&mut edge_mid.arrow_type_end);
            edge_mid.id = format!("{node_id}-cyclic-special-mid");
            edge2.label = String::new();
            "none".clone_into(&mut edge2.arrow_type_start);
            edge2.id = format!("{node_id}-cyclic-special-2");
            let is_group = node.borrow().is_group;
            if is_group {
                edge1.to_cluster = Some(node_id.clone());
                edge2.from_cluster = Some(node_id.clone());
            }
            graph.set_edge(
                &node_id,
                &special_id1,
                Rc::new(RefCell::new(edge1)),
                Some(&format!("{node_id}-cyclic-special-0")),
            );
            graph.set_edge(
                &special_id1,
                &special_id2,
                Rc::new(RefCell::new(edge_mid)),
                Some(&format!("{node_id}-cyclic-special-1")),
            );
            // Preserves the typo in mermaid's edge name.
            graph.set_edge(
                &special_id2,
                &node_id,
                Rc::new(RefCell::new(edge2)),
                Some(&format!("{node_id}-cyc<lic-special-2")),
            );
        } else {
            let copy = Rc::new(RefCell::new(e.clone()));
            graph.set_edge(&e.start, &e.end, copy, Some(&e.id));
        }
    }

    adjust_clusters_and_edges(&mut graph, &mut ctx.state);
    recursive_render(svg_root_g, &mut graph, None, ctx);
}

impl std::fmt::Debug for RecursiveRenderResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecursiveRenderResult")
            .field("diff", &self.diff)
            .finish_non_exhaustive()
    }
}

pub struct RecursiveRenderResult {
    pub elem: Element,
    pub diff: f64,
}

/// Port of `recursiveRender`.
#[allow(clippy::too_many_lines)]
pub fn recursive_render(
    parent_elem: &Element,
    graph: &mut RenderGraph,
    parent_cluster: Option<&NodeRef>,
    ctx: &mut RenderCtx,
) -> RecursiveRenderResult {
    let elem = append(parent_elem, "g");
    set_attr(&elem, "class", "root");
    let clusters = append(&elem, "g");
    set_attr(&clusters, "class", "clusters");
    let edge_paths = append(&elem, "g");
    set_attr(&edge_paths, "class", "edgePaths");
    let edge_labels = append(&elem, "g");
    set_attr(&edge_labels, "class", "edgeLabels");
    let nodes_g = append(&elem, "g");
    set_attr(&nodes_g, "class", "nodes");

    // Insert nodes (and recursively render cluster nodes).
    for v in graph.nodes() {
        let node = graph.node(&v).expect("graph node");
        if let Some(parent_cluster) = parent_cluster {
            // Reattach loose nodes under a fresh copy of the parent
            // cluster's original data (JS clones `clusterData`).
            let cluster_data = parent_cluster.borrow().cluster_data.clone();
            if let Some(cluster_data) = cluster_data {
                let data = Rc::new(RefCell::new(cluster_data.borrow().clone()));
                let pc_id = parent_cluster.borrow().id.clone();
                graph.set_node_with(&pc_id, data);
                if graph.parent(&v).is_none() && v != pc_id {
                    graph.set_parent(&v, Some(&pc_id));
                }
            }
        }

        let is_cluster_node = node.borrow().cluster_node;
        if is_cluster_node {
            // Sub-graph spacing inherits parent ranksep + 25.
            let parent_label = graph.graph();
            let mut sub_graph = node
                .borrow_mut()
                .cluster_graph
                .take()
                .expect("cluster graph");
            {
                let mut label = sub_graph.graph();
                label.ranksep = parent_label.ranksep + 25.0;
                label.nodesep = parent_label.nodesep;
                sub_graph.set_graph(label);
            }
            let o = recursive_render(&nodes_g, &mut sub_graph, Some(&node), ctx);
            node.borrow_mut().cluster_graph = Some(sub_graph);
            // updateNodeBounds: bbox of the rendered subtree.
            let bbox = super::bbox::element_bbox(&o.elem);
            {
                let mut n = node.borrow_mut();
                n.width = super::shapes::f32q(bbox.width());
                n.height = super::shapes::f32q(bbox.height());
                n.diff = o.diff;
            }
            ctx.node_elems.insert(v.clone(), o.elem);
        } else if !graph.children(Some(&v)).is_empty() {
            // Flat cluster: defer to positioning pass.
            let id = node.borrow().id.clone();
            let anchor = find_non_cluster_child(&id, graph, "").unwrap_or_else(|| id.clone());
            ctx.state.cluster_db.map.insert(
                id,
                super::graph::ClusterEntry {
                    id: anchor,
                    cluster_data: None,
                    external_connections: false,
                    node: Some(node.clone()),
                },
            );
        } else {
            let el = insert_node_shape(&nodes_g, &node, &ctx.measurer, &ctx.config);
            let look = node.borrow().look.clone();
            if !look.is_empty() {
                set_attr(&el, "data-look", look);
            }
            ctx.node_elems.insert(v.clone(), el);
        }
    }

    // Insert edge labels.
    for e in graph.edges() {
        let edge = graph.edge_for(&e).expect("edge label");
        let label_elem = insert_edge_label(&edge_labels, &edge, &ctx.measurer, &ctx.config);
        let id = edge.borrow().id.clone();
        ctx.edge_label_elems.insert(id, label_elem);
    }
    // Terminal labels: the upstream loop is unawaited async, so the
    // microtask order groups all startRight terminals before endLeft ones.
    for e in graph.edges() {
        let edge = graph.edge_for(&e).expect("edge label");
        let eb = edge.borrow();
        if !eb.start_label_right.is_empty() {
            let g = super::edges::insert_terminal_label(
                &edge_labels,
                &eb.start_label_right,
                &ctx.measurer,
                true,
            );
            ctx.edge_label_elems
                .insert(format!("{}::startRight", eb.id), g);
        }
    }
    for e in graph.edges() {
        let edge = graph.edge_for(&e).expect("edge label");
        let eb = edge.borrow();
        if !eb.end_label_left.is_empty() {
            let g = super::edges::insert_terminal_label(
                &edge_labels,
                &eb.end_label_left,
                &ctx.measurer,
                false,
            );
            ctx.edge_label_elems
                .insert(format!("{}::endLeft", eb.id), g);
        }
    }

    // Layout.
    layout_with_dagre(graph);

    // Position nodes.
    for v in sort_nodes_by_hierarchy(graph) {
        let node = graph.node(&v).expect("node");
        let is_cluster_node = node.borrow().cluster_node;
        if is_cluster_node {
            {
                let mut n = node.borrow_mut();
                n.y += SUB_GRAPH_TITLE_TOTAL_MARGIN;
            }
            let id = node.borrow().id.clone();
            if let Some(entry) = ctx.state.cluster_db.map.get_mut(&id) {
                entry.node = Some(node.clone());
            }
            position_node(&node, ctx);
        } else if !graph.children(Some(&v)).is_empty() {
            {
                let mut n = node.borrow_mut();
                n.height += SUB_GRAPH_TITLE_TOTAL_MARGIN;
            }
            insert_cluster(&clusters, &node, &ctx.measurer, &ctx.config);
            let id = node.borrow().id.clone();
            if let Some(entry) = ctx.state.cluster_db.map.get_mut(&id) {
                entry.node = Some(node.clone());
            }
        } else {
            {
                let mut n = node.borrow_mut();
                n.y += SUB_GRAPH_TITLE_TOTAL_MARGIN / 2.0;
            }
            position_node(&node, ctx);
        }
    }

    // Insert edges and position labels.
    for e in graph.edges() {
        let edge = graph.edge_for(&e).expect("edge");
        {
            let mut ed = edge.borrow_mut();
            for p in &mut ed.points {
                p.y += SUB_GRAPH_TITLE_TOTAL_MARGIN / 2.0;
            }
        }
        let start_node = graph.node(&e.v).expect("start node");
        let end_node = graph.node(&e.w).expect("end node");
        let paths = insert_edge(
            &edge_paths,
            &edge,
            &ctx.state.cluster_db,
            &ctx.diagram_type,
            &start_node,
            &end_node,
            &ctx.diagram_id,
            &mut ctx.markers,
        );
        position_edge_label(&edge, &paths, ctx);
    }

    let mut diff = 0.0;
    for v in graph.nodes() {
        let node = graph.node(&v).expect("node");
        let n = node.borrow();
        if n.is_group {
            diff = n.diff;
        }
    }

    RecursiveRenderResult { elem, diff }
}

fn position_node(node: &NodeRef, ctx: &RenderCtx) {
    let n = node.borrow();
    let Some(el) = ctx.node_elems.get(&n.id) else {
        return;
    };
    let padding = 8.0;
    if n.cluster_node {
        set_attr(
            el,
            "transform",
            format!(
                "translate({}, {})",
                js_num(n.x + n.diff - n.width / 2.0),
                js_num(n.y - n.height / 2.0 - padding)
            ),
        );
    } else {
        set_attr(
            el,
            "transform",
            format!("translate({}, {})", js_num(n.x), js_num(n.y)),
        );
    }
}

fn position_edge_label(edge: &EdgeRef, paths: &super::edges::InsertedEdge, ctx: &RenderCtx) {
    let e = edge.borrow();
    if !e.label.is_empty()
        && let Some(el) = ctx.edge_label_elems.get(&e.id)
    {
        let mut x = e.x.unwrap_or(0.0);
        let mut y = e.y.unwrap_or(0.0);
        if let Some(updated) = &paths.updated_points {
            let pos = calc_label_position(updated);
            x = pos.x;
            y = pos.y;
        }
        set_attr(
            el,
            "transform",
            format!(
                "translate({}, {})",
                js_num(x),
                js_num(y + SUB_GRAPH_TITLE_TOTAL_MARGIN / 2.0)
            ),
        );
    }

    // Cardinality terminal labels (class diagrams).
    let path_points = paths.updated_points.as_ref().unwrap_or(&e.points);
    if !e.start_label_right.is_empty()
        && let Some(el) = ctx.edge_label_elems.get(&format!("{}::startRight", e.id))
    {
        let marker = if e.arrow_type_start.is_empty() {
            0.0
        } else {
            10.0
        };
        let pos = super::edges::calc_terminal_label_position(marker, "start_right", path_points);
        set_attr(
            el,
            "transform",
            format!("translate({}, {})", js_num(pos.x), js_num(pos.y)),
        );
    }
    if !e.end_label_left.is_empty()
        && let Some(el) = ctx.edge_label_elems.get(&format!("{}::endLeft", e.id))
    {
        let marker = if e.arrow_type_end.is_empty() {
            0.0
        } else {
            10.0
        };
        let pos = super::edges::calc_terminal_label_position(marker, "end_left", path_points);
        set_attr(
            el,
            "transform",
            format!("translate({}, {})", js_num(pos.x), js_num(pos.y)),
        );
    }
}

/// Adapter: runs the dagre port on a `RenderGraph` and copies results back.
fn layout_with_dagre(graph: &RenderGraph) {
    let mut g: dagre::types::LayoutGraph = Graph::new(GraphOptions {
        multigraph: Some(true),
        compound: Some(true),
        ..Default::default()
    });
    let label = graph.graph();
    g.set_graph(Rc::new(RefCell::new(GraphLabel {
        rankdir: label.rankdir.clone(),
        nodesep: label.nodesep,
        ranksep: label.ranksep,
        marginx: label.marginx,
        marginy: label.marginy,
        ..Default::default()
    })));

    for v in graph.nodes() {
        let node = graph.node(&v).expect("node");
        let n = node.borrow();
        g.set_node_with(
            &v,
            node_ref(NodeLabel {
                width: n.width,
                height: n.height,
                ..Default::default()
            }),
        );
        g.set_parent(&v, graph.parent(&v).as_deref());
    }
    for e in graph.edges() {
        let edge = graph.edge_for(&e).expect("edge");
        let ed = edge.borrow();
        g.set_edge_obj(
            &e,
            edge_ref(EdgeLabel {
                minlen: ed.minlen,
                weight: 1.0,
                width: ed.width,
                height: ed.height,
                labelpos: ed.labelpos.clone(),
                labeloffset: 10.0,
                ..Default::default()
            }),
        );
    }

    dagre::layout(&g);

    for v in graph.nodes() {
        let node = graph.node(&v).expect("node");
        let l = g.node(&v).expect("layout node");
        let l = l.borrow();
        let mut n = node.borrow_mut();
        n.x = l.x.unwrap_or(0.0);
        n.y = l.y.unwrap_or(0.0);
        if !g.children(Some(&v)).is_empty() {
            n.width = l.width;
            n.height = l.height;
        }
    }
    for e in graph.edges() {
        let edge = graph.edge_for(&e).expect("edge");
        let l = g.edge_for(&e).expect("layout edge");
        let l = l.borrow();
        let mut ed = edge.borrow_mut();
        ed.points = l.points.clone().unwrap_or_default();
        if l.x.is_some() {
            ed.x = l.x;
            ed.y = l.y;
        }
    }
}
