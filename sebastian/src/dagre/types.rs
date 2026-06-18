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
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

/// The `dummy` property values dagre assigns to synthetic nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dummy {
    /// The nesting-graph root inserted by `nestingGraph.run`.
    Root,
    /// A chain node inserted when normalizing a multi-rank edge.
    Edge,
    /// The dummy that carries an edge's label between ranks.
    EdgeLabel,
    /// A proxy standing in for an edge while self edges are handled.
    EdgeProxy,
    /// A dummy representing a removed self edge.
    SelfEdge,
    /// A subgraph border node.
    Border,
}

/// `borderType` of border dummy nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderType {
    /// Left border of a subgraph cluster.
    BorderLeft,
    /// Right border of a subgraph cluster.
    BorderRight,
}

/// A self edge stashed on its node during layout (`removeSelfEdges`).
#[derive(Debug, Clone)]
pub struct SelfEdgeRec {
    /// The edge endpoints (`{v, w, name}`).
    pub e: EdgeObj,
    /// The edge's label.
    pub label: EdgeLabelRef,
}

/// Node label; the union of every property dagre reads or writes on nodes.
#[derive(Debug, Clone, Default)]
pub struct NodeLabel {
    /// Node width in pixels.
    pub width: f64,
    /// Node height in pixels.
    pub height: f64,
    /// Assigned layer (rank) index, once ranking has run.
    pub rank: Option<f64>,
    /// Position of the node within its rank, once ordering has run.
    pub order: Option<f64>,
    /// Final x coordinate, set by the positioning pass.
    pub x: Option<f64>,
    /// Final y coordinate, set by the positioning pass.
    pub y: Option<f64>,
    /// Which kind of synthetic node this is, if it is a dummy.
    pub dummy: Option<Dummy>,
    /// Set on `edge-label` dummies; mirrors the edge's `labelpos`.
    pub labelpos: Option<String>,
    /// Lowest rank spanned by a collapsed subgraph.
    pub min_rank: Option<f64>,
    /// Highest rank spanned by a collapsed subgraph.
    pub max_rank: Option<f64>,
    /// Border dummy node name for the top edge of a subgraph.
    pub border_top: Option<String>,
    /// Border dummy node name for the bottom edge of a subgraph.
    pub border_bottom: Option<String>,
    /// Border node names per rank (JS uses sparse arrays indexed by rank).
    pub border_left: BTreeMap<i64, String>,
    /// Right-side border node names per rank.
    pub border_right: BTreeMap<i64, String>,
    /// Which subgraph border this dummy represents, if any.
    pub border_type: Option<BorderType>,
    /// Self edges stashed here while removed during layout.
    pub self_edges: Vec<SelfEdgeRec>,
    /// On `edge-proxy` and `selfedge` dummies: the represented edge.
    pub e: Option<EdgeObj>,
    /// On `selfedge` dummies: the original edge label.
    pub se_label: Option<EdgeLabelRef>,
    /// On `edge` dummies created by normalize: the original edge label/object.
    pub edge_label: Option<EdgeLabelRef>,
    /// On `edge` dummies: the original edge endpoints.
    pub edge_obj: Option<EdgeObj>,
}

/// Edge label; the union of every property dagre reads or writes on edges.
#[derive(Debug, Clone)]
pub struct EdgeLabel {
    /// Minimum number of ranks the edge must span.
    pub minlen: f64,
    /// Edge weight used by the ranking and ordering passes.
    pub weight: f64,
    /// Label width in pixels.
    pub width: f64,
    /// Label height in pixels.
    pub height: f64,
    /// Distance the label is offset from the edge.
    pub labeloffset: f64,
    /// Label position relative to the edge (`l`, `c` or `r`).
    pub labelpos: String,
    /// Computed poly-line points of the routed edge.
    pub points: Option<Vec<Point>>,
    /// Final x coordinate of the label.
    pub x: Option<f64>,
    /// Final y coordinate of the label.
    pub y: Option<f64>,
    /// Rank of the edge's label dummy.
    pub label_rank: Option<f64>,
    /// Original edge name, retained when the edge is reversed.
    pub forward_name: Option<String>,
    /// True when the edge was reversed to break a cycle.
    pub reversed: bool,
    /// True for edges inserted by the nesting graph.
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
    /// Horizontal separation between nodes in the same rank.
    pub nodesep: f64,
    /// Separation between adjacent edges.
    pub edgesep: f64,
    /// Vertical separation between ranks.
    pub ranksep: f64,
    /// Horizontal margin added around the whole graph.
    pub marginx: f64,
    /// Vertical margin added around the whole graph.
    pub marginy: f64,
    /// Layout direction (`tb`, `bt`, `lr` or `rl`).
    pub rankdir: String,
    /// Optional node alignment within ranks (`ul`, `ur`, `dl`, `dr`).
    pub align: Option<String>,
    /// Ranking algorithm name (`network-simplex`, `tight-tree`, ...).
    pub ranker: Option<String>,
    /// Cycle-breaking strategy name.
    pub acyclicer: Option<String>,
    /// Name of the nesting-graph root node, while nesting is active.
    pub nesting_root: Option<String>,
    /// Rank multiplier used by the nesting graph.
    pub node_rank_factor: f64,
    /// Heads of the dummy chains created by `normalize`.
    pub dummy_chains: Vec<String>,
    /// Largest rank index after ranking.
    pub max_rank: Option<f64>,
    /// Overall graph width.
    pub width: f64,
    /// Overall graph height.
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

/// Shared, mutable handle to a [`NodeLabel`].
pub type NodeLabelRef = Rc<RefCell<NodeLabel>>;
/// Shared, mutable handle to an [`EdgeLabel`].
pub type EdgeLabelRef = Rc<RefCell<EdgeLabel>>;
/// Shared, mutable handle to a [`GraphLabel`].
pub type GraphLabelRef = Rc<RefCell<GraphLabel>>;

/// The layout graph type threaded through the whole pipeline.
pub type LayoutGraph = Graph<GraphLabelRef, NodeLabelRef, EdgeLabelRef>;

/// Wraps a [`NodeLabel`] in a shared, mutable handle.
#[must_use]
pub fn node_ref(label: NodeLabel) -> NodeLabelRef {
    Rc::new(RefCell::new(label))
}

/// Wraps an [`EdgeLabel`] in a shared, mutable handle.
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
    /// Creates a fresh counter; the first generated id is `1`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `{prefix}{n}`, incrementing the shared counter first.
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
