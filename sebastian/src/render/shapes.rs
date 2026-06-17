//! Port of `shapes/util.ts` (labelHelper) and the flowchart shape handlers.

use crate::dagre::types::Point;
use crate::svg::{Element, append, append_xhtml, insert_first, js_num, set_attr};
use crate::text::TextMeasurer;

use super::data::{IntersectShape, NodeRef};

pub const FONT_SIZE: f64 = 16.0;
pub const LINE_HEIGHT: f64 = 24.0;
pub const WRAPPING_WIDTH: f64 = 200.0;

/// `updateNodeBounds` reads `getBBox()`, which returns 32-bit floats.
#[must_use]
pub fn f32q(v: f64) -> f64 {
    f64::from(v as f32)
}

/// Measured label box.
#[derive(Debug, Clone, Copy)]
pub struct BBox {
    pub width: f64,
    pub height: f64,
    /// True when the text wrapped at the 200px max-width (changes the div's
    /// style in Chrome's layout, see `addHtmlSpan`).
    pub wrapped: bool,
}

/// Splits a label on `<br>`/`<br/>` tags.
fn split_lines(label: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = label;
    loop {
        if let Some(idx) = rest.find("<br") {
            let after = &rest[idx..];
            if let Some(close) = after.find('>') {
                lines.push(rest[..idx].to_owned());
                rest = &after[close + 1..];
                continue;
            }
        }
        lines.push(rest.to_owned());
        break;
    }
    lines
}

/// Word-wraps a line at the wrapping width like Chrome's table-cell layout.
/// Splits a line into atoms at soft break opportunities, matching Chrome's
/// line breaker as probed empirically:
/// - after spaces,
/// - after a hyphen (unless followed by a digit, UAX #14 HY x NU),
/// - before an opening bracket when the previous character is not
///   alphanumeric (LB30 prohibits the break only after AL/NU).
fn break_atoms(line: &str) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut atoms = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if matches!(c, '(' | '[' | '{') && i > 0 && !current.is_empty() {
            let prev = chars[i - 1];
            // LB30 prohibits the break only after alphanumerics; GL (nbsp)
            // and a preceding opener prohibit it too.
            if !prev.is_alphanumeric()
                && prev != ' '
                && prev != '\u{a0}'
                && !matches!(prev, '(' | '[' | '{')
            {
                atoms.push(std::mem::take(&mut current));
            }
        }
        current.push(c);
        let next = chars.get(i + 1);
        let break_after = match c {
            ' ' => true,
            '-' => next.is_some_and(|n| !n.is_ascii_digit() && *n != ' '),
            _ => false,
        };
        if break_after {
            atoms.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        atoms.push(current);
    }
    atoms
}

/// Longest unbreakable run (min-content contribution) of a line.
pub(crate) fn max_unbreakable_width(measurer: &TextMeasurer, line: &str, font_size: f64) -> f64 {
    break_atoms(line)
        .iter()
        .map(|atom| measurer.measure_width(atom.trim_end_matches(' '), font_size))
        .fold(0.0, f64::max)
}

fn wrap_line_sized(
    measurer: &TextMeasurer,
    line: &str,
    wrap_width: f64,
    font_size: f64,
) -> Vec<String> {
    if measurer.measure_width(line, font_size) <= wrap_width {
        return vec![line.to_owned()];
    }
    let mut result = Vec::new();
    let mut current = String::new();
    for atom in break_atoms(line) {
        if current.is_empty() {
            current = atom;
            continue;
        }
        let candidate = format!("{current}{atom}");
        // The fitting measure excludes a trailing breakable space.
        if measurer.measure_width(candidate.trim_end_matches(' '), font_size) > wrap_width {
            result.push(
                std::mem::take(&mut current)
                    .trim_end_matches(' ')
                    .to_owned(),
            );
            current = atom;
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() {
        result.push(current.trim_end_matches(' ').to_owned());
    }
    result
}

/// Measures a (possibly multi-line) HTML label like Chrome.
#[must_use]
pub fn measure_label(measurer: &TextMeasurer, label: &str) -> BBox {
    measure_label_w(measurer, label, WRAPPING_WIDTH)
}

/// Extracts a `font-size: Npx` from a compiled label style string.
#[must_use]
pub fn font_size_from_styles(label_styles: &str) -> f64 {
    font_size_from_styles_or(label_styles, FONT_SIZE)
}

/// As above with an explicit default (the theme font size).
pub fn font_size_from_styles_or(label_styles: &str, default: f64) -> f64 {
    for decl in label_styles.split(';') {
        let mut parts = decl.splitn(2, ':');
        if parts.next().map(str::trim) == Some("font-size") {
            let value = parts.next().unwrap_or("").trim();
            let value = value.trim_end_matches("!important").trim();
            if let Some(px) = value.strip_suffix("px")
                && let Ok(n) = px.trim().parse::<f64>()
            {
                return n;
            }
        }
    }
    default
}

/// Measures with an explicit wrap width.
#[must_use]
pub fn measure_label_w(measurer: &TextMeasurer, label: &str, wrap_width: f64) -> BBox {
    measure_label_sized(measurer, label, wrap_width, FONT_SIZE)
}

/// Measures with explicit wrap width and font size (labels styled with
/// `font-size` lay out at that size; line height is 1.5em).
pub fn measure_label_sized(
    measurer: &TextMeasurer,
    label: &str,
    wrap_width: f64,
    font_size: f64,
) -> BBox {
    if label.is_empty() {
        return BBox {
            width: 0.0,
            height: 0.0,
            wrapped: false,
        };
    }
    let source_lines = split_lines(label);
    // Chrome's table-cell clamps at max-width whenever the max-content width
    // reaches it — even for unbreakable text — which triggers the
    // table/break-spaces switch in addHtmlSpan.
    let wrapped = wrap_width.is_finite()
        && source_lines
            .iter()
            .any(|line| measurer.measure_width(line, font_size) > wrap_width);
    // After the switch the div gets `width: 200px`, but a table box expands
    // to its min-content width (the longest unbreakable word) when larger.
    let effective_width = if wrapped {
        let max_word = source_lines
            .iter()
            .map(|line| max_unbreakable_width(measurer, line, font_size))
            .fold(0.0, f64::max);
        wrap_width.max(max_word)
    } else {
        wrap_width
    };
    let mut lines: Vec<String> = Vec::new();
    for line in &source_lines {
        lines.extend(wrap_line_sized(measurer, line, effective_width, font_size));
    }
    let width = if wrapped {
        effective_width
    } else {
        lines
            .iter()
            .map(|l| measurer.measure_width(l, font_size))
            .fold(0.0, f64::max)
    };
    #[allow(clippy::cast_precision_loss)]
    let height = font_size * 1.5 * lines.len() as f64;
    BBox {
        width,
        height,
        wrapped,
    }
}

/// Builds the `<foreignObject>` HTML label (htmlLabels mode).
/// `addHtmlSpan` with an explicit span class (markdown labels add
/// `markdown-node-label`).
#[allow(clippy::too_many_lines)]
fn build_html_label_classed(
    parent: &Element,
    label: &str,
    bbox: BBox,
    span_class: &str,
    add_background: bool,
    wrap_width: f64,
    label_style: &str,
) -> Element {
    let fo = append(parent, "foreignObject");
    set_attr(&fo, "width", js_num(bbox.width));
    set_attr(&fo, "height", js_num(bbox.height));
    let div = append_xhtml(&fo, "div");
    // createText replaces the first `fill:` with `color:` for labels.
    let label_style = label_style.replacen("fill:", "color:", 1);
    // Attribute order depends on whether applyStyle ran first (style attr
    // created before xmlns) or the style attr was created by CSSOM updates.
    if label_style.is_empty() {
        set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
        if add_background {
            set_attr(&div, "class", "labelBkg");
        }
        set_attr(&div, "style", div_style_w(bbox.wrapped, wrap_width));
    } else {
        set_attr(
            &div,
            "style",
            super::styles::div_style_attr(&label_style, &div_style_w(bbox.wrapped, wrap_width)),
        );
        set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
        if add_background {
            set_attr(&div, "class", "labelBkg");
        }
    }
    let span = append_xhtml(&div, "span");
    if !label_style.is_empty() {
        set_attr(&span, "style", label_style.clone());
    }
    set_attr(&span, "class", span_class);
    write_label_paragraph(&span, label);
    fo
}

/// Writes `<p>` content with `<br/>` tags as elements.
pub fn write_label_paragraph(span: &Element, label: &str) {
    if label.is_empty() {
        return;
    }
    let p = append_xhtml(span, "p");
    let lines = split_lines(label);
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            append_xhtml(&p, "br");
        }
        if !line.is_empty() {
            crate::svg::set_text_append(&p, line);
        }
    }
}

