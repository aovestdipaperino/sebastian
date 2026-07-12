//! Port of the graphlib `Graph` class bundled with dagre-d3-es 7.0.14.
//!
//! Semantics follow the JS implementation exactly, including key iteration
//! order (see [`crate::jsmap::JsMap`]) and label sharing: callers typically
//! use `Rc<RefCell<..>>` labels so that labels aliased across derived graphs
//! observe mutations, as JS object references do.

use std::collections::HashMap;
use std::rc::Rc;

use crate::jsmap::JsMap;

const DEFAULT_EDGE_NAME: &str = "\u{0}";
const GRAPH_NODE: &str = "\u{0}";
const EDGE_KEY_DELIM: char = '\u{1}';

/// Identifies an edge by tail, head and optional multigraph name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EdgeObj {
    pub v: String,
    pub w: String,
    pub name: Option<String>,
}

impl EdgeObj {
    pub fn new(v: impl Into<String>, w: impl Into<String>) -> Self {
        Self {
            v: v.into(),
            w: w.into(),
            name: None,
        }
    }

    pub fn named(v: impl Into<String>, w: impl Into<String>, name: Option<String>) -> Self {
        Self {
            v: v.into(),
            w: w.into(),
            name,
        }
    }
}

fn edge_args_to_id(is_directed: bool, v: &str, w: &str, name: Option<&str>) -> String {
    let (v, w) = if !is_directed && v > w {
        (w, v)
    } else {
        (v, w)
    };
    format!(
        "{v}{EDGE_KEY_DELIM}{w}{EDGE_KEY_DELIM}{}",
        name.unwrap_or(DEFAULT_EDGE_NAME)
    )
}

fn edge_args_to_obj(is_directed: bool, v: &str, w: &str, name: Option<&str>) -> EdgeObj {
    let (v, w) = if !is_directed && v > w {
        (w, v)
    } else {
        (v, w)
    };
    // JS only sets the name property when `name` is truthy; an empty string
    // name is falsy there, but dagre never produces empty edge names.
    EdgeObj::named(v, w, name.map(str::to_owned))
}

type NodeLabelFn<N> = Rc<dyn Fn(&str) -> Option<N>>;

/// Graph options mirroring the JS constructor argument.
#[derive(Debug, Clone, Copy, Default)]
pub struct GraphOptions {
    pub directed: Option<bool>,
    pub multigraph: Option<bool>,
    pub compound: Option<bool>,
}

/// Direct port of graphlib's mixed multigraph/compound graph.
pub struct Graph<G, N: Clone, E: Clone> {
    is_directed: bool,
    is_multigraph: bool,
    is_compound: bool,
    label: Option<G>,
    default_node_label_fn: Option<NodeLabelFn<N>>,
    nodes: JsMap<Option<N>>,
    parent: HashMap<String, String>,
    children: HashMap<String, JsMap<()>>,
    in_: HashMap<String, JsMap<EdgeObj>>,
    preds: HashMap<String, JsMap<u32>>,
    out: HashMap<String, JsMap<EdgeObj>>,
    sucs: HashMap<String, JsMap<u32>>,
    edge_objs: JsMap<EdgeObj>,
    edge_labels: JsMap<Option<E>>,
}

impl<G: Clone, N: Clone, E: Clone> Clone for Graph<G, N, E> {
    fn clone(&self) -> Self {
        Self {
            is_directed: self.is_directed,
            is_multigraph: self.is_multigraph,
            is_compound: self.is_compound,
            label: self.label.clone(),
            default_node_label_fn: self.default_node_label_fn.clone(),
            nodes: self.nodes.clone(),
            parent: self.parent.clone(),
            children: self.children.clone(),
            in_: self.in_.clone(),
            preds: self.preds.clone(),
            out: self.out.clone(),
            sucs: self.sucs.clone(),
            edge_objs: self.edge_objs.clone(),
            edge_labels: self.edge_labels.clone(),
        }
    }
}

