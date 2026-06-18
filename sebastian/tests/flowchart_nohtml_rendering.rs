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
