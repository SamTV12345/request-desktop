//! The design system: tokens, the theme, and every shared building block.
//!
//! Its own crate so there is exactly one place to change how the app looks. Before
//! this, the pieces were spread across five files in `crates/app` and several
//! existed **twice** — `button` and `primary_button` in both `main.rs` and
//! `track.rs`, `chip` in `custom/mod.rs` and `track.rs`, `section` in `main.rs` and
//! `filters.rs`, `note` in two places. Two copies of a button is two things to
//! restyle and one of them gets forgotten, which is how a window ends up with two
//! slightly different buttons on the same row.
//!
//! # What belongs here
//!
//! Anything that is *how the app looks* rather than *what the app does*. A token, a
//! colour, a shape, a control, a row, a piece of chrome. A calendar's arithmetic, a
//! request's execution, a timer's rules — those belong to their own crates and are
//! the reason this one depends on **gpui and nothing else**. If a component needs
//! `rp_store` or `rp_core` to be drawn, it is not a component; pass it the strings
//! and numbers it should render.
//!
//! # Layout
//!
//! | Module | What |
//! | ------ | ---- |
//! | [`tokens`] | the scale: `space`, `radius`, `text`, `size`, `control` |
//! | [`theme`] | [`Theme`], the ramps, the ground switch, the colour roles |
//! | [`icon`] | the embedded icon set and the one correct way to tint it |
//! | [`button`] | every pressable thing, in one family with a [`Tone`] |
//! | [`layout`] | card, divider, section label, sidebar, empty state |
//! | [`data`] | tables, rows, chips, tags — how a value is shown |
//! | [`feedback`] | headings, hints, notes, banners, tooltips |
//! | [`input`] | the single-line field and the multi-line editor |
//! | [`shortcut`] | key chords, spelled per platform, label **and** binding |
//! | [`search_bar`] | the "jump to anything" bar — and the props pattern, explained |
//! | [`fonts`] | the three design faces, resolved against what is installed |
//!
//! # Two rules that are not obvious and cost hours
//!
//! 1. **`icon_tinted`, always.** gpui does not inherit `text_color` into an `svg()`,
//!    so a bare `svg()` renders an invisible hole and reports nothing. Every icon
//!    in the app goes through [`icon::icon_tinted`].
//! 2. **`min_w_0` on every link of a flex chain that must shrink.** A flex child
//!    does not go under its intrinsic width unless allowed to, in gpui as in CSS,
//!    and allowing only the innermost one is not enough — the parent keeps the sum
//!    and the row runs off the window. This has been the cause of four separate
//!    "it hangs off the right edge" bugs in this app.
pub mod assets;
pub mod button;
pub mod data;
pub mod feedback;
pub mod fonts;
pub mod icon;
pub mod input;
pub mod layout;
pub mod search_bar;
pub mod shortcut;
pub mod theme;
pub mod tokens;

pub use assets::Assets;
pub use button::Tone;
pub use fonts::{Fonts, fonts, resolve_fonts};
pub use search_bar::SearchBar;
pub use shortcut::{Chord, Platform};
pub use theme::{ActiveTheme, Theme, set_theme, theme};

/// Register every key binding the input widgets need. Call once at startup.
///
/// One call rather than two, so adding a third widget with bindings does not mean
/// finding the startup path again.
pub fn bind_keys(cx: &mut gpui::App) {
    input::line::bind_keys(cx);
    input::document::bind_keys(cx);
}
