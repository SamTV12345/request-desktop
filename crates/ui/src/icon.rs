//! The embedded icon set, and the one correct way to tint it.
//!
//! **Always [`icon_tinted`].** gpui does not inherit `text_color` into an `svg()`:
//! a bare `svg()` renders an invisible hole and reports no error, which is the most
//! expensive five minutes in this codebase. Every icon in the app goes through
//! here, and every asset carries `currentColor` so tinting is possible at all —
//! `crates/ui/src/assets.rs` has a test that asserts exactly that.
use gpui::{Pixels, Rgba, SharedString, Styled, px, svg};

pub fn icon(name: &'static str) -> gpui::Svg {
    icon_sized(name, px(16.))
}

pub fn icon_sized(name: &'static str, size: Pixels) -> gpui::Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(size)
        .flex_none()
}

/// An icon with its colour set on the element itself.
///
/// gpui resolves `style.text.color` from the element's own refinement rather
/// than inheriting it, so an `svg()` inside a coloured container paints nothing.
/// Forgetting the tint yields an invisible hole with no error — use this.
pub fn icon_tinted(name: &'static str, size: Pixels, color: Rgba) -> gpui::Svg {
    icon_sized(name, size).text_color(color)
}
