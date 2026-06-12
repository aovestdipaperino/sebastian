//! Label types for the dagre layout graph.
//!
//! These correspond to the dynamic JS objects dagre attaches to graph nodes
//! and edges. Optional fields model properties that may be absent
//! (`Object.hasOwnProperty` checks in JS map to `Option::is_some`).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::graphlib::{EdgeObj, Graph};

/// A 2D point on an edge path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// The `dummy` property values dagre assigns to synthetic nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dummy {
    Root,
    Edge,
    EdgeLabel,
    EdgeProxy,
    SelfEdge,
    Border,
}

/// `borderType` of border dummy nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderType {
    BorderLeft,
    BorderRight,
}

/// A self edge stashed on its node during layout (`removeSelfEdges`).
#[derive(Debug, Clone)]
pub struct SelfEdgeRec {
    pub e: EdgeObj,
    pub label: EdgeLabelRef,
}

/// Node label; the union of every property dagre reads or writes on nodes.
#[derive(Debug, Clone, Default)]
pub struct NodeLabel {
    pub width: f64,
    pub height: f64,
    pub rank: Option<f64>,
    pub order: Option<f64>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub dummy: Option<Dummy>,
    /// Set on `edge-label` dummies; mirrors the edge's `labelpos`.
    pub labelpos: Option<String>,
    pub min_rank: Option<f64>,
    pub max_rank: Option<f64>,
    pub border_top: Option<String>,
    pub border_bottom: Option<String>,
    /// Border node names per rank (JS uses sparse arrays indexed by rank).
    pub border_left: BTreeMap<i64, String>,
    pub border_right: BTreeMap<i64, String>,
    pub border_type: Option<BorderType>,
    pub self_edges: Vec<SelfEdgeRec>,
    /// On `edge-proxy` and `selfedge` dummies: the represented edge.
    pub e: Option<EdgeObj>,
    /// On `selfedge` dummies: the original edge label.
    pub se_label: Option<EdgeLabelRef>,
    /// On `edge` dummies created by normalize: the original edge label/object.
    pub edge_label: Option<EdgeLabelRef>,
    pub edge_obj: Option<EdgeObj>,
}

/// Edge label; the union of every property dagre reads or writes on edges.
#[derive(Debug, Clone)]
pub struct EdgeLabel {
    pub minlen: f64,
    pub weight: f64,
    pub width: f64,
    pub height: f64,
    pub labeloffset: f64,
    pub labelpos: String,
    pub points: Option<Vec<Point>>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub label_rank: Option<f64>,
    pub forward_name: Option<String>,
    pub reversed: bool,
    pub nesting_edge: bool,
}

impl Default for EdgeLabel {
    fn default() -> Self {
        // Mirrors dagre's edgeDefaults from layout.js.
        Self {
            minlen: 1.0,
            weight: 1.0,
            width: 0.0,
            height: 0.0,
            labeloffset: 10.0,
            labelpos: "r".to_owned(),
            points: None,
            x: None,
            y: None,
            label_rank: None,
            forward_name: None,
            reversed: false,
            nesting_edge: false,
        }
    }
}

impl EdgeLabel {
    /// A bare `{ weight, minlen }` label as created by `util.simplify` and
    /// the nesting graph.
    #[must_use]
    pub fn rank_label(weight: f64, minlen: f64) -> Self {
        Self {
            minlen,
            weight,
            ..Self::default()
        }
    }
}

/// Graph label for the layout graph.
#[derive(Debug, Clone)]
pub struct GraphLabel {
    pub nodesep: f64,
    pub edgesep: f64,
    pub ranksep: f64,
    pub marginx: f64,
    pub marginy: f64,
    pub rankdir: String,
    pub align: Option<String>,
    pub ranker: Option<String>,
    pub acyclicer: Option<String>,
    pub nesting_root: Option<String>,
    pub node_rank_factor: f64,
    pub dummy_chains: Vec<String>,
    pub max_rank: Option<f64>,
    pub width: f64,
    pub height: f64,
}

impl Default for GraphLabel {
    fn default() -> Self {
        // Mirrors dagre's graphDefaults from layout.js.
        Self {
            nodesep: 50.0,
            edgesep: 20.0,
            ranksep: 50.0,
            marginx: 0.0,
            marginy: 0.0,
            rankdir: "tb".to_owned(),
            align: None,
            ranker: None,
            acyclicer: None,
            nesting_root: None,
            node_rank_factor: 0.0,
            dummy_chains: Vec::new(),
            max_rank: None,
            width: 0.0,
            height: 0.0,
        }
    }
}

pub type NodeLabelRef = Rc<RefCell<NodeLabel>>;
pub type EdgeLabelRef = Rc<RefCell<EdgeLabel>>;
pub type GraphLabelRef = Rc<RefCell<GraphLabel>>;

/// The layout graph type threaded through the whole pipeline.
pub type LayoutGraph = Graph<GraphLabelRef, NodeLabelRef, EdgeLabelRef>;

#[must_use]
pub fn node_ref(label: NodeLabel) -> NodeLabelRef {
    Rc::new(RefCell::new(label))
}

#[must_use]
pub fn edge_ref(label: EdgeLabel) -> EdgeLabelRef {
    Rc::new(RefCell::new(label))
}

/// Port of the lodash global `_.uniqueId` counter.
///
/// dagre's dummy-node names embed this counter, and those names participate
/// in string comparisons and map ordering, so the sequence must match the JS
/// run exactly (one shared counter for all prefixes, starting at 1).
#[derive(Debug, Default)]
pub struct UniqueId {
    counter: Cell<u64>,
}

impl UniqueId {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next(&self, prefix: &str) -> String {
        let id = self.counter.get() + 1;
        self.counter.set(id);
        format!("{prefix}{id}")
    }
}

/// JS `<` where `undefined` compares false against anything.
#[must_use]
pub fn js_lt(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a < b,
        _ => false,
    }
}

/// JS `>` where `undefined` compares false against anything.
#[must_use]
pub fn js_gt(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// JS `Math.max(a, b)`: NaN-propagating, unlike `f64::max`.
#[must_use]
pub fn js_math_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// JS `Math.min(a, b)`: NaN-propagating, unlike `f64::min`.
#[must_use]
pub fn js_math_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else {
        b
    }
}

/// JS string `>` comparing UTF-16 code units (differs from byte order for
/// some non-ASCII strings).
#[must_use]
pub fn js_str_gt(a: &str, b: &str) -> bool {
    a.encode_utf16().gt(b.encode_utf16())
}
