//! Unit tests for pure render helpers: `styles::is_label_style` (which CSS
//! properties belong on the label vs the node) and `edges::calc_label_position`
//! (the midpoint-by-arc-length of an edge polyline used to place its label).

use sebastian::dagre::types::Point;
use sebastian::render::edges::calc_label_position;
use sebastian::render::styles::is_label_style;

fn p(x: f64, y: f64) -> Point {
    Point { x, y }
}

#[test]
fn label_style_keys_are_recognized() {
    for key in [
        "color",
        "font-size",
        "font-family",
        "font-weight",
        "text-align",
        "line-height",
        "white-space",
        "word-break",
        "hyphens",
    ] {
        assert!(is_label_style(key), "{key} should be a label style");
    }
}

#[test]
fn node_style_keys_are_not_label_styles() {
    for key in [
        "fill",
        "stroke",
        "stroke-width",
        "opacity",
        "rx",
        "background",
    ] {
        assert!(!is_label_style(key), "{key} should not be a label style");
    }
}

#[test]
fn label_position_of_single_point_is_that_point() {
    assert_eq!(calc_label_position(&[p(3.0, 4.0)]), p(3.0, 4.0));
}

#[test]
fn label_position_is_midpoint_of_horizontal_segment() {
    assert_eq!(
        calc_label_position(&[p(0.0, 0.0), p(10.0, 0.0)]),
        p(5.0, 0.0)
    );
}

#[test]
fn label_position_is_midpoint_of_vertical_segment() {
    assert_eq!(
        calc_label_position(&[p(0.0, 0.0), p(0.0, 10.0)]),
        p(0.0, 5.0)
    );
}

#[test]
fn label_position_at_half_arc_length_of_polyline() {
    // Total length 20, half = 10, which lands exactly on the middle vertex.
    let pts = [p(0.0, 0.0), p(10.0, 0.0), p(20.0, 0.0)];
    assert_eq!(calc_label_position(&pts), p(10.0, 0.0));
}

#[test]
fn label_position_handles_an_l_shaped_path() {
    // Right 10 then down 10: total 20, half 10 -> the corner at (10, 0).
    let pts = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)];
    assert_eq!(calc_label_position(&pts), p(10.0, 0.0));
}
