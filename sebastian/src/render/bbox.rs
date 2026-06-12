//! Geometric bounding-box computation replicating `SVGGraphicsElement.getBBox()`
//! for the subset of SVG the renderer emits.
//!
//! Blink's pipeline, replicated here:
//! - attribute numbers are parsed by `GenericParseNumber<float>` — an
//!   f32-accumulating hand-rolled parser, *not* correctly-rounded strtof;
//! - every element box is a `gfx::RectF` (f32 x/y/width/height);
//! - path boxes come from Skia's `computeTightBounds` (f32 points, f32
//!   extrema roots/evaluation), converted via `SkRect` width subtraction;
//! - translates offset boxes with f32 adds (size unchanged); unions
//!   recompute `right()` as `f32(x + w)` and store width as an f32
//!   difference.

use crate::svg::{Element, get_attr};

fn f32r(v: f64) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    f64::from(v as f32)
}

/// A `gfx::RectF`: all fields hold f32-exact values.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub min_x: f64,
    pub min_y: f64,
    w: f64,
    h: f64,
    /// True when no geometry contributed (distinct from a zero-size rect).
    empty: bool,
    /// `<line>` boxes are valid even with zero area (Blink unions them).
    line_valid: bool,
}

impl Rect {
    pub const EMPTY: Self = Self {
        min_x: 0.0,
        min_y: 0.0,
        w: 0.0,
        h: 0.0,
        empty: true,
        line_valid: false,
    };

    fn from_xywh(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self {
            min_x: f32r(x),
            min_y: f32r(y),
            w: f32r(w),
            h: f32r(h),
            empty: false,
            line_valid: false,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        // gfx::RectF::IsEmpty(): zero-area counts as empty for unions.
        self.empty || self.w <= 0.0 || self.h <= 0.0
    }

    #[must_use]
    pub fn width(&self) -> f64 {
        self.w
    }

    #[must_use]
    pub fn height(&self) -> f64 {
        self.h
    }

    /// `RectF::right()` — f32 addition.
    fn right(&self) -> f64 {
        f32r(self.min_x + self.w)
    }

    /// `RectF::bottom()` — f32 addition.
    fn bottom(&self) -> f64 {
        f32r(self.min_y + self.h)
    }

    /// `gfx::RectF::Union` (with Blink's valid-even-if-empty handling for
    /// line boxes).
    fn union(&mut self, other: &Rect) {
        if other.is_empty() && (!other.line_valid || other.empty) {
            return;
        }
        if self.is_empty() && (!self.line_valid || self.empty) {
            *self = *other;
            return;
        }
        let rx = self.min_x.min(other.min_x);
        let ry = self.min_y.min(other.min_y);
        let rr = self.right().max(other.right());
        let rb = self.bottom().max(other.bottom());
        let line_valid = self.line_valid || other.line_valid;
        *self = Self::from_xywh(rx, ry, f32r(rr - rx), f32r(rb - ry));
        self.line_valid = line_valid;
    }

    /// `RectF::Offset` — f32 adds, size unchanged.
    fn offset(&self, dx: f64, dy: f64) -> Rect {
        if self.empty {
            return *self;
        }
        Self {
            min_x: f32r(self.min_x + dx),
            min_y: f32r(self.min_y + dy),
            w: self.w,
            h: self.h,
            empty: false,
            line_valid: self.line_valid,
        }
    }
}

/// Blink `GenericParseNumber<float>`: f32-accumulating SVG number parse.
/// Returns the resulting f32 widened to f64.
#[must_use]
pub fn blink_float(s: &str) -> f64 {
    let b = s.trim().as_bytes();
    if b.is_empty() {
        return 0.0;
    }
    let mut i = 0usize;
    let mut sign = 1.0f32;
    if b[0] == b'+' {
        i = 1;
    } else if b[0] == b'-' {
        sign = -1.0;
        i = 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let mut integer = 0.0f32;
    let mut multiplier = 1.0f32;
    for j in (int_start..i).rev() {
        integer += multiplier * f32::from(b[j] - b'0');
        multiplier *= 10.0;
    }
    let mut decimal = 0.0f32;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let mut frac = 1.0f32;
        while i < b.len() && b[i].is_ascii_digit() {
            frac *= 0.1;
            decimal += f32::from(b[i] - b'0') * frac;
            i += 1;
        }
    }
    let mut number = (integer + decimal) * sign;
    if i + 1 < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        let mut exp_negative = false;
        if b[i] == b'+' {
            i += 1;
        } else if b[i] == b'-' {
            exp_negative = true;
            i += 1;
        }
        let mut exponent = 0.0f32;
        while i < b.len() && b[i].is_ascii_digit() {
            exponent = exponent * 10.0 + f32::from(b[i] - b'0');
            i += 1;
        }
        if exp_negative {
            exponent = -exponent;
        }
        if exponent != 0.0 {
            #[allow(clippy::cast_possible_truncation)]
            let p = 10.0f64.powf(f64::from(exponent)) as f32;
            number *= p;
        }
    }
    f64::from(number)
}