/// The div style emitted by `addHtmlSpan`; switches when text wrapped.
#[must_use]
pub fn div_style(wrapped: bool) -> String {
    div_style_w(wrapped, WRAPPING_WIDTH)
}

/// `addHtmlSpan` with an explicit wrap width (`node.width || wrappingWidth`).
#[must_use]
pub fn div_style_w(wrapped: bool, width: f64) -> String {
    let w = crate::svg::js_num(width);
    if wrapped {
        format!(
            "display: table; white-space: break-spaces; line-height: 1.5; max-width: {w}px; text-align: center; width: {w}px;"
        )
    } else {
        format!(
            "display: table-cell; white-space: nowrap; line-height: 1.5; max-width: {w}px; text-align: center;"
        )
    }
}

impl std::fmt::Debug for LabelResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelResult")
            .field("bbox", &self.bbox)
            .finish_non_exhaustive()
    }
}

pub struct LabelResult {
    pub shape_svg: Element,
    pub bbox: BBox,
    pub half_padding: f64,
    pub label: Element,
}

/// Removes every `prop value` declaration from a style string, mirroring the
/// JS regex `/prop[^;]+;?/g` (requires at least one non-`;` value char).
pub(crate) fn remove_style_decl(style: &str, prop: &str) -> String {
    let mut out = String::new();
    let mut rest = style;
    while let Some(pos) = rest.find(prop) {
        let after = &rest[pos + prop.len()..];
        let val_len = after.find(';').unwrap_or(after.len());
        if val_len == 0 {
            // No value char: the regex would not match here. Keep the literal
            // prop text and continue scanning past it.
            out.push_str(&rest[..pos + prop.len()]);
            rest = after;
            continue;
        }
        out.push_str(&rest[..pos]);
        let mut skip = pos + prop.len() + val_len;
        if rest[skip..].starts_with(';') {
            skip += 1;
        }
        rest = &rest[skip..];
    }
    out.push_str(rest);
    out
}

/// createText's `isNode` style transform for SVG text labels: the first
/// `stroke:` becomes `lineColor:`, remaining stroke/stroke-width/fill
/// declarations are stripped, and `color:` maps to `fill:`.
fn node_label_text_style(style: &str) -> String {
    if style.is_empty() {
        return String::new();
    }
    let s = if style.contains("stroke:") {
        style.replacen("stroke:", "lineColor:", 1)
    } else {
        style.to_owned()
    };
    let s = remove_style_decl(&s, "stroke:");
    let s = remove_style_decl(&s, "stroke-width:");
    let s = remove_style_decl(&s, "fill:");
    // `color:` is lowercase; the capital `C` in `lineColor:` is left intact.
    s.replace("color:", "fill:")
}

/// Port of `labelHelper`. Node labels render as a foreignObject HTML span
/// (`htmlLabels: true`, the default) or as an SVG `<text>`/`<tspan>` label
/// (`htmlLabels: false`, via `createFormattedText`).
pub fn label_helper(
    parent: &Element,
    node: &NodeRef,
    classes: Option<&str>,
    measurer: &TextMeasurer,
    config: &super::config::RenderConfig,
) -> LabelResult {
    let n = node.borrow();
    let css_classes = classes.map_or("node default".to_owned(), str::to_owned);

    let shape_svg = append(parent, "g");
    set_attr(&shape_svg, "class", &css_classes);
    set_attr(&shape_svg, "id", n.dom_id.clone());

    let label_el = append(&shape_svg, "g");
    set_attr(&label_el, "class", "label");
    set_attr(&label_el, "style", n.label_style_str.clone());

    let wrap_width = if n.width == 0.0 {
        config.wrapping_width
    } else {
        n.width
    };
    let font_size = font_size_from_styles_or(&n.label_style_str, config.font_size());

    let bbox = if config.node_html_labels() {
        let bbox = measure_label_sized(measurer, &n.label, wrap_width, font_size);
        build_html_label_classed(
            &label_el,
            &n.label,
            bbox,
            if n.label_type == "markdown" {
                "nodeLabel markdown-node-label"
            } else {
                "nodeLabel"
            },
            false,
            wrap_width,
            &n.label_style_str.clone(),
        );
        // labelEl.attr('transform', translate(-bbox.width/2, -bbox.height/2)).
        set_attr(
            &label_el,
            "transform",
            format!(
                "translate({}, {})",
                js_num(-bbox.width / 2.0),
                js_num(-bbox.height / 2.0)
            ),
        );
        bbox
    } else {
        // createText with useHtmlLabels=false, isNode=true: SVG text label.
        // addSvgBackground is false for plain nodes; centerText = !isNode =
        // false (horizontal centering comes from the `.node .label text`
        // `text-anchor: middle` CSS rule).
        let ft = super::svg_label::create_formatted_text(
            &label_el, &n.label, measurer, font_size, wrap_width, false, false,
        );
        // createText isNode branch applies the derived text style to the
        // returned label element (the bare <text> when no background).
        set_attr(
            &ft.label_element,
            "style",
            node_label_text_style(&n.label_style_str),
        );
        let bbox = BBox {
            width: ft.text_bbox.width,
            height: ft.text_bbox.height,
            wrapped: false,
        };
        // labelEl.attr('transform', translate(0, -bbox.height/2)).
        set_attr(
            &label_el,
            "transform",
            format!("translate(0, {})", js_num(-bbox.height / 2.0)),
        );
        bbox
    };

    let half_padding = n.padding / 2.0;
    // labelEl.insert('rect', ':first-child')
    insert_first(&label_el, "rect");
    LabelResult {
        shape_svg,
        bbox,
        half_padding,
        label: label_el,
    }
}

