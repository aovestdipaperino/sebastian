//! Port of `edges.js`, `lineWithOffset.ts`, d3-shape's `curveBasis`, and the
//! label-position helpers from `utils.ts`.

use crate::dagre::types::Point;
use crate::svg::{Element, append, d3_round, js_num, set_attr};
use crate::text::TextMeasurer;

use super::data::{EdgeRef, IntersectShape, NodeRef, RenderNode};

/// `markerOffsets` from lineWithOffset.ts (flowchart-relevant entries).
fn marker_offset(arrow_type: &str) -> Option<f64> {
    match arrow_type {
        "arrow_point" => Some(4.0),
        "arrow_barb" => Some(0.0),
        "aggregation" | "extension" | "composition" => Some(17.25),
        "dependency" => Some(6.0),
        "lollipop" => Some(13.5),
        _ => None,
    }
}

/// Builds the edge label group; sets `edge.width/height`. Port of
/// `insertEdgeLabel` (htmlLabels path, no terminal labels for flowcharts).
/// Returns the outer `g.edgeLabel` element for later positioning.
/// Creates the cardinality terminal label group (`edgeTerminals`).
/// `attach_to_inner` is false for endLabelLeft (an upstream quirk leaves
/// the label outside the inner group).
pub fn insert_terminal_label(
    elem: &Element,
    text: &str,
    measurer: &TextMeasurer,
    attach_to_inner: bool,
) -> Element {
    let terminals = append(elem, "g");
    set_attr(&terminals, "class", "edgeTerminals");
    let inner = append(&terminals, "g");
    set_attr(&inner, "class", "inner");
    let parent = if attach_to_inner { &inner } else { &terminals };
    // createLabel: fo > div (no max-width) > span.edgeLabel > p, at the
    // 11px font from the .edgeTerminals rule.
    let font_size = 11.0;
    let bbox = super::shapes::measure_label_sized(measurer, text, f64::INFINITY, font_size);
    let fo = append(parent, "foreignObject");
    set_attr(&fo, "width", js_num(bbox.width));
    set_attr(&fo, "height", js_num(bbox.height));
    #[allow(clippy::cast_precision_loss)]
    let fo_w = text.chars().count() as f64 * 9.0;
    set_attr(
        &fo,
        "style",
        format!("width: {}px; height: 12px;", js_num(fo_w)),
    );
    let div = crate::svg::append_xhtml(&fo, "div");
    set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
    set_attr(
        &div,
        "style",
        "display: table-cell; white-space: nowrap; line-height: 1.5;",
    );
    let span = crate::svg::append_xhtml(&div, "span");
    set_attr(&span, "class", "edgeLabel");
    super::shapes::write_label_paragraph(&span, text);
    set_attr(
        &inner,
        "transform",
        format!(
            "translate({}, {})",
            js_num(-bbox.width / 2.0),
            js_num(-bbox.height / 2.0)
        ),
    );
    terminals
}

pub fn insert_edge_label(
    elem: &Element,
    edge: &EdgeRef,
    measurer: &TextMeasurer,
    config: &super::config::RenderConfig,
) -> Element {
    let mut e = edge.borrow_mut();

    let edge_label = append(elem, "g");
    set_attr(&edge_label, "class", "edgeLabel");
    let label = append(&edge_label, "g");
    set_attr(&label, "class", "label");
    set_attr(&label, "data-id", e.id.clone());

    let compiled = super::styles::styles2string(&e.css_compiled_styles, &[], &e.label_style);
    let label_style = compiled.label_styles.replacen("fill:", "color:", 1);
    let font_size = config.edge_label_font_size.unwrap_or_else(|| {
        super::shapes::font_size_from_styles_or(&label_style, config.font_size())
    });

    if !config.effective_html_labels() {
        // SVG text labels: createText appends the label group to the edge
        // labels container, then the label element is moved into `label`.
        let ft = super::svg_label::create_formatted_text(
            elem,
            &e.label_raw,
            measurer,
            font_size,
            config.wrapping_width,
            true,
            true,
        );
        crate::svg::move_element(&ft.label_element, &label);
        // createText isNode=false style transforms: the rect keeps non-stroke/
        // fill declarations (background -> fill), the text maps color -> fill.
        if ft.has_background {
            let strip = |s: &str| {
                let s = super::shapes::remove_style_decl(s, "stroke:");
                let s = super::shapes::remove_style_decl(&s, "stroke-width:");
                super::shapes::remove_style_decl(&s, "fill:")
            };
            let rect_style = strip(&label_style).replace("background:", "fill:");
            let text_style = strip(&label_style).replace("color:", "fill:");
            set_attr(
                &ft.label_group
                    .borrow()
                    .children
                    .iter()
                    .find_map(|c| match c {
                        crate::svg::Node::Element(el) if el.borrow().tag == "rect" => {
                            Some(el.clone())
                        }
                        _ => None,
                    })
                    .expect("background rect"),
                "style",
                &rect_style,
            );
            set_attr(&ft.text_element, "style", &text_style);
        }
        set_attr(
            &label,
            "transform",
            format!(
                "translate({}, {})",
                js_num(-(ft.text_bbox.x + ft.text_bbox.width / 2.0)),
                js_num(-(ft.text_bbox.y + ft.text_bbox.height / 2.0))
            ),
        );
        e.width = ft.label_bbox.width;
        e.height = ft.label_bbox.height;
        e.x = None;
        e.y = None;
        return edge_label;
    }

    let bbox =
        super::shapes::measure_label_sized(measurer, &e.label, config.wrapping_width, font_size);
    // createText with isNode=false, addSvgBackground=true
    let fo = append(&label, "foreignObject");
    set_attr(&fo, "width", js_num(bbox.width));
    set_attr(&fo, "height", js_num(bbox.height));
    let div = crate::svg::append_xhtml(&fo, "div");
    if label_style.is_empty() {
        set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
        set_attr(&div, "class", "labelBkg");
        set_attr(&div, "style", super::shapes::div_style(bbox.wrapped));
    } else {
        set_attr(
            &div,
            "style",
            super::styles::div_style_attr(&label_style, &super::shapes::div_style(bbox.wrapped)),
        );
        set_attr(&div, "xmlns", "http://www.w3.org/1999/xhtml");
        set_attr(&div, "class", "labelBkg");
    }
    let span = crate::svg::append_xhtml(&div, "span");
    if !label_style.is_empty() {
        set_attr(&span, "style", label_style.clone());
    }
    set_attr(&span, "class", "edgeLabel");
    super::shapes::write_label_paragraph(&span, &e.label);

    set_attr(
        &label,
        "transform",
        format!(
            "translate({}, {})",
            js_num(-bbox.width / 2.0),
            js_num(-bbox.height / 2.0)
        ),
    );

    e.width = bbox.width;
    e.height = bbox.height;
    e.x = None;
    e.y = None;
    edge_label
}

