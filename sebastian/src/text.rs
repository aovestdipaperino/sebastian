//! Text measurement replicating Chrome's layout of mermaid labels.
//!
//! mermaid (with default `htmlLabels: true`) measures labels via
//! `getBoundingClientRect()` of a `<div>` styled with the theme font
//! (`"trebuchet ms", verdana, arial, sans-serif` at 16px, line-height 1.5).
//! Reproducing those numbers requires the same font and the same advance
//! arithmetic Chrome uses.

use std::collections::HashMap;
use std::rc::Rc;

use ttf_parser::Face;

const FONT_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Trebuchet MS.ttf",
    "/Library/Fonts/Trebuchet MS.ttf",
    "C:/Windows/Fonts/trebuc.ttf",
];

/// Chrome/CoreText fallback cascade observed on macOS for the mermaid font
/// stack ("trebuchet ms", verdana, arial, sans-serif). Calibrated against
/// Chrome-measured advances for symbol glyphs.
const FALLBACK_FONTS: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Verdana.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/System/Library/Fonts/LucidaGrande.ttc",
    "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
    "/System/Library/Fonts/\u{30d2}\u{30e9}\u{30ad}\u{3099}\u{30ce}\u{89d2}\u{30b3}\u{3099}\u{30b7}\u{30c3}\u{30af} W3.ttc",
    "/System/Library/Fonts/Apple Symbols.ttf",
    "/System/Library/Fonts/Menlo.ttc",
];

/// Index (into [`FALLBACK_FONTS`]) of Arial Unicode, which CoreText skips
/// for box-drawing characters in favor of the CJK cascade.
const ARIAL_UNICODE_IDX: usize = 3;

const APPLE_COLOR_EMOJI: &str = "/System/Library/Fonts/Apple Color Emoji.ttc";

const BOLD_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Trebuchet MS Bold.ttf",
    "/Library/Fonts/Trebuchet MS Bold.ttf",
    "C:/Windows/Fonts/trebucbd.ttf",
];

/// Excalifont (SIL OFL, from the Excalidraw project): the `look: handDrawn`
/// label font, embedded so hand-drawn output is identical on every host.
/// It is both inlined as a `@font-face` in the SVG (see
/// `hand_drawn_font_css`) and used here for layout metrics, so node sizes
/// match the rendered glyphs exactly. Excalifont has no bold face; bold
/// runs measure with the same face (renderers synthesize bold).
pub(crate) const EXCALIFONT: &[u8] = include_bytes!("../fonts/Excalifont-Regular.ttf");

/// Excalifont bytes, honoring a host-registered override.
fn hand_drawn_font() -> Vec<u8> {
    read_font("Excalifont-Regular.ttf").unwrap_or_else(|| EXCALIFONT.to_vec())
}

/// Whether classic-look measurement uses the real Trebuchet MS face. When
/// false, [`TextMeasurer`] measures with the embedded Cabin fallback, and
/// the rendered SVG must draw with Cabin too (see
/// `render::css::fallback_font_css`) or labels overflow their boxes.
#[must_use]
pub fn trebuchet_available() -> bool {
    FONT_CANDIDATES.iter().any(|p| read_font(p).is_some())
}

/// Whether sequence-diagram measurement uses the real Times New Roman face;
/// when false, [`SeqMeasurer`] measures with the embedded Tinos fallback.
#[must_use]
pub fn times_available() -> bool {
    TIMES_CANDIDATES.iter().any(|p| read_font(p).is_some())
}