fn get_node_classes(node: &NodeRef) -> String {
    let n = node.borrow();
    // getNodeClasses: handDrawn nodes are "rough-node", classic ones "node".
    let kind = if n.look == "handDrawn" {
        "rough-node"
    } else {
        "node"
    };
    format!("{kind} {} ", n.css_classes)
}

/// Inserts a polygon like `insertPolygonShape`.
fn insert_polygon_shape(parent: &Element, w: f64, h: f64, points: &[Point]) -> Element {
    let polygon = insert_first(parent, "polygon");
    let pts: Vec<String> = points
        .iter()
        .map(|p| format!("{},{}", js_num(p.x), js_num(p.y)))
        .collect();
    set_attr(&polygon, "points", pts.join(" "));
    set_attr(&polygon, "class", "label-container");
    set_attr(
        &polygon,
        "transform",
        format!("translate({},{})", js_num(-w / 2.0), js_num(h / 2.0)),
    );
    polygon
}

fn polygon_bbox(points: &[Point]) -> (f64, f64) {
    let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    (max_x - min_x, max_y - min_y)
}

/// squareRect / roundedRect via `drawRect`.
#[allow(clippy::too_many_arguments)]
fn draw_rect(
    parent: &Element,
    node: &NodeRef,
    rx: f64,
    label_padding_x: f64,
    label_padding_y: f64,
    measurer: &TextMeasurer,
    node_styles: &str,
    config: &super::config::RenderConfig,
) -> Element {
    let result = label_helper(
        parent,
        node,
        Some(&get_node_classes(node)),
        measurer,
        config,
    );
    let mut n = node.borrow_mut();

    let total_width = f64::max(result.bbox.width + label_padding_x * 2.0, 0.0);
    let total_height = f64::max(result.bbox.height + label_padding_y * 2.0, 0.0);
    let x = -total_width / 2.0;
    let y = -total_height / 2.0;

    if n.look == "handDrawn" {
        let corners = [
            Point { x, y },
            Point {
                x: x + total_width,
                y,
            },
            Point {
                x: x + total_width,
                y: y + total_height,
            },
            Point {
                x,
                y: y + total_height,
            },
        ];
        let (fill, stroke) = hd_theme_colors(config);
        let g = super::handdrawn::hd_polygon(
            &result.shape_svg,
            &corners,
            &fill,
            &stroke,
            HD_STROKE_WIDTH,
            node_styles,
            super::handdrawn::seed_from(total_width, total_height),
        );
        set_attr(&g, "class", "basic label-container");
        n.width = f32q(total_width);
        n.height = f32q(total_height);
        n.intersect = Some(IntersectShape::Rect);
        return result.shape_svg;
    }

    let rect = insert_first(&result.shape_svg, "rect");
    set_attr(&rect, "class", "basic label-container");
    set_attr(&rect, "style", node_styles);
    if rx != 0.0 {
        set_attr(&rect, "rx", js_num(rx));
        set_attr(&rect, "ry", js_num(rx));
    }
    set_attr(&rect, "x", js_num(x));
    set_attr(&rect, "y", js_num(y));
    set_attr(&rect, "width", js_num(total_width));
    set_attr(&rect, "height", js_num(total_height));

    n.width = f32q(total_width);
    n.height = f32q(total_height);
    n.intersect = Some(IntersectShape::Rect);
    result.shape_svg
}