/// `node.intersect(point)` from the shape handlers.
#[must_use]
pub fn node_intersect(node: &RenderNode, point: Point) -> Point {
    match node.intersect.as_ref().expect("intersect set") {
        IntersectShape::Rect => intersect_rect(node, point),
        IntersectShape::Polygon(points) => intersect_polygon(node, points, point),
        IntersectShape::Question(points) => {
            let res = intersect_polygon(node, points, point);
            Point {
                x: res.x - 0.5,
                y: res.y - 0.5,
            }
        }
        IntersectShape::Circle { radius } => intersect_ellipse(node, *radius, *radius, point),
        IntersectShape::Cylinder { rx, ry } => {
            let mut pos = intersect_rect(node, point);
            let x = pos.x - node.x;
            if *rx != 0.0
                && (x.abs() < node.width / 2.0
                    || (x.abs() == node.width / 2.0
                        && (pos.y - node.y).abs() > node.height / 2.0 - ry))
            {
                let mut y = ry * ry * (1.0 - (x * x) / (rx * rx));
                if y > 0.0 {
                    y = y.sqrt();
                }
                y = ry - y;
                if point.y - node.y > 0.0 {
                    y = -y;
                }
                pos.y += y;
            }
            pos
        }
    }
}

fn intersect_rect(node: &RenderNode, point: Point) -> Point {
    let x = node.x;
    let y = node.y;
    let dx = point.x - x;
    let dy = point.y - y;
    let mut w = node.width / 2.0;
    let mut h = node.height / 2.0;
    let (sx, sy);
    if dy.abs() * w > dx.abs() * h {
        if dy < 0.0 {
            h = -h;
        }
        sx = if dy == 0.0 { 0.0 } else { h * dx / dy };
        sy = h;
    } else {
        if dx < 0.0 {
            w = -w;
        }
        sx = w;
        sy = if dx == 0.0 { 0.0 } else { w * dy / dx };
    }
    Point {
        x: x + sx,
        y: y + sy,
    }
}

fn intersect_line(p1: Point, p2: Point, q1: Point, q2: Point) -> Option<Point> {
    let a1 = p2.y - p1.y;
    let b1 = p1.x - p2.x;
    let c1 = p2.x * p1.y - p1.x * p2.y;

    let r3 = a1 * q1.x + b1 * q1.y + c1;
    let r4 = a1 * q2.x + b1 * q2.y + c1;

    let epsilon = 1e-6;
    if r3 != 0.0 && r4 != 0.0 && same_sign(r3, r4) {
        return None;
    }

    let a2 = q2.y - q1.y;
    let b2 = q1.x - q2.x;
    let c2 = q2.x * q1.y - q1.x * q2.y;

    let r1 = a2 * p1.x + b2 * p1.y + c2;
    let r2 = a2 * p2.x + b2 * p2.y + c2;
    if r1.abs() < epsilon && r2.abs() < epsilon && same_sign(r1, r2) {
        return None;
    }

    let denom = a1 * b2 - a2 * b1;
    if denom == 0.0 {
        return None;
    }
    let offset = (denom / 2.0).abs();

    let num = b1 * c2 - b2 * c1;
    let x = if num < 0.0 {
        (num - offset) / denom
    } else {
        (num + offset) / denom
    };
    let num = a2 * c1 - a1 * c2;
    let y = if num < 0.0 {
        (num - offset) / denom
    } else {
        (num + offset) / denom
    };
    Some(Point { x, y })
}

