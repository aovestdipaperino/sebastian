//! Pixel-perfect Rust port of the mermaid.js flowchart renderer.
//!
//! The crate ports the rendering pipeline used by mermaid v11 for
//! flowcharts: the flowchart DSL parser, the dagre layout engine (as bundled
//! in dagre-d3-es 7.0.14), and the SVG generation with mermaid's default
//! theme.

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
pub mod packet;
pub mod pie;
pub mod quadrant;
pub mod render;
pub mod sequence;
pub mod state;
pub mod svg;
pub mod text;
pub mod timeline;
pub mod xychart;

#[cfg(feature = "raster")]
pub use diagram::render_png;
pub use diagram::{
    detect_diagram_type, render_class, render_diagram, render_flowchart, render_state,
};
