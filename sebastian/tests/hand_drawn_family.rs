//! `look: handDrawn` draws class diagrams in Comic Neue (which has real bold
//! and italic faces) and every other diagram type in Excalifont.

use sebastian::render_diagram;

#[test]
fn hand_drawn_class_diagram_uses_comic_neue() {
    let src = "%%{init: {'look': 'handDrawn', 'htmlLabels': false}}%%\nclassDiagram\n    class View {\n        <<trait>>\n        +core()*\n    }\n    class Button\n    View <|-- Button\n";
    let svg = render_diagram(src, "my-svg").expect("class diagram renders");
    assert_eq!(
        svg.matches("@font-face{font-family:\"Comic Neue\"").count(),
        4,
        "expected four Comic Neue @font-face rules"
    );
    assert!(svg.contains("font-weight:bold;font-style:italic"));
    assert!(!svg.contains("font-family:\"Excalifont\""));
}

#[test]
fn hand_drawn_flowchart_keeps_excalifont() {
    let src = "%%{init: {'look': 'handDrawn'}}%%\nflowchart TB\n    A --> B\n";
    let svg = render_diagram(src, "my-svg").expect("flowchart renders");
    assert!(svg.contains("font-family:\"Excalifont\""));
    assert!(!svg.contains("Comic Neue"));
}

#[test]
fn family_resets_between_renders() {
    let class = "%%{init: {'look': 'handDrawn'}}%%\nclassDiagram\n    class A\n";
    let flow = "%%{init: {'look': 'handDrawn'}}%%\nflowchart TB\n    A --> B\n";
    render_diagram(class, "my-svg").expect("class renders");
    let svg = render_diagram(flow, "my-svg").expect("flowchart renders");
    assert!(svg.contains("font-family:\"Excalifont\""));
    assert!(!svg.contains("Comic Neue"));
}