fn same_sign(r1: f64, r2: f64) -> bool {
    r1 * r2 > 0.0
}

fn intersect_polygon(node: &RenderNode, poly_points: &[Point], point: Point) -> Point {
    let x1 = node.x;
    let y1 = node.y;
    let mut intersections = Vec::new();

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    for entry in poly_points {
        min_x = min_x.min(entry.x);
        min_y = min_y.min(entry.y);
    }

    let left = x1 - node.width / 2.0 - min_x;
    let top = y1 - node.height / 2.0 - min_y;

    for i in 0..poly_points.len() {
        let p1 = poly_points[i];
        let p2 = poly_points[if i < poly_points.len() - 1 { i + 1 } else { 0 }];
        if let Some(intersect) = intersect_line(
            Point { x: x1, y: y1 },
            point,
            Point {
                x: left + p1.x,
                y: top + p1.y,
            },
            Point {
                x: left + p2.x,
                y: top + p2.y,
            },
        ) {
            intersections.push(intersect);
        }
    }

    if intersections.is_empty() {
        return Point { x: x1, y: y1 };
    }

    if intersections.len() > 1 {
        intersections.sort_by(|p, q| {
            let pdx = p.x - point.x;
            let pdy = p.y - point.y;
            let distp = (pdx * pdx + pdy * pdy).sqrt();
            let qdx = q.x - point.x;
            let qdy = q.y - point.y;
            let distq = (qdx * qdx + qdy * qdy).sqrt();
            distp.partial_cmp(&distq).expect("non-NaN distance")
        });
    }
    intersections[0]
}

fn intersect_ellipse(node: &RenderNode, rx: f64, ry: f64, point: Point) -> Point {
    let cx = node.x;
    let cy = node.y;
    let px = cx - point.x;
    let py = cy - point.y;
    let det = (rx * rx * py * py + ry * ry * px * px).sqrt();
    let mut dx = (rx * ry * px / det).abs();
    if point.x < cx {
        dx = -dx;
    }
    let mut dy = (rx * ry * py / det).abs();
    if point.y < cy {
        dy = -dy;
    }
    Point {
        x: cx + dx,
        y: cy + dy,
    }
}

const fn outside_node(node: &RenderNode, point: Point) -> bool {
    let dx = (point.x - node.x).abs();
    let dy = (point.y - node.y).abs();
    dx >= node.width / 2.0 || dy >= node.height / 2.0
}

/// Port of `intersection` (cluster boundary intersection).
fn intersection(node: &RenderNode, outside_point: Point, inside_point: Point) -> Point {
    let x = node.x;
    let y = node.y;
    let w = node.width / 2.0;
    let h = node.height / 2.0;
    // (JS computes an initial `r` from the inside-point dx; it is always
    // overwritten before use.)
    let r;

    let big_q = (outside_point.y - inside_point.y).abs();
    let big_r = (outside_point.x - inside_point.x).abs();

    if (y - outside_point.y).abs() * w > (x - outside_point.x).abs() * h {
        // Intersection is top or bottom of rect.
        let q = if inside_point.y < outside_point.y {
            outside_point.y - h - y
        } else {
            y - h - outside_point.y
        };
        r = big_r * q / big_q;
        let mut res = Point {
            x: if inside_point.x < outside_point.x {
                inside_point.x + r
            } else {
                inside_point.x - big_r + r
            },
            y: if inside_point.y < outside_point.y {
                inside_point.y + big_q - q
            } else {
                inside_point.y - big_q + q
            },
        };
        if r == 0.0 {
            res.x = outside_point.x;
            res.y = outside_point.y;
        }
        if big_r == 0.0 {
            res.x = outside_point.x;
        }
        if big_q == 0.0 {
            res.y = outside_point.y;
        }
        res
    } else {
        // Intersection on sides of rect.
        r = if inside_point.x < outside_point.x {
            outside_point.x - w - x
        } else {
            x - w - outside_point.x
        };
        let q = big_q * r / big_r;
        let mut res_x = if inside_point.x < outside_point.x {
            inside_point.x + big_r - r
        } else {
            inside_point.x - big_r + r
        };
        let mut res_y = if inside_point.y < outside_point.y {
            inside_point.y + q
        } else {
            inside_point.y - q
        };
        if r == 0.0 {
            res_x = outside_point.x;
            res_y = outside_point.y;
        }
        if big_r == 0.0 {
            res_x = outside_point.x;
        }
        if big_q == 0.0 {
            res_y = outside_point.y;
        }
        Point { x: res_x, y: res_y }
    }
}

fn cut_path_at_intersect(points: &[Point], boundary_node: &RenderNode) -> Vec<Point> {
    let mut result: Vec<Point> = Vec::new();
    let mut last_point_outside = points[0];
    let mut is_inside = false;
    for &point in points {
        if !outside_node(boundary_node, point) && !is_inside {
            let inter = intersection(boundary_node, last_point_outside, point);
            if !result.iter().any(|e| e.x == inter.x && e.y == inter.y) {
                result.push(inter);
            }
            is_inside = true;
        } else {
            last_point_outside = point;
            if !is_inside {
                result.push(point);
            }
        }
    }
    result
}

