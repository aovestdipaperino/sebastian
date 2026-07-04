//! Corpus test: gitGraphs with official mermaid-cli (mermaid 11.15.0) output
//! as the reference. Two masks are applied before comparison:
//! - auto-generated commit ids (`N-xxxxxxx`) embed `Math.random()` upstream;
//! - the viewBox/max-width floats are rounded to 1e-3 (one open question:
//!   a single-f32-ulp difference in Blink's rotated-rect bbox mapping — it
//!   surfaces in `max-width` on the rotated TB/BT tag polygon).

use sebastian::render_diagram;

fn mask(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    // Round `max-width: Npx` to 1e-3 (same f32-ulp bbox tolerance as viewBox).
    let svg = {
        let mut o = String::with_capacity(svg.len());
        let mut r = svg;
        while let Some(i) = r.find("max-width: ") {
            let start = i + "max-width: ".len();
            o.push_str(&r[..start]);
            r = &r[start..];
            let end = r.find("px").unwrap_or(0);
            let rounded = r[..end]
                .parse::<f64>()
                .map_or_else(|_| r[..end].to_owned(), |v| format!("{v:.3}"));
            o.push_str(&rounded);
            r = &r[end..];
        }
        o.push_str(r);
        o
    };
    let svg = svg.as_str();
    let mut rest = svg;
    // Round viewBox numbers.
    while let Some(i) = rest.find("viewBox=\"") {
        let start = i + 9;
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find('"').expect("closing quote");
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
    // Mask random commit ids.
    let bytes: Vec<char> = out.chars().collect();
    let mut masked = String::with_capacity(out.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            // N-xxxxxxx where x are lowercase hex
            let mut j = i;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j + 8 <= bytes.len()
                && bytes[j] == '-'
                && bytes[j + 1..j + 8]
                    .iter()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                && (j + 8 == bytes.len() || !bytes[j + 8].is_ascii_alphanumeric())
            {
                masked.push_str("IDMASK");
                i = j + 8;
                continue;
            }
        }
        masked.push(bytes[i]);
        i += 1;
    }
    masked
}

#[test]
fn gitgraph_corpus() {
    let dir = format!("{}/tests/gitgraph_cases", env!("CARGO_MANIFEST_DIR"));
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("gitgraph_cases dir")
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
                failures.push(format!("{case}: differs at masked byte {pos}"));
            }
            Err(err) => failures.push(format!("{case}: {err}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
