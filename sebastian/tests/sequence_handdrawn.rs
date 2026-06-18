//! Tests for sebastian's `look: handDrawn` support in SEQUENCE diagrams.
//!
//! This is a sebastian-specific extension with NO upstream mermaid equivalent:
//! mermaid's legacy sequence renderer (`sequenceRenderer.ts` + `svgDraw.js`)
//! ignores `look` and always draws crisp shapes (see `src/sequence/render.rs`
//! module docs). Output is not byte-exact (sketch strokes wobble), but the
//! seeded PRNG keeps it deterministic, so we assert the hand-drawn markers and
//! that classic output is unchanged.

use sebastian::sequence::render_sequence;

const SRC: &str = "sequenceDiagram\n    Alice->>Bob: Hello\n    Note over Alice: hi\n    loop every minute\n        Bob-->>Alice: Ack\n    end\n";

fn hand_drawn() -> String {
    let init = "%%{init: {'look':'handDrawn'}}%%\n";
    render_sequence(&format!("{init}{SRC}"), "seq-svg").expect("render")
}

#[test]
fn boxes_are_sketchy_paths_not_rects() {
    let svg = hand_drawn();
    // Actor and note boxes are the only `<rect>` emitters; hand-drawn turns them
    // into sketchy `<path>` groups, so no plain rect should remain.
    assert!(
        !svg.contains("<rect"),
        "hand-drawn sequence should emit no plain <rect> boxes"
    );
    assert!(svg.contains("<path"), "expected sketchy <path> strokes");
    // The box classes are preserved on the sketchy `<g>` wrapper.
    assert!(
        svg.contains("class=\"actor actor-top\""),
        "actor box class preserved on the hand-drawn group"
    );
    assert!(
        svg.contains("class=\"note\""),
        "note box class preserved on the hand-drawn group"
    );
}

#[test]
fn output_is_deterministic() {
    assert_eq!(hand_drawn(), hand_drawn(), "seeded output must be stable");
}

#[test]
fn classic_look_is_unaffected() {
    let svg = render_sequence(SRC, "seq-svg").expect("render");
    // Classic keeps plain `<rect>` boxes and never wobbles.
    assert!(svg.contains("<rect"), "classic sequence uses <rect> boxes");
    assert!(
        svg.contains("class=\"actor actor-top\""),
        "classic actor box class"
    );
}