fn extract_corner_point_positions(points: &[Point]) -> Vec<usize> {
    let mut positions = Vec::new();
    for i in 1..points.len().saturating_sub(1) {
        let prev = points[i - 1];
        let curr = points[i];
        let next = points[i + 1];
        let vertical_then_horizontal = prev.x == curr.x
            && curr.y == next.y
            && (curr.x - next.x).abs() > 5.0
            && (curr.y - prev.y).abs() > 5.0;
        let horizontal_then_vertical = prev.y == curr.y
            && curr.x == next.x
            && (curr.x - prev.x).abs() > 5.0
            && (curr.y - next.y).abs() > 5.0;
        if vertical_then_horizontal || horizontal_then_vertical {
            positions.push(i);
        }
    }
    positions
}

fn find_adjacent_point(point_a: Point, point_b: Point, distance: f64) -> Point {
    let x_diff = point_b.x - point_a.x;
    let y_diff = point_b.y - point_a.y;
    let length = (x_diff * x_diff + y_diff * y_diff).sqrt();
    let ratio = distance / length;
    Point {
        x: point_b.x - ratio * x_diff,
        y: point_b.y - ratio * y_diff,
    }
}

fn fix_corners(line_data: &[Point]) -> Vec<Point> {
    let corner_positions = extract_corner_point_positions(line_data);
    let mut new_line_data = Vec::new();
    for i in 0..line_data.len() {
        if corner_positions.contains(&i) {
            let prev_point = line_data[i - 1];
            let next_point = line_data[i + 1];
            let corner_point = line_data[i];

            let new_prev_point = find_adjacent_point(prev_point, corner_point, 5.0);
            let new_next_point = find_adjacent_point(next_point, corner_point, 5.0);

            let x_diff = new_next_point.x - new_prev_point.x;
            let y_diff = new_next_point.y - new_prev_point.y;
            new_line_data.push(new_prev_point);

            let a = std::f64::consts::SQRT_2 * 2.0;
            let mut new_corner_point = corner_point;
            if (next_point.x - prev_point.x).abs() > 10.0
                && (next_point.y - prev_point.y).abs() >= 10.0
            {
                let r = 5.0;
                if corner_point.x == new_prev_point.x {
                    new_corner_point = Point {
                        x: if x_diff < 0.0 {
                            new_prev_point.x - r + a
                        } else {
                            new_prev_point.x + r - a
                        },
                        y: if y_diff < 0.0 {
                            new_prev_point.y - a
                        } else {
                            new_prev_point.y + a
                        },
                    };
                } else {
                    new_corner_point = Point {
                        x: if x_diff < 0.0 {
                            new_prev_point.x - a
                        } else {
                            new_prev_point.x + a
                        },
                        y: if y_diff < 0.0 {
                            new_prev_point.y - r + a
                        } else {
                            new_prev_point.y + r - a
                        },
                    };
                }
            }
            new_line_data.push(new_corner_point);
            new_line_data.push(new_next_point);
        } else {
            new_line_data.push(line_data[i]);
        }
    }
    new_line_data
}

/// d3 path context that formats numbers with three-decimal rounding
/// (d3-shape `line()` default digits).
#[derive(Debug, Default)]
struct D3Path {
    d: String,
}

impl D3Path {
    fn move_to(&mut self, x: f64, y: f64) {
        use std::fmt::Write;
        let _ = write!(self.d, "M{},{}", js_num(d3_round(x)), js_num(d3_round(y)));
    }

    fn line_to(&mut self, x: f64, y: f64) {
        use std::fmt::Write;
        let _ = write!(self.d, "L{},{}", js_num(d3_round(x)), js_num(d3_round(y)));
    }

    #[allow(clippy::many_single_char_names)]
    fn bezier_curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        use std::fmt::Write;
        let _ = write!(
            self.d,
            "C{},{},{},{},{},{}",
            js_num(d3_round(x1)),
            js_num(d3_round(y1)),
            js_num(d3_round(x2)),
            js_num(d3_round(y2)),
            js_num(d3_round(x)),
            js_num(d3_round(y))
        );
    }
}

/// d3-shape curveBasis writing into a path context.
#[derive(Debug)]
struct BasisCurve<'a> {
    ctx: &'a mut D3Path,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    point_state: u8,
}

impl<'a> BasisCurve<'a> {
    fn new(ctx: &'a mut D3Path) -> Self {
        Self {
            ctx,
            x0: f64::NAN,
            y0: f64::NAN,
            x1: f64::NAN,
            y1: f64::NAN,
            point_state: 0,
        }
    }

    fn basis_point(&mut self, x: f64, y: f64) {
        self.ctx.bezier_curve_to(
            (2.0 * self.x0 + self.x1) / 3.0,
            (2.0 * self.y0 + self.y1) / 3.0,
            (self.x0 + 2.0 * self.x1) / 3.0,
            (self.y0 + 2.0 * self.y1) / 3.0,
            (self.x0 + 4.0 * self.x1 + x) / 6.0,
            (self.y0 + 4.0 * self.y1 + y) / 6.0,
        );
    }

