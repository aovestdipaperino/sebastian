//! `render_diagram` must never panic, no matter how malformed the input:
//! it either renders or returns an error. Exercises the concrete inputs
//! that used to panic, plus a deterministic mutation fuzz over every corpus
//! fixture (a seeded LCG, so failures reproduce exactly).

use std::fs;
use std::path::PathBuf;

/// Inputs that panicked before the hostile-input hardening pass.
#[test]
fn past_panics_now_error_or_render() {
    let cases: &[&str] = &[
        // subgraph nested inside itself -> graphlib parent cycle
        "flowchart LR\n    subgraph A\n        subgraph A\n            B\n        end\n    end\n",
        // BOM inside a keyword -> char-boundary slice in strip_keyword
        "sequenceDiagram\n    Note\u{feff} over A: hi\n",
        // transition with an empty endpoint -> node \"\" in the graph
        "stateDiagram-v2\n    [*] --> \n",
        // class marker with no id on the target
        "stateDiagram-v2\n    a --> :::A\n",
        // header only -> empty graph through the dagre pipeline
        "stateDiagram-v2\n",
        // activation of a participant that never appears
        "sequenceDiagram\n    activate Worker\n",
        // used to wedge the flowchart lexer in a zero-progress loop that
        // allocated tokens until OOM
        "graph TB\n    subgraph D= call stack depth\n    end\n",
    ];
    for src in cases {
        // Either Ok or Err is fine; panicking is the bug.
        let _ = sebastian::render_diagram(src, "hostile");
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 16
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() as usize) % n.max(1)
    }
}

fn mutate(src: &str, rng: &mut Rng) -> String {
    let mut s = src.as_bytes().to_vec();
    match rng.below(7) {
        0 => s.truncate(rng.below(s.len() + 1)),
        1 => {
            for _ in 0..=rng.below(8) {
                if s.is_empty() {
                    break;
                }
                let i = rng.below(s.len());
                s[i] = (rng.next() & 0xff) as u8;
            }
        }
        2 => {
            if !s.is_empty() {
                let a = rng.below(s.len());
                let b = (a + 1 + rng.below(40)).min(s.len());
                s.drain(a..b);
            }
        }
        3 => {
            if !s.is_empty() {
                let a = rng.below(s.len());
                let b = (a + 1 + rng.below(40)).min(s.len());
                let chunk: Vec<u8> = s[a..b].to_vec();
                let at = rng.below(s.len());
                s.splice(at..at, chunk);
            }
        }
        4 => {
            const TOKENS: &[&str] = &[
                "%%{init:", "}}%%", "-->", "((", "]]", ":::", "|", "\"", "\\", "&", "<br/>",
                "subgraph", "end", "activate", "-1", "1e999", "\u{0}", "\u{feff}", "🦀", "\r\n",
            ];
            let t = TOKENS[rng.below(TOKENS.len())].as_bytes().to_vec();
            let at = rng.below(s.len() + 1);
            s.splice(at..at, t);
        }
        5 => {
            let mut lines: Vec<&[u8]> = s.split(|&b| b == b'\n').collect();
            for _ in 0..3 {
                if lines.len() >= 2 {
                    let a = rng.below(lines.len());
                    let b = rng.below(lines.len());
                    lines.swap(a, b);
                }
            }
            s = lines.join(&b'\n');
        }
        _ => {
            let n = rng.below(300);
            s = (0..n).map(|_| (rng.next() & 0xff) as u8).collect();
        }
    }
    String::from_utf8_lossy(&s).into_owned()
}

#[test]
fn mutation_fuzz_never_panics() {
    let mut corpus: Vec<String> = Vec::new();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");
    for dir in fs::read_dir(&root).unwrap().flatten() {
        if dir.path().is_dir() {
            for f in fs::read_dir(dir.path()).unwrap().flatten() {
                if f.path().extension().is_some_and(|e| e == "mmd") {
                    corpus.push(fs::read_to_string(f.path()).unwrap());
                }
            }
        }
    }
    assert!(!corpus.is_empty(), "no .mmd fixtures found");

    for i in 0..3000u64 {
        let mut rng = Rng((i + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        let base = &corpus[rng.below(corpus.len())];
        let mut input = mutate(base, &mut rng);
        if rng.below(2) == 0 {
            input = mutate(&input, &mut rng);
        }
        // A panic here aborts the test; the iteration index in the failure
        // output plus the seeded RNG make the input reproducible.
        let _ = sebastian::render_diagram(&input, "fuzz");
    }
}