impl<G, N: Clone, E: Clone> std::fmt::Debug for Graph<G, N, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph")
            .field("directed", &self.is_directed)
            .field("multigraph", &self.is_multigraph)
            .field("compound", &self.is_compound)
            .field("nodes", &self.nodes.keys())
            .field("edges", &self.edge_objs.keys())
            .finish_non_exhaustive()
    }
}

impl<G, N: Clone, E: Clone> Graph<G, N, E> {
    #[must_use]
    pub fn new(opts: GraphOptions) -> Self {
        let is_compound = opts.compound.unwrap_or(false);
        let mut children = HashMap::new();
        if is_compound {
            children.insert(GRAPH_NODE.to_owned(), JsMap::new());
        }
        Self {
            is_directed: opts.directed.unwrap_or(true),
            is_multigraph: opts.multigraph.unwrap_or(false),
            is_compound,
            label: None,
            default_node_label_fn: None,
            nodes: JsMap::new(),
            parent: HashMap::new(),
            children,
            in_: HashMap::new(),
            preds: HashMap::new(),
            out: HashMap::new(),
            sucs: HashMap::new(),
            edge_objs: JsMap::new(),
            edge_labels: JsMap::new(),
        }
    }

    pub fn is_directed(&self) -> bool {
        self.is_directed
    }

    pub fn is_multigraph(&self) -> bool {
        self.is_multigraph
    }

    pub fn is_compound(&self) -> bool {
        self.is_compound
    }

    pub fn set_graph(&mut self, label: G) {
        self.label = Some(label);
    }

    /// The graph label. Panics if never set, which is a porting bug: dagre
    /// only reads the label after assigning it.
    pub fn graph(&self) -> G
    where
        G: Clone,
    {
        self.label.clone().expect("graph label read before set")
    }