    fn point(&mut self, x: f64, y: f64) {
        match self.point_state {
            0 => {
                self.point_state = 1;
                self.ctx.move_to(x, y);
            }
            1 => {
                self.point_state = 2;
            }
            2 => {
                self.point_state = 3;
                self.ctx.line_to(
                    (5.0 * self.x0 + self.x1) / 6.0,
                    (5.0 * self.y0 + self.y1) / 6.0,
                );
                self.basis_point(x, y);
            }
            _ => {
                self.basis_point(x, y);
            }
        }
        self.x0 = self.x1;
        self.x1 = x;
        self.y0 = self.y1;
        self.y1 = y;
    }

    fn line_end(&mut self) {
        match self.point_state {
            3 => {
                let (x1, y1) = (self.x1, self.y1);
                self.basis_point(x1, y1);
                self.ctx.line_to(self.x1, self.y1);
            }
            2 => {
                self.ctx.line_to(self.x1, self.y1);
            }
            _ => {}
        }
    }
}

/// Linear curve.
fn linear_path(ctx: &mut D3Path, points: &[(f64, f64)]) {
    for (i, &(x, y)) in points.iter().enumerate() {
        if i == 0 {
            ctx.move_to(x, y);
        } else {
            ctx.line_to(x, y);
        }
    }
}

/// Builds a `curveBasis` path string (with marker offsets applied) from the
/// clipped edge points. Shared by the block diagram's legacy edge renderer.
pub(crate) fn basis_edge_path(points: &[Point], arrow_start: &str, arrow_end: &str) -> String {
    let line_data: Vec<Point> = points.iter().copied().filter(|p| !p.y.is_nan()).collect();
    let with_offsets = offset_points(&line_data, arrow_start, arrow_end);
    let mut path = D3Path::default();
    let mut curve = BasisCurve::new(&mut path);
    for &(x, y) in &with_offsets {
        curve.point(x, y);
    }
    curve.line_end();
    path.d
}

/// `getLineFunctionsWithOffset` — x/y accessors with marker offsets.
#[allow(clippy::similar_names)]
fn offset_points(line_data: &[Point], arrow_start: &str, arrow_end: &str) -> Vec<(f64, f64)> {
    let n = line_data.len();
    let mut out = Vec::with_capacity(n);
    let first = line_data[0];
    let last = line_data[n - 1];
    let dir_right = first.x < last.x;
    let dir_down = first.y < last.y;
    let start_marker_height = marker_offset(arrow_start);
    let end_marker_height = marker_offset(arrow_end);
    for (i, d) in line_data.iter().enumerate() {
        // X accessor
        let mut offset_x = 0.0;
        if i == 0
            && let Some(mo) = start_marker_height
        {
            let (angle, delta_x) = delta_angle(line_data.first(), line_data.get(1));
            offset_x = mo * angle.cos() * if delta_x >= 0.0 { 1.0 } else { -1.0 };
        } else if i == n - 1
            && let Some(mo) = end_marker_height
        {
            let (angle, delta_x) =
                delta_angle(line_data.get(n - 1), line_data.get(n.wrapping_sub(2)));
            offset_x = mo * angle.cos() * if delta_x >= 0.0 { 1.0 } else { -1.0 };
        }

        let difference_to_end = (d.x - last.x).abs();
        let difference_in_y_end = (d.y - last.y).abs();
        let difference_to_start = (d.x - first.x).abs();
        let difference_in_y_start = (d.y - first.y).abs();
        let extra_room = 1.0;
        if let Some(end_h) = end_marker_height
            && difference_to_end < end_h
            && difference_to_end > 0.0
            && difference_in_y_end < end_h
        {
            let mut adjustment = end_h + extra_room - difference_to_end;
            adjustment *= if dir_right { -1.0 } else { 1.0 };
            offset_x -= adjustment;
        }
        if let Some(start_h) = start_marker_height
            && difference_to_start < start_h
            && difference_to_start > 0.0
            && difference_in_y_start < start_h
        {
            let mut adjustment = start_h + extra_room - difference_to_start;
            adjustment *= if dir_right { -1.0 } else { 1.0 };
            offset_x += adjustment;
        }

        // Y accessor
        let mut offset_y = 0.0;
        if i == 0
            && let Some(mo) = start_marker_height
        {
            let (angle, delta_y) = delta_angle_y(line_data.first(), line_data.get(1));
            offset_y = mo * angle.sin().abs() * if delta_y >= 0.0 { 1.0 } else { -1.0 };
        } else if i == n - 1
            && let Some(mo) = end_marker_height
        {
            let (angle, delta_y) =
                delta_angle_y(line_data.get(n - 1), line_data.get(n.wrapping_sub(2)));
            offset_y = mo * angle.sin().abs() * if delta_y >= 0.0 { 1.0 } else { -1.0 };
        }

        let difference_to_end_y = (d.y - last.y).abs();
        let difference_in_x_end = (d.x - last.x).abs();
        let difference_to_start_y = (d.y - first.y).abs();
        let difference_in_x_start = (d.x - first.x).abs();
        if let Some(end_h) = end_marker_height
            && difference_to_end_y < end_h
            && difference_to_end_y > 0.0
            && difference_in_x_end < end_h
        {
            let mut adjustment = end_h + extra_room - difference_to_end_y;
            adjustment *= if dir_down { -1.0 } else { 1.0 };
            offset_y -= adjustment;
        }
        if let Some(start_h) = start_marker_height
            && difference_to_start_y < start_h
            && difference_to_start_y > 0.0
            && difference_in_x_start < start_h
        {
            let mut adjustment = start_h + extra_room - difference_to_start_y;
            adjustment *= if dir_down { -1.0 } else { 1.0 };
            offset_y += adjustment;
        }

        out.push((d.x + offset_x, d.y + offset_y));
    }
    out
}