/// Dispatch on the node's shape; returns the shape `g` element.
#[allow(clippy::too_many_lines)]
pub fn insert_node_shape(
    parent: &Element,
    node: &NodeRef,
    measurer: &TextMeasurer,
    config: &super::config::RenderConfig,
) -> Element {
    let shape = node.borrow().shape.clone();
    let compiled = {
        let n = node.borrow();
        super::styles::styles2string(&n.css_compiled_styles, &n.css_styles, &[])
    };
    node.borrow_mut()
        .label_style_str
        .clone_from(&compiled.label_styles);
    let node_styles = compiled.node_styles;
    match shape.as_str() {
        "squareRect" | "rect" => {
            let padding = node.borrow().padding;
            draw_rect(
                parent,
                node,
                0.0,
                padding * 2.0,
                padding,
                measurer,
                &node_styles,
                config,
            )
        }
        "roundedRect" => {
            let padding = node.borrow().padding;
            draw_rect(
                parent,
                node,
                5.0,
                padding,
                padding,
                measurer,
                &node_styles,
                config,
            )
        }
        "diamond" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let w = result.bbox.width + n.padding;
            let h = result.bbox.height + n.padding;
            let s = w + h;
            let points = vec![
                Point { x: s / 2.0, y: 0.0 },
                Point { x: s, y: -s / 2.0 },
                Point { x: s / 2.0, y: -s },
                Point {
                    x: 0.0,
                    y: -s / 2.0,
                },
            ];
            let transform = format!("translate({}, {})", js_num(-s / 2.0 + 0.5), js_num(s / 2.0));
            if n.look == "handDrawn" {
                let (fill, stroke) = hd_theme_colors(config);
                let g = super::handdrawn::hd_polygon(
                    &result.shape_svg,
                    &points,
                    &fill,
                    &stroke,
                    HD_STROKE_WIDTH,
                    &node_styles,
                    super::handdrawn::seed_from(s, h),
                );
                set_attr(&g, "class", "basic label-container");
                set_attr(&g, "transform", transform);
                let (bw, bh) = polygon_bbox(&points);
                n.width = f32q(bw);
                n.height = f32q(bh);
                n.intersect = Some(IntersectShape::Question(points));
                return result.shape_svg;
            }
            let polygon = insert_polygon_shape(&result.shape_svg, s, s, &points);
            set_attr(&polygon, "transform", transform);
            if !node_styles.is_empty() {
                set_attr(&polygon, "style", node_styles.clone());
            }
            let (bw, bh) = polygon_bbox(&points);
            n.width = f32q(bw);
            n.height = f32q(bh);
            n.intersect = Some(IntersectShape::Question(points));
            result.shape_svg
        }
        "circle" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let radius = result.bbox.width / 2.0 + result.half_padding;
            if n.look == "handDrawn" {
                let (fill, stroke) = hd_theme_colors(config);
                let g = super::handdrawn::hd_ellipse(
                    &result.shape_svg,
                    radius,
                    radius,
                    &fill,
                    &stroke,
                    HD_STROKE_WIDTH,
                    &node_styles,
                    super::handdrawn::seed_from(radius, radius),
                );
                set_attr(&g, "class", "basic label-container");
                n.width = f32q(radius * 2.0);
                n.height = f32q(radius * 2.0);
                n.intersect = Some(IntersectShape::Circle { radius });
                return result.shape_svg;
            }
            let circle = insert_first(&result.shape_svg, "circle");
            set_attr(&circle, "class", "basic label-container");
            set_attr(&circle, "style", node_styles.clone());
            set_attr(&circle, "r", js_num(radius));
            set_attr(&circle, "cx", "0");
            set_attr(&circle, "cy", "0");
            n.width = f32q(radius * 2.0);
            n.height = f32q(radius * 2.0);
            n.intersect = Some(IntersectShape::Circle { radius });
            result.shape_svg
        }
        "doublecircle" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let gap = 5.0;
            let label_padding = n.padding;
            let outer_radius = result.bbox.width / 2.0 + label_padding;
            let inner_radius = outer_radius - gap;
            let group = insert_first(&result.shape_svg, "g");
            set_attr(&group, "class", "basic label-container");
            set_attr(&group, "style", node_styles.clone());
            let outer = append(&group, "circle");
            let inner = append(&group, "circle");
            for (c, r) in [(&outer, outer_radius), (&inner, inner_radius)] {
                set_attr(c, "class", "outer-circle");
                set_attr(c, "style", node_styles.clone());
                set_attr(c, "r", js_num(r));
                set_attr(c, "cx", "0");
                set_attr(c, "cy", "0");
            }
            set_attr(&inner, "class", "inner-circle");
            n.width = f32q(outer_radius * 2.0);
            n.height = f32q(outer_radius * 2.0);
            n.intersect = Some(IntersectShape::Circle {
                radius: outer_radius,
            });
            result.shape_svg
        }
        "subroutine" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let label_padding_x = n.padding;
            let label_padding_y = n.padding;
            let total_width = result.bbox.width + 2.0 * 8.0 + label_padding_x;
            let total_height = result.bbox.height + label_padding_y;
            let w = total_width - 16.0;
            let h = total_height;
            let points = vec![
                Point { x: 0.0, y: 0.0 },
                Point { x: w, y: 0.0 },
                Point { x: w, y: -h },
                Point { x: 0.0, y: -h },
                Point { x: 0.0, y: 0.0 },
                Point { x: -8.0, y: 0.0 },
                Point { x: w + 8.0, y: 0.0 },
                Point { x: w + 8.0, y: -h },
                Point { x: -8.0, y: -h },
                Point { x: -8.0, y: 0.0 },
            ];
            let el = insert_polygon_shape(&result.shape_svg, w, h, &points);
            if !node_styles.is_empty() {
                set_attr(&el, "style", node_styles.clone());
            }
            let (bw, bh) = polygon_bbox(&points);
            n.width = f32q(bw);
            n.height = f32q(bh);
            n.intersect = Some(IntersectShape::Polygon(points));
            result.shape_svg
        }
        "cylinder" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let label_padding_x = n.padding;
            let label_padding_y = n.padding;
            let w = result.bbox.width + label_padding_y;
            let rx = w / 2.0;
            let ry = rx / (2.5 + w / 50.0);
            let h = result.bbox.height + label_padding_x + ry;
            let d = format!(
                "M{},{} a{},{} 0,0,0 {},0 a{},{} 0,0,0 {},0 l0,{} a{},{} 0,0,0 {},0 l0,{}",
                js_num(0.0),
                js_num(ry),
                js_num(rx),
                js_num(ry),
                js_num(w),
                js_num(rx),
                js_num(ry),
                js_num(-w),
                js_num(h),
                js_num(rx),
                js_num(ry),
                js_num(w),
                js_num(-h)
            );
            let path = insert_first(&result.shape_svg, "path");
            set_attr(&path, "d", d);
            set_attr(&path, "class", "basic label-container outer-path");
            set_attr(&path, "style", node_styles.clone());
            // (cylinder sets label-offset-y in mermaid, but DOMPurify strips
            // the non-standard attribute from the final SVG)
            set_attr(
                &path,
                "transform",
                format!(
                    "translate({}, {})",
                    js_num(-w / 2.0),
                    js_num(-(h / 2.0 + ry))
                ),
            );
            n.width = f32q(w);
            n.height = f32q(h + 2.0 * ry);
            set_attr(
                &result.label,
                "transform",
                format!(
                    "translate({}, {})",
                    js_num(-result.bbox.width / 2.0),
                    js_num(-result.bbox.height / 2.0 + n.padding / 1.5)
                ),
            );
            n.intersect = Some(IntersectShape::Cylinder { rx, ry });
            result.shape_svg
        }
        "hexagon" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let f = 4.0;
            let label_padding_x = n.padding;
            let label_padding_y = n.padding;
            let h = result.bbox.height + label_padding_x;
            let m = h / f;
            let w = result.bbox.width + 2.0 * m + label_padding_y;
            let points = vec![
                Point { x: m, y: 0.0 },
                Point { x: w - m, y: 0.0 },
                Point { x: w, y: -h / 2.0 },
                Point { x: w - m, y: -h },
                Point { x: m, y: -h },
                Point {
                    x: 0.0,
                    y: -h / 2.0,
                },
            ];
            let polygon = insert_polygon_shape(&result.shape_svg, w, h, &points);
            if !node_styles.is_empty() {
                set_attr(&polygon, "style", node_styles.clone());
            }
            n.width = f32q(w);
            n.height = f32q(h);
            n.intersect = Some(IntersectShape::Polygon(points));
            result.shape_svg
        }
        "lean_right" | "lean_left" | "trapezoid" | "inv_trapezoid" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let label_padding_y = n.padding;
            let label_padding_x = n.padding;
            let (w, h) = if shape == "inv_trapezoid" {
                (
                    f64::max(result.bbox.width + label_padding_x * 2.0, 0.0),
                    f64::max(result.bbox.height + label_padding_y * 2.0, 0.0),
                )
            } else {
                (
                    result.bbox.width + label_padding_x,
                    result.bbox.height + label_padding_y,
                )
            };
            let points = match shape.as_str() {
                "lean_right" => vec![
                    Point {
                        x: -3.0 * h / 6.0,
                        y: 0.0,
                    },
                    Point { x: w, y: 0.0 },
                    Point {
                        x: w + 3.0 * h / 6.0,
                        y: -h,
                    },
                    Point { x: 0.0, y: -h },
                ],
                "lean_left" => vec![
                    Point { x: 0.0, y: 0.0 },
                    Point {
                        x: w + 3.0 * h / 6.0,
                        y: 0.0,
                    },
                    Point { x: w, y: -h },
                    Point {
                        x: -3.0 * h / 6.0,
                        y: -h,
                    },
                ],
                "trapezoid" => vec![
                    Point {
                        x: -3.0 * h / 6.0,
                        y: 0.0,
                    },
                    Point {
                        x: w + 3.0 * h / 6.0,
                        y: 0.0,
                    },
                    Point { x: w, y: -h },
                    Point { x: 0.0, y: -h },
                ],
                _ => vec![
                    Point { x: 0.0, y: 0.0 },
                    Point { x: w, y: 0.0 },
                    Point {
                        x: w + 3.0 * h / 6.0,
                        y: -h,
                    },
                    Point {
                        x: -3.0 * h / 6.0,
                        y: -h,
                    },
                ],
            };
            let polygon = insert_polygon_shape(&result.shape_svg, w, h, &points);
            if !node_styles.is_empty() {
                set_attr(&polygon, "style", node_styles.clone());
            }
            let (bw, bh) = polygon_bbox(&points);
            n.width = f32q(bw);
            n.height = f32q(bh);
            n.intersect = Some(IntersectShape::Polygon(points));
            result.shape_svg
        }
        "labelRect" => {
            let result = label_helper(parent, node, Some("label"), measurer, config);
            let mut n = node.borrow_mut();
            let rect = insert_first(&result.shape_svg, "rect");
            set_attr(&rect, "width", "0.1");
            set_attr(&rect, "height", "0.1");
            set_attr(&result.shape_svg, "class", "label edgeLabel");
            set_attr(
                &result.label,
                "transform",
                format!(
                    "translate({}, {})",
                    js_num(-result.bbox.width / 2.0),
                    js_num(-result.bbox.height / 2.0)
                ),
            );
            n.width = f32q(0.1);
            n.height = f32q(0.1);
            n.intersect = Some(IntersectShape::Rect);
            result.shape_svg
        }
        // Stadium and odd use rough.js output in mermaid; geometry matches,
        // path serialization is refined against mmdc in verification.
        "stadium" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let label_padding_x = n.padding;
            let label_padding_y = n.padding;
            let h = result.bbox.height + label_padding_y;
            let w = result.bbox.width + h / 4.0 + label_padding_x;
            let radius = h / 2.0;
            let mut points = vec![
                Point {
                    x: -w / 2.0 + radius,
                    y: -h / 2.0,
                },
                Point {
                    x: w / 2.0 - radius,
                    y: -h / 2.0,
                },
            ];
            points.extend(generate_circle_points(
                -w / 2.0 + radius,
                0.0,
                radius,
                50,
                90.0,
                270.0,
            ));
            points.push(Point {
                x: w / 2.0 - radius,
                y: h / 2.0,
            });
            points.extend(generate_circle_points(
                w / 2.0 - radius,
                0.0,
                radius,
                50,
                270.0,
                450.0,
            ));
            let styles = if node_styles.is_empty() {
                n.css_styles.join(",")
            } else {
                node_styles.clone()
            };
            let g = rough_polygon(&result.shape_svg, &points, &styles);
            set_attr(&g, "class", "basic label-container outer-path");
            let (bw, bh) = polygon_bbox(&points);
            n.width = f32q(bw);
            n.height = f32q(bh);
            n.intersect = Some(IntersectShape::Polygon(points));
            result.shape_svg
        }
        "odd" | "rect_left_inv_arrow" => {
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            let mut n = node.borrow_mut();
            let label_padding_x = n.padding;
            let label_padding_y = n.padding;
            let w = result.bbox.width + label_padding_x;
            let h = result.bbox.height + label_padding_y;
            let x = -w / 2.0;
            let y = -h / 2.0;
            let notch = y / 2.0;
            let points = vec![
                Point { x: x + notch, y },
                Point { x, y: 0.0 },
                Point {
                    x: x + notch,
                    y: -y,
                },
                Point { x: -x, y: -y },
                Point { x: -x, y },
            ];
            let styles = if node_styles.is_empty() {
                n.css_styles.join(",")
            } else {
                node_styles.clone()
            };
            let g = rough_polygon(&result.shape_svg, &points, &styles);
            set_attr(&g, "class", "basic label-container outer-path");
            set_attr(
                &g,
                "transform",
                format!("translate({},0)", js_num(-notch / 2.0)),
            );
            set_attr(
                &result.label,
                "transform",
                format!(
                    "translate({}, {})",
                    js_num(-notch / 2.0 - result.bbox.width / 2.0),
                    js_num(-result.bbox.height / 2.0)
                ),
            );
            let (bw, bh) = polygon_bbox(&points);
            n.width = f32q(bw);
            n.height = f32q(bh);
            n.intersect = Some(IntersectShape::Polygon(points));
            result.shape_svg
        }
        "stateStart" => {
            let n = node.borrow();
            let shape_svg = append(parent, "g");
            set_attr(&shape_svg, "class", "node default");
            set_attr(&shape_svg, "id", n.dom_id.clone());
            let circle = insert_first(&shape_svg, "circle");
            set_attr(&circle, "class", "state-start");
            set_attr(&circle, "r", "7");
            set_attr(&circle, "width", "14");
            set_attr(&circle, "height", "14");
            drop(n);
            let mut n = node.borrow_mut();
            n.width = 14.0;
            n.height = 14.0;
            n.intersect = Some(IntersectShape::Circle { radius: 7.0 });
            shape_svg
        }
        "stateEnd" => {
            let theme = &config.computed_theme;
            let v = |k: &str| super::themes::get(theme, k);
            let line_color = v("lineColor");
            let main_bkg = v("mainBkg");
            let inner_fill = {
                let sb = v("stateBorder");
                if sb.is_empty() { v("nodeBorder") } else { sb }
            };
            let n = node.borrow();
            let shape_svg = append(parent, "g");
            set_attr(&shape_svg, "class", "node default");
            set_attr(&shape_svg, "id", n.dom_id.clone());
            drop(n);

            let outer = insert_first(&shape_svg, "g");
            set_attr(&outer, "class", "outer-path");
            let d_outer = rough_ellipse_d(7.0, 7.0);
            rough_path_pair(&outer, &d_outer, &main_bkg, &line_color, "2");
            let inner = append(&outer, "g");
            let d_inner = rough_ellipse_d(2.5, 2.5);
            rough_path_pair(&inner, &d_inner, &inner_fill, &inner_fill, "2");
            // cssStyles is an empty array (truthy in JS): every descendant
            // path gets style="".
            set_path_styles(&outer, "");

            let bounds = super::bbox::element_bbox(&outer);
            let mut n = node.borrow_mut();
            n.width = f32q(bounds.width());
            n.height = f32q(bounds.height());
            let radius = n.width / 2.0;
            n.intersect = Some(IntersectShape::Circle { radius });
            shape_svg
        }
        "note" => {
            let theme = &config.computed_theme;
            let note_bkg = super::themes::get(theme, "noteBkgColor");
            let note_border = super::themes::get(theme, "noteBorderColor");
            let result = label_helper(
                parent,
                node,
                Some(&get_node_classes(node)),
                measurer,
                config,
            );
            set_attr(&result.label, "class", "label noteLabel");
            let mut n = node.borrow_mut();
            let total_width = f64::max(result.bbox.width + n.padding * 2.0, n.width);
            let total_height = f64::max(result.bbox.height + n.padding * 2.0, n.height);
            let x = -total_width / 2.0;
            let y = -total_height / 2.0;
            let rect = rough_rectangle(
                &result.shape_svg,
                x,
                y,
                total_width,
                total_height,
                &note_bkg,
                &note_border,
            );
            set_attr(&rect, "class", "basic label-container outer-path");
            set_path_styles(&rect, "");
            n.width = f32q(total_width);
            n.height = f32q(total_height);
            n.intersect = Some(IntersectShape::Rect);
            result.shape_svg
        }
        "classBox" => class_box(parent, node, measurer, config),
        other => panic!("no such shape: {other}. Please check your syntax."),
    }
}

