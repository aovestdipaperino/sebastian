//! Corpus test: sankey-beta diagrams with official mermaid-cli (mermaid
//! 11.15.0) output as the reference. All cases must be byte-identical.
//!
//! Cases are chosen so node labels stay within the node bounding box, since
//! (as in the rest of this engine) `getBBox` viewBox sizing ignores `<text>`.

use sebastian::render_diagram;

#[test]
fn sankey_corpus() {
    let dir = format!("{}/tests/sankey_cases", env!("CARGO_MANIFEST_DIR"));
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("sankey_cases dir")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".mmd").map(str::to_owned)
        })
        .collect();
    cases.sort();
    assert!(!cases.is_empty());

    let mut failures: Vec<String> = Vec::new();
    for case in &cases {
        let source = std::fs::read_to_string(format!("{dir}/{case}.mmd")).expect("source");
        let reference = std::fs::read_to_string(format!("{dir}/{case}.svg")).expect("reference");
        match render_diagram(&source, "my-svg") {
            Ok(svg) if svg == reference => {}
            Ok(svg) => {
                let pos = svg
                    .bytes()
                    .zip(reference.bytes())
                    .position(|(a, b)| a != b)
                    .unwrap_or(svg.len().min(reference.len()));
                failures.push(format!("{case}: differs at byte {pos}"));
            }
            Err(err) => failures.push(format!("{case}: {err}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