fn parse_translate(transform: &str) -> (f64, f64) {
    let Some(start) = transform.find("translate(") else {
        return (0.0, 0.0);
    };
    let rest = &transform[start + "translate(".len()..];
    let Some(end) = rest.find(')') else {
        return (0.0, 0.0);
    };
    let inner = &rest[..end];
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    let x = parts.first().map_or(0.0, |s| blink_float(s));
    let y = parts.get(1).map_or(0.0, |s| blink_float(s));
    (x, y)
}

/// SVG length attributes (rect/circle/line geometry) parse through the CSS
/// path in double precision, unlike path data and transform lists.
fn len_num(el: &Element, name: &str) -> f64 {
    get_attr(el, name).map_or(0.0, |v| v.trim().parse().unwrap_or(0.0))
}

/// f32 min/max accumulator (an `SkRect` during geometry construction).
#[derive(Clone, Copy)]
struct SkAcc {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

impl SkAcc {
    const EMPTY: Self = Self {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };

    fn add_point(&mut self, x: f64, y: f64) {
        let x = f32r(x);
        let y = f32r(y);
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }

    fn to_rectf(self) -> Rect {
        if self.min_x > self.max_x {
            return Rect::EMPTY;
        }
        // SkRect -> gfx::RectF: width/height from f32 subtraction.
        Rect {
            min_x: self.min_x,
            min_y: self.min_y,
            w: f32r(self.max_x - self.min_x),
            h: f32r(self.max_y - self.min_y),
            empty: false,
            line_valid: false,
        }
    }
}

/// Computes the bbox of `el` and its descendants (in `el`'s coordinates).
#[must_use]
pub fn element_bbox(el: &Element) -> Rect {
    subtree_bbox(el)
}

impl Rect {
    /// Builds a rect from f64 geometry (f32-rounded), for callers that
    /// union extra boxes (e.g. text elements) manually.
    #[must_use]
    pub fn from_geometry(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self::from_xywh(x, y, w, h)
    }

    /// Public union (Blink container semantics).
    pub fn union_with(&mut self, other: &Rect) {
        self.union(other);
    }
}

/// Union of `el`'s own geometry and its children's mapped boxes, in `el`'s
/// local coordinates.
fn subtree_bbox(el: &Element) -> Rect {
    let tag = el.borrow().tag.clone();
    let mut out = Rect::EMPTY;
    if matches!(tag.as_str(), "marker" | "style" | "defs" | "title") {
        return out;
    }

    match tag.as_str() {
        "rect" => {
            let w = len_num(el, "width");
            let h = len_num(el, "height");
            if w > 0.0 || h > 0.0 {
                let x = len_num(el, "x");
                let y = len_num(el, "y");
                out.union(&Rect::from_xywh(x, y, w, h));
            }
        }
        "foreignObject" => {
            let w = len_num(el, "width");
            let h = len_num(el, "height");
            if w > 0.0 && h > 0.0 {
                out.union(&Rect::from_xywh(0.0, 0.0, w, h));
            }
            return out; // html content does not affect getBBox
        }
        "circle" => {
            let cx = len_num(el, "cx");
            let cy = len_num(el, "cy");
            let r = len_num(el, "r");
            out.union(&Rect::from_xywh(
                f32r(cx - r),
                f32r(cy - r),
                f32r(2.0 * r),
                f32r(2.0 * r),
            ));
        }
        "polygon" => {
            let mut sk = SkAcc::EMPTY;
            if let Some(points) = get_attr(el, "points") {
                for pair in points.split(' ') {
                    let mut it = pair.split(',');
                    if let (Some(x), Some(y)) = (it.next(), it.next()) {
                        sk.add_point(blink_float(x), blink_float(y));
                    }
                }
            }
            out.union(&sk.to_rectf());
        }
        "path" => {
            if let Some(d) = get_attr(el, "d") {
                let mut sk = SkAcc::EMPTY;
                path_bbox(&d, &mut sk);
                out.union(&sk.to_rectf());
            }
        }
        "line" => {
            // Lengths parse as doubles; sizes are f32s of double differences.
            let x1 = len_num(el, "x1");
            let y1 = len_num(el, "y1");
            let x2 = len_num(el, "x2");
            let y2 = len_num(el, "y2");
            let mut r = Rect::from_xywh(x1.min(x2), y1.min(y2), (x2 - x1).abs(), (y2 - y1).abs());
            r.line_valid = true;
            out.union(&r);
        }
        _ => {}
    }

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
        let cr = subtree_bbox(&child);
        if cr.is_empty() && (!cr.line_valid || cr.empty) {
            continue;
        }
        let (dx, dy) = get_attr(&child, "transform").map_or((0.0, 0.0), |t| parse_translate(&t));
        out.union(&cr.offset(dx, dy));
    }
    out
}

