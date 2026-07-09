//! Transcendental math matching the rendering browser.
//!
//! Native builds use `core-math` (correctly rounded, matches Chrome's V8).
//! Its vendored C cannot be cross-compiled to wasm, so wasm builds fall back
//! to the pure-Rust `libm` — output there may differ from mmdc in the final
//! ULPs of some coordinates.

#[cfg(not(target_arch = "wasm32"))]
pub use core_math::{atan2, cos, pow, sin};

#[cfg(target_arch = "wasm32")]
pub use libm::{atan2, cos, pow, sin};
