//! Structural smoke tests for the `system_chart` diagram — a sebastian
//! extension with no mermaid equivalent, so there is no reference SVG to
//! byte-diff against `mmdc`. Gated on the `mermaid-extensions` feature
//! (default).

#![cfg(feature = "mermaid-extensions")]

use sebastian::render_diagram;

const SAMPLE: &str = r#"system_chart
  title Query pipeline
  query: chat "AI Agent Query" "What is our churn rate?"
  rt: router "Router" "(Classify)"
  okf: wiki "OKF" "(Wiki)"
  rag: db "RAG" "(Vector DB)"
  ai: llm "LLM" "(Synthesize)"
  query --> rt
  rt --> okf : Canonical?
  rt --> rag : Exploratory?
  okf --> ai
  rag --> ai
"#;

#[test]
fn system_chart_renders_nodes_and_edges() {
    let svg = render_diagram(SAMPLE, "my-svg").expect("system_chart renders");
    assert!(svg.contains("aria-roledescription=\"system_chart\""));
    assert_eq!(svg.matches("class=\"system-chart-node\"").count(), 5);
    assert_eq!(svg.matches("class=\"system-chart-edge\"").count(), 5);
    // Both edge labels are present.
    assert!(svg.contains("Canonical?"));
    assert!(svg.contains("Exploratory?"));
    // Title and a subtitle made it into the output.
    assert!(svg.contains("Query pipeline"));
    assert!(svg.contains("(Vector DB)"));
}

#[test]
fn system_chart_node_without_subtitle_renders() {
    let src = "system_chart\n  a: user \"Alice\"\n  b: db \"Store\"\n  a --> b\n";
    let svg = render_diagram(src, "my-svg").expect("renders");
    assert_eq!(svg.matches("class=\"system-chart-node\"").count(), 2);
    assert_eq!(svg.matches("class=\"system-chart-edge\"").count(), 1);
}

#[test]
fn system_chart_edge_kinds_render_distinct_styles() {
    let src = "system_chart\n  a: user \"A\"\n  b: db \"B\"\n  c: queue \"C\"\n  d: box \"D\"\n\
               a --> b\n  a ..> c : evt\n  c ==> d : msg\n  b --- d\n";
    let svg = render_diagram(src, "my-svg").expect("renders");
    assert_eq!(svg.matches("system-chart-edge-event").count(), 2); // class + css
    assert_eq!(svg.matches("system-chart-edge-message").count(), 2);
    assert_eq!(svg.matches("system-chart-edge-assoc").count(), 2);
    // Message edges get an envelope glyph; assoc edges have no arrowhead.
    assert!(svg.contains("class=\"system-chart-envelope\""));
    assert_eq!(
        svg.matches("marker-end").count(),
        3,
        "assoc edge must not carry an arrowhead"
    );
}

#[test]
fn system_chart_hand_drawn_sketches_boxes_and_edges() {
    let src = format!("%%{{init: {{\"look\": \"handDrawn\"}}}}%%\n{SAMPLE}");
    let svg = render_diagram(&src, "my-svg").expect("renders");
    // Node boxes become rough polygons (no crisp <rect> nodes) and edges
    // become multi-pass wobbly paths (several M subpaths per edge).
    assert!(!svg.contains("class=\"system-chart-node-rect\""));
    assert_eq!(svg.matches("class=\"system-chart-edge\"").count(), 5);
}

#[test]
fn system_chart_legend_lists_only_used_connection_types() {
    // Off by default.
    let svg = render_diagram(SAMPLE, "my-svg").expect("renders");
    assert!(!svg.contains("class=\"system-chart-legend\""));
    // On: SAMPLE uses only --> edges, so one row ("call"), no "event" row.
    let with_legend = SAMPLE.replacen("title", "legend\n  title", 1);
    let svg = render_diagram(&with_legend, "my-svg").expect("renders");
    assert!(svg.contains("class=\"system-chart-legend\""));
    assert!(svg.contains(">call</text>"));
    assert!(!svg.contains(">event</text>"));
    assert!(!svg.contains(">association</text>"));
}

#[test]
fn system_chart_boxless_nodes_drop_the_box_and_grow_the_icon() {
    let src = "system_chart\n  a: (user) \"Alice\" \"(Admin)\"\n  b: db \"Store\"\n  a --> b\n";
    let svg = render_diagram(src, "my-svg").expect("renders");
    // Only the boxed node gets a rect; the bare node is icon + text.
    assert_eq!(svg.matches("class=\"system-chart-node-rect\"").count(), 1);
    assert_eq!(
        svg.matches("class=\"system-chart-node system-chart-node-bare\"")
            .count(),
        1
    );
    // Bare icons are drawn larger: 60/24 = 2.5 vs boxed 39/24 = 1.625.
    assert!(svg.contains("scale(2.5)"));
    assert!(svg.contains("scale(1.625)"));
    // Title and subtitle still render.
    assert!(svg.contains("Alice"));
    assert!(svg.contains("(Admin)"));
}

#[test]
fn system_chart_boxless_hand_drawn_sketches_no_box() {
    let src = "%%{init: {\"look\": \"handDrawn\"}}%%\nsystem_chart\n  a: (user) \"Alice\"\n";
    let svg = render_diagram(src, "my-svg").expect("renders");
    // Hand-drawn boxed nodes emit rough fill+outline paths carrying the
    // symbol's fill tint; a bare node must emit only its icon and text.
    assert!(svg.contains("system-chart-node-bare"));
    assert!(!svg.contains("#F0F9FF"), "no box fill tint for a bare node");
}

#[test]
fn system_chart_rejects_undeclared_edge_endpoint() {
    let src = "system_chart\n  a: user \"Alice\"\n  a --> ghost\n";
    let err = render_diagram(src, "my-svg").expect_err("undeclared node");
    assert!(err.to_string().contains("ghost"));
}

#[test]
fn system_chart_rejects_missing_title() {
    let src = "system_chart\n  a: user\n";
    assert!(render_diagram(src, "my-svg").is_err());
}
