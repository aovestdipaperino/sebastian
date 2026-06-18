//! Corpus test: 553 real-world flowcharts extracted from `../books`, each
//! with the official mermaid-cli (mermaid 11.15.0) output as the reference.
//!
//! Categories:
//! - byte-identical (544, including the 17 `%%{init: ...}%%` directive
//!   cases — themes, themeVariables, and `htmlLabels:false`): output must
//!   match the reference exactly;
//! - `ROUGH_RANDOM` (3): stadium/odd shapes, where mermaid itself embeds
//!   `Math.random()` in (collinear) curve control points — two mmdc runs
//!   differ in those bytes; compared modulo rough path data;
//! - `SUBPIXEL` (6): differences confined to numbers (viewBox, path data)
//!   below 0.01px from Chrome's arc/extrema arithmetic, plus one space-kern
//!   case (book274, ≤1px in two labels); compared numerically.

use sebastian::render_flowchart;

const ROUGH_RANDOM: &[&str] = &["book262", "book323", "book345"];

const SUBPIXEL: &[&str] = &[
    "book012", "book236", "book315", "book368", "book406", "book274",
];

fn dir() -> String {
    format!("{}/tests/book_cases", env!("CARGO_MANIFEST_DIR"))
}

/// Replaces the `d` attribute payloads of rough two-path shapes with a
/// placeholder, so the (random, collinear) control points don't participate.
fn mask_rough_paths(svg: &str) -> String {
    let mut out = String::with_capacity(svg.len());
    let mut rest = svg;
    while let Some(idx) = rest.find("outer-path\"><path d=\"") {
        let start = idx + "outer-path\"><path d=\"".len();
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        // Mask this path's d and the immediately following sibling's.
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

/// Token-wise comparison: non-numeric spans must match exactly; numbers must
/// agree within `tol`.
fn numerically_close(a: &str, b: &str, tol: f64) -> bool {
    let tokenize = |s: &str| -> Vec<(bool, String)> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut numeric = false;
        for c in s.chars() {
            let is_num = c.is_ascii_digit() || c == '.' || c == '-';
            if is_num != numeric && !current.is_empty() {
                tokens.push((numeric, std::mem::take(&mut current)));
            }
            numeric = is_num;
            current.push(c);
        }
        if !current.is_empty() {
            tokens.push((numeric, current));
        }
        tokens
    };
    let (ta, tb) = (tokenize(a), tokenize(b));
    if ta.len() != tb.len() {
        return false;
    }
    for ((na, va), (nb, vb)) in ta.iter().zip(&tb) {
        if na != nb {
            return false;
        }
        if *na {
            match (va.parse::<f64>(), vb.parse::<f64>()) {
                (Ok(x), Ok(y)) => {
                    if (x - y).abs() > tol {
                        return false;
                    }
                }
                _ => {
                    if va != vb {
                        return false;
                    }
                }
            }
        } else if va != vb {
            return false;
        }
    }
    true
}

#[test]
fn book_corpus() {
    let dir = dir();
    let mut cases: Vec<String> = std::fs::read_dir(&dir)
        .expect("book_cases dir")
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

        let result = std::panic::catch_unwind(|| render_flowchart(&source, "my-svg"));
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

        let tol = if case == "book274" { 2.0 } else { 0.01 };
        // data-points holds base64-encoded JSON whose bytes scramble under
        // sub-pixel differences; the same coordinates are still compared via
        // the path `d` attributes.
        let mask_points = |s: &str| -> String {
            let mut out = String::with_capacity(s.len());
            let mut rest = s;
            while let Some(idx) = rest.find("data-points=\"") {
                let start = idx + "data-points=\"".len();
                out.push_str(&rest[..start]);
                rest = &rest[start..];
                let end = rest.find('"').expect("closing quote");
                out.push_str("POINTS");
                rest = &rest[end..];
            }
            out.push_str(rest);
            out
        };
        if SUBPIXEL.contains(&case.as_str())
            && numerically_close(&mask_points(&svg), &mask_points(&reference), tol)
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
        "{} corpus failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // Guard against silent regressions of the byte-identical count.
    assert!(
        identical >= 544,
        "byte-identical count regressed: {identical} < 544"
    );
}