thread_local! {
    static HAND_DRAWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Switches text measurement to the hand-drawn (`look: handDrawn`) label
/// font for measurers constructed afterwards on this thread. Set at the
/// `render_diagram` boundary; individual renderers don't need to opt in.
pub fn set_hand_drawn(on: bool) {
    HAND_DRAWN.with(|h| h.set(on));
}

/// Whether hand-drawn measurement is active on this thread.
#[must_use]
pub fn is_hand_drawn() -> bool {
    HAND_DRAWN.with(std::cell::Cell::get)
}

const TIMES_CANDIDATES: &[&str] = &[
    "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
    "/Library/Fonts/Times New Roman.ttf",
    "C:/Windows/Fonts/times.ttf",
];

/// Embedded last-resort faces for hosts without the proprietary fonts
/// (bare Linux, wasm): Cabin (SIL OFL) stands in for Trebuchet MS and
/// Tinos (SIL OFL, Times-metric-compatible) for Times New Roman. With a
/// fallback in play, output is well-proportioned but NOT byte-exact vs mmdc.
pub(crate) const CABIN_FALLBACK: &[u8] = include_bytes!("../fonts/Cabin.ttf");
pub(crate) const TINOS_FALLBACK: &[u8] = include_bytes!("../fonts/Tinos-Regular.ttf");

thread_local! {
    /// Host-registered font bytes, keyed by file name (e.g. "Trebuchet MS.ttf").
    /// Consulted before the filesystem; the only font source on wasm, where
    /// there is no filesystem at all.
    static FONT_REGISTRY: std::cell::RefCell<HashMap<String, Vec<u8>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Registers font bytes under a file name (e.g. `"Trebuchet MS.ttf"`), taking
/// precedence over the same file on disk. On wasm this is the only way to
/// provide fonts; `"Trebuchet MS.ttf"` is required, `"Trebuchet MS Bold.ttf"`
/// and `"Times New Roman.ttf"` unlock bold and sequence-diagram metrics, and
/// the remaining fallback faces are optional.
pub fn register_font(file_name: &str, data: Vec<u8>) {
    FONT_REGISTRY.with(|r| r.borrow_mut().insert(file_name.to_string(), data));
}

/// Font bytes for `path`: the registry (keyed by file name) first, then the
/// filesystem on targets that have one.
fn read_font(path: &str) -> Option<Vec<u8>> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let registered = FONT_REGISTRY.with(|r| r.borrow().get(name).cloned());
    if registered.is_some() {
        return registered;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read(path).ok()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// BMP characters with `Emoji_Presentation=Yes` (UTS #51); SMP emoji are
/// handled by codepoint range.
fn is_emoji_presentation(c: char) -> bool {
    let cp = c as u32;
    if cp >= 0x1F000 {
        return matches!(cp,
            0x1F004 | 0x1F0CF | 0x1F18E | 0x1F191..=0x1F19A
            | 0x1F1E6..=0x1F1FF | 0x1F201 | 0x1F21A | 0x1F22F
            | 0x1F232..=0x1F236 | 0x1F238..=0x1F23A | 0x1F250..=0x1F251
            | 0x1F300..=0x1F5FF | 0x1F600..=0x1F64F | 0x1F680..=0x1F6FF
            | 0x1F900..=0x1F9FF | 0x1FA00..=0x1FAFF
        ) && !matches!(cp,
            // Text-presentation pictographs within those ranges.
            0x1F321..=0x1F32C | 0x1F336 | 0x1F37D | 0x1F396..=0x1F397
            | 0x1F399..=0x1F39B | 0x1F39E..=0x1F39F | 0x1F3CB..=0x1F3CE
            | 0x1F3D4..=0x1F3DF | 0x1F3F3 | 0x1F3F5 | 0x1F3F7
            | 0x1F43F | 0x1F441 | 0x1F4FD | 0x1F549..=0x1F54A
            | 0x1F56F..=0x1F570 | 0x1F573..=0x1F579 | 0x1F587
            | 0x1F58A..=0x1F58D | 0x1F590 | 0x1F5A5 | 0x1F5A8
            | 0x1F5B1..=0x1F5B2 | 0x1F5BC | 0x1F5C2..=0x1F5C4
            | 0x1F5D1..=0x1F5D3 | 0x1F5DC..=0x1F5DE | 0x1F5E1
            | 0x1F5E3 | 0x1F5E8 | 0x1F5EF | 0x1F5F3 | 0x1F5FA
            | 0x1F6CB | 0x1F6CD..=0x1F6CF | 0x1F6E0..=0x1F6E5
            | 0x1F6E9 | 0x1F6F0 | 0x1F6F3
        );
    }
    matches!(cp,
        0x231A..=0x231B | 0x23E9..=0x23EC | 0x23F0 | 0x23F3
        | 0x25FD..=0x25FE | 0x2614..=0x2615 | 0x2648..=0x2653 | 0x267F
        | 0x2693 | 0x26A1 | 0x26AA..=0x26AB | 0x26BD..=0x26BE
        | 0x26C4..=0x26C5 | 0x26CE | 0x26D4 | 0x26EA | 0x26F2..=0x26F3
        | 0x26F5 | 0x26FA | 0x26FD | 0x2705 | 0x270A..=0x270B | 0x2728
        | 0x274C | 0x274E | 0x2753..=0x2755 | 0x2757 | 0x2795..=0x2797
        | 0x27B0 | 0x27BF | 0x2B1B..=0x2B1C | 0x2B50 | 0x2B55
    )
}

const fn is_box_drawing(c: char) -> bool {
    matches!(c as u32, 0x2500..=0x259F)
}

const HELVETICA: &str = "/System/Library/Fonts/Helvetica.ttc";
const APPLE_SYMBOLS: &str = "/System/Library/Fonts/Apple Symbols.ttf";

/// CoreText routes some Unicode blocks to specific fonts ahead of the
/// regular cascade (observed via Chrome-measured advances).
fn block_font(c: char) -> Option<&'static str> {
    match c as u32 {
        // Super/subscripts → Helvetica.
        0x2070..=0x209F => Some(HELVETICA),
        // Mathematical operators → Apple Symbols.
        0x2200..=0x22FF => Some(APPLE_SYMBOLS),
        _ => None,
    }
}

/// Measures strings with the metrics of the mermaid default theme font.
#[derive(Clone)]
pub struct TextMeasurer {
    inner: Rc<MeasurerInner>,
}

impl std::fmt::Debug for TextMeasurer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextMeasurer")
            .field("units_per_em", &self.inner.units_per_em)
            .finish()
    }
}