/// Tokenizes an SVG path `d` attribute and accumulates Skia tight bounds.
#[allow(clippy::too_many_lines)]
fn path_bbox(d: &str, out: &mut SkAcc) {
    let mut nums: Vec<f64> = Vec::new();
    let mut cmd = ' ';
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut i = 0;
    let chars: Vec<char> = d.chars().collect();

    let flush = |cmd: char, nums: &[f64], cx: &mut f64, cy: &mut f64, out: &mut SkAcc| {
        match cmd {
            'M' | 'L' => {
                for pair in nums.chunks(2) {
                    if pair.len() == 2 {
                        *cx = pair[0];
                        *cy = pair[1];
                        out.add_point(*cx, *cy);
                    }
                }
            }
            'H' => {
                for &x in nums {
                    *cx = x;
                    out.add_point(*cx, *cy);
                }
            }
            'V' => {
                for &y in nums {
                    *cy = y;
                    out.add_point(*cx, *cy);
                }
            }
            'l' => {
                for pair in nums.chunks(2) {
                    if pair.len() == 2 {
                        *cx += pair[0];
                        *cy += pair[1];
                        out.add_point(*cx, *cy);
                    }
                }
            }
            'h' => {
                for &dx in nums {
                    *cx += dx;
                    out.add_point(*cx, *cy);
                }
            }
            'v' => {
                for &dy in nums {
                    *cy += dy;
                    out.add_point(*cx, *cy);
                }
            }
            'q' => {
                for seg in nums.chunks(4) {
                    if seg.len() == 4 {
                        let (qx, qy) = (*cx + seg[0], *cy + seg[1]);
                        let (ex, ey) = (*cx + seg[2], *cy + seg[3]);
                        let c1x = *cx + 2.0 / 3.0 * (qx - *cx);
                        let c1y = *cy + 2.0 / 3.0 * (qy - *cy);
                        let c2x = ex + 2.0 / 3.0 * (qx - ex);
                        let c2y = ey + 2.0 / 3.0 * (qy - ey);
                        cubic_bbox(*cx, *cy, c1x, c1y, c2x, c2y, ex, ey, out);
                        *cx = ex;
                        *cy = ey;
                    }
                }
            }
            'C' => {
                for seg in nums.chunks(6) {
                    if seg.len() == 6 {
                        cubic_bbox(
                            *cx, *cy, seg[0], seg[1], seg[2], seg[3], seg[4], seg[5], out,
                        );
                        *cx = seg[4];
                        *cy = seg[5];
                    }
                }
            }
            'Q' => {
                for seg in nums.chunks(4) {
                    if seg.len() == 4 {
                        // Convert quadratic to cubic for the bbox.
                        let c1x = *cx + 2.0 / 3.0 * (seg[0] - *cx);
                        let c1y = *cy + 2.0 / 3.0 * (seg[1] - *cy);
                        let c2x = seg[2] + 2.0 / 3.0 * (seg[0] - seg[2]);
                        let c2y = seg[3] + 2.0 / 3.0 * (seg[1] - seg[3]);
                        cubic_bbox(*cx, *cy, c1x, c1y, c2x, c2y, seg[2], seg[3], out);
                        *cx = seg[2];
                        *cy = seg[3];
                    }
                }
            }
            'a' => {
                for seg in nums.chunks(7) {
                    if seg.len() == 7 {
                        let (rx, ry) = (seg[0].abs(), seg[1].abs());
                        let (ex, ey) = (*cx + seg[5], *cy + seg[6]);
                        out.add_point(ex, ey);
                        // Axis-aligned half-ellipse arcs (our cylinder shape):
                        // include the vertical extreme between the endpoints.
                        if (*cy - ey).abs() < 1e-9 {
                            let midx = f64::midpoint(*cx, ex);
                            out.add_point(midx, *cy - ry);
                            out.add_point(midx, *cy + ry);
                            let _ = rx;
                        }
                        *cx = ex;
                        *cy = ey;
                    }
                }
            }
            _ => {}
        }
    };

    let mut num_buf = String::new();
    let push_num = |buf: &mut String, nums: &mut Vec<f64>| {
        if !buf.is_empty() {
            // Blink parses path numbers with the f32-accumulating parser.
            nums.push(blink_float(buf));
            buf.clear();
        }
    };

    while i < chars.len() {
        let c = chars[i];
        if c.is_alphabetic() && c != 'e' && c != 'E' {
            push_num(&mut num_buf, &mut nums);
            flush(cmd, &nums, &mut cx, &mut cy, out);
            nums.clear();
            cmd = if c == 'Z' || c == 'z' { ' ' } else { c };
        } else if c == ',' || c == ' ' {
            push_num(&mut num_buf, &mut nums);
        } else if c == '-'
            && !num_buf.is_empty()
            && !num_buf.ends_with('e')
            && !num_buf.ends_with('E')
        {
            push_num(&mut num_buf, &mut nums);
            num_buf.push(c);
        } else {
            num_buf.push(c);
        }
        i += 1;
    }
    push_num(&mut num_buf, &mut nums);
    flush(cmd, &nums, &mut cx, &mut cy, out);
}

