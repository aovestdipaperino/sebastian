//! WASM bindings for [sebastian](https://crates.io/crates/sebastian).
//!
//! wasm has no filesystem, so the host must register font bytes before the
//! first render: `"Trebuchet MS.ttf"` is required, `"Trebuchet MS Bold.ttf"`
//! and `"Times New Roman.ttf"` unlock bold and sequence-diagram metrics, and
//! the remaining fallback faces (Verdana, Arial, …) are optional.
//!
//! Build with `wasm-pack build --target web` (or `--target nodejs`).

use wasm_bindgen::prelude::*;

/// Registers font bytes under a file name (e.g. `"Trebuchet MS.ttf"`).
/// Must be called before [`render`]; see the crate docs for which faces
/// are required.
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
