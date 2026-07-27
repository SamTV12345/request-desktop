//! The two text surfaces.
//!
//! Siblings, not one widget with a `multiline: bool`. The single-line one is a
//! near-verbatim copy of gpui's own `examples/input.rs` and is worth keeping
//! re-syncable against it; the multi-line one needs a different element — one
//! `ShapedLine` per visual line, its own scroll offset — so the overlap between
//! them is cheaper than the flag would be.
pub mod document;
pub mod line;

pub use document::Editor;
pub use line::TextInput;
