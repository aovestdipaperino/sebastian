//! Pixel-perfect Rust port of the [mermaid.js](https://mermaid.js.org)
//! diagram renderers (mermaid 11.15.0) — no browser, no JavaScript.
//!
//! For the byte-exact diagram types (flowchart, sequence, state, class,
//! gantt, pie, ER, gitGraph, timeline, journey, quadrant, xychart, packet,
//! radar, sankey, block, treemap, kanban) the output SVG is **byte-for-byte
//! identical** to mermaid-cli's, at native speed (roughly 30× faster than
//! `mmdc`, which launches a headless Chrome per render). Mindmap,
//! architecture, requirement and C4 render as approximate (non-byte-exact)
//! diagrams. See the [repository README](https://github.com/aovestdipaperino/sebastian)
//! for the full status table.
//!
//! # Quick start
//!
//! ```
//! let svg = sebastian::render_diagram(
//!     "flowchart TD\n    A[Start] --> B{Ready?}\n    B -- yes --> C[Go]",
//!     "my-svg", // the SVG element id
//! ).expect("valid diagram");
//! assert!(svg.starts_with("<svg"));
//! ```
//!
//! [`render_diagram`] auto-detects the diagram type ([`detect_diagram_type`])
//! and never panics: malformed input yields an `Err`. With the `raster`
//! feature, `render_png` rasterizes without a browser.
//!
//! # Fonts and byte-exactness
//!
//! Layout depends on text metrics. With the real fonts available —
//! Trebuchet MS everywhere, Times New Roman for sequence diagrams
//! (preinstalled on macOS/Windows) — output is byte-exact vs mermaid-cli.
//! Without them (bare Linux, wasm) sebastian falls back to embedded
//! SIL-OFL faces and output is well-proportioned but not byte-exact; hosts
//! can supply real font bytes at runtime via [`text::register_font`].
//!
//! The crate also compiles for `wasm32-unknown-unknown` (see the
//! `sebastian-wasm` workspace crate and the
//! [live demo](https://aovestdipaperino.github.io/sebastian/)).

pub mod architecture;
pub mod block;
pub mod c4;
pub mod classdiag;
pub mod dagre;
pub mod diagram;
pub mod er;
pub mod flowchart;
pub mod gantt;
pub mod gitgraph;
pub mod graphlib;
pub mod journey;
pub mod jsmap;
pub mod kanban;
mod mathx;
pub mod mindmap;
pub mod packet;
pub mod pie;
/// sebastian-original diagram types with no mermaid equivalent (see the
/// `mermaid-extensions` feature, on by default).
#[cfg(feature = "mermaid-extensions")]
pub mod pyramid;
pub mod quadrant;
pub mod radar;
pub mod render;
pub mod requirement;
pub mod sankey;
pub mod sequence;
pub mod state;
pub mod svg;
pub mod text;
pub mod timeline;
pub mod treemap;
pub mod xychart;

#[cfg(feature = "raster")]
pub use diagram::render_png;
pub use diagram::{
    detect_diagram_type, render_class, render_diagram, render_flowchart, render_state,
};
