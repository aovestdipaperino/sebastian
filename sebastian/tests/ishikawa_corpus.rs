//! Corpus test: ishikawa diagrams with official mermaid-cli (mermaid 11.15.0)
//! output as the reference. All cases are byte-identical except for one open
//! question: Blink's f32 union of the head path with the label boxes can land
//! one ulp above ours in the root `viewBox` width (and the `max-width` that
//! mirrors it). Those two numbers are compared rounded to 1e-3.

use sebastian::render_diagram;

fn round_after(svg: &str, marker: &str, terminator: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(i) = rest.find(marker) {
        let start = i + marker.len();
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(terminator).unwrap_or(rest.len());
        let rounded: Vec<String> = rest[..end]
            .split(' ')
            .map(|t| {
                t.parse::<f64>()
                    .map_or_else(|_| t.to_owned(), |v| format!("{v:.3}"))
            })
            .collect();
        out.push_str(&rounded.join(" "));
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

fn mask(svg: &str) -> String {
    round_after(&round_after(svg, "max-width: ", "px"), "viewBox=\"", "\"")
}

#[test]
fn ishikawa_corpus() {
    let dir = format!("{}/tests/ishikawa_cases", env!("CARGO_MANIFEST_DIR"));
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("ishikawa_cases dir")
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
            Ok(svg) if svg == reference || mask(&svg) == mask(&reference) => {}
            Ok(svg) => {
                let (a, b) = (mask(&svg), mask(&reference));
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
