//! Regression guard for the flowchart lexer (`flowchart::lexer`).
//!
//! These tests pin the exact token stream produced by `Lexer::tokenize` so the
//! `next_token` state machine can be refactored for readability without any
//! behavioral drift. Two layers:
//!
//! 1. Explicit, hand-derived assertions for core constructs and every lexer
//!    state — these document intended behavior and fail loudly on a regression.
//! 2. A golden snapshot over a broad input corpus. Regenerate intentionally
//!    with `REGEN=1 cargo test -p sebastian --test lexer_regression`; otherwise
//!    a mismatch is a regression.

use sebastian::flowchart::lexer::{Lexer, Tok};

fn lex(src: &str) -> Vec<Tok> {
    let chars: Vec<char> = src.chars().collect();
    Lexer::new(&chars).tokenize()
}

/// Token stream with the trailing `Eof` stripped, for concise assertions.
fn toks(src: &str) -> Vec<Tok> {
    let mut t = lex(src);
    assert_eq!(t.last(), Some(&Tok::Eof), "stream must end with Eof");
    t.pop();
    t
}

use Tok::*;

fn s(text: &str) -> String {
    text.to_owned()
}

// ---------------------------------------------------------------------------
// Initial state: graph headers and direction
// ---------------------------------------------------------------------------

#[test]
fn graph_header_with_direction() {
    assert_eq!(toks("graph TD"), vec![Graph, Dir(s("TD"))]);
    assert_eq!(toks("flowchart LR"), vec![Graph, Dir(s("LR"))]);
    assert_eq!(toks("flowchart-elk TB"), vec![Graph, Dir(s("TB"))]);
}

#[test]
fn direction_statement() {
    assert_eq!(toks("direction BT"), vec![Direction(s("BT"))]);
}

#[test]
fn graph_without_direction_yields_nodir() {
    // The Dir state consumes the newline while emitting NoDir.
    assert_eq!(toks("graph\n"), vec![Graph, NoDir]);
}

// ---------------------------------------------------------------------------
// Initial state: links (full + start) across all three kinds
// ---------------------------------------------------------------------------

#[test]
fn full_links() {
    assert_eq!(
        toks("A-->B"),
        vec![NodeString(s("A")), Link(s("-->")), NodeString(s("B"))]
    );
    assert_eq!(
        toks("A==>B"),
        vec![NodeString(s("A")), Link(s("==>")), NodeString(s("B"))]
    );
    assert_eq!(
        toks("A-.->B"),
        vec![NodeString(s("A")), Link(s("-.->")), NodeString(s("B"))]
    );
    assert_eq!(
        toks("A---B"),
        vec![NodeString(s("A")), Link(s("---")), NodeString(s("B"))]
    );
}

#[test]
fn tilde_link() {
    assert_eq!(
        toks("A~~~B"),
        vec![NodeString(s("A")), Link(s("~~~")), NodeString(s("B"))]
    );
}

#[test]
fn start_links_open_edge_text_states() {
    // `--` opens EdgeText; the start-link body is just `--` (trailing inline
    // whitespace is consumed but not kept), and the label runs to the closer.
    assert_eq!(
        toks("A-- label -->B"),
        vec![
            NodeString(s("A")),
            StartLink(s("--")),
            EdgeText(s("label ")),
            Link(s("-->")),
            NodeString(s("B")),
        ],
    );
    assert_eq!(
        toks("A== label ==>B"),
        vec![
            NodeString(s("A")),
            StartLink(s("==")),
            EdgeText(s("label ")),
            Link(s("==>")),
            NodeString(s("B")),
        ],
    );
}

#[test]
fn link_id() {
    assert_eq!(
        toks("e1@-->B"),
        vec![LinkId(s("e1@")), Link(s("-->")), NodeString(s("B"))],
    );
}

// ---------------------------------------------------------------------------
// Initial state: shapes (openers + Text/EllipseText/TrapText close states)
// ---------------------------------------------------------------------------