struct MeasurerInner {
    data: Vec<u8>,
    /// Trebuchet MS Bold data, used while `bold` is set.
    bold_data: Vec<u8>,
    bold: std::cell::Cell<bool>,
    units_per_em: f64,
    fallbacks: Vec<Vec<u8>>,
    emoji_font: Option<Vec<u8>>,
    helvetica: Option<Vec<u8>>,
    apple_symbols: Option<Vec<u8>>,
    /// Cache of char -> (font index, glyph id, advance in px at 16px).
    advances: std::cell::RefCell<HashMap<char, (usize, u16, f64)>>,
}

impl TextMeasurer {
    /// Loads Trebuchet MS from the registry or the system, falling back to
    /// the embedded Cabin face (output then stops being byte-exact vs mmdc).
    #[must_use]
    pub fn new() -> Self {
        let hand_drawn = is_hand_drawn();
        let data = if hand_drawn {
            hand_drawn_font()
        } else {
            FONT_CANDIDATES
                .iter()
                .find_map(|p| read_font(p))
                .unwrap_or_else(|| CABIN_FALLBACK.to_vec())
        };
        let face = Face::parse(&data, 0).expect("valid font");
        let units_per_em = f64::from(face.units_per_em());
        let fallbacks = FALLBACK_FONTS
            .iter()
            .map(|p| read_font(p).unwrap_or_default())
            .collect();
        let emoji_font = read_font(APPLE_COLOR_EMOJI);
        let helvetica = read_font(HELVETICA);
        let apple_symbols = read_font(APPLE_SYMBOLS);
        let bold_data = if hand_drawn {
            data.clone()
        } else {
            BOLD_CANDIDATES
                .iter()
                .find_map(|p| read_font(p))
                .unwrap_or_default()
        };
        Self {
            inner: Rc::new(MeasurerInner {
                data,
                bold_data,
                bold: std::cell::Cell::new(false),
                units_per_em,
                fallbacks,
                emoji_font,
                helvetica,
                apple_symbols,
                advances: std::cell::RefCell::new(HashMap::new()),
            }),
        }
    }