/// `calculateDeltaAndAngle` (lineWithOffset.ts) — note `Math.atan(dy/dx)`.
fn delta_angle(p1: Option<&Point>, p2: Option<&Point>) -> (f64, f64) {
    let (Some(p1), Some(p2)) = (p1, p2) else {
        return (0.0, 0.0);
    };
    let delta_x = p2.x - p1.x;
    let delta_y = p2.y - p1.y;
    ((delta_y / delta_x).atan(), delta_x)
}

fn delta_angle_y(p1: Option<&Point>, p2: Option<&Point>) -> (f64, f64) {
    let (Some(p1), Some(p2)) = (p1, p2) else {
        return (0.0, 0.0);
    };
    let delta_x = p2.x - p1.x;
    let delta_y = p2.y - p1.y;
    ((delta_y / delta_x).atan(), delta_y)
}

impl std::fmt::Debug for InsertedEdge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InsertedEdge")
            .field("d", &self.d)
            .finish_non_exhaustive()
    }
}

pub struct InsertedEdge {
    pub updated_points: Option<Vec<Point>>,
    pub d: String,
}

/// Port of `insertEdge` — computes the path, appends it, adds markers.
#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
pub fn insert_edge(
    elem: &Element,
    edge: &EdgeRef,
    cluster_db: &super::graph::ClusterDb,
    diagram_type: &str,
    start_node: &NodeRef,
    end_node: &NodeRef,
    diagram_id: &str,
    markers: &mut MarkerState,
) -> InsertedEdge {
    let e = edge.borrow();
    let mut points: Vec<Point> = e.points.clone();
    let mut points_has_changed = false;

    // Replace terminal points with shape intersections (skipped when either
    // endpoint has no intersect, e.g. recursive cluster nodes).
    let can_intersect =
        start_node.borrow().intersect.is_some() && end_node.borrow().intersect.is_some();
    if can_intersect {
        let original = &e.points;
        let mut sliced: Vec<Point> = original[1..original.len() - 1].to_vec();
        let tail = start_node.borrow();
        let head = end_node.borrow();
        let first = sliced.first().copied().unwrap_or_else(|| Point {
            x: head.x,
            y: head.y,
        });
        sliced.insert(0, node_intersect(&tail, first));
        let last = sliced[sliced.len() - 1];
        sliced.push(node_intersect(&head, last));
        points = sliced;
    }

    let points_str = base64(&points);

    if let Some(to_cluster) = &e.to_cluster {
        let boundary = cluster_db.map[to_cluster]
            .node
            .clone()
            .expect("cluster node");
        points = cut_path_at_intersect(&e.points, &boundary.borrow());
        points_has_changed = true;
    }
    if let Some(from_cluster) = &e.from_cluster {
        let boundary = cluster_db.map[from_cluster]
            .node
            .clone()
            .expect("cluster node");
        let mut reversed: Vec<Point> = points.clone();
        reversed.reverse();
        let mut cut = cut_path_at_intersect(&reversed, &boundary.borrow());
        cut.reverse();
        points = cut;
        points_has_changed = true;
    }

    let line_data: Vec<Point> = points.iter().copied().filter(|p| !p.y.is_nan()).collect();
    let curve_type = e.curve.as_str();
    let line_data = if curve_type == "rounded" {
        line_data
    } else {
        fix_corners(&line_data)
    };

    let with_offsets = offset_points(&line_data, &e.arrow_type_start, &e.arrow_type_end);
    let mut path = D3Path::default();
    match curve_type {
        "linear" => linear_path(&mut path, &with_offsets),
        "basis" => {
            let mut curve = BasisCurve::new(&mut path);
            for &(x, y) in &with_offsets {
                curve.point(x, y);
            }
            curve.line_end();
        }
        _ => {
            // Other curve families fall back to basis (the flowchart default).
            let mut curve = BasisCurve::new(&mut path);
            for &(x, y) in &with_offsets {
                curve.point(x, y);
            }
            curve.line_end();
        }
    }
    let line_path = if e.look == "handDrawn" && with_offsets.len() > 1 {
        let pts: Vec<Point> = with_offsets.iter().map(|&(x, y)| Point { x, y }).collect();
        let seed = super::handdrawn::seed_from(with_offsets[0].0, with_offsets[0].1);
        super::handdrawn::hd_edge_d(&pts, seed)
    } else {
        path.d
    };

    let mut stroke_classes = match e.thickness.as_str() {
        "thick" => "edge-thickness-thick",
        "invisible" => "edge-thickness-invisible",
        _ => "edge-thickness-normal",
    }
    .to_owned();
    match e.pattern.as_str() {
        "dotted" => stroke_classes.push_str(" edge-pattern-dotted"),
        "dashed" => stroke_classes.push_str(" edge-pattern-dashed"),
        _ => stroke_classes.push_str(" edge-pattern-solid"),
    }

    let svg_path = append(elem, "path");
    set_attr(&svg_path, "d", line_path.clone());
    set_attr(&svg_path, "id", format!("{diagram_id}-{}", e.id));
    let mut class = format!(" {stroke_classes}");
    if !e.classes.is_empty() {
        class.push(' ');
        class.push_str(&e.classes);
    }
    set_attr(&svg_path, "class", class);

    // Style string quirk from edges.js: styles + ';' + styles joined by ';'.
    let styles_from_classes: Vec<&String> = e
        .css_compiled_styles
        .iter()
        .filter(|s| !is_label_style(s))
        .collect();
    let styles_joined = styles_from_classes
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(";");
    let styles: String = e.style.iter().fold(String::new(), |acc, s| acc + s + ";");
    let edge_styles_reduce: String = e.style.iter().fold(String::new(), |acc, s| acc + ";" + s);
    let path_style = format!(
        "{};{}",
        if styles_joined.is_empty() {
            styles.clone()
        } else {
            format!("{styles_joined};{styles};")
        },
        edge_styles_reduce
    );
    set_attr(&svg_path, "style", path_style.clone());
    let stroke_color = path_style
        .find("stroke:")
        .map(|i| {
            let rest = &path_style[i + 7..];
            rest.split(';').next().unwrap_or("").to_owned()
        })
        .unwrap_or_default();

    set_attr(&svg_path, "data-edge", "true");
    set_attr(&svg_path, "data-et", "edge");
    set_attr(&svg_path, "data-id", e.id.clone());
    set_attr(&svg_path, "data-points", points_str);
    set_attr(&svg_path, "data-look", e.look.clone());

    // Markers.
    add_edge_markers(
        &svg_path,
        &e.arrow_type_start,
        &e.arrow_type_end,
        diagram_id,
        diagram_type,
        &stroke_color,
        markers,
    );

    let mid_index = points.len() / 2;
    let point = points[mid_index];
    if !is_label_coordinate_in_path(point, &line_path) {
        points_has_changed = true;
    }

    InsertedEdge {
        updated_points: if points_has_changed {
            Some(points)
        } else {
            None
        },
        d: line_path,
    }
}

