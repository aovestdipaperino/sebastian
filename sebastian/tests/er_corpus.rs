//! Corpus test: erDiagrams with official mermaid-cli (mermaid 11.15.0)
//! output as the reference. Rough-path payloads embed mermaid's own
//! `Math.random()` control points, so `d="...C..."` payloads are masked.

use sebastian::render_diagram;

fn mask_rough(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(idx) = rest.find("d=\"") {
        let start = idx + 3;
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find('"').expect("closing quote");
        if rest[..end].contains('C') {
            out.push_str("MASK");
        } else {
            out.push_str(&rest[..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn er_corpus() {
    let dir = format!("{}/tests/er_cases", env!("CARGO_MANIFEST_DIR"));
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("er_cases dir")
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
            Ok(svg) if svg == reference || mask_rough(&svg) == mask_rough(&reference) => {}
            Ok(svg) => {
                let (a, b) = (mask_rough(&svg), mask_rough(&reference));
                let pos = a
                    .bytes()
                    .zip(b.bytes())
                    .position(|(x, y)| x != y)
                    .unwrap_or(a.len().min(b.len()));
                failures.push(format!("{case}: differs at masked byte {pos}"));
            }
            Err(err) => failures.push(format!("{case}: {err}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
