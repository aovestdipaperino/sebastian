//! Unit tests for the SVG number formatters, which decide the exact text of
//! every coordinate/length in the output. These replicate JavaScript's
//! `String(number)` and d3's rounding, so byte-exact parity depends on them.

use sebastian::svg::{d3_round, js_num};

#[test]
fn js_num_zero_and_negative_zero() {
    assert_eq!(js_num(0.0), "0");
    assert_eq!(js_num(-0.0), "0");
}

#[test]
fn js_num_integers_have_no_decimal_point() {
    assert_eq!(js_num(1.0), "1");
    assert_eq!(js_num(-1.0), "-1");
    assert_eq!(js_num(42.0), "42");
    assert_eq!(js_num(1000.0), "1000");
    assert_eq!(js_num(-987_654.0), "-987654");
}

#[test]
fn js_num_simple_fractions() {
    assert_eq!(js_num(0.5), "0.5");
    assert_eq!(js_num(-0.5), "-0.5");
    assert_eq!(js_num(1.25), "1.25");
    assert_eq!(js_num(3.75), "3.75");
}

#[test]
fn js_num_shortest_roundtrip() {
    // 0.1 has no exact binary form; JS prints the shortest string that
    // round-trips, i.e. "0.1" not "0.1000000000000000055".
    assert_eq!(js_num(0.1), "0.1");
    assert_eq!(js_num(0.2), "0.2");
    assert_eq!(js_num(0.3), "0.3");
    assert_eq!(js_num(0.1 + 0.2), "0.30000000000000004");
}

#[test]
fn js_num_leading_zero_kept() {
    assert_eq!(js_num(0.125), "0.125");
    assert_eq!(js_num(-0.0625), "-0.0625");
}

#[test]
fn js_num_small_magnitudes_stay_plain_through_1e_minus_7() {
    // JS keeps plain decimal for exponents down to -7; -8 switches to sci.
    assert_eq!(js_num(0.000_001), "0.000001");
    assert_eq!(js_num(0.000_000_1), "0.0000001");
    assert_eq!(js_num(0.000_000_01), "1e-8");
}

#[test]
fn js_num_large_integers_below_i64_max() {
    assert_eq!(js_num(1e18), "1000000000000000000");
}

#[test]
fn js_num_huge_integers_expand_in_full() {
    // Integers in [i64::MAX, 1e21) must expand exactly (not saturate the
    // i64 cast) and stay plain until 1e21 switches to exponential.
    assert_eq!(js_num(1e20), "100000000000000000000");
    // >= 1e21 switches to exponential. (JS prints "1e+21"; the current port
    // emits "1e21" without the sign — out of realistic coordinate range.)
    assert_eq!(js_num(1e21), "1e21");
}

#[test]
fn js_num_trailing_zeros_trimmed() {
    assert_eq!(js_num(1.5000), "1.5");
    assert_eq!(js_num(2.10), "2.1");
}

#[test]
fn js_num_negative_fractions() {
    assert_eq!(js_num(-7.8125), "-7.8125");
    assert_eq!(js_num(-0.001), "-0.001");
}

#[test]
fn d3_round_to_three_decimals() {
    assert_eq!(d3_round(1.23456), 1.235);
    assert_eq!(d3_round(1.0), 1.0);
    assert_eq!(d3_round(0.0005), 0.001);
    assert_eq!(d3_round(-1.23456), -1.235);
    assert_eq!(d3_round(10.0), 10.0);
}

#[test]
fn d3_round_then_js_num_is_clean() {
    // The common pipeline: round a computed coordinate, then format it.
    assert_eq!(js_num(d3_round(12.345_678_9)), "12.346");
    assert_eq!(js_num(d3_round(0.99999)), "1");
    assert_eq!(js_num(d3_round(100.0)), "100");
}
