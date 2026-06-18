//! Unit tests for pure geometry/utility functions in `dagre::util`.
//!
//! `intersect_rect` finds where the ray from a rectangle's center to an
//! external point crosses the rectangle's border. It is used to clip edge
//! endpoints to node boundaries, so its corner/axis behavior must be exact.

use sebastian::dagre::types::{NodeLabel, Point};
use sebastian::dagre::util::intersect_rect;

/// A rect centered at (cx, cy) with the given full width/height.
fn rect(cx: f64, cy: f64, w: f64, h: f64) -> NodeLabel {
    NodeLabel {
        x: Some(cx),
        y: Some(cy),
        width: w,
        height: h,
        ..NodeLabel::default()
    }
}

fn approx(a: Point, x: f64, y: f64) {
    assert!((a.x - x).abs() < 1e-9, "x: {} != {}", a.x, x);
    assert!((a.y - y).abs() < 1e-9, "y: {} != {}", a.y, y);
}

#[test]
fn exits_right_edge_for_point_to_the_right() {
    let r = rect(0.0, 0.0, 10.0, 10.0);
    approx(intersect_rect(&r, Point { x: 10.0, y: 0.0 }), 5.0, 0.0);
}

#[test]
fn exits_left_edge_for_point_to_the_left() {
    let r = rect(0.0, 0.0, 10.0, 10.0);
    approx(intersect_rect(&r, Point { x: -10.0, y: 0.0 }), -5.0, 0.0);
}

#[test]
fn exits_top_edge_for_point_above() {
    let r = rect(0.0, 0.0, 10.0, 10.0);
    approx(intersect_rect(&r, Point { x: 0.0, y: -10.0 }), 0.0, -5.0);
}

#[test]
fn exits_bottom_edge_for_point_below() {
    let r = rect(0.0, 0.0, 10.0, 10.0);
    approx(intersect_rect(&r, Point { x: 0.0, y: 10.0 }), 0.0, 5.0);
}

#[test]
fn hits_corner_on_exact_diagonal_of_square() {
    // On a square, a 45° ray exits exactly at the corner.
    let r = rect(0.0, 0.0, 10.0, 10.0);
    approx(intersect_rect(&r, Point { x: 10.0, y: 10.0 }), 5.0, 5.0);
}

#[test]
fn respects_offset_center() {
    let r = rect(100.0, 50.0, 20.0, 40.0);
    approx(intersect_rect(&r, Point { x: 130.0, y: 50.0 }), 110.0, 50.0);
    approx(
        intersect_rect(&r, Point { x: 100.0, y: 100.0 }),
        100.0,
        70.0,
    );
}

#[test]
fn tall_rect_clips_shallow_ray_to_the_side() {
    // width 40 (half 20), height 100 (half 50): a ray mostly sideways
    // exits the left/right edge, scaling y by w*dy/dx.
    let r = rect(0.0, 0.0, 40.0, 100.0);
    let p = intersect_rect(&r, Point { x: 40.0, y: 10.0 });
    // dy.abs()*w = 10*20=200, dx.abs()*h = 40*50=2000 -> side branch.
    approx(p, 20.0, 5.0);
}

#[test]
fn wide_rect_clips_steep_ray_to_top_bottom() {
    // width 100 (half 50), height 40 (half 20): a steep ray exits top/bottom.
    let r = rect(0.0, 0.0, 100.0, 40.0);
    let p = intersect_rect(&r, Point { x: 10.0, y: 40.0 });
    // dy.abs()*w = 40*50=2000 > dx.abs()*h = 10*20=200 -> top/bottom branch.
    approx(p, 5.0, 20.0);
}

#[test]
#[should_panic(expected = "not possible to find intersection")]
fn panics_when_point_is_the_center() {
    let r = rect(0.0, 0.0, 10.0, 10.0);
    let _ = intersect_rect(&r, Point { x: 0.0, y: 0.0 });
}
