//! Layout data structures, mirroring mermaid's `LayoutData` node/edge shape.

use std::cell::RefCell;
use std::rc::Rc;

use crate::dagre::types::Point;

/// A node as produced by `flowDb.getData()` and consumed by the renderer.
#[derive(Debug, Clone, Default)]
pub struct RenderNode {
    pub id: String,
    pub label: String,
    /// Pre-innerHTML label text (used by SVG-text labels).
    pub label_raw: String,
    pub label_type: String,
    pub parent_id: Option<String>,
    pub padding: f64,
    pub css_styles: Vec<String>,
    pub css_compiled_styles: Vec<String>,
    pub css_classes: String,
    /// Compiled label style string (set by shape handlers).
    pub label_style_str: String,
    pub shape: String,
    pub dir: Option<String>,
    pub dom_id: String,
    pub look: String,
    pub link: Option<String>,
    pub link_target: Option<String>,
    pub tooltip: Option<String>,
    pub is_group: bool,

    // Set during rendering:
    pub width: f64,
    pub height: f64,
    pub x: f64,
    pub y: f64,
    /// Cluster-relative horizontal adjustment (see clusters.js `node.diff`).
    pub diff: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub label_bbox: Option<(f64, f64)>,
    /// Geometry needed to compute edge/shape intersections after layout.
    pub intersect: Option<IntersectShape>,
    /// True when this node was converted into a recursive cluster node.
    pub cluster_node: bool,
    /// The extracted subgraph for `cluster_node` nodes.
    pub cluster_graph: Option<super::graph::RenderGraph>,
    /// Original subgraph node data (JS `clusterData`).
    pub cluster_data: Option<NodeRef>,
    /// classBox compartments: (display text, classifier style) pairs.
    pub class_annotations: Vec<String>,
    pub class_members: Vec<(String, String)>,
    /// ER entity attributes: (type, name, keys-joined, comment).
    pub er_attributes: Vec<(String, String, String, String)>,
    /// ER entity alias (rendered instead of the label when set).
    pub er_alias: String,
    pub class_methods: Vec<(String, String)>,
}

/// Shape geometry used by `node.intersect(point)` calls.
#[derive(Debug, Clone)]
pub enum IntersectShape {
    Rect,
    /// Polygon points relative to the node (shape-local, as in shape files).
    Polygon(Vec<Point>),
    /// Question shape: polygon + the (-0.5, -0.5) result adjustment.
    Question(Vec<Point>),
    Circle {
        radius: f64,
    },
    Cylinder {
        rx: f64,
        ry: f64,
    },
}

pub type NodeRef = Rc<RefCell<RenderNode>>;

/// An edge as produced by `flowDb.getData()`.
#[derive(Debug, Clone, Default)]
pub struct RenderEdge {
    pub id: String,
    pub start: String,
    pub end: String,
    pub edge_type: String,
    pub label: String,
    /// Pre-innerHTML label text (used by SVG-text labels).
    pub label_raw: String,
    pub label_type: String,
    pub labelpos: String,
    pub thickness: String,
    pub pattern: String,
    pub minlen: f64,
    pub classes: String,
    pub arrow_type_start: String,
    pub arrow_type_end: String,
    /// Cardinality terminal labels (class diagrams).
    pub start_label_right: String,
    pub end_label_left: String,
    pub style: Vec<String>,
    pub label_style: Vec<String>,
    pub css_compiled_styles: Vec<String>,
    pub curve: String,
    pub look: String,

    // Set during rendering:
    pub width: f64,
    pub height: f64,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub points: Vec<Point>,
    pub from_cluster: Option<String>,
    pub to_cluster: Option<String>,
}

pub type EdgeRef = Rc<RefCell<RenderEdge>>;

/// The result of `flowDb.getData()`.
#[derive(Debug, Default)]
pub struct LayoutData {
    pub nodes: Vec<NodeRef>,
    pub edges: Vec<EdgeRef>,
    pub direction: String,
    pub diagram_id: String,
}
