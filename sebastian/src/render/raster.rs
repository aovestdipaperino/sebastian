//! SVG → PNG rasterization (resvg + an embedded font). Gated behind the
//! `raster` feature so pure-SVG consumers don't pull in resvg.
//!
//! sebastian emits the mermaid-standard `trebuchet ms, verdana, arial,
//! sans-serif` font stack. Two font strategies are offered via [`FontSource`]:
//!
//! - [`FontSource::Embedded`] (default): load only the bundled Cabin face and
//!   point every generic family at it, so the stack falls through to Cabin.
//!   Output is self-contained and deterministic across machines — no Trebuchet
//!   install required. This is what downstream tools (e.g. mex) want.
//! - [`FontSource::System`]: load the system fonts and leave family resolution
//!   to usvg, so `trebuchet ms` resolves to the installed face. This reproduces
//!   what mermaid-cli's Chrome renders and is the mode to use for **pixel-perfect
//!   raster comparison against the mermaid TypeScript output**.

use resvg::usvg;

/// Cabin (SIL OFL 1.1, see `fonts/OFL.txt`), embedded for self-contained,
/// deterministic text rendering.
const CABIN_FONT: &[u8] = include_bytes!("../../fonts/Cabin.ttf");

/// Which fonts the rasterizer resolves the SVG's font stack against.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FontSource {
    /// Use only the bundled Cabin font — self-contained and deterministic.
    #[default]
    Embedded,
    /// Load the system fonts so the mermaid-standard `trebuchet ms, ...` stack
    /// resolves to the installed faces. Use for pixel-perfect comparison against
    /// mermaid-cli (mmdc), whose headless Chrome renders the same stack.
    System,
}

/// Failure rasterizing an SVG to PNG.
#[derive(Debug)]
pub enum RasterError {
    /// The SVG could not be parsed.
    Parse(String),
    /// The pixmap could not be allocated or PNG-encoded.
    Encode(String),
}

impl std::fmt::Display for RasterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RasterError::Parse(m) => write!(f, "SVG parse error: {m}"),
            RasterError::Encode(m) => write!(f, "PNG encode error: {m}"),
        }
    }
}

impl std::error::Error for RasterError {}

/// Options for [`rasterize_svg`].
#[derive(Debug, Clone)]
pub struct RasterOptions {
    /// Canvas background fill as RGBA; `None` leaves it transparent.
    pub background: Option<[u8; 4]>,
    /// Extra blank height (px) added below the diagram, e.g. for a footer band.
    pub extra_height: u32,
    /// An optional SVG composited on top after the diagram (authored for the
    /// full `width × (height + extra_height)` canvas) — e.g. a caller-supplied
    /// watermark sitting in the extra band.
    pub overlay_svg: Option<String>,
    /// Which fonts to resolve the SVG's font stack against.
    pub fonts: FontSource,
}

impl Default for RasterOptions {
    fn default() -> Self {
        Self {
            background: Some([255, 255, 255, 255]),
            extra_height: 0,
            overlay_svg: None,
            fonts: FontSource::Embedded,
        }
    }
}

/// usvg options for the chosen [`FontSource`].
///
/// `Embedded` registers only the bundled Cabin face and points every generic
/// family at it (so any stack falls through to Cabin). `System` loads the
/// installed fonts and leaves usvg's default family resolution in place, so
/// named families like `trebuchet ms` resolve exactly as mermaid-cli's Chrome
/// would — the basis for pixel-perfect comparison.
fn font_options(fonts: FontSource) -> usvg::Options<'static> {
    let mut fontdb = usvg::fontdb::Database::new();
    // usvg does not process @font-face rules, so the Excalifont face inlined
    // in hand-drawn SVGs must be registered here to rasterize.
    fontdb.load_font_data(crate::text::EXCALIFONT.to_vec());
    match fonts {
        FontSource::Embedded => {
            fontdb.load_font_data(CABIN_FONT.to_vec());
            // The generic families map to Cabin, not the Excalifont face
            // registered above (which is only reached by name).
            let family = fontdb
                .faces()
                .find(|f| f.families.iter().any(|(name, _)| name.contains("Cabin")))
                .and_then(|f| f.families.first().map(|(name, _)| name.clone()))
                .unwrap_or_else(|| "Cabin".to_string());
            fontdb.set_sans_serif_family(&family);
            fontdb.set_serif_family(&family);
            fontdb.set_cursive_family(&family);
            fontdb.set_fantasy_family(&family);
            fontdb.set_monospace_family(&family);
            usvg::Options {
                font_family: family,
                fontdb: std::sync::Arc::new(fontdb),
                ..usvg::Options::default()
            }
        }
        FontSource::System => {
            fontdb.load_system_fonts();
            usvg::Options {
                fontdb: std::sync::Arc::new(fontdb),
                ..usvg::Options::default()
            }
        }
    }
}

/// Rendered pixel size `(width, height)` of an SVG (ceil), using the embedded
/// font for layout. Useful for sizing an overlay before [`rasterize_svg`].
///
/// # Errors
/// Returns [`RasterError::Parse`] if the SVG cannot be parsed.
pub fn measure_svg(svg: &str) -> Result<(u32, u32), RasterError> {
    // Size comes from the SVG's root width/height (sebastian bakes the
    // Trebuchet-metric layout into them), so the font choice here is immaterial.
    let tree = usvg::Tree::from_str(svg, &font_options(FontSource::Embedded))
        .map_err(|e| RasterError::Parse(e.to_string()))?;
    let size = tree.size();
    Ok((size.width().ceil() as u32, size.height().ceil() as u32))
}

/// Rasterize an SVG to PNG bytes using the embedded font.
///
/// # Errors
/// Returns [`RasterError`] if the SVG cannot be parsed, the pixmap cannot be
/// allocated, or PNG encoding fails.
pub fn rasterize_svg(svg: &str, opts: &RasterOptions) -> Result<Vec<u8>, RasterError> {
    let options = font_options(opts.fonts);
    let tree =
        usvg::Tree::from_str(svg, &options).map_err(|e| RasterError::Parse(e.to_string()))?;
    let size = tree.size();
    let width = size.width().ceil() as u32;
    let height = size.height().ceil() as u32;
    let canvas_height = height + opts.extra_height;

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, canvas_height)
        .ok_or_else(|| RasterError::Encode("failed to allocate pixmap".to_string()))?;
    if let Some([r, g, b, a]) = opts.background {
        pixmap.fill(resvg::tiny_skia::Color::from_rgba8(r, g, b, a));
    }
    resvg::render(&tree, usvg::Transform::default(), &mut pixmap.as_mut());

    // Composite the caller's overlay (e.g. a watermark) over the full canvas.
    if let Some(Ok(ov)) = opts
        .overlay_svg
        .as_ref()
        .map(|o| usvg::Tree::from_str(o, &options))
    {
        resvg::render(&ov, usvg::Transform::default(), &mut pixmap.as_mut());
    }

    pixmap
        .encode_png()
        .map_err(|e| RasterError::Encode(e.to_string()))
}