fn is_label_style(style: &str) -> bool {
    style.contains("color:") && !style.contains("stroke") && !style.contains("fill")
}

/// Marker creation state shared across edges of one diagram.
impl std::fmt::Debug for MarkerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkerState")
            .field("created", &self.created)
            .finish_non_exhaustive()
    }
}

pub struct MarkerState {
    pub root: Element,
    pub created: std::collections::HashSet<String>,
}

fn add_edge_markers(
    svg_path: &Element,
    arrow_type_start: &str,
    arrow_type_end: &str,
    id: &str,
    diagram_type: &str,
    stroke_color: &str,
    markers: &mut MarkerState,
) {
    for (position, arrow_type) in [("start", arrow_type_start), ("end", arrow_type_end)] {
        let (marker_type, fill) = match arrow_type {
            "arrow_cross" => ("cross", false),
            "arrow_point" => ("point", true),
            "arrow_circle" => ("circle", false),
            "arrow_barb" => ("barb", true),
            "aggregation" => ("aggregation", false),
            "extension" => ("extension", false),
            "composition" => ("composition", false),
            "dependency" => ("dependency", false),
            "lollipop" => ("lollipop", false),
            "only_one" => ("onlyOne", false),
            "zero_or_one" => ("zeroOrOne", false),
            "one_or_more" => ("oneOrMore", false),
            "zero_or_more" => ("zeroOrMore", false),
            _ => continue,
        };
        let suffix = if position == "start" { "Start" } else { "End" };
        let original_marker_id = format!("{id}_{diagram_type}-{marker_type}{suffix}");
        let marker_id = if stroke_color.trim().is_empty() {
            original_marker_id
        } else {
            let color_id: String = stroke_color
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            let colored_id = format!("{original_marker_id}_{color_id}");
            if !markers.created.contains(&colored_id) {
                markers.created.insert(colored_id.clone());
                super::markers::create_colored_marker(
                    &markers.root,
                    diagram_type,
                    id,
                    &format!("{marker_type}{suffix}"),
                    &colored_id,
                    stroke_color,
                    fill,
                );
            }
            colored_id
        };
        set_attr(
            svg_path,
            &format!("marker-{position}"),
            format!("url(#{marker_id})"),
        );
    }
}

fn base64(points: &[Point]) -> String {
    // btoa(JSON.stringify(points)) — full-precision JSON.
    let json = format!(
        "[{}]",
        points
            .iter()
            .map(|p| format!("{{\"x\":{},\"y\":{}}}", js_num(p.x), js_num(p.y)))
            .collect::<Vec<_>>()
            .join(",")
    );
    base64_encode(json.as_bytes())
}

