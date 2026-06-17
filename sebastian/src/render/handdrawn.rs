//! Opt-in `look: handDrawn` rendering: sketchy node shapes and edges plus a
//! handwritten label font, giving an Excalidraw-style look.
//!
//! This is a stylization layer, **not** a byte-exact port. Upstream mermaid
//! draws hand-drawn shapes with rough.js seeded by `Math.random`, so two
//! `mmdc` runs of the same diagram differ. We instead use a deterministic
//! seeded PRNG (a port of rough.js's mulberry32 generator) so sebastian's
//! hand-drawn output is stable run to run. The wobble algorithm follows
//! rough.js's `_line`/`_doubleLine`, the source of the sketchy double stroke.

use crate::dagre::types::Point;
use crate::svg::{Element, append, insert_first, js_num, set_attr};
use std::fmt::Write;

/// rough.js default-ish drawing parameters tuned for diagram-sized shapes.
const MAX_OFFSET: f64 = 2.0;
const ROUGHNESS: f64 = 1.0;
const BOWING: f64 = 1.0;

/// Deterministic PRNG: a port of rough.js's seeded `mulberry32`.
struct Rng {
    state: u32,
}

impl Rng {
    fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// rough.js `Random.next()` in the seeded branch.
    fn next(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x6D2B_79F5);
        let a = self.state;
        let mut t = (a ^ (a >> 15)).wrapping_mul(a | 0x1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 0x3D));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// rough.js `_offset(min, max)`: `roughness * gain * (rand*(max-min)+min)`.
    fn offset(&mut self, min: f64, max: f64, gain: f64) -> f64 {
        ROUGHNESS * gain * (self.next() * (max - min) + min)
    }

    /// rough.js `_offsetOpt(x)` == `_offset(-x, x)`.
    fn offset_sym(&mut self, x: f64, gain: f64) -> f64 {
        self.offset(-x, x, gain)
    }
}

/// Derives a stable seed from a shape's position so each shape wobbles
/// differently but reproducibly.
#[must_use]
pub fn seed_from(x: f64, y: f64) -> u32 {
    let mix = (x * 73.0 + y * 179.0).to_bits();
    ((mix >> 32) ^ mix) as u32 | 1
}

/// rough.js length-based roughness gain.
fn roughness_gain(len_sq: f64) -> f64 {
    let len = len_sq.sqrt();
    if len < 200.0 {
        1.0
    } else if len > 500.0 {
        0.4
    } else {
        -0.001_666_8 * len + 1.233_334
    }
}

/// One rough.js `_line` pass: appends `M …` + `C …` for a single segment.
fn line_pass(rng: &mut Rng, a: Point, b: Point, out: &mut String) {
    let len_sq = (a.x - b.x).powi(2) + (a.y - b.y).powi(2);
    let gain = roughness_gain(len_sq);
    let mut offset = MAX_OFFSET;
    if offset * offset * 100.0 > len_sq {
        offset = len_sq.sqrt() / 10.0;
    }
    let half = offset / 2.0;
    let diverge = 0.2 + rng.next() * 0.2;
    let mid_x = rng.offset_sym(BOWING * MAX_OFFSET * (b.y - a.y) / 200.0, gain);
    let mid_y = rng.offset_sym(BOWING * MAX_OFFSET * (a.x - b.x) / 200.0, gain);

    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let _ = write!(
        out,
        "M{} {} C{} {}, {} {}, {} {} ",
        js_num(a.x + rng.offset_sym(half, gain)),
        js_num(a.y + rng.offset_sym(half, gain)),
        js_num(mid_x + a.x + dx * diverge + rng.offset_sym(half, gain)),
        js_num(mid_y + a.y + dy * diverge + rng.offset_sym(half, gain)),
        js_num(mid_x + a.x + dx * 2.0 * diverge + rng.offset_sym(half, gain)),
        js_num(mid_y + a.y + dy * 2.0 * diverge + rng.offset_sym(half, gain)),
        js_num(b.x + rng.offset_sym(half, gain)),
        js_num(b.y + rng.offset_sym(half, gain)),
    );
}

/// rough.js `_doubleLine`: `passes` passes per segment (2 for the sketchy
/// double-stroke outline of a shape, 1 for a single edge line).
fn stroke_path_d(points: &[Point], closed: bool, passes: usize, seed: u32) -> String {
    let mut rng = Rng::new(seed);
    let mut segs: Vec<[Point; 2]> = points.windows(2).map(|w| [w[0], w[1]]).collect();
    if closed && points.len() > 1 {
        segs.push([points[points.len() - 1], points[0]]);
    }
    let mut d = String::new();
    for seg in &segs {
        for _ in 0..passes {
            line_pass(&mut rng, seg[0], seg[1], &mut d);
        }
    }
    d.trim_end().to_owned()
}

/// Clean closed fill path (`M L … Z`) under the sketchy outline.
fn fill_path_d(points: &[Point]) -> String {
    let mut d = format!("M{} {}", js_num(points[0].x), js_num(points[0].y));
    for p in &points[1..] {
        let _ = write!(d, " L{} {}", js_num(p.x), js_num(p.y));
    }
    d.push_str(" Z");
    d
}

/// Emits a sketchy filled polygon (`<g>` with a clean fill path under a
/// double-stroked wobbly outline). `style` carries node-specific overrides.
#[allow(clippy::too_many_arguments)]
pub fn hd_polygon(
    parent: &Element,
    points: &[Point],
    fill: &str,
    stroke: &str,
    stroke_width: &str,
    style: &str,
    seed: u32,
) -> Element {
    let g = insert_first(parent, "g");
    let fill_el = append(&g, "path");
    set_attr(&fill_el, "d", fill_path_d(points));
    set_attr(&fill_el, "stroke", "none");
    set_attr(&fill_el, "fill", fill);
    if !style.is_empty() {
        set_attr(&fill_el, "style", style);
    }
    let stroke_el = append(&g, "path");
    set_attr(&stroke_el, "d", stroke_path_d(points, true, 2, seed));
    set_attr(&stroke_el, "stroke", stroke);
    set_attr(&stroke_el, "stroke-width", stroke_width);
    set_attr(&stroke_el, "fill", "none");
    if !style.is_empty() {
        set_attr(&stroke_el, "style", style);
    }
    g
}

/// Ellipse approximated by a wobbly polygon through sampled boundary points.
#[allow(clippy::too_many_arguments)]
pub fn hd_ellipse(
    parent: &Element,
    rx: f64,
    ry: f64,
    fill: &str,
    stroke: &str,
    stroke_width: &str,
    style: &str,
    seed: u32,
) -> Element {
    let steps = 24;
    let mut points = Vec::with_capacity(steps);
    for i in 0..steps {
        #[allow(clippy::cast_precision_loss)]
        let angle = std::f64::consts::TAU * (i as f64) / (steps as f64);
        points.push(Point {
            x: rx * core_math::cos(angle),
            y: ry * core_math::sin(angle),
        });
    }
    hd_polygon(parent, &points, fill, stroke, stroke_width, style, seed)
}

/// A sketchy open polyline for edges: a single rough pass through the points.
#[must_use]
pub fn hd_edge_d(points: &[Point], seed: u32) -> String {
    stroke_path_d(points, false, 1, seed)
}
