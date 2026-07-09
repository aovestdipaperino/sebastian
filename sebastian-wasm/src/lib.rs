//! WASM bindings for [sebastian](https://crates.io/crates/sebastian).
//!
//! Works out of the box via embedded SIL-OFL fallback faces (Cabin for
//! Trebuchet MS, Tinos for Times New Roman); for byte-exact-vs-mmdc output,
//! register the real font bytes before the first render — wasm has no
//! filesystem, so fonts only come from [`register_font`].
//!
//! Build with `wasm-pack build --target web` (or `--target nodejs`).

use wasm_bindgen::prelude::*;

/// Registers font bytes under a file name (e.g. `"Trebuchet MS.ttf"`),
/// overriding the embedded fallback faces. Call before [`render`] for
/// byte-exact-vs-mmdc output; without it, embedded Cabin/Tinos are used.
#[wasm_bindgen]
pub fn register_font(file_name: &str, data: &[u8]) {
    sebastian::text::register_font(file_name, data.to_vec());
}

/// Renders mermaid `source` to an SVG string. `id` becomes the SVG element
/// id (mermaid uses ids like `"mermaid-0"`).
///
/// # Errors
/// Returns a JS error with the parse/render failure message.
#[wasm_bindgen]
pub fn render(source: &str, id: &str) -> Result<String, JsError> {
    sebastian::render_diagram(source, id).map_err(|e| JsError::new(&e.to_string()))
}

/// The diagram type keyword sebastian detects for `source` (e.g.
/// `"flowchart"`, `"sequence"`).
#[must_use]
#[wasm_bindgen]
pub fn detect_diagram_type(source: &str) -> String {
    sebastian::detect_diagram_type(source).to_string()
}
