//! Timeline corpus: 1 diagram from `../books` plus hand-made cases
//! (sections, multiple events, wrapping), all byte-identical to official
//! mermaid-cli (mermaid 11.15.0) output.

use sebastian::render::render_diagram;

#[test]
fn timeline_corpus() {
    let dir = format!("{}/tests/timeline_cases", env!("CARGO_MANIFEST_DIR"));
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("timeline_cases dir")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".mmd").map(str::to_owned)
        })
        .collect();
    cases.sort();

    let mut failures: Vec<String> = Vec::new();
    for case in &cases {
        let source = std::fs::read_to_string(format!("{dir}/{case}.mmd")).expect("source readable");
        let reference =
            std::fs::read_to_string(format!("{dir}/{case}.svg")).expect("reference readable");
        match std::panic::catch_unwind(|| render_diagram(&source, "my-svg")) {
            Ok(Ok(svg)) if svg == reference => {}
            Ok(Ok(svg)) => {
                let pos = svg
                    .bytes()
                    .zip(reference.bytes())
                    .position(|(a, b)| a != b)
                    .unwrap_or(svg.len().min(reference.len()));
                failures.push(format!("{case}: differs at byte {pos}"));
            }
            Ok(Err(err)) => failures.push(format!("{case}: parse error: {err}")),
            Err(_) => failures.push(format!("{case}: render panicked")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} timeline corpus failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