#[test]
fn square_and_round_shapes() {
    assert_eq!(
        toks("A[label]"),
        vec![NodeString(s("A")), Sqs, Text(s("label")), Sqe],
    );
    assert_eq!(
        toks("A(label)"),
        vec![NodeString(s("A")), Ps, Text(s("label")), Pe],
    );
    assert_eq!(
        toks("A{label}"),
        vec![
            NodeString(s("A")),
            DiamondStart,
            Text(s("label")),
            DiamondStop
        ],
    );
}

#[test]
fn stadium_subroutine_cylinder_doublecircle() {
    assert_eq!(
        toks("A([x])"),
        vec![NodeString(s("A")), StadiumStart, Text(s("x")), StadiumEnd],
    );
    assert_eq!(
        toks("A[[x]]"),
        vec![
            NodeString(s("A")),
            SubroutineStart,
            Text(s("x")),
            SubroutineEnd
        ],
    );
    assert_eq!(
        toks("A[(x)]"),
        vec![NodeString(s("A")), CylinderStart, Text(s("x")), CylinderEnd],
    );
    assert_eq!(
        toks("A(((x)))"),
        vec![
            NodeString(s("A")),
            DoubleCircleStart,
            Text(s("x")),
            DoubleCircleEnd
        ],
    );
}

#[test]
fn ellipse_and_trapezoid() {
    assert_eq!(
        toks("A(-x-)"),
        vec![NodeString(s("A")), EllipseStart, Text(s("x")), EllipseEnd],
    );
    // Lean trapezoids: the closing slash determines the end token, so an
    // opening `/` closes with InvTrapEnd and an opening `\` closes with TrapEnd.
    assert_eq!(
        toks("A[/x/]"),
        vec![NodeString(s("A")), TrapStart, Text(s("x")), InvTrapEnd],
    );
    assert_eq!(
        toks(r"A[\x\]"),
        vec![NodeString(s("A")), InvTrapStart, Text(s("x")), TrapEnd],
    );
}

// ---------------------------------------------------------------------------
// Text state: strings, markdown strings, pipe labels
// ---------------------------------------------------------------------------

