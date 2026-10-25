//! Corpus test: Mermaid documentation examples (collected by obsidianmd/pele)
//! with mermaid 11.15.0 output as the reference. `KNOWN_FAILURES.txt` is a
//! ratchet: it may only shrink.

use sebastian::render_diagram;
use std::collections::BTreeSet;

/// The gantt "today" marker depends on the render date, so the position of
/// that line is masked on both sides.
fn mask_today(svg: &str) -> String {
    const OPEN: &str = "<g class=\"today\"><line x1=\"";
    let Some(at) = svg.find(OPEN) else {
        return svg.to_owned();
    };
    let start = at + OPEN.len();
    let Some(end) = svg[start..].find(" y1=") else {
        return svg.to_owned();
    };
    format!("{}0\" x2=\"0\"{}", &svg[..start], &svg[start + end..])
}

#[test]
fn pele_corpus() {
    let dir = format!("{}/tests/pele_cases", env!("CARGO_MANIFEST_DIR"));
    let known: BTreeSet<String> = std::fs::read_to_string(format!("{dir}/KNOWN_FAILURES.txt"))
        .expect("known failures")
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect();

    let mut cases: Vec<String> = Vec::new();
    for ty in std::fs::read_dir(&dir).expect("pele_cases dir").flatten() {
        if !ty.path().is_dir() {
            continue;
        }
        let ty_name = ty.file_name().into_string().expect("utf8");
        for f in std::fs::read_dir(ty.path()).expect("type dir").flatten() {
            let name = f.file_name().into_string().expect("utf8");
            if let Some(stem) = name.strip_suffix(".mmd") {
                cases.push(format!("{ty_name}/{stem}"));
            }
        }
    }
    cases.sort();
    assert!(!cases.is_empty());

    let mut regressions = Vec::new();
    let mut fixed = Vec::new();
    for case in &cases {
        let source = std::fs::read_to_string(format!("{dir}/{case}.mmd")).expect("source");
        let reference = std::fs::read_to_string(format!("{dir}/{case}.svg")).expect("reference");
        let passes = render_diagram(&source, "my-svg")
            .is_ok_and(|svg| mask_today(&svg) == mask_today(&reference));
        match (passes, known.contains(case)) {
            (false, false) => regressions.push(case.clone()),
            (true, true) => fixed.push(case.clone()),
            _ => {}
        }
    }
    assert!(
        regressions.is_empty() && fixed.is_empty(),
        "regressions (not in KNOWN_FAILURES.txt): {regressions:?}\nnow passing (remove from KNOWN_FAILURES.txt): {fixed:?}"
    );
}