    fn face(&self) -> Face<'_> {
        if self.inner.bold.get() && !self.inner.bold_data.is_empty() {
            Face::parse(&self.inner.bold_data, 0).expect("valid font")
        } else {
            Face::parse(&self.inner.data, 0).expect("valid font")
        }
    }

    /// Switches measurement to the bold face (class diagram titles).
    pub fn set_bold(&self, bold: bool) {
        self.inner.bold.set(bold);
        // The advance cache is face-specific.
        self.inner.advances.borrow_mut().clear();
    }

    /// Width in CSS pixels of `text` at `font_size`, matching Chrome's
    /// `getBoundingClientRect` on a shrink-to-fit block: the raw advance sum
    /// rounded up to Chrome's `LayoutUnit` (1/64 px).
    #[must_use]
    pub fn measure_width(&self, text: &str, font_size: f64) -> f64 {
        (self.measure_advance(text, font_size) * 64.0).ceil() / 64.0
    }

    /// Advance sum as SVG `getComputedTextLength` reports it: the exact
    /// advance+kern sum rounded half-up to the 1/64 px grid.
    #[must_use]
    pub fn measure_advance_svg(&self, text: &str, font_size: f64) -> f64 {
        let w = self.measure_advance(text, font_size);
        (w * 64.0).round() / 64.0
    }

    /// SVG text `getBBox` width with glyph ink (Trebuchet): the same
    /// model as [`SeqMeasurer::line_ink_width`] — advance run rounded
    /// half-up to 1/64, unioned with raw bbox-table glyph ink, no kerning.
    #[must_use]
    pub fn ink_width(&self, text: &str, font_size: f64) -> f64 {
        let face = self.face();
        let upem = self.inner.units_per_em;
        let mut pen = 0.0f64;
        let mut ink_left = 0.0f64;
        let mut ink_right = 0.0f64;
        for ch in text.chars() {
            let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
            if let Some(b) = face.glyph_bounding_box(gid) {
                ink_left = ink_left.min(pen + f64::from(b.x_min) * font_size / upem);
                ink_right = ink_right.max(pen + f64::from(b.x_max) * font_size / upem);
            }
            pen += f64::from(face.glyph_hor_advance(gid).unwrap_or(0)) * font_size / upem;
        }
        let pen_rounded = (pen * 64.0 + 0.5).floor() / 64.0;
        let width = pen_rounded.max(ink_right) - ink_left.min(0.0);
        #[allow(clippy::cast_possible_truncation)]
        f64::from(width as f32)
    }

    /// SVG text `getBBox` height (Trebuchet): the integer font box
    /// (round(ascender) + round(descender)) extended by glyph ink beyond it.
    #[must_use]
    pub fn ink_height(&self, text: &str, font_size: f64) -> f64 {
        let face = self.face();
        let upem = self.inner.units_per_em;
        // Trebuchet's hhea ascent/descent; in hand-drawn mode the face's own
        // metrics apply instead (the constants are Trebuchet-specific).
        let (asc, desc) = if is_hand_drawn() {
            (
                (f64::from(face.ascender()) * font_size / upem).round(),
                (f64::from(-i32::from(face.descender())) * font_size / upem).round(),
            )
        } else {
            (
                (1923.0 * font_size / 2048.0).round(),
                (455.0 * font_size / 2048.0).round(),
            )
        };
        let mut top = -asc;
        let mut bottom = desc;
        for ch in text.chars() {
            let Some(gid) = face.glyph_index(ch) else {
                continue;
            };
            if let Some(b) = face.glyph_bounding_box(gid) {
                top = top.min(-f64::from(b.y_max) * font_size / upem);
                bottom = bottom.max(-f64::from(b.y_min) * font_size / upem);
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        f64::from((bottom - top) as f32)
    }

    /// Raw advance-sum width in CSS pixels (no `LayoutUnit` snapping).
    #[must_use]
    pub fn measure_advance(&self, text: &str, font_size: f64) -> f64 {
        let face = self.face();
        let scale16 = font_size / 16.0;
        let mut total: f64 = 0.0;
        // Kerning applies only between consecutive glyphs of the primary font.
        let mut prev_glyph: Option<ttf_parser::GlyphId> = None;
        for ch in text.chars() {
            let (font_idx, gid, advance_px16) = self.char_advance(ch);
            let mut advance_px = advance_px16;
            if font_idx == 0 {
                let gid = ttf_parser::GlyphId(gid);
                if let Some(prev) = prev_glyph {
                    advance_px +=
                        f64::from(kern(&face, prev, gid)) * 16.0 / self.inner.units_per_em;
                }
                prev_glyph = Some(gid);
            } else {
                prev_glyph = None;
            }
            total += advance_px * scale16;
        }
        total
    }

    /// Advance of `ch` in px at 16px, resolved through the fallback cascade.
    fn char_advance(&self, ch: char) -> (usize, u16, f64) {
        if let Some(&hit) = self.inner.advances.borrow().get(&ch) {
            return hit;
        }
        let mut result: Option<(usize, u16, f64)> = None;
        let face = self.face();
        // Default-emoji-presentation characters render in Apple Color Emoji,
        // whose effective advance is 1.25em (sbix strike metrics).
        if is_emoji_presentation(ch)
            && let Some(data) = &self.inner.emoji_font
            && let Ok(fb) = Face::parse(data, 0)
            && let Some(gid) = fb.glyph_index(ch)
        {
            let adv = f64::from(fb.glyph_hor_advance(gid).unwrap_or(0)) * 16.0
                / f64::from(fb.units_per_em())
                * 1.25;
            result = Some((usize::MAX, gid.0, adv));
        } else if let Some(gid) = face.glyph_index(ch) {
            let adv = f64::from(face.glyph_hor_advance(gid).unwrap_or(0)) * 16.0
                / self.inner.units_per_em;
            result = Some((0, gid.0, adv));
        } else if let Some(path) = block_font(ch) {
            let data = if path == HELVETICA {
                &self.inner.helvetica
            } else {
                &self.inner.apple_symbols
            };
            if let Some(data) = data
                && let Ok(fb) = Face::parse(data, 0)
                && let Some(gid) = fb.glyph_index(ch)
            {
                let adv = f64::from(fb.glyph_hor_advance(gid).unwrap_or(0)) * 16.0
                    / f64::from(fb.units_per_em());
                result = Some((usize::MAX - 1, gid.0, adv));
            }
        }
        if result.is_none() && !is_emoji_presentation(ch) {
            for (i, data) in self.inner.fallbacks.iter().enumerate() {
                // CoreText routes box-drawing characters past Arial Unicode
                // to the CJK cascade.
                if i == ARIAL_UNICODE_IDX && is_box_drawing(ch) {
                    continue;
                }
                if data.is_empty() {
                    continue;
                }
                let Ok(fb) = Face::parse(data, 0) else {
                    continue;
                };
                if let Some(gid) = fb.glyph_index(ch) {
                    let adv = f64::from(fb.glyph_hor_advance(gid).unwrap_or(0)) * 16.0
                        / f64::from(fb.units_per_em());
                    result = Some((i + 1, gid.0, adv));
                    break;
                }
            }
        }
        // Last resort: the primary font's .notdef advance.
        let result = result.unwrap_or_else(|| {
            let adv = f64::from(face.glyph_hor_advance(ttf_parser::GlyphId(0)).unwrap_or(0)) * 16.0
                / self.inner.units_per_em;
            (0, 0, adv)
        });
        self.inner.advances.borrow_mut().insert(ch, result);
        result
    }
}

impl Default for TextMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

fn kern(face: &Face<'_>, left: ttf_parser::GlyphId, right: ttf_parser::GlyphId) -> i16 {
    let Some(kern_table) = face.tables().kern else {
        return 0;
    };
    for subtable in kern_table.subtables {
        if subtable.horizontal
            && let Some(value) = subtable.glyphs_kerning(left, right)
        {
            return value;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ground-truth values measured by Chrome (mmdc) for 16px Trebuchet MS.
    #[test]
    fn matches_chrome_measurements() {
        let m = TextMeasurer::new();
        let cases = [
            ("Start", 35.015_625),
            ("OK", 20.0),
            ("End", 26.234_375),
            ("Is it?", 32.531_25),
            ("Yes", 22.656_25),
            ("No", 18.796_875),
        ];
        for (text, expected) in cases {
            let actual = m.measure_width(text, 16.0);
            println!("{text}: actual={actual} expected={expected}");
        }
        for (text, expected) in cases {
            let actual = m.measure_width(text, 16.0);
            assert!(
                (actual - expected).abs() < 0.01,
                "{text}: got {actual}, want {expected}"
            );
        }
    }
}

/// Measurement for sequence diagrams. The configured font families are
/// undefined at runtime, and mermaid's measuring SVG sits outside the
/// styled `#id` element, so Chrome measures with its DEFAULT font —
/// Times New Roman on macOS.
#[derive(Debug)]
pub struct SeqMeasurer {
    data: Vec<u8>,
}

impl Default for SeqMeasurer {
    fn default() -> Self {
        Self::new()
    }
}

impl SeqMeasurer {
    /// Loads Times New Roman from the registry or the system, falling back
    /// to the embedded Tinos face (Times-metric-compatible, not byte-exact).
    #[must_use]
    pub fn new() -> Self {
        let data = if is_hand_drawn() {
            hand_drawn_font()
        } else {
            TIMES_CANDIDATES
                .iter()
                .find_map(|p| read_font(p))
                .unwrap_or_else(|| TINOS_FALLBACK.to_vec())
        };
        Self { data }
    }

    /// Measurer for the ER/class box metrics, where upstream mermaid
    /// measures via Chrome's *default* font (Times) while the labels draw
    /// in the theme font. With the real Times present that mismatch is
    /// replicated for byte-exactness. On fallback hosts the drawn face is
    /// Cabin (see `render::css::fallback_font_css`) — measuring there with
    /// Tinos would track neither font, so measure with the drawing face
    /// instead (Cabin, or Excalifont in hand-drawn mode).
    #[must_use]
    pub fn for_ink() -> Self {
        let data = if is_hand_drawn() {
            hand_drawn_font()
        } else {
            TIMES_CANDIDATES
                .iter()
                .find_map(|p| read_font(p))
                .unwrap_or_else(|| CABIN_FALLBACK.to_vec())
        };
        Self { data }
    }

    fn face(&self) -> Face<'_> {
        Face::parse(&self.data, 0).expect("valid font")
    }

    /// SVG text bbox width: the advance+kern sum rounded half-up to the
    /// 1/64px grid, as f32.
    #[must_use]
    pub fn line_width(&self, text: &str, font_size: f64) -> f64 {
        let face = self.face();
        let upem = f64::from(face.units_per_em());
        let mut advance = 0.0f64;
        let mut prev: Option<ttf_parser::GlyphId> = None;
        for ch in text.chars() {
            let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
            advance += f64::from(face.glyph_hor_advance(gid).unwrap_or(0)) * font_size / upem;
            if let Some(p) = prev {
                advance += f64::from(kern(&face, p, gid)) * font_size / upem;
            }
            prev = Some(gid);
        }
        let snapped = (advance * 64.0).round() / 64.0;
        #[allow(clippy::cast_possible_truncation)]
        f64::from(snapped as f32)
    }

    /// SVG text bbox width with glyph ink: `max(advance run end, ink
    /// right) - min(0, ink left)`, snapped to the 1/64px grid as f32.
    /// (Blink's getBBox unions glyph ink with the run advance; `PK` in
    /// Times is wider than its advance sum because of K's right ink.)
    #[must_use]
    pub fn line_ink_width(&self, text: &str, font_size: f64) -> f64 {
        let face = self.face();
        let upem = f64::from(face.units_per_em());
        let mut pen = 0.0f64;
        let mut ink_left = 0.0f64;
        let mut ink_right = 0.0f64;
        for ch in text.chars() {
            let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
            if let Some(b) = face.glyph_bounding_box(gid) {
                ink_left = ink_left.min(pen + f64::from(b.x_min) * font_size / upem);
                ink_right = ink_right.max(pen + f64::from(b.x_max) * font_size / upem);
            }
            pen += f64::from(face.glyph_hor_advance(gid).unwrap_or(0)) * font_size / upem;
        }
        // The advance-run end is rounded half-up to the 1/64px grid; glyph
        // ink positions are used raw (1/128 grid at 16px/2048upem).
        let pen_rounded = (pen * 64.0 + 0.5).floor() / 64.0;
        let width = pen_rounded.max(ink_right) - ink_left.min(0.0);
        #[allow(clippy::cast_possible_truncation)]
        f64::from(width as f32)
    }

    /// `utils.calculateTextDimensions` with the ink-aware width (ER boxes):
    /// width = max over lines of Math.round(ink bbox width); height = sum of
    /// rounded line heights.
    #[must_use]
    pub fn ink_text_dimensions(&self, text: &str, font_size: f64) -> (f64, f64) {
        let lines = split_breaks(text);
        let mut width = 0.0f64;
        let mut height = 0.0f64;
        for line in &lines {
            let w = (self.line_ink_width(line, font_size) + 0.5).floor();
            width = width.max(w);
            height += (self.line_bbox_height(line, font_size) + 0.5).floor();
        }
        (width, height)
    }

    /// SVG text bbox height: the integer font box (round(ascender) +
    /// round(descender)) extended by glyph ink that reaches beyond it.
    #[must_use]
    pub fn line_bbox_height(&self, text: &str, font_size: f64) -> f64 {
        let face = self.face();
        let upem = f64::from(face.units_per_em());
        let asc = (f64::from(face.ascender()) * font_size / upem).round();
        let desc = (f64::from(-i32::from(face.descender())) * font_size / upem).round();
        let mut top = -asc;
        let mut bottom = desc;
        for ch in text.chars() {
            let Some(gid) = face.glyph_index(ch) else {
                continue;
            };
            if let Some(b) = face.glyph_bounding_box(gid) {
                let ink_top = -f64::from(b.y_max) * font_size / upem;
                let ink_bottom = -f64::from(b.y_min) * font_size / upem;
                top = top.min(ink_top);
                bottom = bottom.max(ink_bottom);
            }
        }
        #[allow(clippy::cast_possible_truncation)]
        f64::from((bottom - top) as f32)
    }

    /// Integer line height (`Math.round(bbox.height)`), per line.
    #[must_use]
    pub fn line_height(&self, text: &str, font_size: f64) -> f64 {
        (self.line_bbox_height(text, font_size) + 0.5).floor()
    }

    /// `utils.calculateTextDimensions`: width = max over lines of
    /// Math.round(bbox width); height = sum of rounded line heights.
    #[must_use]
    pub fn text_dimensions(&self, text: &str, font_size: f64) -> (f64, f64) {
        let lines = split_breaks(text);
        let mut width = 0.0f64;
        let mut height = 0.0f64;
        for line in &lines {
            let w = (self.line_width(line, font_size) + 0.5).floor();
            width = width.max(w);
            height += self.line_height(line, font_size);
        }
        (width, height)
    }
}

/// `common.splitBreaks`: split on `<br\s*/?>` (case-insensitive).
#[must_use]
pub fn split_breaks(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let lower = text.to_lowercase();
    let mut rest = 0usize;
    let mut idx = 0usize;
    while let Some(found) = lower[idx..].find("<br") {
        let abs = idx + found;
        // find closing '>' allowing whitespace and '/'.
        let tail = &text[abs + 3..];
        let close = tail.find('>');
        if let Some(c) = close {
            let inner = &tail[..c];
            if inner.trim().is_empty() || inner.trim() == "/" {
                lines.push(text[rest..abs].to_owned());
                rest = abs + 3 + c + 1;
                idx = rest;
                continue;
            }
        }
        idx = abs + 3;
    }
    lines.push(text[rest..].to_owned());
    lines
}

impl TextMeasurer {
    /// Trebuchet x-height in px at `font_size` (CSS `ex` unit).
    ///
    #[must_use]
    pub fn x_height_px(&self, font_size: f64) -> f64 {
        let face = self.face();
        // Trebuchet has no OS/2 sxHeight; Blink falls back to the ink
        // height of the 'x' glyph.
        let units = face.x_height().map_or_else(
            || {
                face.glyph_index('x')
                    .and_then(|g| face.glyph_bounding_box(g))
                    .map_or(0, |b| b.y_max)
            },
            i16::from,
        );
        f64::from(units) * font_size / f64::from(face.units_per_em())
    }

    /// Bold-face integer ascent/descent and svg advance for `text` at
    /// `font_size` (used by the timeline title); embedded Cabin when
    /// Trebuchet MS Bold is unavailable.
    #[must_use]
    pub fn bold_metrics(&self, text: &str, font_size: f64) -> (f64, f64, f64) {
        let data = if is_hand_drawn() {
            hand_drawn_font()
        } else {
            BOLD_CANDIDATES
                .iter()
                .find_map(|p| read_font(p))
                .unwrap_or_else(|| CABIN_FALLBACK.to_vec())
        };
        let face = Face::parse(&data, 0).expect("valid font");
        let upem = f64::from(face.units_per_em());
        let asc = (f64::from(face.ascender()) * font_size / upem).round();
        let desc = (f64::from(-i32::from(face.descender())) * font_size / upem).round();
        let mut advance = 0.0f64;
        let mut prev: Option<ttf_parser::GlyphId> = None;
        for ch in text.chars() {
            let gid = face.glyph_index(ch).unwrap_or(ttf_parser::GlyphId(0));
            advance += f64::from(face.glyph_hor_advance(gid).unwrap_or(0)) * font_size / upem;
            if let Some(p) = prev {
                advance += f64::from(kern(&face, p, gid)) * font_size / upem;
            }
            prev = Some(gid);
        }
        let snapped = (advance * 64.0).round() / 64.0;
        #[allow(clippy::cast_possible_truncation)]
        (asc, desc, f64::from(snapped as f32))
    }
}
