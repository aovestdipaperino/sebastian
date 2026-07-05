//! ELK layered-layout backend for `layout: elk` flowcharts, via the native-Rust
//! [`elkrs`] crate (a port of the Eclipse Layout Kernel).
//!
//! This module builds the ELK graph exactly as mermaid's `@mermaid-js/layout-elk`
//! does — same `layoutOptions`, same per-node dimensions (which sebastian already
//! measures byte-exact for flowcharts), same edge-label reservations — runs it
//! through `elkrs`, and reads back node coordinates and edge routing.
//!
//! **Byte-exactness.** A 2026-07 investigation captured the exact ELK-JSON
//! mermaid feeds elkjs 0.9.3 for a real flowchart and ran the identical input
//! through `elkrs` (ELK 0.11): every node coordinate was byte-identical. So node
//! **placement** is byte-exact for the acyclic case; only cyclic/back-edge graphs
//! can diverge (ELK's cycle-breaking changed between 0.9 and 0.11). Edge routing
//! (bendpoints/label placement) still needs the `layout-elk` geometry port before
//! whole-SVG byte-exactness — this module currently exposes placement + raw ELK
//! edge sections.
//!
//! Gated behind the `elk` cargo feature so the default build stays lean.

use serde_json::{Value, json};

/// A node to lay out: its ELK id and measured dimensions.
#[derive(Debug, Clone)]
pub struct ElkNodeInput {
    pub id: String,
    pub width: f64,
    pub height: f64,
}

/// An edge to lay out, with its (already measured) label box.
#[derive(Debug, Clone)]
pub struct ElkEdgeInput {
    pub id: String,
    pub source: String,
    pub target: String,
    /// Edge label text (empty when the edge has no label). ELK only reserves a
    /// label layer — which affects between-layer spacing — when this is
    /// non-empty, so it must be threaded through, not just the measured size.
    pub label_text: String,
    /// Measured edge-label size (0×0 when the edge has no label).
    pub label_width: f64,
    pub label_height: f64,
}

/// A laid-out node: top-left ELK coordinates and its size echoed back.
#[derive(Debug, Clone)]
pub struct ElkNodeLayout {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A point on a routed edge.
#[derive(Debug, Clone, Copy)]
pub struct ElkPoint {
    pub x: f64,
    pub y: f64,
}

/// A laid-out edge: the ELK section polyline (start → bendpoints → end).
#[derive(Debug, Clone)]
pub struct ElkEdgeLayout {
    pub id: String,
    pub points: Vec<ElkPoint>,
}

/// The result of an ELK layout pass.
#[derive(Debug, Clone)]
pub struct ElkLayout {
    pub nodes: Vec<ElkNodeLayout>,
    pub edges: Vec<ElkEdgeLayout>,
    pub width: f64,
    pub height: f64,
}

/// Maps a mermaid flow direction to ELK's `elk.direction`.
#[must_use]
pub fn elk_direction(flow_dir: &str) -> &'static str {
    match flow_dir {
        "BT" => "UP",
        "LR" => "RIGHT",
        "RL" => "LEFT",
        // "TB", "TD", and anything else default to DOWN (mermaid's default).
        _ => "DOWN",
    }
}

/// Builds the ELK graph JSON mermaid's `layout-elk` feeds to elkjs.
///
/// The `layoutOptions` and structure mirror the captured mermaid input verbatim
/// so `elkrs` reproduces the same coordinates.
#[must_use]
pub fn build_elk_json(nodes: &[ElkNodeInput], edges: &[ElkEdgeInput], flow_dir: &str) -> Value {
    let children: Vec<Value> = nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "width": n.width,
                "height": n.height,
            })
        })
        .collect();

    let edges_json: Vec<Value> = edges
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "sources": [e.source],
                "targets": [e.target],
                "labels": [{
                    "width": e.label_width,
                    "height": e.label_height,
                    "text": e.label_text,
                    "layoutOptions": {
                        "edgeLabels.inline": "true",
                        "edgeLabels.placement": "CENTER",
                    },
                }],
            })
        })
        .collect();

    json!({
        "id": "root",
        "layoutOptions": {
            "elk.hierarchyHandling": "INCLUDE_CHILDREN",
            "elk.algorithm": "elk.layered",
            "nodePlacement.strategy": "BRANDES_KOEPF",
            "elk.layered.mergeEdges": false,
            "elk.direction": elk_direction(flow_dir),
            "spacing.baseValue": 35,
            "elk.layered.unnecessaryBendpoints": true,
        },
        "children": children,
        "edges": edges_json,
    })
}

/// Parses an `elkrs` layout result into [`ElkLayout`].
#[cfg(feature = "elk")]
fn parse_layout(v: &Value) -> ElkLayout {
    let nodes = v["children"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|c| ElkNodeLayout {
                    id: c["id"].as_str().unwrap_or_default().to_owned(),
                    x: c["x"].as_f64().unwrap_or(0.0),
                    y: c["y"].as_f64().unwrap_or(0.0),
                    width: c["width"].as_f64().unwrap_or(0.0),
                    height: c["height"].as_f64().unwrap_or(0.0),
                })
                .collect()
        })
        .unwrap_or_default();

    let edges = v["edges"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|e| {
                    let mut points = Vec::new();
                    // ELK edge routing lives in `sections[].{startPoint,bendPoints,endPoint}`.
                    if let Some(sections) = e["sections"].as_array() {
                        for s in sections {
                            push_point(&mut points, &s["startPoint"]);
                            if let Some(bends) = s["bendPoints"].as_array() {
                                for b in bends {
                                    push_point(&mut points, b);
                                }
                            }
                            push_point(&mut points, &s["endPoint"]);
                        }
                    }
                    ElkEdgeLayout {
                        id: e["id"].as_str().unwrap_or_default().to_owned(),
                        points,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    ElkLayout {
        nodes,
        edges,
        width: v["width"].as_f64().unwrap_or(0.0),
        height: v["height"].as_f64().unwrap_or(0.0),
    }
}

#[cfg(feature = "elk")]
fn push_point(points: &mut Vec<ElkPoint>, p: &Value) {
    if let (Some(x), Some(y)) = (p["x"].as_f64(), p["y"].as_f64()) {
        points.push(ElkPoint { x, y });
    }
}

/// Runs an ELK layered layout over the given nodes/edges.
///
/// # Errors
/// Returns the `elkrs` error string when layout fails.
#[cfg(feature = "elk")]
pub fn layout(
    nodes: &[ElkNodeInput],
    edges: &[ElkEdgeInput],
    flow_dir: &str,
) -> Result<ElkLayout, String> {
    let graph = build_elk_json(nodes, edges, flow_dir);
    let elk = elkrs::create_elk();
    let result = elk.layout_json(&graph.to_string())?;
    Ok(parse_layout(&result))
}
