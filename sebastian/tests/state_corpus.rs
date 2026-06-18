//! Corpus test: 23 real-world stateDiagram-v2 diagrams extracted from
//! `../books`, with official mermaid-cli (mermaid 11.15.0) output as the
//! reference.
//!
//! 20 must be byte-identical. The 3 diagrams with notes contain rough.js
//! rectangles whose curve control points embed `Math.random()` in mermaid
//! itself (collinear, so pixel-identical); they are compared with the rough
//! path payloads masked.

use sebastian::render_diagram;

/// Diagrams whose note rectangles carry mermaid's own random control points.
const ROUGH_RANDOM: &[&str] = &["state007", "state008", "state014"];

fn dir() -> String {
    format!("{}/tests/state_cases", env!("CARGO_MANIFEST_DIR"))
}

/// Masks the `d` payloads of the two-path rough shapes that follow an
/// `outer-path` group.
fn mask_rough_paths(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(idx) = rest.find("outer-path\"><path d=\"") {
        let start = idx + "outer-path\"><path d=\"".len();
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        for pass in 0..2 {
            let end = rest.find('"').expect("closing quote");
            out.push_str("ROUGH");
            rest = &rest[end..];
            if pass == 0 {
                if let Some(next) = rest.find("<path d=\"") {
                    let upto = next + "<path d=\"".len();
                    out.push_str(&rest[..upto]);
                    rest = &rest[upto..];
                } else {
                    break;
                }
            }
        }
    }
    out.push_str(rest);
    out
}

#[test]
fn state_corpus() {
    let dir = dir();
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("state_cases dir")
        .filter_map(|e| {
            let name = e.ok()?.file_name().into_string().ok()?;
            name.strip_suffix(".mmd").map(str::to_owned)
        })
        .collect();
    cases.sort();

    let mut identical = 0;
    let mut failures: Vec<String> = Vec::new();

    for case in &cases {
        let source = std::fs::read_to_string(format!("{dir}/{case}.mmd")).expect("source readable");
        let reference =
            std::fs::read_to_string(format!("{dir}/{case}.svg")).expect("reference readable");

        let result = std::panic::catch_unwind(|| render_diagram(&source, "my-svg"));
        let svg = match result {
            Ok(Ok(svg)) => svg,
            Ok(Err(err)) => {
                failures.push(format!("{case}: parse error: {err}"));
                continue;
            }
            Err(_) => {
                failures.push(format!("{case}: render panicked"));
                continue;
            }
        };

        if svg == reference {
            identical += 1;
            continue;
        }

        if ROUGH_RANDOM.contains(&case.as_str())
            && mask_rough_paths(&svg) == mask_rough_paths(&reference)
        {
            continue;
        }

        let pos = svg
            .bytes()
            .zip(reference.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(svg.len().min(reference.len()));
        failures.push(format!("{case}: differs at byte {pos}"));
    }

    assert!(
        failures.is_empty(),
        "{} state corpus failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        identical >= 20,
        "byte-identical count regressed: {identical} < 20"
    );
}