    pub fn set_default_node_label(&mut self, f: impl Fn(&str) -> Option<N> + 'static) {
        self.default_node_label_fn = Some(Rc::new(f));
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn nodes(&self) -> Vec<String> {
        self.nodes.keys()
    }

    pub fn sources(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter(|v| self.in_.get(v).is_none_or(JsMap::is_empty))
            .collect()
    }

    pub fn sinks(&self) -> Vec<String> {
        self.nodes()
            .into_iter()
            .filter(|v| self.out.get(v).is_none_or(JsMap::is_empty))
            .collect()
    }

    /// `setNode(v)` — creates the node with the default label, or leaves an
    /// existing node untouched.
    pub fn set_node(&mut self, v: &str) {
        if self.nodes.contains_key(v) {
            return;
        }
        let label = self.default_node_label_fn.as_ref().and_then(|f| f(v));
        self.insert_node(v, label);
    }

    /// `setNode(v, value)` — creates or overwrites the node label.
    pub fn set_node_with(&mut self, v: &str, value: N) {
        if self.nodes.contains_key(v) {
            self.nodes.insert(v, Some(value));
            return;
        }
        self.insert_node(v, Some(value));
    }

    fn insert_node(&mut self, v: &str, label: Option<N>) {
        self.nodes.insert(v, label);
        if self.is_compound {
            self.parent.insert(v.to_owned(), GRAPH_NODE.to_owned());
            self.children.insert(v.to_owned(), JsMap::new());
            self.children
                .get_mut(GRAPH_NODE)
                .expect("graph root child list")
                .insert(v, ());
        }
        self.in_.insert(v.to_owned(), JsMap::new());
        self.preds.insert(v.to_owned(), JsMap::new());
        self.out.insert(v.to_owned(), JsMap::new());
        self.sucs.insert(v.to_owned(), JsMap::new());
    }

    /// Returns the node label, or `None` when the node is missing or its
    /// label is `undefined`.
    pub fn node(&self, v: &str) -> Option<N> {
        self.nodes.get(v).and_then(Clone::clone)
    }

    pub fn has_node(&self, v: &str) -> bool {
        self.nodes.contains_key(v)
    }

    pub fn remove_node(&mut self, v: &str) {
        if !self.nodes.contains_key(v) {
            return;
        }
        self.nodes.remove(v);
        if self.is_compound {
            self.remove_from_parents_child_list(v);
            self.parent.remove(v);
            for child in self.children(Some(v)) {
                self.set_parent(&child, None);
            }
            self.children.remove(v);
        }
        let in_keys: Vec<String> = self.in_.get(v).map(JsMap::keys).unwrap_or_default();
        for e in in_keys {
            let edge = self.edge_objs.get(&e).expect("in edge obj").clone();
            self.remove_edge_obj(&edge);
        }
        self.in_.remove(v);
        self.preds.remove(v);
        let out_keys: Vec<String> = self.out.get(v).map(JsMap::keys).unwrap_or_default();
        for e in out_keys {
            let edge = self.edge_objs.get(&e).expect("out edge obj").clone();
            self.remove_edge_obj(&edge);
        }
        self.out.remove(v);
        self.sucs.remove(v);
    }

    /// `setParent(v, parent)`; `parent = None` removes the parent.
    pub fn set_parent(&mut self, v: &str, parent: Option<&str>) {
        assert!(
            self.is_compound,
            "cannot set parent in a non-compound graph"
        );
        let parent = match parent {
            None => GRAPH_NODE.to_owned(),
            Some(p) => {
                // JS graphlib throws here; that only happens on malformed
                // input (a subgraph nested inside itself), so instead of
                // aborting the render we ignore the assignment and leave the
                // node where it is.
                let mut ancestor = Some(p.to_owned());
                while let Some(a) = ancestor {
                    if a == v {
                        return;
                    }
                    ancestor = self.parent(&a);
                }
                self.set_node(p);
                p.to_owned()
            }
        };
        self.set_node(v);
        self.remove_from_parents_child_list(v);
        self.parent.insert(v.to_owned(), parent.clone());
        self.children
            .get_mut(&parent)
            .expect("parent child list")
            .insert(v, ());
    }

    fn remove_from_parents_child_list(&mut self, v: &str) {
        if let Some(p) = self.parent.get(v).cloned()
            && let Some(list) = self.children.get_mut(&p)
        {
            list.remove(v);
        }
    }

    pub fn parent(&self, v: &str) -> Option<String> {
        if self.is_compound {
            let p = self.parent.get(v)?;
            if p != GRAPH_NODE {
                return Some(p.clone());
            }
        }
        None
    }

    /// `children(v)`; `None` queries the top-level (root) children.
    pub fn children(&self, v: Option<&str>) -> Vec<String> {
        let v = v.unwrap_or(GRAPH_NODE);
        if self.is_compound {
            return self.children.get(v).map(JsMap::keys).unwrap_or_default();
        }
        if v == GRAPH_NODE {
            return self.nodes();
        }
        Vec::new()
    }

    pub fn predecessors(&self, v: &str) -> Vec<String> {
        self.preds.get(v).map(JsMap::keys).unwrap_or_default()
    }

    pub fn successors(&self, v: &str) -> Vec<String> {
        self.sucs.get(v).map(JsMap::keys).unwrap_or_default()
    }

    /// Union of predecessors and successors, predecessors-first as in lodash
    /// `_.union`.
    pub fn neighbors(&self, v: &str) -> Vec<String> {
        let mut result = self.predecessors(v);
        for s in self.successors(v) {
            if !result.contains(&s) {
                result.push(s);
            }
        }
        result
    }

    pub fn edge_count(&self) -> usize {
        self.edge_objs.len()
    }

    pub fn edges(&self) -> Vec<EdgeObj> {
        self.edge_objs.values().into_iter().cloned().collect()
    }

    /// `setEdge(v, w)` with no value: an existing edge is untouched; a new
    /// edge gets an `undefined` label.
    pub fn set_edge_default(&mut self, v: &str, w: &str) {
        self.set_edge_impl(v, w, None, false, None);
    }

    /// `setEdge(v, w, value, name?)`.
    pub fn set_edge(&mut self, v: &str, w: &str, value: E, name: Option<&str>) {
        self.set_edge_impl(v, w, Some(value), true, name);
    }

    /// `setEdge(edgeObj, value)`.
    pub fn set_edge_obj(&mut self, e: &EdgeObj, value: E) {
        self.set_edge_impl(&e.v, &e.w, Some(value), true, e.name.as_deref());
    }

    fn set_edge_impl(
        &mut self,
        v: &str,
        w: &str,
        value: Option<E>,
        value_specified: bool,
        name: Option<&str>,
    ) {
        let id = edge_args_to_id(self.is_directed, v, w, name);
        if self.edge_labels.contains_key(&id) {
            if value_specified {
                self.edge_labels.insert(id, value);
            }
            return;
        }
        assert!(
            name.is_none() || self.is_multigraph,
            "cannot set a named edge when isMultigraph = false"
        );
        self.set_node(v);
        self.set_node(w);
        self.edge_labels.insert(id.clone(), value);
        let edge_obj = edge_args_to_obj(self.is_directed, v, w, name);
        let (v, w) = (edge_obj.v.clone(), edge_obj.w.clone());
        let preds_w = self.preds.get_mut(&w).expect("preds of w");
        match preds_w.get_mut(&v) {
            Some(count) => *count += 1,
            None => {
                preds_w.insert(v.clone(), 1);
            }
        }
        let sucs_v = self.sucs.get_mut(&v).expect("sucs of v");
        match sucs_v.get_mut(&w) {
            Some(count) => *count += 1,
            None => {
                sucs_v.insert(w.clone(), 1);
            }
        }
        self.in_
            .get_mut(&w)
            .expect("in of w")
            .insert(id.clone(), edge_obj.clone());
        self.out
            .get_mut(&v)
            .expect("out of v")
            .insert(id.clone(), edge_obj.clone());
        self.edge_objs.insert(id, edge_obj);
    }

    pub fn edge(&self, v: &str, w: &str, name: Option<&str>) -> Option<E> {
        let id = edge_args_to_id(self.is_directed, v, w, name);
        self.edge_labels.get(&id).and_then(Clone::clone)
    }

    pub fn edge_for(&self, e: &EdgeObj) -> Option<E> {
        self.edge(&e.v, &e.w, e.name.as_deref())
    }

    pub fn has_edge(&self, v: &str, w: &str, name: Option<&str>) -> bool {
        let id = edge_args_to_id(self.is_directed, v, w, name);
        self.edge_labels.contains_key(&id)
    }

    pub fn remove_edge(&mut self, v: &str, w: &str, name: Option<&str>) {
        let id = edge_args_to_id(self.is_directed, v, w, name);
        let Some(edge) = self.edge_objs.get(&id).cloned() else {
            return;
        };
        let (v, w) = (edge.v, edge.w);
        self.edge_labels.remove(&id);
        self.edge_objs.remove(&id);
        if let Some(preds_w) = self.preds.get_mut(&w)
            && let Some(count) = preds_w.get_mut(&v)
        {
            *count -= 1;
            if *count == 0 {
                preds_w.remove(&v);
            }
        }
        if let Some(sucs_v) = self.sucs.get_mut(&v)
            && let Some(count) = sucs_v.get_mut(&w)
        {
            *count -= 1;
            if *count == 0 {
                sucs_v.remove(&w);
            }
        }
        if let Some(in_w) = self.in_.get_mut(&w) {
            in_w.remove(&id);
        }
        if let Some(out_v) = self.out.get_mut(&v) {
            out_v.remove(&id);
        }
    }

    pub fn remove_edge_obj(&mut self, e: &EdgeObj) {
        self.remove_edge(&e.v, &e.w, e.name.as_deref());
    }

    pub fn in_edges(&self, v: &str, u: Option<&str>) -> Vec<EdgeObj> {
        let Some(in_v) = self.in_.get(v) else {
            return Vec::new();
        };
        let edges: Vec<EdgeObj> = in_v.values().into_iter().cloned().collect();
        match u {
            None => edges,
            Some(u) => edges.into_iter().filter(|e| e.v == u).collect(),
        }
    }

    pub fn out_edges(&self, v: &str, w: Option<&str>) -> Vec<EdgeObj> {
        let Some(out_v) = self.out.get(v) else {
            return Vec::new();
        };
        let edges: Vec<EdgeObj> = out_v.values().into_iter().cloned().collect();
        match w {
            None => edges,
            Some(w) => edges.into_iter().filter(|e| e.w == w).collect(),
        }
    }

    pub fn node_edges(&self, v: &str, w: Option<&str>) -> Vec<EdgeObj> {
        let mut edges = self.in_edges(v, w);
        edges.extend(self.out_edges(v, w));
        edges
    }
}

/// graphlib `alg.postorder` / `alg.preorder` over directed (successors) or
/// undirected (neighbors) graphs.
pub mod alg {
    use std::collections::HashSet;

