//! Byte-exact validation of the `elk` layout backend (feature-gated).
//!
//! The node dimensions and expected coordinates below were captured from
//! mermaid 11.15.0 itself: the exact ELK-JSON `@mermaid-js/layout-elk` feeds
//! elkjs 0.9.3 for the flowchart
//!
//! ```text
//! flowchart TB
//!   A[Start] --> B{Decision}
//!   B -->|yes| C[OK]
//!   B -->|no| D[Stop]
//!   C --> E[End]
//!   D --> E
//! ```
//!
//! was intercepted (by hooking `ELK.prototype.layout` in an esbuild bundle run
//! under puppeteer), and elkjs's output coordinates recorded. This test feeds
//! the identical graph through `elkrs` and asserts the coordinates match
//! byte-for-byte — proving the ELK placement half of the port is byte-exact.
//!
//! Run with: `cargo test -p sebastian --features elk --test elk_layout`

#![cfg(feature = "elk")]

use sebastian::render::elk::{ElkEdgeInput, ElkNodeInput, layout};

fn node(id: &str, w: f64, h: f64) -> ElkNodeInput {
    ElkNodeInput {
        id: id.to_owned(),
        width: w,
        height: h,
    }
}

fn edge(id: &str, s: &str, t: &str, text: &str, lw: f64, lh: f64) -> ElkEdgeInput {
    ElkEdgeInput {
        id: id.to_owned(),
        source: s.to_owned(),
        target: t.to_owned(),
        label_text: text.to_owned(),
        label_width: lw,
        label_height: lh,
    }
}

#[test]
fn elk_placement_matches_mermaid_elkjs_byte_for_byte() {
    // Node dimensions as measured by mermaid (byte-exact; sebastian's flowchart
    // measurement produces the same values).
    let nodes = vec![
        node("A", 95.0078125, 54.0),
        node("B", 113.390625, 113.390625),
        node("C", 79.9921875, 54.0),
        node("D", 91.5390625, 54.0),
        node("E", 86.2265625, 54.0),
    ];
    let edges = vec![
        edge("L_A_B_0_0", "A", "B", "", 0.0, 0.0),
        edge("L_B_C_0_0", "B", "C", "yes", 23.09375, 24.0),
        edge("L_B_D_0_0", "B", "D", "no", 17.328125, 24.0),
        edge("L_C_E_0_0", "C", "E", "", 0.0, 0.0),
        edge("L_D_E_0_0", "D", "E", "", 0.0, 0.0),
    ];

    let result = layout(&nodes, &edges, "TB").expect("elk layout");

    // Expected top-left coordinates from mermaid's elkjs 0.9.3 output.
    let expected: &[(&str, f64, f64)] = &[
        ("A", 29.1640625, 12.0),
        ("B", 19.97265625, 101.0),
        ("C", 138.5390625, 308.390625),
        ("D", 12.0, 308.390625),
        ("E", 29.02734375, 397.390625),
    ];

    for (id, ex, ey) in expected {
        let n = result
            .nodes
            .iter()
            .find(|n| n.id == *id)
            .unwrap_or_else(|| panic!("node {id} in layout"));
        assert_eq!(n.x, *ex, "node {id} x: got {} want {ex}", n.x);
        assert_eq!(n.y, *ey, "node {id} y: got {} want {ey}", n.y);
    }
}

#[test]
fn elk_subgraph_falls_back_to_dagre_and_renders_cluster() {
    // ELK cluster layout isn't ported; a `layout: elk` flowchart *with a
    // subgraph* must fall back to dagre for the whole render so the cluster box
    // is drawn correctly (not a broken zero-height rect).
    let src = "%%{init: {\"layout\": \"elk\"}}%%\nflowchart TB\n\
        A[Start] --> B\n\
        subgraph one [Group One]\n\
        B[In group] --> C[Also in]\n\
        end\n\
        C --> D[End]\n";
    let svg = sebastian::render_diagram(src, "my-svg").expect("elk subgraph renders");
    assert!(svg.contains("Group One"), "cluster label present");
    assert!(svg.contains("class=\"cluster\""), "cluster group present");
    // The broken-ELK symptom was a zero-height cluster rect; ensure it's gone.
    assert!(
        !svg.contains("height=\"0\"/>"),
        "no zero-height cluster rect (broken ELK cluster)"
    );
}

#[test]
fn elk_directions_and_selfloops_render() {
    // Flat ELK graphs across directions + a self-loop should render coherently.
    for src in [
        "%%{init: {\"layout\": \"elk\"}}%%\nflowchart LR\n  A[A]-->B[B]-->C[C]\n",
        "%%{init: {\"layout\": \"elk\"}}%%\nflowchart TB\n  A[A]-->A\n  A-->B[B]\n",
        "%%{init: {\"layout\": \"elk\"}}%%\nflowchart TB\n  A-->B\n  A-->B\n",
    ] {
        let svg = sebastian::render_diagram(src, "my-svg").expect("renders");
        assert!(svg.starts_with("<svg"));
        assert!(svg.contains("class=\"flowchart\""));
    }
}

#[test]
fn elk_flowchart_render_places_nodes_near_exact() {
    // End-to-end: sebastian parses + measures the flowchart, routes layout
    // through the ELK backend, and positions node groups (center = ELK top-left
    // + size/2). Placement matches mermaid's own ELK render to within ~1/128px.
    //
    // The residual gap is a *known* node-dimension measurement-path difference:
    // mermaid's `@mermaid-js/layout-elk` feeds ELK a node width that is exactly
    // 1/128 smaller than the drawn rect (which sebastian reproduces byte-exact
    // for dagre via Blink's round-up `getBBox`). Closing it to byte-exact needs
    // the layout-elk node-dimension measurement ported. The `y` layer positions
    // (from node heights) are already exact. See TODO.md.
    let src = "%%{init: {\"layout\": \"elk\"}}%%\nflowchart TB\n\
        A[Start] --> B{Decision}\n\
        B -->|yes| C[OK]\n\
        B -->|no| D[Stop]\n\
        C --> E[End]\n\
        D --> E\n";
    let svg = sebastian::render_diagram(src, "my-svg").expect("elk flowchart renders");

    // (expected center from mermaid's ELK render, per node).
    let expected: &[(f64, f64)] = &[
        (76.66796875, 39.0),
        (76.66796875, 157.6953125),
        (178.53515625, 335.390625),
        (57.76953125, 335.390625),
        (72.140625, 424.390625),
    ];
    let got: Vec<(f64, f64)> = svg
        .match_indices("<g class=\"node default")
        .filter_map(|(i, _)| {
            let rest = &svg[i..];
            let t = rest.find("transform=\"translate(")? + "transform=\"translate(".len();
            let close = rest[t..].find(')')?;
            let inner = &rest[t..t + close];
            let (x, y) = inner.split_once(", ")?;
            Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
        })
        .collect();

    assert_eq!(got.len(), expected.len(), "node count");
    for ((gx, gy), (ex, ey)) in got.iter().zip(expected) {
        assert!(
            (gx - ex).abs() < 0.02,
            "node x {gx} vs {ex} (gap {})",
            (gx - ex).abs()
        );
        assert!((gy - ey).abs() < 1e-6, "node y {gy} vs {ey} must be exact");
    }
}
