//! End-to-end tests: rendered SVG must be byte-identical to the official
//! mermaid-cli (mermaid 11.15.0) output captured in `tests/flowchart_cases`.
//!
//! Requires the system Trebuchet MS font (present on macOS/Windows); text
//! metrics and therefore the whole layout depend on it.

use sebastian::render::render_flowchart;

fn run_case(name: &str) {
    let dir = format!("{}/tests/flowchart_cases", env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(format!("{dir}/{name}.mmd")).expect("mmd source");
    let expected = std::fs::read_to_string(format!("{dir}/{name}.svg")).expect("reference svg");
    let actual = render_flowchart(&source, "my-svg").expect("render");
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
    chain, multibr, invisible,
);
