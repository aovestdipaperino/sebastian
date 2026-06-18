//! SVG rendering pipeline (port of mermaid's rendering-util for flowcharts).

pub mod bbox;
pub mod clusters;
pub mod config;
pub mod css;
pub mod dagre_render;
pub mod data;
pub mod edges;
pub mod graph;
pub mod handdrawn;
pub mod khroma;
pub mod markers;
#[cfg(feature = "raster")]
pub mod raster;
pub mod shapes;
pub mod styles;
pub mod svg_label;
pub mod themes;

use crate::svg::{append, js_num, new_element, serialize, set_attr, set_text};
use crate::text::TextMeasurer;

/// Formats a CSS pixel length like Chrome's CSSOM (≤6 significant digits).
pub(crate) fn css_length(n: f64) -> String {
    let s = format!("{n:.5e}");
    // Reformat to %.6g-style: parse mantissa/exponent.
    let mut value = format!("{n}");
    if let Some((mantissa, exp)) = s.split_once('e') {
        let exp: i32 = exp.parse().expect("exponent");
        let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
        let digits = digits.trim_end_matches('0');
        let neg = mantissa.starts_with('-');
        if !digits.is_empty() && exp.abs() < 21 {
            let mut out = String::new();
            if neg {
                out.push('-');
            }
            let point = exp + 1;
            if point <= 0 {
                out.push_str("0.");
                for _ in 0..-point {
                    out.push('0');
                }
                out.push_str(digits);
            } else if (point as usize) >= digits.len() {
                out.push_str(digits);
                for _ in 0..(point as usize - digits.len()) {
                    out.push('0');
                }
            } else {
                out.push_str(&digits[..point as usize]);
                out.push('.');
                out.push_str(&digits[point as usize..]);
            }
            value = out;
        }
    }
    value
}

fn render_markers_state(root_g: &crate::svg::Element) -> edges::MarkerState {
    edges::MarkerState {
        root: root_g.clone(),
        created: std::collections::HashSet::new(),
    }
}

/// Diagram-type-specific chrome around the shared dagre pipeline.
pub(crate) struct DiagramChrome {
    pub(crate) svg_class: &'static str,
    pub(crate) aria: &'static str,
    /// Marker/type string (`data4Layout.type`).
    pub(crate) diagram_type: &'static str,
    pub(crate) css: String,
}

/// The shared rendering pipeline (svg scaffold, dagre layout render, defs,
/// viewport).
///
/// Driven by the top-level orchestration layer, which supplies the
/// already-parsed [`data::LayoutData`] plus the diagram-specific
/// [`DiagramChrome`].
pub(crate) fn render_unified(
    data: &data::LayoutData,
    config: &config::RenderConfig,
    theme_vars: &serde_json::Map<String, serde_json::Value>,
    chrome: &DiagramChrome,
    id: &str,
) -> String {
    let measurer = TextMeasurer::new();

    // Root SVG element.
    let svg = new_element("svg");
    set_attr(&svg, "id", id);
    set_attr(&svg, "width", "100%");
    set_attr(&svg, "xmlns", "http://www.w3.org/2000/svg");
    set_attr(&svg, "xmlns:xlink", "http://www.w3.org/1999/xlink");
    // Placeholder positions; style/viewBox filled at the end in the same
    // attribute order mmdc produces.
    set_attr(&svg, "class", chrome.svg_class);
    set_attr(&svg, "style", "");
    set_attr(&svg, "viewBox", "");
    set_attr(&svg, "role", "graphics-document document");
    set_attr(&svg, "aria-roledescription", chrome.aria);

    let style = append(&svg, "style");
    set_text(&style, &chrome.css);

    let root_g = append(&svg, "g");

    // Prefix node domIds with the diagram id (render.ts).
    for node in &data.nodes {
        let mut n = node.borrow_mut();
        let original = if n.dom_id.is_empty() {
            n.id.clone()
        } else {
            n.dom_id.clone()
        };
        n.dom_id = format!("{id}-{original}");
    }

    let mut ctx = dagre_render::RenderCtx {
        measurer,
        config: config.clone(),
        markers: render_markers_state(&root_g),
        state: graph::GraphlibState::default(),
        diagram_id: id.to_owned(),
        diagram_type: chrome.diagram_type.to_owned(),
        node_elems: std::collections::HashMap::new(),
        edge_label_elems: std::collections::HashMap::new(),
    };
    dagre_render::render(data, &root_g, &mut ctx);

    //

    // drop-shadow defs appended by render.ts after layout render.
    let flood_color = if config.theme.contains("dark") {
        "#FFFFFF"
    } else {
        "#000000"
    };
    for (suffix, hw, dxy) in [
        ("drop-shadow", "130%", "4"),
        ("drop-shadow-small", "150%", "2"),
    ] {
        let defs = append(&svg, "defs");
        let filter = append(&defs, "filter");
        set_attr(&filter, "id", format!("{id}-{suffix}"));
        set_attr(&filter, "height", hw);
        set_attr(&filter, "width", hw);
        let shadow = append(&filter, "feDropShadow");
        set_attr(&shadow, "dx", dxy);
        set_attr(&shadow, "dy", dxy);
        set_attr(&shadow, "stdDeviation", "0");
        set_attr(&shadow, "flood-opacity", "0.06");
        set_attr(&shadow, "flood-color", flood_color);
    }

    // Gradient defs (render.ts) when the theme uses gradients.
    if themes::get_bool(theme_vars, "useGradient") {
        let gradient = append(&svg, "linearGradient");
        set_attr(&gradient, "id", format!("{id}-gradient"));
        set_attr(&gradient, "gradientUnits", "objectBoundingBox");
        set_attr(&gradient, "x1", "0%");
        set_attr(&gradient, "y1", "0%");
        set_attr(&gradient, "x2", "100%");
        set_attr(&gradient, "y2", "0%");
        for (offset, color_key) in [("0%", "gradientStart"), ("100%", "gradientStop")] {
            let stop = append(&gradient, "stop");
            set_attr(&stop, "offset", offset);
            set_attr(&stop, "stop-color", themes::get(theme_vars, color_key));
            set_attr(&stop, "stop-opacity", "1");
        }
    }

    // setupViewPortForSVG: bbox of the rendered content + padding.
    let padding = 8.0;
    let bounds = bbox::element_bbox(&root_g);
    // getBBox() returns an SVGRect backed by 32-bit floats.
    let f32q = |v: f64| f64::from(v as f32);
    let (bx, by, bw, bh) = if bounds.is_empty() {
        (0.0, 0.0, 0.0, 0.0)
    } else {
        (
            f32q(bounds.min_x),
            f32q(bounds.min_y),
            f32q(bounds.width()),
            f32q(bounds.height()),
        )
    };
    let width = bw + padding * 2.0;
    let height = bh + padding * 2.0;
    set_attr(
        &svg,
        "style",
        format!(
            "max-width: {}px; background-color: white;",
            css_length(width)
        ),
    );
    set_attr(
        &svg,
        "viewBox",
        format!(
            "{} {} {} {}",
            js_num(bx - padding),
            js_num(by - padding),
            js_num(width),
            js_num(height)
        ),
    );

    let mut out = String::new();
    serialize(&svg, &mut out);
    out
}
