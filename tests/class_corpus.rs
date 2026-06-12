//! Class diagram corpus: hand-made cases covering members/methods,
//! annotations, classifiers, cardinalities, and relation arrows. The rough
//! rectangle/divider strokes embed mermaid's own `Math.random()` control
//! points (collinear), so they are compared with the rough payloads masked.

use mermaid_rust::render::render_diagram;

/// Masks the rough fill+stroke pair after `outer-path` and divider paths.
fn mask_rough(svg: &str) -> String {
    let mut out = svg.to_owned();
    for marker in [
        "outer-path\"><path d=\"",
        "class=\"divider\" style=\"\"><path d=\"",
    ] {
        let mut acc = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(idx) = rest.find(marker) {
            let start = idx + marker.len();
            acc.push_str(&rest[..start]);
            rest = &rest[start..];
            let end = rest.find('"').expect("closing quote");
            acc.push_str("ROUGH");
            rest = &rest[end..];
            // mask the second path of the pair when present
            if marker.contains("outer-path")
                && let Some(next) = rest.find("<path d=\"")
            {
                let upto = next + "<path d=\"".len();
                acc.push_str(&rest[..upto]);
                rest = &rest[upto..];
                let end2 = rest.find('"').expect("closing quote");
                acc.push_str("ROUGH");
                rest = &rest[end2..];
            }
        }
        acc.push_str(rest);
        out = acc;
    }
    out
}

#[test]
fn class_corpus() {
    let dir = format!("{}/tests/class_cases", env!("CARGO_MANIFEST_DIR"));
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("class_cases dir")
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
            Ok(Ok(svg)) => {
                if mask_rough(&svg) != mask_rough(&reference) {
                    let a = mask_rough(&svg);
                    let b = mask_rough(&reference);
                    let pos = a
                        .bytes()
                        .zip(b.bytes())
                        .position(|(x, y)| x != y)
                        .unwrap_or(a.len().min(b.len()));
                    failures.push(format!("{case}: differs at masked byte {pos}"));
                }
            }
            Ok(Err(err)) => failures.push(format!("{case}: parse error: {err}")),
            Err(_) => failures.push(format!("{case}: render panicked")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} class corpus failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