/// Sets `style` on every descendant `path` (d3 `selectAll('path')`).
fn set_path_styles(el: &Element, style: &str) {
    let children: Vec<Element> = el
        .borrow()
        .children
        .iter()
        .filter_map(|c| match c {
            crate::svg::Node::Element(e) => Some(e.clone()),
            crate::svg::Node::Text(_) => None,
        })
        .collect();
    for child in children {
        if child.borrow().tag == "path" {
            set_attr(&child, "style", style);
        }
        set_path_styles(&child, style);
    }
}

/// rough.js fill+stroke path pair for a precomputed path `d`.
fn rough_path_pair(parent: &Element, d: &str, fill: &str, stroke: &str, stroke_width: &str) {
    let fill_path = append(parent, "path");
    set_attr(&fill_path, "d", d);
    set_attr(&fill_path, "stroke", "none");
    set_attr(&fill_path, "stroke-width", "0");
    set_attr(&fill_path, "fill", fill);
    let stroke_path = append(parent, "path");
    set_attr(&stroke_path, "d", d);
    set_attr(&stroke_path, "stroke", stroke);
    set_attr(&stroke_path, "stroke-width", stroke_width);
    set_attr(&stroke_path, "fill", "none");
    set_attr(&stroke_path, "stroke-dasharray", "0 0");
}

