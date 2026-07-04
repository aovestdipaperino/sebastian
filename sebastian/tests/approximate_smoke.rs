//! Smoke tests for the APPROXIMATE (non-byte-exact) renderers: mindmap and
//! architecture. These use sebastian's own deterministic layouts (mermaid uses
//! Math.random-seeded force engines), so they are validated structurally —
//! valid SVG with the expected elements — not by byte-diff against mmdc.

use sebastian::render_diagram;

#[test]
fn mindmap_renders() {
    let src = "mindmap\n  root((core))\n    A\n      A1\n    B\n";
    let svg = render_diagram(src, "my-svg").expect("mindmap renders");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("aria-roledescription=\"mindmap\""));
    assert!(svg.contains("viewBox="));
    // root + A + A1 + B = 4 nodes
    assert_eq!(svg.matches("class=\"mindmap-node section").count(), 4);
    // deterministic: same input → same output
    let svg2 = render_diagram(src, "my-svg").expect("again");
    assert_eq!(svg, svg2);
}

#[test]
fn architecture_renders() {
    let src = "architecture-beta\n  group api(cloud)[API]\n  service db(database)[DB] in api\n  service web(server)[Web] in api\n  db:R -- L:web\n";
    let svg = render_diagram(src, "my-svg").expect("architecture renders");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("aria-roledescription=\"architecture\""));
    assert_eq!(svg.matches("class=\"architecture-service\"").count(), 2);
    assert_eq!(svg.matches("class=\"architecture-group\"").count(), 1);
    assert_eq!(svg.matches("class=\"edge\"").count(), 1);
    let svg2 = render_diagram(src, "my-svg").expect("again");
    assert_eq!(svg, svg2);
}
