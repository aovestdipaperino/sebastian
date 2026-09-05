//! `htmlLabels: false` must make class boxes emit SVG `<text>` labels instead
//! of `<foreignObject>` HTML, so rasterizers without an HTML engine (resvg)
//! still show class names and members.

use sebastian::render_diagram;

#[test]
fn class_box_honours_html_labels_false() {
    let src = "%%{init: {'htmlLabels': false}}%%\nclassDiagram\n    class View {\n        <<trait>>\n        +core()*\n        -bounds\n    }\n    class Button\n    View <|-- Button : implements\n";
    let svg = render_diagram(src, "my-svg").expect("class diagram renders");
    assert!(
        !svg.contains("<foreignObject"),
        "foreignObject still emitted with htmlLabels:false"
    );
    for needle in [
        "View",
        "Button",
        "+core()",
        "-bounds",
        "«trait»",
        "implements",
    ] {
        assert!(svg.contains(needle), "missing label text {needle:?}");
    }
}