/// rough.js ellipse path at roughness 0 (`_computeEllipsePoints` coreOnly +
/// `_curve` with curveTightness 0), serialized via `opsToPath`.
fn rough_ellipse_d(rx: f64, ry: f64) -> String {
    let psq = (std::f64::consts::TAU * f64::midpoint(rx * rx, ry * ry).sqrt()).sqrt();
    let step_count = f64::max(9.0, 9.0 / 200.0_f64.sqrt() * psq).ceil();
    let increment = std::f64::consts::TAU / step_count / 4.0;

    // Chrome's V8 ships correctly-rounded sin/cos (CORE-MATH); system libm
    // and fdlibm differ in the last ulp at some angles.
    let at = |angle: f64| (rx * core_math::cos(angle), ry * core_math::sin(angle));
    let mut pts: Vec<(f64, f64)> = vec![at(-increment)];
    let mut angle = 0.0_f64;
    while angle <= std::f64::consts::TAU {
        pts.push(at(angle));
        angle += increment;
    }
    pts.push(at(0.0));
    pts.push(at(increment));

    let mut d = format!("M{} {} ", js_num(pts[1].0), js_num(pts[1].1));
    for i in 1..pts.len() - 2 {
        let b1 = (
            pts[i].0 + (pts[i + 1].0 - pts[i - 1].0) / 6.0,
            pts[i].1 + (pts[i + 1].1 - pts[i - 1].1) / 6.0,
        );
        let b2 = (
            pts[i + 1].0 + (pts[i].0 - pts[i + 2].0) / 6.0,
            pts[i + 1].1 + (pts[i].1 - pts[i + 2].1) / 6.0,
        );
        let _ = std::fmt::Write::write_fmt(
            &mut d,
            format_args!(
                "C{} {}, {} {}, {} {} ",
                js_num(b1.0),
                js_num(b1.1),
                js_num(b2.0),
                js_num(b2.1),
                js_num(pts[i + 1].0),
                js_num(pts[i + 1].1)
            ),
        );
    }
    d.trim_end().to_owned()
}

/// rough.js `rc.rectangle` at roughness 0: straight-line solid fill plus the
/// double-stroked outline (collinear random control points → diverge 0.3).
fn rough_rectangle(
    parent: &Element,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: &str,
    stroke: &str,
) -> Element {
    let g = insert_first(parent, "g");
    let corners = [
        Point { x, y },
        Point { x: x + w, y },
        Point { x: x + w, y: y + h },
        Point { x, y: y + h },
    ];
    // solidFillPolygon: M + L over the corner list (not closed).
    let mut fill_d = format!("M{}", rough_pair(corners[0]));
    for p in &corners[1..] {
        let _ = std::fmt::Write::write_fmt(&mut fill_d, format_args!(" L{}", rough_pair(*p)));
    }
    let fill_path = append(&g, "path");
    set_attr(&fill_path, "d", fill_d);
    set_attr(&fill_path, "stroke", "none");
    set_attr(&fill_path, "stroke-width", "0");
    set_attr(&fill_path, "fill", fill);

    let mut closed: Vec<Point> = corners.to_vec();
    closed.push(corners[0]);
    let mut stroke_d = String::new();
    for seg in closed.windows(2) {
        for diverge in [0.3, 0.3] {
            if !stroke_d.is_empty() {
                stroke_d.push(' ');
            }
            let _ =
                std::fmt::Write::write_fmt(&mut stroke_d, format_args!("M{}", rough_pair(seg[0])));
            stroke_d.push(' ');
            stroke_d.push_str(&rough_line_curve(seg[0], seg[1], diverge));
        }
    }
    let stroke_path = append(&g, "path");
    set_attr(&stroke_path, "d", stroke_d);
    set_attr(&stroke_path, "stroke", stroke);
    set_attr(&stroke_path, "stroke-width", "1.3");
    set_attr(&stroke_path, "fill", "none");
    set_attr(&stroke_path, "stroke-dasharray", "0 0");
    g
}

/// Theme colors applied as rough.js path attributes (default theme).
const MAIN_BKG: &str = "#ECECFF";
const NODE_BORDER: &str = "#9370DB";

/// Stroke width for hand-drawn (`look: handDrawn`) shape outlines.
const HD_STROKE_WIDTH: &str = "1.5";

/// Fill/stroke colors for hand-drawn shapes, from the computed theme with the
/// default-theme values as fallback.
fn hd_theme_colors(config: &super::config::RenderConfig) -> (String, String) {
    let theme = &config.computed_theme;
    let pick = |key: &str, default: &str| {
        let v = super::themes::get(theme, key);
        if v.is_empty() { default.to_owned() } else { v }
    };
    (pick("mainBkg", MAIN_BKG), pick("nodeBorder", NODE_BORDER))
}

/// Formats a rough.js coordinate pair.
fn rough_pair(p: Point) -> String {
    format!("{} {}", js_num(p.x), js_num(p.y))
}

/// A rough.js cubic for a straight segment with collinear control points.
/// rough places them at divergePoint and 2*divergePoint along the segment;
/// the value is random in [0.2, 0.4] but collinear, so geometry is identical.
fn rough_line_curve(a: Point, b: Point, diverge: f64) -> String {
    let lerp = |t: f64| Point {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
    };
    format!(
        "C{}, {}, {}",
        rough_pair(lerp(diverge)),
        rough_pair(lerp(2.0 * diverge)),
        rough_pair(b)
    )
}

/// Emits the rough.js solid-fill + stroke path pair for a closed polyline.
fn rough_polygon(parent: &Element, points: &[Point], styles: &str) -> Element {
    let g = insert_first(parent, "g");

    let mut closed: Vec<Point> = points.to_vec();
    closed.push(points[0]);

    // Fill path: one pass over the closed polygon.
    let mut fill_d = format!("M{}", rough_pair(closed[0]));
    for w in closed.windows(2) {
        fill_d.push(' ');
        fill_d.push_str(&rough_line_curve(w[0], w[1], 0.3));
    }
    let fill = append(&g, "path");
    set_attr(&fill, "d", fill_d);
    set_attr(&fill, "stroke", "none");
    set_attr(&fill, "stroke-width", "0");
    set_attr(&fill, "fill", MAIN_BKG);
    set_attr(&fill, "style", styles);

    // Stroke path: each segment drawn twice (rough's double line).
    let mut stroke_d = String::new();
    for w in closed.windows(2) {
        for diverge in [0.3, 0.3] {
            if !stroke_d.is_empty() {
                stroke_d.push(' ');
            }
            let _ =
                std::fmt::Write::write_fmt(&mut stroke_d, format_args!("M{}", rough_pair(w[0])));
            stroke_d.push(' ');
            stroke_d.push_str(&rough_line_curve(w[0], w[1], diverge));
        }
    }
    let stroke = append(&g, "path");
    set_attr(&stroke, "d", stroke_d);
    set_attr(&stroke, "stroke", NODE_BORDER);
    set_attr(&stroke, "stroke-width", "1.3");
    set_attr(&stroke, "fill", "none");
    set_attr(&stroke, "stroke-dasharray", "0 0");
    set_attr(&stroke, "style", styles);

    g
}

