//! Port of `markers.js` for the marker set flowcharts use: point, circle,
//! cross (plus their `-margin` variants).

use crate::svg::{Element, append, set_attr};

struct MarkerSpec {
    id_suffix: &'static str,
    class: &'static str,
    view_box: Option<&'static str>,
    ref_x: &'static str,
    ref_y: &'static str,
    marker_width: &'static str,
    marker_height: &'static str,
    child: Child,
    style: &'static str,
    /// refY listed before refX in the JS for circleEnd-margin.
    ref_y_first: bool,
}

enum Child {
    Path(&'static str),
    Polygon(&'static str),
    Circle,
}

#[allow(clippy::too_many_lines)]
fn marker_specs() -> Vec<MarkerSpec> {
    vec![
        MarkerSpec {
            id_suffix: "pointEnd",
            class: "marker",
            view_box: Some("0 0 10 10"),
            ref_x: "5",
            ref_y: "5",
            marker_width: "8",
            marker_height: "8",
            child: Child::Path("M 0 0 L 10 5 L 0 10 z"),
            style: "stroke-width: 1; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "pointStart",
            class: "marker",
            view_box: Some("0 0 10 10"),
            ref_x: "4.5",
            ref_y: "5",
            marker_width: "8",
            marker_height: "8",
            child: Child::Path("M 0 5 L 10 10 L 10 0 z"),
            style: "stroke-width: 1; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "pointEnd-margin",
            class: "marker",
            view_box: Some("0 0 11.5 14"),
            ref_x: "11.5",
            ref_y: "7",
            marker_width: "10.5",
            marker_height: "14",
            child: Child::Path("M 0 0 L 11.5 7 L 0 14 z"),
            style: "stroke-width: 0; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "pointStart-margin",
            class: "marker",
            view_box: Some("0 0 11.5 14"),
            ref_x: "1",
            ref_y: "7",
            marker_width: "11.5",
            marker_height: "14",
            child: Child::Polygon("0,7 11.5,14 11.5,0"),
            style: "stroke-width: 0; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "circleEnd",
            class: "marker",
            view_box: Some("0 0 10 10"),
            ref_x: "11",
            ref_y: "5",
            marker_width: "11",
            marker_height: "11",
            child: Child::Circle,
            style: "stroke-width: 1; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "circleStart",
            class: "marker",
            view_box: Some("0 0 10 10"),
            ref_x: "-1",
            ref_y: "5",
            marker_width: "11",
            marker_height: "11",
            child: Child::Circle,
            style: "stroke-width: 1; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "circleEnd-margin",
            class: "marker",
            view_box: Some("0 0 10 10"),
            ref_x: "12.25",
            ref_y: "5",
            marker_width: "14",
            marker_height: "14",
            child: Child::Circle,
            style: "stroke-width: 0; stroke-dasharray: 1, 0;",
            ref_y_first: true,
        },
        MarkerSpec {
            id_suffix: "circleStart-margin",
            class: "marker",
            view_box: Some("0 0 10 10"),
            ref_x: "-2",
            ref_y: "5",
            marker_width: "14",
            marker_height: "14",
            child: Child::Circle,
            style: "stroke-width: 0; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "crossEnd",
            class: "marker cross",
            view_box: Some("0 0 11 11"),
            ref_x: "12",
            ref_y: "5.2",
            marker_width: "11",
            marker_height: "11",
            child: Child::Path("M 1,1 l 9,9 M 10,1 l -9,9"),
            style: "stroke-width: 2; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "crossStart",
            class: "marker cross",
            view_box: Some("0 0 11 11"),
            ref_x: "-1",
            ref_y: "5.2",
            marker_width: "11",
            marker_height: "11",
            child: Child::Path("M 1,1 l 9,9 M 10,1 l -9,9"),
            style: "stroke-width: 2; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "crossEnd-margin",
            class: "marker cross",
            view_box: Some("0 0 15 15"),
            ref_x: "17.7",
            ref_y: "7.5",
            marker_width: "12",
            marker_height: "12",
            child: Child::Path("M 1,1 L 14,14 M 1,14 L 14,1"),
            style: "stroke-width: 2.5;",
            ref_y_first: false,
        },
        MarkerSpec {
            id_suffix: "crossStart-margin",
            class: "marker cross",
            view_box: Some("0 0 15 15"),
            ref_x: "-3.5",
            ref_y: "7.5",
            marker_width: "12",
            marker_height: "12",
            child: Child::Path("M 1,1 L 14,14 M 1,14 L 14,1"),
            style: "stroke-width: 2.5; stroke-dasharray: 1, 0;",
            ref_y_first: false,
        },
    ]
}

/// Inserts the point/circle/cross markers (flowchart marker set).
pub fn insert_markers(elem: &Element, diagram_type: &str, id: &str) {
    if diagram_type == "stateDiagram" {
        insert_barb(elem, diagram_type, id);
        return;
    }
    if diagram_type == "class" {
        insert_class_markers(elem, diagram_type, id);
        return;
    }
    for spec in marker_specs() {
        let marker = create_marker(elem, &spec, diagram_type, id);
        let _ = marker;
    }
}

/// The `barb` marker (state diagrams), wrapped in its own `<defs>`.
fn insert_barb(elem: &Element, diagram_type: &str, id: &str) {
    let defs = append(elem, "defs");
    let marker = append(&defs, "marker");
    set_attr(&marker, "id", format!("{id}_{diagram_type}-barbEnd"));
    set_attr(&marker, "refX", "19");
    set_attr(&marker, "refY", "7");
    set_attr(&marker, "markerWidth", "20");
    set_attr(&marker, "markerHeight", "14");
    set_attr(&marker, "markerUnits", "userSpaceOnUse");
    set_attr(&marker, "orient", "auto");
    let path = append(&marker, "path");
    set_attr(&path, "d", "M 19,7 L9,13 L14,7 L9,1 Z");
}

fn create_marker(elem: &Element, spec: &MarkerSpec, diagram_type: &str, id: &str) -> Element {
    {
        let marker = append(elem, "marker");
        set_attr(
            &marker,
            "id",
            format!("{id}_{diagram_type}-{}", spec.id_suffix),
        );
        set_attr(&marker, "class", format!("{} {diagram_type}", spec.class));
        if let Some(vb) = spec.view_box {
            set_attr(&marker, "viewBox", vb);
        }
        if spec.ref_y_first {
            set_attr(&marker, "refY", spec.ref_y);
            set_attr(&marker, "refX", spec.ref_x);
        } else {
            set_attr(&marker, "refX", spec.ref_x);
            set_attr(&marker, "refY", spec.ref_y);
        }
        set_attr(&marker, "markerUnits", "userSpaceOnUse");
        set_attr(&marker, "markerWidth", spec.marker_width);
        set_attr(&marker, "markerHeight", spec.marker_height);
        set_attr(&marker, "orient", "auto");
        match spec.child {
            Child::Path(d) => {
                let path = append(&marker, "path");
                set_attr(&path, "d", d);
                set_attr(&path, "class", "arrowMarkerPath");
                set_attr(&path, "style", spec.style);
            }
            Child::Polygon(points) => {
                let poly = append(&marker, "polygon");
                set_attr(&poly, "points", points);
                set_attr(&poly, "class", "arrowMarkerPath");
                set_attr(&poly, "style", spec.style);
            }
            Child::Circle => {
                let c = append(&marker, "circle");
                set_attr(&c, "cx", "5");
                set_attr(&c, "cy", "5");
                set_attr(&c, "r", "5");
                set_attr(&c, "class", "arrowMarkerPath");
                set_attr(&c, "style", spec.style);
            }
        }
        marker
    }
}

/// Clones a base marker with stroke/fill applied (edgeMarker.ts colored
/// variant). Appends to `root` like `parentNode.appendChild`.
pub fn create_colored_marker(
    root: &Element,
    diagram_type: &str,
    id: &str,
    marker_suffix: &str,
    colored_id: &str,
    stroke_color: &str,
    fill: bool,
) {
    let Some(spec) = marker_specs()
        .into_iter()
        .find(|s| s.id_suffix == marker_suffix)
    else {
        return;
    };
    let marker = create_marker(root, &spec, diagram_type, id);
    set_attr(&marker, "id", colored_id);
    // Apply colors to path/circle/line children.
    let children: Vec<Element> = marker
        .borrow()
        .children
        .iter()
        .filter_map(|c| match c {
            crate::svg::Node::Element(e) => Some(e.clone()),
            crate::svg::Node::Text(_) => None,
        })
        .collect();
    for child in children {
        let tag = child.borrow().tag.clone();
        if matches!(tag.as_str(), "path" | "circle" | "line") {
            set_attr(&child, "stroke", stroke_color);
            if fill {
                set_attr(&child, "fill", stroke_color);
            }
        }
    }
}

/// The class diagram marker set (aggregation/extension/composition/
/// dependency/lollipop, plus -margin variants), ported verbatim from
/// `markers.js`.
#[allow(clippy::too_many_lines)]
fn insert_class_markers(elem: &Element, ty: &str, id: &str) {
    struct M<'a> {
        suffix: &'a str,
        class: &'a str,
        attrs: &'a [(&'a str, &'a str)],
        child_tag: &'a str,
        child_attrs: &'a [(&'a str, &'a str)],
        in_defs: bool,
    }
    let markers: Vec<M<'_>> = vec![
        M {
            suffix: "aggregationStart",
            class: "marker aggregation ",
            attrs: &[
                ("refX", "18"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
            ],
            child_tag: "path",
            child_attrs: &[("d", "M 18,7 L9,13 L1,7 L9,1 Z")],
            in_defs: true,
        },
        M {
            suffix: "aggregationEnd",
            class: "marker aggregation ",
            attrs: &[
                ("refX", "1"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
            ],
            child_tag: "path",
            child_attrs: &[("d", "M 18,7 L9,13 L1,7 L9,1 Z")],
            in_defs: true,
        },
        M {
            suffix: "aggregationStart-margin",
            class: "marker aggregation ",
            attrs: &[
                ("refX", "15"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "path",
            child_attrs: &[
                ("d", "M 18,7 L9,13 L1,7 L9,1 Z"),
                ("style", "stroke-width: 2;"),
            ],
            in_defs: true,
        },
        M {
            suffix: "aggregationEnd-margin",
            class: "marker aggregation ",
            attrs: &[
                ("refX", "1"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "path",
            child_attrs: &[
                ("d", "M 18,7 L9,13 L1,7 L9,1 Z"),
                ("style", "stroke-width: 2;"),
            ],
            in_defs: true,
        },
        M {
            suffix: "extensionStart",
            class: "marker extension ",
            attrs: &[
                ("refX", "18"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "path",
            child_attrs: &[("d", "M 1,7 L18,13 V 1 Z")],
            in_defs: true,
        },
        M {
            suffix: "extensionEnd",
            class: "marker extension ",
            attrs: &[
                ("refX", "1"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
            ],
            child_tag: "path",
            child_attrs: &[("d", "M 1,1 V 13 L18,7 Z")],
            in_defs: true,
        },
        M {
            suffix: "extensionStart-margin",
            class: "marker extension ",
            attrs: &[
                ("refX", "18"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
                ("viewBox", "0 0 20 14"),
            ],
            child_tag: "polygon",
            child_attrs: &[
                ("points", "10,7 18,13 18,1"),
                ("style", "stroke-width: 2; stroke-dasharray: 0;"),
            ],
            in_defs: false,
        },
        M {
            suffix: "extensionEnd-margin",
            class: "marker extension ",
            attrs: &[
                ("refX", "9"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
                ("viewBox", "0 0 20 14"),
            ],
            child_tag: "polygon",
            child_attrs: &[
                ("points", "10,1 10,13 18,7"),
                ("style", "stroke-width: 2; stroke-dasharray: 0;"),
            ],
            in_defs: true,
        },
        M {
            suffix: "compositionStart",
            class: "marker composition ",
            attrs: &[
                ("refX", "18"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
            ],
            child_tag: "path",
            child_attrs: &[("d", "M 18,7 L9,13 L1,7 L9,1 Z")],
            in_defs: true,
        },
        M {
            suffix: "compositionEnd",
            class: "marker composition ",
            attrs: &[
                ("refX", "1"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
            ],
            child_tag: "path",
            child_attrs: &[("d", "M 18,7 L9,13 L1,7 L9,1 Z")],
            in_defs: true,
        },
        M {
            suffix: "compositionStart-margin",
            class: "marker composition ",
            attrs: &[
                ("refX", "15"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "path",
            child_attrs: &[
                ("viewBox", "0 0 15 15"),
                ("d", "M 18,7 L9,13 L1,7 L9,1 Z"),
                ("style", "stroke-width: 0;"),
            ],
            in_defs: true,
        },
        M {
            suffix: "compositionEnd-margin",
            class: "marker composition ",
            attrs: &[
                ("refX", "3.5"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "path",
            child_attrs: &[
                ("d", "M 18,7 L9,13 L1,7 L9,1 Z"),
                ("style", "stroke-width: 0;"),
            ],
            in_defs: true,
        },
        M {
            suffix: "dependencyStart",
            class: "marker dependency ",
            attrs: &[
                ("refX", "6"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
            ],
            child_tag: "path",
            child_attrs: &[("d", "M 5,7 L9,13 L1,7 L9,1 Z")],
            in_defs: true,
        },
        M {
            suffix: "dependencyEnd",
            class: "marker dependency ",
            attrs: &[
                ("refX", "13"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
            ],
            child_tag: "path",
            child_attrs: &[("d", "M 18,7 L9,13 L14,7 L9,1 Z")],
            in_defs: true,
        },
        M {
            suffix: "dependencyStart-margin",
            class: "marker dependency ",
            attrs: &[
                ("refX", "4"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "path",
            child_attrs: &[
                ("d", "M 5,7 L9,13 L1,7 L9,1 Z"),
                ("style", "stroke-width: 0;"),
            ],
            in_defs: true,
        },
        M {
            suffix: "dependencyEnd-margin",
            class: "marker dependency ",
            attrs: &[
                ("refX", "16"),
                ("refY", "7"),
                ("markerWidth", "20"),
                ("markerHeight", "28"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "path",
            child_attrs: &[
                ("d", "M 18,7 L9,13 L14,7 L9,1 Z"),
                ("style", "stroke-width: 0;"),
            ],
            in_defs: true,
        },
        M {
            suffix: "lollipopStart",
            class: "marker lollipop ",
            attrs: &[
                ("refX", "13"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
            ],
            child_tag: "circle",
            child_attrs: &[
                ("fill", "transparent"),
                ("cx", "7"),
                ("cy", "7"),
                ("r", "6"),
            ],
            in_defs: true,
        },
        M {
            suffix: "lollipopEnd",
            class: "marker lollipop ",
            attrs: &[
                ("refX", "1"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
            ],
            child_tag: "circle",
            child_attrs: &[
                ("fill", "transparent"),
                ("cx", "7"),
                ("cy", "7"),
                ("r", "6"),
            ],
            in_defs: true,
        },
        M {
            suffix: "lollipopStart-margin",
            class: "marker lollipop ",
            attrs: &[
                ("refX", "13"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "circle",
            child_attrs: &[
                ("fill", "transparent"),
                ("cx", "7"),
                ("cy", "7"),
                ("r", "6"),
                ("stroke-width", "2"),
            ],
            in_defs: true,
        },
        M {
            suffix: "lollipopEnd-margin",
            class: "marker lollipop ",
            attrs: &[
                ("refX", "1"),
                ("refY", "7"),
                ("markerWidth", "190"),
                ("markerHeight", "240"),
                ("orient", "auto"),
                ("markerUnits", "userSpaceOnUse"),
            ],
            child_tag: "circle",
            child_attrs: &[
                ("fill", "transparent"),
                ("cx", "7"),
                ("cy", "7"),
                ("r", "6"),
                ("stroke-width", "2"),
            ],
            in_defs: true,
        },
    ];
    for m in markers {
        let parent = if m.in_defs {
            append(elem, "defs")
        } else {
            elem.clone()
        };
        let marker = append(&parent, "marker");
        set_attr(&marker, "id", format!("{id}_{ty}-{}", m.suffix));
        set_attr(&marker, "class", format!("{}{ty}", m.class));
        for (k, v) in m.attrs {
            set_attr(&marker, k, *v);
        }
        let child = append(&marker, m.child_tag);
        for (k, v) in m.child_attrs {
            set_attr(&child, k, *v);
        }
    }
}
