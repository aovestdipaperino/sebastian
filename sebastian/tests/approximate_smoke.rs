//! Smoke tests for the APPROXIMATE (non-byte-exact) renderers: mindmap,
//! architecture, requirement, and C4. Mindmap/architecture use sebastian's own
//! deterministic layouts (mermaid uses Math.random-seeded force engines);
//! requirement/C4 opt out of byte-exactness because mermaid sizes their boxes
//! with Blink `getBBox()` ink metrics we don't reproduce. All are validated
//! structurally — valid SVG with the expected elements — not by byte-diff.

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

#[test]
fn requirement_renders() {
    let src = "requirementDiagram\n\n\
        requirement test_req {\n  id: 1\n  text: the test text.\n  risk: high\n  verifyMethod: test\n}\n\n\
        element test_entity {\n  type: simulation\n}\n\n\
        test_entity - satisfies -> test_req\n";
    let svg = render_diagram(src, "my-svg").expect("requirement renders");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("class=\"requirementDiagram\""));
    assert!(svg.contains("viewBox="));
    // requirement + element = 2 node boxes, each an HTML label foreignObject,
    // plus the relationship edge label = 3 foreignObjects.
    assert_eq!(svg.matches("<foreignObject").count(), 3);
    for needle in [
        "Requirement",
        "test_req",
        "the test text",
        "Risk: High",
        "Verification: Test",
        "Element",
        "simulation",
        "satisfies",
    ] {
        assert!(svg.contains(needle), "missing {needle:?}");
    }
    let svg2 = render_diagram(src, "my-svg").expect("again");
    assert_eq!(svg, svg2);
}

#[test]
fn c4_renders() {
    let src = "C4Context\n\
        title System Context diagram for Internet Banking System\n\
        Enterprise_Boundary(b0, \"BankBoundary0\") {\n\
        Person(customerA, \"Banking Customer A\", \"A customer of the bank.\")\n\
        System(SystemAA, \"Internet Banking System\", \"Allows customers to view information.\")\n\
        System_Ext(SystemE, \"Mainframe Banking System\", \"Stores all core banking info.\")\n\
        }\n\
        Rel(customerA, SystemAA, \"Uses\")\n\
        Rel(SystemAA, SystemE, \"Uses\", \"SOAP/XML\")\n";
    let svg = render_diagram(src, "my-svg").expect("c4 renders");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("aria-roledescription=\"c4\""));
    assert!(svg.contains("viewBox="));
    assert_eq!(svg.matches("class=\"c4-shape-rect\"").count(), 3);
    assert_eq!(svg.matches("class=\"c4-boundary-rect\"").count(), 1);
    assert_eq!(svg.matches("class=\"c4-rel-line\"").count(), 2);
    assert_eq!(svg.matches("class=\"c4-person-head\"").count(), 1);
    for needle in [
        "Banking Customer A",
        "Internet Banking System",
        "Mainframe Banking System",
        "External System",
        "BankBoundary0",
        "SOAP/XML",
    ] {
        assert!(svg.contains(needle), "missing {needle:?}");
    }
    let svg2 = render_diagram(src, "my-svg").expect("again");
    assert_eq!(svg, svg2);
}
