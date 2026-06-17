//! Tests for the opt-in `look: handDrawn` stylization. Output is not
//! byte-exact against mmdc (mermaid's rough.js output is randomized), but the
//! seeded PRNG makes sebastian's output deterministic, so we assert the
//! hand-drawn markers, determinism, and that classic look is unaffected.

use sebastian::render::render_flowchart;

const SRC: &str = "flowchart TD\n  A[Start] --> B{Decision}\n  B -->|yes| C[Do it]\n";

fn hand_drawn() -> String {
    let init = "%%{init: {'look':'handDrawn', 'htmlLabels': false, 'flowchart': {'htmlLabels': false}}}%%\n";
    render_flowchart(&format!("{init}{SRC}"), "my-svg").expect("render")
}

#[test]
fn nodes_are_rough_and_handwritten() {
    let svg = hand_drawn();
    // Nodes carry the rough-node class instead of node.
    assert!(
        svg.contains("class=\"rough-node"),
        "expected rough-node class"
    );
    // The handwritten font override is present.
    assert!(
        svg.contains("Comic Sans MS"),
        "expected handwritten font css"
    );
    // Rect/diamond shapes render as sketchy <path> groups, not plain <rect>.
    assert!(
        !svg.contains("<rect class=\"basic label-container\""),
        "hand-drawn nodes should not emit a plain label-container rect"
    );
    assert!(svg.contains("<path"), "expected sketchy path strokes");
}

#[test]
fn output_is_deterministic() {
    assert_eq!(hand_drawn(), hand_drawn(), "seeded output must be stable");
}

#[test]
fn classic_look_is_unaffected() {
    let svg = render_flowchart(SRC, "my-svg").expect("render");
    // The `.rough-node` selector always appears in the stylesheet; what must
    // not appear is a node carrying that class, or the handwritten font.
    assert!(
        !svg.contains("class=\"rough-node"),
        "classic nodes must not use the rough-node class"
    );
    assert!(
        !svg.contains("Comic Sans MS"),
        "classic must not swap the font"
    );
    assert!(
        svg.contains("class=\"node default\""),
        "classic keeps node class"
    );
}
