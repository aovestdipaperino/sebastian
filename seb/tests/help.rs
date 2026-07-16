//! End-to-end tests for `seb --help`: usage plus the extension-format topics.

use std::process::Command;

fn seb(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_seb"))
        .args(args)
        .output()
        .expect("seb runs")
}

#[test]
fn help_prints_usage_and_topics() {
    for flag in ["--help", "-h"] {
        let out = seb(&[flag]);
        assert!(out.status.success());
        let text = String::from_utf8(out.stdout).expect("utf-8");
        assert!(text.contains("usage: seb"));
        assert!(text.contains("system_chart"));
        assert!(text.contains("pyramid"));
    }
}

#[test]
fn help_system_chart_documents_the_format() {
    let out = seb(&["--help", "system_chart"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("utf-8");
    // Node forms (boxed and box-less), all four edge operators, symbols.
    assert!(text.contains("id: symbol \"Title\""));
    assert!(text.contains("id: (symbol) \"Title\""));
    for op in ["-->", "..>", "==>", "---"] {
        assert!(text.contains(op), "missing edge operator {op}");
    }
    assert!(text.contains("legend"));
    assert!(text.contains("handDrawn"));
}

#[test]
fn help_pyramid_documents_the_format() {
    let out = seb(&["--help", "pyramid"]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert!(text.contains("<Label>: a, b, c"));
    assert!(text.contains("handDrawn"));
}

#[test]
fn help_rejects_unknown_topic() {
    let out = seb(&["--help", "nope"]);
    assert!(!out.status.success());
    let err = String::from_utf8(out.stderr).expect("utf-8");
    assert!(err.contains("unknown help topic"));
    assert!(err.contains("system_chart, pyramid"));
}