/// Skia `valid_unit_divide`: returns `numer/denom` iff strictly in (0, 1).
fn valid_unit_divide(mut numer: f32, mut denom: f32) -> Option<f32> {
    if numer < 0.0 {
        numer = -numer;
        denom = -denom;
    }
    if denom == 0.0 || numer == 0.0 || numer >= denom {
        return None;
    }
    let r = numer / denom;
    if r.is_nan() || r == 0.0 {
        return None;
    }
    Some(r)
}

/// Skia `SkFindUnitQuadRoots` (float coefficients, double discriminant).
fn sk_find_unit_quad_roots(a: f32, b: f32, c: f32) -> Vec<f32> {
    if a == 0.0 {
        return valid_unit_divide(-c, b).into_iter().collect();
    }
    let dr = f64::from(b) * f64::from(b) - 4.0 * f64::from(a) * f64::from(c);
    if dr < 0.0 {
        return Vec::new();
    }
    #[allow(clippy::cast_possible_truncation)]
    let r = dr.sqrt() as f32;
    let q = if b < 0.0 {
        -(b - r) / 2.0
    } else {
        -(b + r) / 2.0
    };
    let mut roots: Vec<f32> = valid_unit_divide(q, a).into_iter().collect();
    roots.extend(valid_unit_divide(c, q));
    if roots.len() == 2 {
        if roots[0] > roots[1] {
            roots.swap(0, 1);
        }
        if roots[0] == roots[1] {
            roots.pop();
        }
    }
    roots
}

/// Skia `SkFindCubicExtrema` coefficients.
fn sk_find_cubic_extrema(a: f32, b: f32, c: f32, d: f32) -> Vec<f32> {
    let na = d - a + 3.0 * (b - c);
    let nb = 2.0 * (a - b - b + c);
    let nc = b - a;
    sk_find_unit_quad_roots(na, nb, nc)
}

/// Skia `SkCubicCoeff::eval` (float Horner form).
fn sk_eval_cubic(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let a = p3 + 3.0 * (p1 - p2) - p0;
    let b = 3.0 * (p2 - (p1 + p1) + p0);
    let c = 3.0 * (p1 - p0);
    let d = p0;
    ((a * t + b) * t + c) * t + d
}

/// Cubic segment bbox as Skia's `computeTightBounds` produces it: control
/// points and arithmetic in f32, extrema via `SkFindCubicExtrema`.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::similar_names)]
fn cubic_bbox(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x3: f64,
    y3: f64,
    out: &mut SkAcc,
) {
    out.add_point(x0, y0);
    out.add_point(x3, y3);
    #[allow(clippy::cast_possible_truncation)]
    let (fx0, fx1, fx2, fx3) = (x0 as f32, x1 as f32, x2 as f32, x3 as f32);
    #[allow(clippy::cast_possible_truncation)]
    let (fy0, fy1, fy2, fy3) = (y0 as f32, y1 as f32, y2 as f32, y3 as f32);
    let mut ts = sk_find_cubic_extrema(fx0, fx1, fx2, fx3);
    ts.extend(sk_find_cubic_extrema(fy0, fy1, fy2, fy3));
    for t in ts {
        let px = sk_eval_cubic(fx0, fx1, fx2, fx3, t);
        let py = sk_eval_cubic(fy0, fy1, fy2, fy3, t);
        out.add_point(f64::from(px), f64::from(py));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blink_float_quirks() {
        // The f32-midpoint value rounds UP through Blink's f32 accumulation,
        // unlike correctly-rounded strtof (which ties to even, i.e. down).
        assert_eq!(blink_float("799.7435607910156"), 799.743_591_308_593_8);
        assert_eq!(blink_float("8"), 8.0);
        assert_eq!(blink_float("-0.5"), -0.5);
    }
}
