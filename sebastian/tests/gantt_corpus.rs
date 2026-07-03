//! Corpus test: gantt charts with official mermaid-cli (mermaid 11.15.0)
//! output as the reference. The today-marker line x position depends on the
//! render time, so it is masked before comparison.

use sebastian::render_diagram;

fn mask_today(svg: &str) -> String {
    let Some(start) = svg.find("<g class=\"today\"><line x1=\"") else {
        return svg.to_owned();
    };
    let x2 = svg[start..].find(" y1=").map_or(svg.len(), |i| start + i);
    format!(
        "{}<g class=\"today\"><line TODAY{}",
        &svg[..start],
        &svg[x2..]
    )
}

#[test]
fn gantt_corpus() {
    let dir = format!("{}/tests/gantt_cases", env!("CARGO_MANIFEST_DIR"));
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("gantt_cases dir")
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
            Ok(svg) if mask_today(&svg) == mask_today(&reference) => {}
            Ok(svg) => {
                let (a, b) = (mask_today(&svg), mask_today(&reference));
                let pos = a
                    .bytes()
                    .zip(b.bytes())
                    .position(|(x, y)| x != y)
                    .unwrap_or(a.len().min(b.len()));
                failures.push(format!("{case}: differs at byte {pos}"));
            }
            Err(err) => failures.push(format!("{case}: {err}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
