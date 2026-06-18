//! Unit tests for the `khroma` color helpers (port of the khroma JS library).
//! Theme colors flow through these, so their channel extraction, luminance,
//! and lighten/darken/invert behavior must stay correct. Tests assert
//! invariants (bounds, monotonicity, round-trips) rather than exact output
//! strings, so they stay robust to formatting.

use sebastian::render::khroma::{
    channel, darken, invert, is_dark, lighten, luminance, parse, rgba, stringify,
};

#[test]
fn channel_extracts_rgb_bytes() {
    assert_eq!(channel("#ff8000", 'r'), 255.0);
    assert_eq!(channel("#ff8000", 'g'), 128.0);
    assert_eq!(channel("#ff8000", 'b'), 0.0);
}

#[test]
fn channel_extracts_arbitrary_hex() {
    assert_eq!(channel("#abcdef", 'r'), 171.0);
    assert_eq!(channel("#abcdef", 'g'), 205.0);
    assert_eq!(channel("#abcdef", 'b'), 239.0);
}

#[test]
fn luminance_black_is_zero_white_is_one() {
    assert!(luminance("#000000") < 0.001);
    assert!(luminance("#ffffff") > 0.999);
}

#[test]
fn luminance_is_monotonic_in_brightness() {
    assert!(luminance("#ffffff") > luminance("#808080"));
    assert!(luminance("#808080") > luminance("#000000"));
}

#[test]
fn green_contributes_more_luminance_than_blue() {
    // Rec. 709 weights: green dominates, blue contributes least.
    assert!(luminance("#00ff00") > luminance("#ff0000"));
    assert!(luminance("#ff0000") > luminance("#0000ff"));
}

#[test]
fn is_dark_threshold() {
    assert!(is_dark("#000000"));
    assert!(is_dark("#222222"));
    assert!(!is_dark("#ffffff"));
    assert!(!is_dark("#cccccc"));
}

#[test]
fn lighten_raises_luminance() {
    let base = luminance("#808080");
    assert!(luminance(&lighten("#808080", 20.0)) > base);
}

#[test]
fn darken_lowers_luminance() {
    let base = luminance("#808080");
    assert!(luminance(&darken("#808080", 20.0)) < base);
}

#[test]
fn lighten_to_max_approaches_white() {
    assert!(luminance(&lighten("#808080", 100.0)) > 0.999);
}

#[test]
fn darken_to_min_approaches_black() {
    assert!(luminance(&darken("#808080", 100.0)) < 0.001);
}

#[test]
fn invert_of_black_is_white() {
    assert!(luminance(&invert("#000000")) > 0.999);
}

#[test]
fn double_invert_round_trips() {
    let once = invert("#123456");
    let twice = invert(&once);
    // Channels recover their original values.
    assert_eq!(channel(&twice, 'r'), channel("#123456", 'r'));
    assert_eq!(channel(&twice, 'g'), channel("#123456", 'g'));
    assert_eq!(channel(&twice, 'b'), channel("#123456", 'b'));
}

#[test]
fn rgba_clamps_out_of_range_components() {
    let s = rgba(300.0, -5.0, 128.0, 2.0);
    assert_eq!(channel(&s, 'r'), 255.0);
    assert_eq!(channel(&s, 'g'), 0.0);
    assert_eq!(channel(&s, 'b'), 128.0);
}

#[test]
fn parse_then_stringify_preserves_channels() {
    let mut ch = parse("#3366cc");
    let s = stringify(&mut ch);
    assert_eq!(channel(&s, 'r'), 51.0);
    assert_eq!(channel(&s, 'g'), 102.0);
    assert_eq!(channel(&s, 'b'), 204.0);
}