    use super::Graph;

    fn do_dfs<G, N: Clone, E: Clone>(
        g: &Graph<G, N, E>,
        v: &str,
        postorder: bool,
        visited: &mut HashSet<String>,
        acc: &mut Vec<String>,
    ) {
        if visited.contains(v) {
            return;
        }
        visited.insert(v.to_owned());
        if !postorder {
            acc.push(v.to_owned());
        }
        let next = if g.is_directed() {
            g.successors(v)
        } else {
            g.neighbors(v)
        };
        for w in next {
            do_dfs(g, &w, postorder, visited, acc);
        }
        if postorder {
            acc.push(v.to_owned());
        }
    }

    fn dfs<G, N: Clone, E: Clone>(
        g: &Graph<G, N, E>,
        vs: &[String],
        postorder: bool,
    ) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut acc = Vec::new();
        for v in vs {
            assert!(g.has_node(v), "graph does not have node: {v}");
            do_dfs(g, v, postorder, &mut visited, &mut acc);
        }
        acc
    }

    pub fn postorder<G, N: Clone, E: Clone>(g: &Graph<G, N, E>, vs: &[String]) -> Vec<String> {
        dfs(g, vs, true)
    }

    pub fn preorder<G, N: Clone, E: Clone>(g: &Graph<G, N, E>, vs: &[String]) -> Vec<String> {
        dfs(g, vs, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multigraph_edges_in_insertion_order() {
        let mut g: Graph<(), i32, i32> = Graph::new(GraphOptions {
            multigraph: Some(true),
            ..Default::default()
        });
        g.set_edge("a", "b", 1, Some("e1"));
        g.set_edge("a", "b", 2, Some("e2"));
        g.set_edge("a", "b", 3, None);
        let edges = g.edges();
        assert_eq!(edges.len(), 3);
        assert_eq!(edges[0].name.as_deref(), Some("e1"));
        assert_eq!(edges[2].name, None);
        assert_eq!(g.edge("a", "b", Some("e2")), Some(2));
    }

    #[test]
    fn undirected_edge_normalizes_endpoints() {
        let mut g: Graph<(), i32, i32> = Graph::new(GraphOptions {
            directed: Some(false),
            ..Default::default()
        });
        g.set_edge("b", "a", 7, None);
        assert_eq!(g.edge("a", "b", None), Some(7));
        assert_eq!(g.edges()[0].v, "a");
    }

    #[test]
    fn removing_node_removes_incident_edges() {
        let mut g: Graph<(), i32, i32> = Graph::new(GraphOptions::default());
        g.set_edge("a", "b", 1, None);
        g.set_edge("b", "c", 2, None);
        g.remove_node("b");
        assert_eq!(g.edge_count(), 0);
        assert!(g.has_node("a") && g.has_node("c"));
    }

    #[test]
    fn compound_parent_children() {
        let mut g: Graph<(), i32, i32> = Graph::new(GraphOptions {
            compound: Some(true),
            ..Default::default()
        });
        g.set_parent("a", Some("p"));
        g.set_parent("b", Some("p"));
        assert_eq!(g.parent("a").as_deref(), Some("p"));
        assert_eq!(g.children(Some("p")), vec!["a", "b"]);
        assert_eq!(g.children(None), vec!["p"]);
        g.remove_node("p");
        assert_eq!(g.parent("a"), None);
    }
}