#[test]
fn quoted_string_in_label() {
    assert_eq!(
        toks(r#"A["hello world"]"#),
        vec![NodeString(s("A")), Sqs, Str(s("hello world")), Sqe],
    );
}

#[test]
fn markdown_string() {
    assert_eq!(
        toks(r#"A["`md`"]"#),
        vec![NodeString(s("A")), Sqs, MdStr(s("md")), Sqe],
    );
}

#[test]
fn pipe_edge_label() {
    assert_eq!(
        toks("A-->|yes|B"),
        vec![
            NodeString(s("A")),
            Link(s("-->")),
            Pipe,
            Text(s("yes")),
            Pipe,
            NodeString(s("B")),
        ],
    );
}

// ---------------------------------------------------------------------------
// Initial state: keywords and class/style machinery
// ---------------------------------------------------------------------------

#[test]
fn keywords() {
    assert_eq!(toks("subgraph"), vec![Subgraph]);
    assert_eq!(toks("end"), vec![End]);
    assert_eq!(toks("style"), vec![Style]);
    assert_eq!(toks("classDef"), vec![ClassDef]);
    assert_eq!(toks("class"), vec![Class]);
    assert_eq!(toks("linkStyle"), vec![LinkStyle]);
    assert_eq!(toks("interpolate"), vec![Interpolate]);
    assert_eq!(toks("default"), vec![Default]);
}

#[test]
fn keyword_prefix_of_identifier_is_node_string() {
    // `classify` must not tokenize as `class` + `ify`.
    assert_eq!(toks("classify"), vec![NodeString(s("classify"))]);
    assert_eq!(toks("ended"), vec![NodeString(s("ended"))]);
}

#[test]
fn style_separator_and_colon() {
    assert_eq!(
        toks("A:::cls"),
        vec![NodeString(s("A")), StyleSeparator, NodeString(s("cls"))]
    );
    assert_eq!(
        toks("A:B"),
        vec![NodeString(s("A")), Colon, NodeString(s("B"))]
    );
}

#[test]
fn punctuation_singletons() {
    assert_eq!(toks("#"), vec![Brkt]);
    assert_eq!(toks("&"), vec![Amp]);
    assert_eq!(toks(";"), vec![Semi]);
    assert_eq!(toks(","), vec![Comma]);
    assert_eq!(toks("*"), vec![Mult]);
    assert_eq!(toks("^"), vec![Up]);
}

// ---------------------------------------------------------------------------
// Initial state: numbers, node strings, unicode, whitespace
// ---------------------------------------------------------------------------

#[test]
fn numbers_vs_node_strings() {
    assert_eq!(toks("123"), vec![Num(s("123"))]);
    // A digit run followed by identifier chars prefers the longer NODE_STRING.
    assert_eq!(toks("123abc"), vec![NodeString(s("123abc"))]);
}

#[test]
fn unicode_text() {
    assert_eq!(toks("日本語"), vec![UnicodeText(s("日本語"))]);
}

#[test]
fn whitespace_and_newlines() {
    assert_eq!(
        toks("A B"),
        vec![NodeString(s("A")), Space, NodeString(s("B"))]
    );
    assert_eq!(
        toks("A\n\nB"),
        vec![NodeString(s("A")), Newline, NodeString(s("B"))]
    );
}

// ---------------------------------------------------------------------------
// Accessibility, shape data, click/callback states
// ---------------------------------------------------------------------------

#[test]
fn acc_title_and_descr() {
    assert_eq!(
        toks("accTitle: My Title"),
        vec![AccTitle, Str(s("My Title"))]
    );
    assert_eq!(
        toks("accDescr: My Descr"),
        vec![AccDescr, Str(s("My Descr"))]
    );
}

#[test]
fn acc_descr_multiline() {
    assert_eq!(
        toks("accDescr {\nline\n}"),
        vec![AccDescr, Str(s("\nline\n"))],
    );
}

#[test]
fn shape_data() {
    assert_eq!(
        toks("A@{ shape: rect }"),
        vec![
            NodeString(s("A")),
            ShapeData(s("")),
            ShapeData(s(" shape: rect "))
        ],
    );
}

#[test]
fn click_and_callback() {
    assert_eq!(
        toks("click A call cb()"),
        vec![Click(s("A")), CallbackName(s("cb"))],
    );
}

#[test]
fn comments_are_skipped() {
    assert_eq!(toks("%% a comment\nA"), vec![Newline, NodeString(s("A"))]);
}

// ---------------------------------------------------------------------------
// A realistic multi-line graph exercising many transitions at once
// ---------------------------------------------------------------------------

#[test]
fn realistic_graph() {
    let src = "graph TD\n  A[Start] --> B{Choice}\n  B -->|yes| C(Done)\n";
    // match_link absorbs the whitespace immediately around `-->`, so no Space
    // tokens appear adjacent to a link.
    assert_eq!(
        toks(src),
        vec![
            Graph,
            Dir(s("TD")),
            Newline,
            Space,
            Space,
            NodeString(s("A")),
            Sqs,
            Text(s("Start")),
            Sqe,
            Link(s("-->")),
            NodeString(s("B")),
            DiamondStart,
            Text(s("Choice")),
            DiamondStop,
            Newline,
            Space,
            Space,
            NodeString(s("B")),
            Link(s("-->")),
            Pipe,
            Text(s("yes")),
            Pipe,
            Space,
            NodeString(s("C")),
            Ps,
            Text(s("Done")),
            Pe,
            Newline,
        ],
    );
}

// ---------------------------------------------------------------------------
// Golden snapshot over a broad corpus. Regenerate with REGEN=1.
// ---------------------------------------------------------------------------

/// Named inputs chosen to touch every match arm in `next_token`.
const CORPUS: &[(&str, &str)] = &[
    ("header_td", "graph TD"),
    ("header_lr", "flowchart LR"),
    ("header_elk", "flowchart-elk TB"),
    ("header_nodir", "graph\nA"),
    ("direction", "flowchart\ndirection RL"),
    ("link_arrow", "A-->B"),
    ("link_thick", "A==>B"),
    ("link_dotted", "A-.->B"),
    ("link_open", "A---B"),
    ("link_tilde", "A~~~B"),
    ("link_circle_end", "A--oB"),
    ("link_cross_end", "A--xB"),
    ("edge_label", "A-- yes -->B"),
    ("edge_label_thick", "A== no ==>B"),
    ("edge_label_dotted", "A-. maybe .->B"),
    ("pipe_label", "A-->|go|B"),
    ("link_id", "e1@-->B"),
    ("shape_square", "A[label]"),
    ("shape_round", "A(label)"),
    ("shape_diamond", "A{label}"),
    ("shape_stadium", "A([label])"),
    ("shape_subroutine", "A[[label]]"),
    ("shape_cylinder", "A[(db)]"),
    ("shape_double_circle", "A(((x)))"),
    ("shape_ellipse", "A(-x-)"),
    ("shape_trap", "A[/x/]"),
    ("shape_inv_trap", r"A[\x\]"),
    ("vertex_props", "A[|prop|]"),
    ("string_label", r#"A["hi there"]"#),
    ("md_string", r#"A["`**bold**`"]"#),
    ("subgraph", "subgraph one\nA\nend"),
    ("classdef", "classDef big fill:#f00"),
    ("class_apply", "class A big"),
    ("class_shorthand", "A:::big"),
    ("style", "style A fill:#fff"),
    ("linkstyle", "linkStyle 0 stroke:red"),
    ("interpolate", "linkStyle default interpolate basis"),
    ("click_href", "click A href \"http://x\" _blank"),
    ("click_call", "click A call cb(1,2)"),
    ("acc_title", "accTitle: A diagram"),
    ("acc_descr", "accDescr: Some description"),
    ("acc_descr_ml", "accDescr {\nmany\nlines\n}"),
    ("shape_data", "A@{ shape: rounded }"),
    ("shape_data_str", "A@{ label: \"hi\" }"),
    ("comment", "%% comment\nA-->B"),
    ("number", "A123 --> 456"),
    ("unicode", "A[日本語] --> B"),
    ("ampersand", "A & B --> C"),
    ("semicolons", "A-->B;\nC-->D;"),
    ("tag_html", "A-->|<b>x</b>|B"),
    (
        "multi",
        "graph LR\n  A[Start]-->|go|B{Q}\n  B-->C(((End)))\n  classDef x fill:#0f0\n  class A,B x\n",
    ),
];

fn render_tokens(toks: &[Tok]) -> String {
    toks.iter()
        .map(|t| format!("{t:?}"))
        .collect::<Vec<_>>()
        .join("\n  ")
}

#[test]
fn golden_corpus_snapshot() {
    let mut out = String::new();
    for (name, src) in CORPUS {
        out.push_str(&format!(
            "=== {name} ===\n  {}\n\n",
            render_tokens(&lex(src))
        ));
    }

    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/lexer_cases/tokens.golden"
    );
    if std::env::var("REGEN").is_ok() {
        std::fs::create_dir_all(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/lexer_cases")).unwrap();
        std::fs::write(path, &out).unwrap();
        eprintln!("regenerated {path}");
        return;
    }

    let expected = std::fs::read_to_string(path).unwrap_or_else(|_| {
        panic!("missing golden file {path}; run with REGEN=1 to create it");
    });
    assert_eq!(
        out, expected,
        "lexer token stream drifted from golden snapshot"
    );
}
