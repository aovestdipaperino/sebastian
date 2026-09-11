//! End-to-end tests for the `htmlLabels: false` **state diagram** path.
//!
//! State nodes set `nodeData.centerLabel = true` in mermaid's stateDb, and
//! `labelHelper` lets that override the SVG-text transform:
//!
//! ```js
//! if (useHtmlLabels) { label.attr('transform', translate(-w/2, -h/2)); }
//! else               { label.attr('transform', translate(0,    -h/2)); }
//! if (node.centerLabel) { label.attr('transform', translate(-w/2, -h/2)); }
//! ```
//!
//! With html labels on, the first branch already centres, so the existing
//! `state_corpus` never exercised the override. With them off (how mex
//! rasterizes, since resvg cannot draw foreignObject) the missing override
//! left every state label starting at the shape's centre and running off its
//! right edge: state diagrams emit no `text-anchor: middle` rule for node
//! labels, unlike flowcharts.
//!
//! References captured from mermaid-cli 11.15.0 with
//! `-c '{"htmlLabels": false, "flowchart": {"htmlLabels": false}}'`.
//!
//! Requires the system Trebuchet MS font (present on macOS/Windows).

use sebastian::render_diagram;

const INIT: &str = "%%{init: {'htmlLabels': false, 'flowchart': {'htmlLabels': false}}}%%\n";

fn case(name: &str) -> (String, String) {
    let dir = format!("{}/tests/state_nohtml_cases", env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(format!("{dir}/{name}.mmd")).expect("mmd source");
    let expected = std::fs::read_to_string(format!("{dir}/{name}.svg")).expect("reference svg");
    let actual = render_diagram(&format!("{INIT}{source}"), "my-svg").expect("render");
    (actual, expected)
}

/// Every `<g class="label" style="" transform="translate(x, y)">`, which is
/// the node label group `labelHelper` positions.
fn node_label_transforms(svg: &str) -> Vec<(f64, f64)> {
    const PAT: &str = "<g class=\"label\" style=\"\" transform=\"translate(";
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(i) = rest.find(PAT) {
        rest = &rest[i + PAT.len()..];
        let end = rest.find(')').expect("closing paren");
        let (a, b) = rest[..end].split_once(", ").expect("x, y");
        out.push((a.parse().expect("x"), b.parse().expect("y")));
        rest = &rest[end..];
    }
    out
}

/// The node label offsets must match mermaid's, and in particular must not be
/// zero: `translate(0, -h/2)` is the bug this file exists to prevent.
#[test]
fn state_node_labels_are_centered_like_mermaid() {
    let (actual, expected) = case("simple");
    let got = node_label_transforms(&actual);
    let want = node_label_transforms(&expected);

    assert!(!want.is_empty(), "reference should carry node labels");
    assert_eq!(got, want, "node label transforms differ from mermaid-cli");
    assert!(
        got.iter().all(|&(x, _)| x < 0.0),
        "every state label must be offset by -width/2, got {got:?}"
    );
}

/// `note` shapes set `centerLabel` themselves when html labels are off.
///
/// Compared with a tolerance rather than byte-for-byte: sebastian's no-html
/// note path has two divergences from mermaid that predate `centerLabel` and
/// are out of scope here, a `noteLabel` class mermaid does not emit and a
/// ~0.5px difference in the measured note box. What must hold is that the
/// label is centred at all, which before the fix it was not (x was 0).
#[test]
fn note_labels_are_centered_like_mermaid() {
    let (actual, expected) = case("notes");
    let got = node_label_transforms(&actual);
    let want = node_label_transforms(&expected);

    // The two plain state nodes still have to match exactly.
    assert_eq!(got[..2], want[..2], "state node labels differ from mermaid");

    // The note label is found under its own class, so pick it up separately.
    const PAT: &str = "<g class=\"label noteLabel\" style=\"\" transform=\"translate(";
    let i = actual.find(PAT).expect("note label group");
    let rest = &actual[i + PAT.len()..];
    let end = rest.find(')').expect("closing paren");
    let (a, _) = rest[..end].split_once(", ").expect("x, y");
    let note_x: f64 = a.parse().expect("x");

    let want_note_x = want.last().expect("reference note label").0;
    assert!(
        note_x < 0.0,
        "note label must be offset by -width/2, got {note_x}"
    );
    assert!(
        (note_x - want_note_x).abs() < 1.0,
        "note label offset {note_x} should track mermaid's {want_note_x}"
    );
}

/// With html labels on, `centerLabel` is a no-op: the `useHtmlLabels` branch
/// already centres, so turning the flag on must not perturb the byte-exact
/// `state_corpus` output.
#[test]
fn html_label_path_is_unaffected() {
    let source = std::fs::read_to_string(format!(
        "{}/tests/state_nohtml_cases/simple.mmd",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("mmd source");
    let svg = render_diagram(&source, "my-svg").expect("render");
    assert!(
        svg.contains("<foreignObject"),
        "default path should still emit foreignObject labels"
    );
    for (x, _) in node_label_transforms(&svg) {
        assert!(x < 0.0, "html labels centre via -width/2 too, got {x}");
    }
}

/// A self-loop is split into three edges, and only the middle one keeps the
/// label. The two helper edges must have `label_raw` cleared as well as
/// `label`: the `htmlLabels: false` path renders `label_raw`, and
/// `position_edge_label` skips an edge whose `label` is empty, so a stray
/// copy would be drawn unpositioned at the container origin.
#[test]
fn self_loop_label_is_not_duplicated() {
    let source = "\
stateDiagram-v2
    Unauthenticated --> Authenticated : login() succeeds
    Unauthenticated --> Unauthenticated : login() fails
";
    let svg = render_diagram(&format!("{INIT}{source}"), "my-svg").expect("render");

    let fails = svg.matches("fails").count();
    assert_eq!(
        fails, 1,
        "the self-loop label should be rendered once, found {fails}"
    );

    // Every label group carrying text must be positioned; an unset transform
    // leaves it at the origin.
    for part in svg.split("<g class=\"edgeLabel\"").skip(1) {
        let head = &part[..part.find('>').expect("tag end")];
        let body = &part[..part.find("</g></g>").unwrap_or(part.len())];
        if body.contains("text-inner-tspan") {
            assert!(
                head.contains("transform=\"translate("),
                "a labelled edge was left unpositioned: {head:?}"
            );
        }
    }
}
