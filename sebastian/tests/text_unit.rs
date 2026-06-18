//! Unit tests for `text` helpers: `split_breaks` (split a label on `<br>` tags)
//! and `TextMeasurer` width measurement, which drives node/label sizing.

use sebastian::text::{TextMeasurer, split_breaks};

#[test]
fn split_breaks_on_br_variants() {
    assert_eq!(split_breaks("a<br>b"), vec!["a", "b"]);
    assert_eq!(split_breaks("a<br/>b"), vec!["a", "b"]);
    assert_eq!(split_breaks("a<br />b"), vec!["a", "b"]);
    assert_eq!(split_breaks("a<BR/>b"), vec!["a", "b"]); // case-insensitive
}

#[test]
fn split_breaks_multiple_segments() {
    assert_eq!(split_breaks("a<br/>b<br/>c"), vec!["a", "b", "c"]);
}

#[test]
fn split_breaks_without_break_returns_whole_string() {
    assert_eq!(split_breaks("plain text"), vec!["plain text"]);
}

#[test]
fn split_breaks_ignores_non_break_tags() {
    // <brx> is not a line break; the string is left intact.
    assert_eq!(split_breaks("a<brx>b"), vec!["a<brx>b"]);
}

#[test]
fn split_breaks_empty_segments() {
    assert_eq!(split_breaks("<br/>"), vec!["", ""]);
}

#[test]
fn measure_width_is_positive() {
    let m = TextMeasurer::new();
    assert!(m.measure_width("hello", 16.0) > 0.0);
}

#[test]
fn measure_width_grows_with_text_length() {
    let m = TextMeasurer::new();
    let one = m.measure_width("i", 16.0);
    let many = m.measure_width("iiiiiiiiii", 16.0);
    assert!(many > one);
}

#[test]
fn measure_width_scales_with_font_size() {
    let m = TextMeasurer::new();
    let small = m.measure_width("width", 16.0);
    let large = m.measure_width("width", 32.0);
    assert!(large > small);
}

#[test]
fn empty_string_has_zero_width() {
    let m = TextMeasurer::new();
    assert_eq!(m.measure_width("", 16.0), 0.0);
}