fn generate_circle_points(
    center_x: f64,
    center_y: f64,
    radius: f64,
    num_points: usize,
    start_angle: f64,
    end_angle: f64,
) -> Vec<Point> {
    let start = start_angle.to_radians();
    let end = end_angle.to_radians();
    #[allow(clippy::cast_precision_loss)]
    let step = (end - start) / (num_points as f64 - 1.0);
    (0..num_points)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let angle = start + i as f64 * step;
            Point {
                x: -(center_x + radius * angle.cos()),
                y: -(center_y + radius * angle.sin()),
            }
        })
        .collect()
}

/// `classBox` shape (class diagrams): compartments via `textHelper` plus a
/// rough rectangle and divider lines.
#[allow(clippy::too_many_lines)]
fn class_box(
    parent: &Element,
    node: &NodeRef,
    measurer: &TextMeasurer,
    config: &super::config::RenderConfig,
) -> Element {
    const PADDING: f64 = 12.0;
    const GAP: f64 = 12.0;
    let seq = crate::text::SeqMeasurer::new();
    let n = node.borrow();

    let shape_svg = append(parent, "g");
    set_attr(&shape_svg, "class", get_node_classes(node));
    set_attr(&shape_svg, "id", n.dom_id.clone());

    // addText: label g + foreignObject; returns (width, height).
    // `measure_text` is ClassMember.text (escaped visibility) — the
    // wrap-width measurement sees the backslash, the display does not.
    let add_text = |group: &Element,
                    text: &str,
                    measure_text: &str,
                    style: &str,
                    y_offset: f64,
                    bold: bool|
     -> (f64, f64) {
        let label_g = append(group, "g");
        set_attr(&label_g, "class", "label");
        set_attr(&label_g, "style", style);
        let (times_w, _) = seq.text_dimensions(measure_text, 16.0);
        let wrap_width = times_w + 50.0;
        let bbox = measure_label_sized_styled(measurer, text, wrap_width, 16.0, bold);
        build_html_label_classed(
            &label_g,
            text,
            bbox,
            "nodeLabel markdown-node-label",
            false,
            wrap_width,
            "",
        );
        #[allow(clippy::cast_precision_loss)]
        let lines = split_lines(text).len() as f64;
        set_attr(
            &label_g,
            "transform",
            format!(
                "translate(0,{})",
                js_num(-bbox.height / (2.0 * lines) + y_offset)
            ),
        );
        (bbox.width, bbox.height)
    };

    // Groups in textHelper order.
    let annotation_group = append(&shape_svg, "g");
    set_attr(&annotation_group, "class", "annotation-group text");
    let mut annotation_box = (0.0f64, 0.0f64); // (width, height)
    if let Some(a) = n.class_annotations.first() {
        let t = format!("\u{ab}{a}\u{bb}");
        let (w, h) = add_text(&annotation_group, &t, &t, "", 0.0, false);
        annotation_box = (w, h);
    }
    let annotation_h = annotation_box.1;

    let label_group = append(&shape_svg, "g");
    set_attr(&label_group, "class", "label-group text");
    let (label_w, label_h) = add_text(
        &label_group,
        &n.label,
        &n.label,
        "font-weight: bolder",
        0.0,
        true,
    );

    let members_group = append(&shape_svg, "g");
    set_attr(&members_group, "class", "members-group text");
    let mut y_offset = 0.0;
    let mut members_min = f64::INFINITY;
    let mut members_max = f64::NEG_INFINITY;
    let mut members_w = 0.0f64;
    for (text, style) in &n.class_members {
        let measure = escape_member_text(text);
        let (w, h) = add_text(&members_group, text, &measure, style, y_offset, false);
        members_min = members_min.min(y_offset - h / 2.0);
        members_max = members_max.max(y_offset - h / 2.0 + h);
        members_w = members_w.max(w);
        y_offset += h;
    }
    let mut members_h = if members_min.is_finite() {
        members_max - members_min
    } else {
        0.0
    };
    if members_h <= 0.0 {
        members_h = GAP / 2.0;
    }

    let methods_group = append(&shape_svg, "g");
    set_attr(&methods_group, "class", "methods-group text");
    let mut m_offset = 0.0;
    let mut methods_min = f64::INFINITY;
    let mut methods_max = f64::NEG_INFINITY;
    let mut methods_w = 0.0f64;
    for (text, style) in &n.class_methods {
        let measure = escape_member_text(text);
        let (w, h) = add_text(&methods_group, text, &measure, style, m_offset, false);
        methods_min = methods_min.min(m_offset - h / 2.0);
        methods_max = methods_max.max(m_offset - h / 2.0 + h);
        methods_w = methods_w.max(w);
        m_offset += h;
    }
    let _methods_h = if methods_min.is_finite() {
        methods_max - methods_min
    } else {
        0.0
    };

    // Group transforms (textHelper).
    if annotation_h > 0.0 {
        set_attr(
            &annotation_group,
            "transform",
            format!("translate({})", js_num(-annotation_box.0 / 2.0)),
        );
    }
    set_attr(
        &label_group,
        "transform",
        format!(
            "translate({}, {})",
            js_num(-label_w / 2.0),
            js_num(annotation_h)
        ),
    );
    let members_ty = annotation_h + label_h + GAP * 2.0;
    set_attr(
        &members_group,
        "transform",
        format!("translate({}, {})", js_num(0.0), js_num(members_ty)),
    );
    let methods_ty = annotation_h
        + label_h
        + if members_h > 0.0 {
            members_h + GAP * 4.0
        } else {
            GAP * 2.0
        };
    set_attr(
        &methods_group,
        "transform",
        format!("translate({}, {})", js_num(0.0), js_num(methods_ty)),
    );

    // shapeSvg bbox (over the label groups at their current transforms).
    let mut bb_min_x = f64::INFINITY;
    let mut bb_max_x = f64::NEG_INFINITY;
    let mut bb_min_y = f64::INFINITY;
    let mut bb_max_y = f64::NEG_INFINITY;
    let mut add_box = |x0: f64, y0: f64, x1: f64, y1: f64| {
        bb_min_x = bb_min_x.min(x0);
        bb_max_x = bb_max_x.max(x1);
        bb_min_y = bb_min_y.min(y0);
        bb_max_y = bb_max_y.max(y1);
    };
    if annotation_h > 0.0 {
        add_box(
            -annotation_box.0 / 2.0,
            -annotation_box.1 / 2.0,
            annotation_box.0 / 2.0,
            annotation_box.1 / 2.0,
        );
    }
    add_box(
        -label_w / 2.0,
        annotation_h - label_h / 2.0,
        label_w / 2.0,
        annotation_h + label_h / 2.0,
    );
    if members_min.is_finite() {
        add_box(
            0.0,
            members_ty + members_min,
            members_w,
            members_ty + members_max,
        );
    }
    if methods_min.is_finite() {
        add_box(
            0.0,
            methods_ty + methods_min,
            methods_w,
            methods_ty + methods_max,
        );
    }
    let bbox_w = if bb_min_x.is_finite() {
        bb_max_x - bb_min_x
    } else {
        0.0
    };
    let bbox_h = if bb_min_y.is_finite() {
        bb_max_y - bb_min_y
    } else {
        0.0
    };

    let has_members = !n.class_members.is_empty();
    let has_methods = !n.class_methods.is_empty();
    let render_extra_box = !has_members && !has_methods;
    let w = bbox_w.max(0.0);
    let mut h = bbox_h.max(0.0);
    if !has_members && !has_methods {
        h += GAP;
    } else if has_members && !has_methods {
        h += GAP * 2.0;
    }
    let x = -w / 2.0;
    let y = -h / 2.0;
    let extra_height = if render_extra_box {
        PADDING * 2.0
    } else if !has_members && !has_methods {
        -PADDING
    } else {
        0.0
    };
    let rect_y_adjust = if render_extra_box {
        PADDING
    } else if !has_members && !has_methods {
        -PADDING / 2.0
    } else {
        0.0
    };

    let theme = &config.computed_theme;
    let main_bkg = super::themes::get(theme, "mainBkg");
    let node_border = super::themes::get(theme, "nodeBorder");
    let rect = rough_rectangle(
        &shape_svg,
        x - PADDING,
        y - PADDING - rect_y_adjust,
        w + 2.0 * PADDING,
        h + 2.0 * PADDING + extra_height,
        &main_bkg,
        &node_border,
    );
    set_attr(&rect, "class", "basic label-container outer-path");
    let rect_x = x - PADDING;
    let rect_w = w + 2.0 * PADDING;
    let rect_h = h + 2.0 * PADDING + extra_height;

    // .text adjustment loop, using renderExtraBox-adjusted group heights.
    let box_adj = if render_extra_box { PADDING / 2.0 } else { 0.0 };
    let annotation_adj = if annotation_h == 0.0 && box_adj == 0.0 {
        0.0
    } else {
        annotation_h - box_adj
    };
    let label_adj = label_h - box_adj;
    let raw_members_bbox = if members_min.is_finite() {
        members_max - members_min
    } else {
        0.0
    };
    let members_adj = if raw_members_bbox == 0.0 && box_adj == 0.0 {
        0.0
    } else {
        raw_members_bbox - box_adj
    };
    let adjust =
        |group: &Element, orig_ty: f64, is_label_like: bool, group_w: f64, is_methods: bool| {
            let mut ty = orig_ty + y + PADDING - rect_y_adjust;
            if is_methods {
                let members_for_methods = members_adj.max(GAP / 2.0);
                ty = annotation_adj + label_adj + members_for_methods + y + GAP * 4.0 + PADDING;
            }
            let tx = if is_label_like { -group_w / 2.0 } else { x };
            set_attr(
                group,
                "transform",
                format!("translate({}, {})", js_num(tx), js_num(ty)),
            );
        };
    adjust(&annotation_group, 0.0, true, annotation_box.0, false);
    adjust(&label_group, annotation_h, true, label_w, false);
    adjust(&members_group, members_ty, false, members_w, false);
    adjust(&methods_group, methods_ty, false, methods_w, true);

    // Divider lines.
    if has_members || has_methods || render_extra_box {
        let first_line_y = annotation_adj + label_adj + y + PADDING;
        let g = rough_line_g(
            &shape_svg,
            rect_x,
            first_line_y,
            rect_x + rect_w,
            first_line_y + 0.001,
            &node_border,
        );
        set_attr(&g, "class", "divider");
        set_attr(&g, "style", "");
    }
    if render_extra_box || has_members || has_methods {
        let second_line_y = annotation_adj + label_adj + members_adj + y + GAP * 2.0 + PADDING;
        let g = rough_line_g(
            &shape_svg,
            rect_x,
            second_line_y,
            rect_x + rect_w,
            second_line_y + 0.001,
            &node_border,
        );
        set_attr(&g, "class", "divider");
        set_attr(&g, "style", "");
    }

    // Style application: every path style "", spans style "".
    set_path_styles(&shape_svg, "");
    set_span_styles(&shape_svg, "");

    drop(n);
    let mut nm = node.borrow_mut();
    nm.width = f32q(rect_w);
    nm.height = f32q(rect_h);
    // The rect is vertically offset; updateNodeBounds centers on the bbox.
    nm.offset_y = 0.0;
    nm.intersect = Some(IntersectShape::Rect);
    let _ = rect_y_adjust;
    shape_svg
}

