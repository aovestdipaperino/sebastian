//! Structural smoke tests for the `pyramid` diagram — a sebastian extension
//! (no mermaid equivalent), so it is validated by structure rather than a
//! byte-diff against `mmdc`. Gated on the `mermaid-extensions` feature (default).

#![cfg(feature = "mermaid-extensions")]

use sebastian::render_diagram;

#[test]
fn pyramid_hand_drawn_sketches_bands_and_component_boxes() {
    let src = "%%{init: {\"look\": \"handDrawn\"}}%%\npyramid\n  title Arch\n\
               Presentation: Web, Mobile\n  Data\n";
    let svg = render_diagram(src, "my-svg").expect("renders");
    // Bands and component boxes become rough paths: no crisp <polygon>
    // bands and no crisp component <rect>s.
    assert!(!svg.contains("class=\"pyramid-band\""));
    assert!(!svg.contains("class=\"pyramid-component-rect\""));
    assert!(svg.contains("class=\"pyramid-component\""));
    // Deterministic wobble.
    let svg2 = render_diagram(src, "my-svg").expect("again");
    assert_eq!(svg, svg2);
}

#[test]
fn pyramid_chart_renders_stacked_bands() {
    let src = "pyramid\n  title Company Hierarchy\n  CEO\n  Directors\n  Managers\n  Staff\n";
    let svg = render_diagram(src, "my-svg").expect("pyramid renders");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("aria-roledescription=\"pyramid\""));
    assert!(svg.contains("viewBox="));
    // One trapezoid band per level.
    assert_eq!(svg.matches("class=\"pyramid-band\"").count(), 4);
    for needle in ["Company Hierarchy", "CEO", "Directors", "Managers", "Staff"] {
        assert!(svg.contains(needle), "missing {needle:?}");
    }
    // Deterministic.
    let svg2 = render_diagram(src, "my-svg").expect("again");
    assert_eq!(svg, svg2);
}

#[test]
fn pyramid_of_components_renders_boxes_per_tier() {
    let src = "pyramid\n  title System Architecture\n\
        Presentation: Web, Mobile\n\
        Business: Auth, Orders, Billing\n\
        Data: Postgres, Redis, Queue\n";
    let svg = render_diagram(src, "my-svg").expect("pyramid renders");
    assert!(svg.contains("aria-roledescription=\"pyramid\""));
    assert_eq!(svg.matches("class=\"pyramid-band\"").count(), 3);
    // 2 + 3 + 3 = 8 component boxes.
    assert_eq!(svg.matches("class=\"pyramid-component-rect\"").count(), 8);
    for needle in [
        "Web", "Mobile", "Auth", "Orders", "Billing", "Postgres", "Redis", "Queue",
    ] {
        assert!(svg.contains(needle), "missing {needle:?}");
    }
}

#[test]
fn pyramid_mixes_plain_and_component_levels() {
    let src = "pyramid\n  Vision\n  Strategy: Plan A, Plan B\n  Execution\n";
    let svg = render_diagram(src, "my-svg").expect("pyramid renders");
    assert_eq!(svg.matches("class=\"pyramid-band\"").count(), 3);
    assert_eq!(svg.matches("class=\"pyramid-component-rect\"").count(), 2);
    assert!(svg.contains("Vision") && svg.contains("Execution") && svg.contains("Plan A"));
}
