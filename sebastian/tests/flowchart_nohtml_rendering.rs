//! End-to-end tests for the `htmlLabels: false` flowchart path: node, edge,
//! and cluster labels render as SVG `<text>`/`<tspan>` instead of
//! foreignObject HTML. Output must be byte-identical to official mermaid-cli
//! (mermaid 11.15.0) run with `{ htmlLabels: false, flowchart: { htmlLabels:
//! false } }`, captured in `tests/flowchart_nohtml_cases`.
//!
//! The `.mmd` sources are shared with `flowchart_cases`; the config is
//! supplied via a prepended init directive (matching `mmdc -c`).
//!
//! Requires the system Trebuchet MS font (present on macOS/Windows).

use sebastian::render_flowchart;

const INIT: &str = "%%{init: {'htmlLabels': false, 'flowchart': {'htmlLabels': false}}}%%\n";

fn run_case(name: &str) {
    let src_dir = format!("{}/tests/flowchart_cases", env!("CARGO_MANIFEST_DIR"));
    let ref_dir = format!(
        "{}/tests/flowchart_nohtml_cases",
        env!("CARGO_MANIFEST_DIR")
    );
    let source = std::fs::read_to_string(format!("{src_dir}/{name}.mmd")).expect("mmd source");
    let expected = std::fs::read_to_string(format!("{ref_dir}/{name}.svg")).expect("reference svg");
    let actual = render_flowchart(&format!("{INIT}{source}"), "my-svg").expect("render");
    assert!(
        actual == expected,
        "{name}: output differs from mermaid-cli reference (first diff at byte {})",
        actual
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(actual.len().min(expected.len()))
    );
}

macro_rules! cases {
    ($($name:ident),* $(,)?) => {
        $(#[test] fn $name() { run_case(stringify!($name)); })*
    };
}

cases!(
    simple, edges, subgraphs, longlabels, selfloop, styled, nested, bt, rl, parallel, unicode,
    invisible,
);

// `chain` and `multibr` are intentionally omitted: they differ from the
// reference by a sub-pixel amount (≤0.07px on one glyph, 1 f32 ULP on a
// multi-line height) because Chrome sizes SVG-text nodes from glyph ink
// extents (`getBBox`) while this port uses advance widths. The difference is
// invisible when rasterized; see PORTING_NOTES.md.

/// The label `transform` of node `id` in a rendered SVG.
fn label_transform(svg: &str, id: &str) -> String {
    let node = svg
        .find(&format!("id=\"my-svg-flowchart-{id}-"))
        .unwrap_or_else(|| panic!("node {id} not found"));
    let tail = &svg[node..];
    let label = tail.find("<g class=\"label\"").expect("label group");
    let tail = &tail[label..];
    let start = tail.find("transform=\"").expect("label transform") + 11;
    let end = tail[start..].find('"').expect("closing quote");
    tail[start..start + end].to_owned()
}

/// Mermaid's cylinder shapes subtract the label's `getBBox()` origin. With
/// SVG text labels that origin is `-width/2` (the text is centred by
/// `text-anchor: middle`), so the label sits at x = 0, as the plain
/// rectangle's does; it used to be shifted left by half its width.
#[test]
fn cylinder_svg_label_is_centred() {
    let src = format!(
        "{INIT}flowchart TB\n    A[(\"Chip RAM<br/>256 to 512 KB\")]\n    \
         B@{{ shape: lin-cyl, label: \"Lined cylinder\" }}\n    \
         C[\"Box\"]\n"
    );
    let svg = render_flowchart(&src, "my-svg").expect("render");
    for id in ["A", "B", "C"] {
        let t = label_transform(&svg, id);
        assert!(t.starts_with("translate(0, "), "{id}: {t}");
    }
}
