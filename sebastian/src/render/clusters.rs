//! Port of `clusters.js` (the `rect` cluster shape used by flowcharts).

use crate::svg::{Element, append, insert_first, js_num, set_attr};
use crate::text::TextMeasurer;

use super::data::NodeRef;
use super::shapes::BBox;

impl std::fmt::Debug for InsertedCluster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InsertedCluster").finish_non_exhaustive()
    }
}

pub struct InsertedCluster {
    pub cluster: Element,
    pub label_bbox: BBox,
}

/// The SVG-text variant of the cluster label (htmlLabels: false).
#[allow(clippy::too_many_arguments)]
fn insert_cluster_svg_label(
    _parent: &Element,
    _node: &NodeRef,
    measurer: &TextMeasurer,
    shape_svg: &Element,
    label_el: &Element,
    font_size: f64,
    compiled: super::styles::CompiledStyles,
    mut n: std::cell::RefMut<'_, super::data::RenderNode>,
) -> InsertedCluster {
    let ft = super::svg_label::create_formatted_text(
        label_el,
        &n.label_raw,
        measurer,
        font_size,
        f64::INFINITY,
        false,
        false,
    );
    // createText isNode=true sets the (empty) node label text style.
    set_attr(&ft.label_element, "style", compiled.label_styles);
    let bbox = BBox {
        width: ft.text_bbox.width,
        height: ft.text_bbox.height,
        wrapped: false,
    };

    let width = if n.width <= bbox.width + n.padding {
        bbox.width + n.padding
    } else {
        n.width
    };
    if n.width <= bbox.width + n.padding {
        n.diff = (width - n.width) / 2.0 - n.padding;
    } else {
        n.diff = -n.padding;
    }

    let height = n.height;
    let x = n.x - width / 2.0;
    let y = n.y - height / 2.0;

    let rect = insert_first(shape_svg, "rect");
    set_attr(&rect, "style", compiled.node_styles);
    set_attr(&rect, "x", js_num(x));
    set_attr(&rect, "y", js_num(y));
    set_attr(&rect, "width", js_num(width));
    set_attr(&rect, "height", js_num(height));

    set_attr(
        label_el,
        "transform",
        format!(
            "translate({}, {})",
            js_num(n.x - bbox.width / 2.0),
            js_num(n.y - n.height / 2.0)
        ),
    );

    n.offset_x = 0.0;
    n.width = super::shapes::f32q(width);
    n.height = super::shapes::f32q(height);
    n.offset_y = bbox.height - n.padding / 2.0;
    n.intersect = Some(super::data::IntersectShape::Rect);

    InsertedCluster {
        cluster: shape_svg.clone(),
        label_bbox: bbox,
    }
}

/// Inserts a subgraph rectangle with its title label.
pub fn insert_cluster(
    parent: &Element,
    node: &NodeRef,
    measurer: &TextMeasurer,
    config: &super::config::RenderConfig,
) -> InsertedCluster {
    let mut n = node.borrow_mut();

    if n.shape == "noteGroup" {
        let shape_svg = append(parent, "g");
        set_attr(&shape_svg, "class", "note-cluster");
        set_attr(&shape_svg, "id", n.dom_id.clone());
        let rect = insert_first(&shape_svg, "rect");
        set_attr(&rect, "x", js_num(n.x - n.width / 2.0));
        set_attr(&rect, "y", js_num(n.y - n.height / 2.0));
        set_attr(&rect, "width", js_num(n.width));
        set_attr(&rect, "height", js_num(n.height));
        set_attr(&rect, "fill", "none");
        n.width = super::shapes::f32q(n.width);
        n.height = super::shapes::f32q(n.height);
        n.intersect = Some(super::data::IntersectShape::Rect);
        return InsertedCluster {
            cluster: shape_svg,
            label_bbox: BBox {
                width: 0.0,
                height: 0.0,
                wrapped: false,
            },
        };
    }

    let compiled = super::styles::styles2string(&n.css_compiled_styles, &n.css_styles, &[]);

    let shape_svg = append(parent, "g");
    set_attr(&shape_svg, "class", format!("cluster {}", n.css_classes));
    set_attr(&shape_svg, "id", n.dom_id.clone());
    set_attr(&shape_svg, "data-look", n.look.clone());

    // Label (createLabel path, isNode=true, useHtmlLabels).
    let label_el = append(&shape_svg, "g");
    set_attr(&label_el, "class", "cluster-label ");

    // createLabel passes width: Infinity — cluster titles never wrap.
    let font_size =
        super::shapes::font_size_from_styles_or(&compiled.label_styles, config.font_size());

    if !config.effective_html_labels() {
        return insert_cluster_svg_label(
            parent, node, measurer, &shape_svg, &label_el, font_size, compiled, n,
        );
    }

    let bbox = super::shapes::measure_label_sized(measurer, &n.label, f64::INFINITY, font_size);
    let fo = append(&label_el, "foreignObject");
    set_attr(&fo, "width", js_num(bbox.width));
    set_attr(&fo, "height", js_num(bbox.height));
    let div = crate::svg::append_xhtml(&fo, "div");
    // createLabel is invoked with node.labelStyle unset, so the div carries
    // only the base style (width: Infinity — no max-width/text-align); the
    // span's style is appended after creation (clusters.js
    // `span.attr('style', labelStyles)`), landing after the class attribute.
    set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
    set_attr(
        &div,
        "style",
        "display: table-cell; white-space: nowrap; line-height: 1.5;",
    );
    let span = crate::svg::append_xhtml(&div, "span");
    set_attr(&span, "class", "nodeLabel");
    if !compiled.label_styles.is_empty() {
        set_attr(&span, "style", compiled.label_styles.clone());
    }
    super::shapes::write_label_paragraph(&span, &n.label);

    let width = if n.width <= bbox.width + n.padding {
        bbox.width + n.padding
    } else {
        n.width
    };
    if n.width <= bbox.width + n.padding {
        n.diff = (width - n.width) / 2.0 - n.padding;
    } else {
        n.diff = -n.padding;
    }

    let height = n.height;
    let x = n.x - width / 2.0;
    let y = n.y - height / 2.0;

    let rect = insert_first(&shape_svg, "rect");
    set_attr(&rect, "style", compiled.node_styles.clone());
    set_attr(&rect, "x", js_num(x));
    set_attr(&rect, "y", js_num(y));
    set_attr(&rect, "width", js_num(width));
    set_attr(&rect, "height", js_num(height));

    let sub_graph_title_top_margin = 0.0;
    set_attr(
        &label_el,
        "transform",
        format!(
            "translate({}, {})",
            js_num(n.x - bbox.width / 2.0),
            js_num(n.y - n.height / 2.0 + sub_graph_title_top_margin)
        ),
    );

    n.offset_x = 0.0;
    n.width = super::shapes::f32q(width);
    n.height = super::shapes::f32q(height);
    n.offset_y = bbox.height - n.padding / 2.0;
    n.intersect = Some(super::data::IntersectShape::Rect);

    InsertedCluster {
        cluster: shape_svg,
        label_bbox: bbox,
    }
}
