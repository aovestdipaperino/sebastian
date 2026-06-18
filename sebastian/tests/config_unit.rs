//! Unit tests for `%%{init}%%` directive parsing (`render::config::detect_init`)
//! and the effective-config accessors. Covers the directive branches beyond the
//! basics: look, flowchart spacing/curve, html-label precedence, the
//! `initialize` alias, single-quote conversion, malformed-JSON skipping, and
//! multi-directive merging.

use sebastian::render::config::{RenderConfig, detect_init};

#[test]
fn no_directive_yields_defaults() {
    let cfg = detect_init("graph TD\nA-->B\n");
    assert_eq!(cfg.theme, "default");
    assert_eq!(cfg.look, "classic");
    assert_eq!(cfg.node_spacing, 50.0);
    assert!(!cfg.is_hand_drawn());
}

#[test]
fn look_hand_drawn_is_detected() {
    let cfg = detect_init("%%{init: {'look': 'handDrawn'}}%%\ngraph TD\nA\n");
    assert_eq!(cfg.look, "handDrawn");
    assert!(cfg.is_hand_drawn());
}

#[test]
fn flowchart_spacing_and_curve() {
    let cfg = detect_init(
        "%%{init: {'flowchart': {'nodeSpacing': 12, 'rankSpacing': 34, 'padding': 7, 'wrappingWidth': 123, 'curve': 'basis'}}}%%\ngraph TD\n",
    );
    assert_eq!(cfg.node_spacing, 12.0);
    assert_eq!(cfg.rank_spacing, 34.0);
    assert_eq!(cfg.padding, 7.0);
    assert_eq!(cfg.wrapping_width, 123.0);
    assert_eq!(cfg.curve.as_deref(), Some("basis"));
}

#[test]
fn initialize_is_an_alias_for_init() {
    let cfg = detect_init("%%{initialize: {'theme': 'forest'}}%%\ngraph TD\n");
    assert_eq!(cfg.theme, "forest");
}

#[test]
fn malformed_json_is_skipped_leaving_defaults() {
    let cfg = detect_init("%%{init: {not valid json}}%%\ngraph TD\n");
    assert_eq!(cfg.theme, "default");
}

#[test]
fn non_init_directive_is_ignored() {
    let cfg = detect_init("%%{wrap: {'theme': 'dark'}}%%\ngraph TD\n");
    assert_eq!(cfg.theme, "default");
}

#[test]
fn later_directive_overrides_earlier() {
    let cfg =
        detect_init("%%{init: {'theme': 'dark'}}%%\n%%{init: {'theme': 'neutral'}}%%\ngraph TD\n");
    assert_eq!(cfg.theme, "neutral");
}

#[test]
fn double_quoted_json_also_parses() {
    let cfg = detect_init("%%{init: {\"theme\": \"base\"}}%%\ngraph TD\n");
    assert_eq!(cfg.theme, "base");
}

#[test]
fn effective_html_labels_precedence() {
    // flowchart.htmlLabels wins over top-level htmlLabels.
    let cfg = detect_init(
        "%%{init: {'htmlLabels': true, 'flowchart': {'htmlLabels': false}}}%%\ngraph TD\n",
    );
    assert_eq!(cfg.flowchart_html_labels, Some(false));
    assert_eq!(cfg.top_html_labels, Some(true));
    assert!(!cfg.effective_html_labels()); // flowchart value wins
    assert!(cfg.node_html_labels()); // node labels read top-level only
}

#[test]
fn effective_html_labels_defaults_true() {
    let cfg = RenderConfig::default();
    assert!(cfg.effective_html_labels());
    assert!(cfg.node_html_labels());
}

#[test]
fn font_size_parses_px_theme_variable() {
    let cfg = detect_init("%%{init: {'themeVariables': {'fontSize': '20px'}}}%%\ngraph TD\n");
    assert_eq!(cfg.font_size(), 20.0);
}

#[test]
fn font_size_defaults_to_16() {
    assert_eq!(RenderConfig::default().font_size(), 16.0);
}