pub(crate) fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Port of `utils.isLabelCoordinateInPath`.
fn is_label_coordinate_in_path(point: Point, d_attr: &str) -> bool {
    let rounded_x = js_round(point.x);
    let rounded_y = js_round(point.y);
    let sanitized = round_decimals_in_string(d_attr);
    sanitized.contains(&rounded_x.to_string()) || sanitized.contains(&rounded_y.to_string())
}

/// JS `Math.round`: half-up (towards +Infinity), unlike Rust's round.
fn js_round(n: f64) -> i64 {
    (n + 0.5).floor() as i64
}

fn round_decimals_in_string(d: &str) -> String {
    // Replaces /(\d+\.\d+)/g with Math.round(parseFloat(m)).
    let mut out = String::new();
    let chars: Vec<char> = d.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len()
                && chars[i] == '.'
                && chars.get(i + 1).is_some_and(char::is_ascii_digit)
            {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let num: String = chars[start..i].iter().collect();
                let value: f64 = num.parse().expect("numeric");
                out.push_str(&js_round(value).to_string());
            } else {
                let num: String = chars[start..i].iter().collect();
                out.push_str(&num);
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Port of `utils.calcLabelPosition` (midpoint by arc length).
#[must_use]
pub fn calc_label_position(points: &[Point]) -> Point {
    if points.len() == 1 {
        return points[0];
    }
    let mut total_distance = 0.0;
    let mut prev: Option<Point> = None;
    for &p in points {
        if let Some(prev) = prev {
            total_distance += dist(p, prev);
        }
        prev = Some(p);
    }
    calculate_point(points, total_distance / 2.0)
}

/// `utils.calcTerminalLabelPosition`.
#[must_use]
pub fn calc_terminal_label_position(
    terminal_marker_size: f64,
    position: &str,
    points_in: &[Point],
) -> Point {
    let mut points: Vec<Point> = points_in.to_vec();
    if position != "start_left" && position != "start_right" {
        points.reverse();
    }
    let distance_to_cardinality = 25.0 + terminal_marker_size;
    let center = calculate_point(&points, distance_to_cardinality);
    let d = 10.0 + terminal_marker_size * 0.5;
    let angle = crate::mathx::atan2(points[0].y - center.y, points[0].x - center.x);
    let mut pos = Point { x: 0.0, y: 0.0 };
    match position {
        "start_left" => {
            pos.x = crate::mathx::sin(angle + std::f64::consts::PI) * d
                + f64::midpoint(points[0].x, center.x);
            pos.y = -crate::mathx::cos(angle + std::f64::consts::PI) * d
                + f64::midpoint(points[0].y, center.y);
        }
        "end_right" => {
            pos.x = crate::mathx::sin(angle - std::f64::consts::PI) * d
                + f64::midpoint(points[0].x, center.x)
                - 5.0;
            pos.y = -crate::mathx::cos(angle - std::f64::consts::PI) * d
                + f64::midpoint(points[0].y, center.y)
                - 5.0;
        }
        "end_left" => {
            pos.x = crate::mathx::sin(angle) * d + f64::midpoint(points[0].x, center.x) - 5.0;
            pos.y = -crate::mathx::cos(angle) * d + f64::midpoint(points[0].y, center.y) - 5.0;
        }
        _ => {
            pos.x = crate::mathx::sin(angle) * d + f64::midpoint(points[0].x, center.x);
            pos.y = -crate::mathx::cos(angle) * d + f64::midpoint(points[0].y, center.y);
        }
    }
    pos
}

fn dist(a: Point, b: Point) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

fn round_number(n: f64, precision: i32) -> f64 {
    let factor = 10f64.powi(precision);
    (n * factor).round() / factor
}

fn calculate_point(points: &[Point], distance_to_traverse: f64) -> Point {
    let mut prev: Option<Point> = None;
    let mut remaining = distance_to_traverse;
    for &point in points {
        if let Some(p) = prev {
            let vector_distance = dist(point, p);
            if vector_distance == 0.0 {
                return p;
            }
            if vector_distance < remaining {
                remaining -= vector_distance;
            } else {
                let ratio = remaining / vector_distance;
                if ratio <= 0.0 {
                    return p;
                }
                if ratio >= 1.0 {
                    return point;
                }
                return Point {
                    x: round_number((1.0 - ratio) * p.x + ratio * point.x, 5),
                    y: round_number((1.0 - ratio) * p.y + ratio * point.y, 5),
                };
            }
        }
        prev = Some(point);
    }
    panic!("could not find a suitable point for the given distance");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basis_curve_matches_d3_output() {
        // From mmdc simple.svg: A->B edge, marker offset already applied.
        let pts = [
            (114.558_593_75, 62.0),
            (114.558_593_75, 87.0),
            (114.558_593_75, 108.0),
        ];
        let mut path = D3Path::default();
        let mut curve = BasisCurve::new(&mut path);
        for &(x, y) in &pts {
            curve.point(x, y);
        }
        curve.line_end();
        assert_eq!(
            path.d,
            "M114.559,62L114.559,66.167C114.559,70.333,114.559,78.667,114.559,86.333C114.559,94,114.559,101,114.559,104.5L114.559,108"
        );
    }
}
