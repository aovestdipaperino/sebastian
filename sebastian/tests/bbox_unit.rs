//! Unit tests for `render::bbox`: the `Rect` geometry type, the Blink SVG
//! number parser (`blink_float`), and `element_bbox` over the emitted SVG tree.
//! These drive the final viewBox, so the geometry must be exact.

use sebastian::render::bbox::{Rect, blink_float, element_bbox};
use sebastian::svg::{append, new_element, set_attr};

#[test]
fn blink_float_parses_plain_numbers() {
    assert_eq!(blink_float("100"), 100.0);
    assert_eq!(blink_float("0"), 0.0);
    assert_eq!(blink_float("12.5"), 12.5);
    assert_eq!(blink_float("-5.5"), -5.5);
    assert_eq!(blink_float("+3.25"), 3.25);
}

#[test]
fn blink_float_trims_and_handles_empty() {
    assert_eq!(blink_float("  42  "), 42.0);
    assert_eq!(blink_float(""), 0.0);
}

#[test]
fn rect_from_geometry_exposes_dimensions() {
    let r = Rect::from_geometry(1.0, 2.0, 3.0, 4.0);
    assert_eq!(r.min_x, 1.0);
    assert_eq!(r.min_y, 2.0);
    assert_eq!(r.width(), 3.0);
    assert_eq!(r.height(), 4.0);
    assert!(!r.is_empty());
}

#[test]
fn zero_area_rect_is_empty() {
    assert!(Rect::from_geometry(0.0, 0.0, 0.0, 0.0).is_empty());
    assert!(Rect::EMPTY.is_empty());
}

#[test]
fn union_with_expands_bounds() {
    let mut a = Rect::from_geometry(0.0, 0.0, 10.0, 10.0);
    a.union_with(&Rect::from_geometry(5.0, 5.0, 10.0, 10.0));
    assert_eq!(a.min_x, 0.0);
    assert_eq!(a.min_y, 0.0);
    assert_eq!(a.width(), 15.0);
    assert_eq!(a.height(), 15.0);
}

#[test]
fn union_with_empty_is_a_noop() {
    let mut a = Rect::from_geometry(2.0, 3.0, 4.0, 5.0);
    a.union_with(&Rect::EMPTY);
    assert_eq!(a.min_x, 2.0);
    assert_eq!(a.width(), 4.0);
    assert_eq!(a.height(), 5.0);
}

#[test]
fn union_into_empty_takes_other() {
    let mut a = Rect::EMPTY;
    a.union_with(&Rect::from_geometry(1.0, 1.0, 6.0, 8.0));
    assert_eq!(a.min_x, 1.0);
    assert_eq!(a.width(), 6.0);
    assert_eq!(a.height(), 8.0);
}

#[test]
fn element_bbox_of_a_rect() {
    let rect = new_element("rect");
    set_attr(&rect, "x", "10");
    set_attr(&rect, "y", "20");
    set_attr(&rect, "width", "30");
    set_attr(&rect, "height", "40");

    let b = element_bbox(&rect);
    assert!(!b.is_empty());
    assert_eq!(b.min_x, 10.0);
    assert_eq!(b.min_y, 20.0);
    assert_eq!(b.width(), 30.0);
    assert_eq!(b.height(), 40.0);
}

#[test]
fn element_bbox_includes_child_geometry() {
    let g = new_element("g");
    let rect = append(&g, "rect");
    set_attr(&rect, "x", "0");
    set_attr(&rect, "y", "0");
    set_attr(&rect, "width", "50");
    set_attr(&rect, "height", "25");

    let b = element_bbox(&g);
    assert_eq!(b.width(), 50.0);
    assert_eq!(b.height(), 25.0);
}

#[test]
fn element_bbox_of_style_element_is_empty() {
    let style = new_element("style");
    assert!(element_bbox(&style).is_empty());
}