/// `ClassMember.text`: the visibility prefix is backslash-escaped.
fn escape_member_text(display: &str) -> String {
    match display.chars().next() {
        Some(c @ ('+' | '-' | '#' | '~')) => format!("\\{c}{}", &display[c.len_utf8()..]),
        _ => display.to_owned(),
    }
}

/// rough.js `rc.line` (two random-control curve passes).
fn rough_line_g(parent: &Element, x1: f64, y1: f64, x2: f64, y2: f64, stroke: &str) -> Element {
    let g = append(parent, "g");
    let a = Point { x: x1, y: y1 };
    let b = Point { x: x2, y: y2 };
    let mut d = String::new();
    for diverge in [0.3, 0.3] {
        if !d.is_empty() {
            d.push(' ');
        }
        let _ = std::fmt::Write::write_fmt(&mut d, format_args!("M{}", rough_pair(a)));
        d.push(' ');
        d.push_str(&rough_line_curve(a, b, diverge));
    }
    let path = append(&g, "path");
    set_attr(&path, "d", d);
    set_attr(&path, "stroke", stroke);
    set_attr(&path, "stroke-width", "1.3");
    set_attr(&path, "fill", "none");
    set_attr(&path, "stroke-dasharray", "0 0");
    g
}

/// Sets `style` on every descendant `span` (d3 `selectAll('span')`).
fn set_span_styles(el: &Element, style: &str) {
    let children: Vec<Element> = el
        .borrow()
        .children
        .iter()
        .filter_map(|c| match c {
            crate::svg::Node::Element(e) => Some(e.clone()),
            crate::svg::Node::Text(_) => None,
        })
        .collect();
    for child in children {
        if child.borrow().tag == "span" {
            set_attr(&child, "style", style);
        }
        set_span_styles(&child, style);
    }
}

/// `measure_label_sized` with an optional bold face for the title.
fn measure_label_sized_styled(
    measurer: &TextMeasurer,
    label: &str,
    wrap_width: f64,
    font_size: f64,
    bold: bool,
) -> BBox {
    if bold {
        measurer.set_bold(true);
        let b = measure_label_sized(measurer, label, wrap_width, font_size);
        measurer.set_bold(false);
        b
    } else {
        measure_label_sized(measurer, label, wrap_width, font_size)
    }
}
