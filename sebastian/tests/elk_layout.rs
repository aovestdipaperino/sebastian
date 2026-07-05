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
