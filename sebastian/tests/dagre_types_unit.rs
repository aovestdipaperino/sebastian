//! Unit tests for the JS-semantics helper functions and small types in
//! `dagre::types`. These pin the deliberately non-Rust-native behaviors
//! (NaN propagation, `undefined` comparisons, UTF-16 string order, the shared
//! uniqueId counter) that the dagre port depends on for byte-exact output.

use sebastian::dagre::types::{
    EdgeLabel, GraphLabel, NodeLabel, UniqueId, edge_ref, js_gt, js_lt, js_math_max, js_math_min,
    js_str_gt, node_ref,
};

#[test]
fn js_lt_requires_both_present() {
    assert!(js_lt(Some(1.0), Some(2.0)));
    assert!(!js_lt(Some(2.0), Some(1.0)));
    assert!(!js_lt(Some(1.0), Some(1.0)));
    // undefined on either side is always false (JS `undefined < x`).
    assert!(!js_lt(None, Some(2.0)));
    assert!(!js_lt(Some(1.0), None));
    assert!(!js_lt(None, None));
}

#[test]
fn js_gt_requires_both_present() {
    assert!(js_gt(Some(2.0), Some(1.0)));
    assert!(!js_gt(Some(1.0), Some(2.0)));
    assert!(!js_gt(Some(1.0), Some(1.0)));
    assert!(!js_gt(None, Some(1.0)));
    assert!(!js_gt(Some(2.0), None));
    assert!(!js_gt(None, None));
}

#[test]
fn js_math_max_propagates_nan() {
    assert_eq!(js_math_max(1.0, 2.0), 2.0);
    assert_eq!(js_math_max(2.0, 1.0), 2.0);
    assert_eq!(js_math_max(-5.0, -1.0), -1.0);
    assert!(js_math_max(f64::NAN, 1.0).is_nan());
    assert!(js_math_max(1.0, f64::NAN).is_nan());
}

#[test]
fn js_math_min_propagates_nan() {
    assert_eq!(js_math_min(1.0, 2.0), 1.0);
    assert_eq!(js_math_min(2.0, 1.0), 1.0);
    assert_eq!(js_math_min(-5.0, -1.0), -5.0);
    assert!(js_math_min(f64::NAN, 1.0).is_nan());
    assert!(js_math_min(1.0, f64::NAN).is_nan());
}

#[test]
fn js_math_max_min_differ_from_std_on_nan() {
    // f64::max/min ignore NaN; the JS variants must not.
    assert!(!1.0_f64.max(f64::NAN).is_nan());
    assert!(js_math_max(1.0, f64::NAN).is_nan());
}

#[test]
fn js_str_gt_is_lexicographic_for_ascii() {
    assert!(js_str_gt("b", "a"));
    assert!(!js_str_gt("a", "b"));
    assert!(!js_str_gt("a", "a"));
    assert!(js_str_gt("ab", "aa"));
    assert!(js_str_gt("abc", "ab"));
}

#[test]
fn js_str_gt_uses_utf16_code_units() {
    // U+1F600 (😀) is a surrogate pair in UTF-16; its leading code unit
    // (0xD83D) sorts below U+E000, unlike raw byte/codepoint order.
    assert!(js_str_gt("\u{E000}", "\u{1F600}"));
    assert!(!js_str_gt("\u{1F600}", "\u{E000}"));
}

#[test]
fn unique_id_starts_at_one_and_shares_counter() {
    let ids = UniqueId::new();
    assert_eq!(ids.next("a"), "a1");
    assert_eq!(ids.next("a"), "a2");
    // The counter is shared across prefixes (one global sequence).
    assert_eq!(ids.next("b"), "b3");
    assert_eq!(ids.next("x"), "x4");
}

#[test]
fn unique_id_default_matches_new() {
    let a = UniqueId::new();
    let b = UniqueId::default();
    assert_eq!(a.next("n"), b.next("n"));
}

#[test]
fn edge_label_defaults_match_dagre() {
    let e = EdgeLabel::default();
    assert_eq!(e.minlen, 1.0);
    assert_eq!(e.weight, 1.0);
    assert_eq!(e.labeloffset, 10.0);
    assert_eq!(e.labelpos, "r");
    assert!(!e.reversed);
    assert!(!e.nesting_edge);
    assert!(e.points.is_none());
}

#[test]
fn edge_label_rank_label_overrides_only_weight_and_minlen() {
    let e = EdgeLabel::rank_label(3.0, 2.0);
    assert_eq!(e.weight, 3.0);
    assert_eq!(e.minlen, 2.0);
    // Everything else stays at the default.
    assert_eq!(e.labeloffset, 10.0);
    assert_eq!(e.labelpos, "r");
}

#[test]
fn graph_label_defaults_match_dagre() {
    let g = GraphLabel::default();
    assert_eq!(g.nodesep, 50.0);
    assert_eq!(g.edgesep, 20.0);
    assert_eq!(g.ranksep, 50.0);
    assert_eq!(g.rankdir, "tb");
    assert_eq!(g.marginx, 0.0);
    assert_eq!(g.marginy, 0.0);
    assert!(g.align.is_none());
    assert!(g.dummy_chains.is_empty());
}

#[test]
fn node_label_default_is_zeroed() {
    let n = NodeLabel::default();
    assert_eq!(n.width, 0.0);
    assert_eq!(n.height, 0.0);
    assert!(n.rank.is_none());
    assert!(n.order.is_none());
    assert!(n.dummy.is_none());
    assert!(n.self_edges.is_empty());
    assert!(n.border_left.is_empty());
}

#[test]
fn node_ref_and_edge_ref_wrap_shared_handles() {
    let n = node_ref(NodeLabel {
        width: 7.0,
        ..NodeLabel::default()
    });
    assert_eq!(n.borrow().width, 7.0);
    n.borrow_mut().height = 3.0;
    assert_eq!(n.borrow().height, 3.0);

    let e = edge_ref(EdgeLabel::rank_label(2.0, 1.0));
    assert_eq!(e.borrow().weight, 2.0);
    // A clone of the Rc observes mutations (shared ownership).
    let e2 = e.clone();
    e.borrow_mut().reversed = true;
    assert!(e2.borrow().reversed);
}
