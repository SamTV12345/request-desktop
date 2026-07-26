//! Design tokens, from the user's **Ant Design System** project on
//! claude.ai/design (`colors_and_type.css`).
//!
//! The brand and neutral palettes are hex-true to that file. The *semantic*
//! layer there is light (`--bg-container: #FFFFFF`); this app is dark, so the
//! semantic names below map onto Ant's dark equivalents built from the same
//! neutral scale. That mapping is derived, not quoted — it is the one part of
//! this file that is not verbatim from the design system, and it is why every
//! semantic token below names the palette step it came from.
//!
//! Rules for using this module:
//!
//! * **Never write a literal colour in a view.** If a shade is missing, add a
//!   token here so it exists once.
//! * **Use the scales.** Padding and gaps come from [`space`], text from
//!   [`text`], corners from [`radius`], control heights from [`control`]. Ant is
//!   a 4-px system and it only looks tidy if everything lands on the grid.
//! * Radii are small on purpose: Ant's default is 2 px, not the 6–8 px of a
//!   consumer app.
// A token set legitimately holds shades a given screen does not use; the point
// of the palette is that the value exists once and is named. Unused *views* are
// still caught, this only exempts the tokens.
#![allow(dead_code)]

use gpui::{ParentElement, Rgba, SharedString, Styled, div, px, svg};

// ---------------------------------------------------------------------------
// Palette — verbatim from the design system.
// ---------------------------------------------------------------------------

/// `gpui::rgb` is not const, so the conversion is inlined.
const fn c(hex: u32) -> Rgba {
    let [_, r, g, b] = hex.to_be_bytes();
    Rgba { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
}

/// White at the given opacity — Ant expresses foreground as translucent ink.
const fn ink(alpha: f32) -> Rgba {
    Rgba { r: 1.0, g: 1.0, b: 1.0, a: alpha }
}

/// Black at the given opacity, for fills over a light-ish surface.
const fn shade(alpha: f32) -> Rgba {
    Rgba { r: 0.0, g: 0.0, b: 0.0, a: alpha }
}

/// Daybreak Blue — the primary palette.
pub mod blue {
    use super::{Rgba, c};
    pub const S1: Rgba = c(0xE6F7FF);
    pub const S3: Rgba = c(0x91D5FF);
    pub const S5: Rgba = c(0x40A9FF);
    pub const S6: Rgba = c(0x1890FF);
    pub const S7: Rgba = c(0x096DD9);
}

/// The status and method palettes, at the steps this app actually uses.
pub mod hue {
    use super::{Rgba, c};
    pub const RED_4: Rgba = c(0xFF7875);
    pub const RED_5: Rgba = c(0xFF4D4F);
    pub const RED_6: Rgba = c(0xF5222D);
    pub const VOLCANO_6: Rgba = c(0xFA541C);
    pub const ORANGE_6: Rgba = c(0xFA8C16);
    pub const GOLD_5: Rgba = c(0xFFC53D);
    pub const GOLD_6: Rgba = c(0xFAAD14);
    pub const GREEN_5: Rgba = c(0x73D13D);
    pub const GREEN_6: Rgba = c(0x52C41A);
    pub const CYAN_6: Rgba = c(0x13C2C2);
    pub const GEEKBLUE_6: Rgba = c(0x2F54EB);
    pub const PURPLE_5: Rgba = c(0x9254DE);
    pub const PURPLE_6: Rgba = c(0x722ED1);
    pub const MAGENTA_6: Rgba = c(0xEB2F96);
}

/// Neutrals, 13 steps as in the design system.
pub mod gray {
    use super::{Rgba, c};
    pub const S1: Rgba = c(0xFFFFFF);
    pub const S5: Rgba = c(0xD9D9D9);
    pub const S7: Rgba = c(0x8C8C8C);
    pub const S8: Rgba = c(0x595959);
    pub const S9: Rgba = c(0x434343);
    pub const S10: Rgba = c(0x262626);
    pub const S11: Rgba = c(0x1F1F1F);
    pub const S12: Rgba = c(0x141414);
    pub const S13: Rgba = c(0x000000);
}

// ---------------------------------------------------------------------------
// Semantic layer — the derived dark mapping.
// ---------------------------------------------------------------------------

/// Window background, behind everything. Ant dark layout = gray-13.
pub const LAYOUT: Rgba = gray::S13;
/// Default surface for panes and lists. Ant dark container = gray-12.
pub const SURFACE: Rgba = gray::S12;
/// A surface that sits above another: cards, popovers, active tabs. gray-11.
pub const RAISED: Rgba = gray::S11;
/// Sunken areas such as the rail and inputs.
pub const SUNKEN: Rgba = gray::S13;

/// Subtle wash for hover, from Ant's translucent `--bg-fill` idea.
pub const FILL: Rgba = ink(0.04);
/// A stronger wash: pressed, or a selected row.
pub const FILL_STRONG: Rgba = ink(0.08);
/// Overlay behind a modal.
pub const MASK: Rgba = shade(0.45);

/// Default border. Ant dark border = gray-9.
pub const BORDER: Rgba = gray::S9;
/// Dividers and hairlines inside a surface — quieter than a real border.
pub const DIVIDER: Rgba = ink(0.12);

pub const TEXT: Rgba = ink(0.85);
pub const TEXT_SECONDARY: Rgba = ink(0.65);
pub const TEXT_TERTIARY: Rgba = ink(0.45);
pub const TEXT_DISABLED: Rgba = ink(0.25);
pub const TEXT_ON_ACCENT: Rgba = gray::S1;

pub const ACCENT: Rgba = blue::S6;
pub const ACCENT_HOVER: Rgba = blue::S5;
pub const ACCENT_ACTIVE: Rgba = blue::S7;
/// Tinted background for a selected/primary-tinted element.
pub const ACCENT_WASH: Rgba = Rgba { r: 0.094, g: 0.565, b: 1.0, a: 0.16 };

pub const SUCCESS: Rgba = hue::GREEN_6;
pub const WARNING: Rgba = hue::GOLD_6;
pub const DANGER: Rgba = hue::RED_5;
pub const DANGER_HOVER: Rgba = hue::RED_4;
pub const DANGER_ACTIVE: Rgba = hue::RED_6;
pub const INFO: Rgba = blue::S6;

/// Tinted surfaces for the status roles, the counterpart to [`ACCENT_WASH`].
///
/// Ant's light theme uses the `-1` step of each palette for this. On a dark
/// surface a flat pastel would glow, so these are the `-6` hue at low opacity,
/// which keeps the tint reading as the same colour as its text.
pub const fn wash(base: Rgba) -> Rgba {
    Rgba { a: 0.16, ..base }
}

/// Fully transparent, for a border that must occupy space without being seen.
///
/// Painting such a border in the surface colour behind it also works until the
/// surface changes; this does not.
pub const TRANSPARENT: Rgba = Rgba { r: 0., g: 0., b: 0., a: 0. };

/// A hairline. Named because `border_1()` and a manual `px(1.)` divider must
/// agree, and because it is a border width, not a spacing value.
pub const HAIRLINE: gpui::Pixels = px(1.);
pub const SUCCESS_WASH: Rgba = wash(hue::GREEN_6);
pub const WARNING_WASH: Rgba = wash(hue::GOLD_6);
pub const DANGER_WASH: Rgba = wash(hue::RED_5);

/// Focus outline. Ant draws focus as a ring in the primary hue; a control that
/// has keyboard focus must be distinguishable from one that is merely hovered.
pub const FOCUS: Rgba = blue::S5;

/// Size of a status dot, e.g. "a timer is running". Not a spacing value, which
/// is why it is named rather than borrowed from [`space`].
pub const DOT: gpui::Pixels = px(8.);

// ---------------------------------------------------------------------------
// Scales.
// ---------------------------------------------------------------------------

/// Ant's 4-px spacing scale. Use these, not ad-hoc numbers.
pub mod space {
    use gpui::{Pixels, px};
    pub const XXS: Pixels = px(4.);
    pub const XS: Pixels = px(8.);
    pub const SM: Pixels = px(12.);
    pub const MD: Pixels = px(16.);
    pub const LG: Pixels = px(20.);
    pub const XL: Pixels = px(24.);
    pub const XXL: Pixels = px(32.);
}

/// Type scale. `SIZE`/`LINE` pairs, so line height never has to be guessed.
pub mod text {
    use gpui::{Pixels, px};
    pub const H3: Pixels = px(24.);
    pub const H3_LINE: Pixels = px(32.);
    pub const H4: Pixels = px(20.);
    pub const H4_LINE: Pixels = px(28.);
    pub const H5: Pixels = px(16.);
    pub const H5_LINE: Pixels = px(24.);
    pub const BODY: Pixels = px(14.);
    pub const BODY_LINE: Pixels = px(22.);
    pub const SMALL: Pixels = px(12.);
    pub const SMALL_LINE: Pixels = px(20.);
    /// Above Ant's own scale, for a hero readout — a stopwatch or a pressed
    /// key, where the number *is* the screen. Ant has no such step because it is
    /// a web component library, not a dashboard.
    pub const DISPLAY: Pixels = px(48.);
    pub const DISPLAY_LINE: Pixels = px(56.);
}

/// Fixed element dimensions: panel widths, aligned columns, reserved heights.
///
/// Separate from [`space`] because a 320-px panel is not "80 units of padding";
/// multiplying a spacing token to reach a width reads as arithmetic rather than
/// intent. All multiples of 4, so they land on the same grid.
pub mod size {
    use gpui::{Pixels, px};
    /// Left icon rail.
    pub const RAIL: Pixels = px(160.);
    /// Sidebar list column.
    pub const SIDEBAR: Pixels = px(260.);
    /// Wider sidebar, for the tracker's controls.
    pub const SIDEBAR_WIDE: Pixels = px(320.);
    /// Method badge, wide enough for `OPTIONS`.
    pub const METHOD: Pixels = px(56.);
    /// Label column in a two-column metadata list.
    pub const LABEL: Pixels = px(160.);
    /// Tab title before truncation.
    pub const TAB_TITLE: Pixels = px(140.);
}

/// Ant's radii. The default really is 2 px.
pub mod radius {
    use gpui::{Pixels, px};
    pub const XS: Pixels = px(2.);
    pub const SM: Pixels = px(4.);
    pub const MD: Pixels = px(6.);
    pub const LG: Pixels = px(8.);
    pub const PILL: Pixels = px(100.);
}

/// Control heights, so buttons, inputs and chips line up.
pub mod control {
    use gpui::{Pixels, px};
    pub const SM: Pixels = px(24.);
    pub const MD: Pixels = px(32.);
    pub const LG: Pixels = px(40.);
}

/// Font stacks from the design system's `--font-sans` / `--font-mono`, reduced
/// to what a desktop is likely to have. Request and response bodies are
/// monospaced: alignment carries meaning there.
pub const FONT_UI: &str = "Segoe UI";
pub const FONT_MONO: &str = "Consolas";

// ---------------------------------------------------------------------------
// Building blocks.
// ---------------------------------------------------------------------------

/// An icon from the embedded set, tinted by the current text colour.
///
/// `name` is a file stem under `assets/icons`, e.g. `"send"`. Sized to the body
/// line height so it sits on the text baseline grid.
pub fn icon(name: &'static str) -> gpui::Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(16.))
        .flex_none()
}

pub fn icon_sized(name: &'static str, size: gpui::Pixels) -> gpui::Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(size)
        .flex_none()
}

/// An icon with its colour set on the element itself.
///
/// gpui resolves `style.text.color` from the element's own refinement, not by
/// inheriting from the parent, so an `svg()` inside a coloured container paints
/// nothing. Every call site would otherwise have to repeat `.text_color(...)`,
/// and forgetting it yields an invisible hole with no error — use this.
pub fn icon_tinted(name: &'static str, size: gpui::Pixels, color: Rgba) -> gpui::Svg {
    icon_sized(name, size).text_color(color)
}

/// A raised card: the default container for a group of related controls.
pub fn card() -> gpui::Div {
    div()
        .bg(RAISED)
        .border_1()
        .border_color(BORDER)
        .rounded(radius::SM)
        .p(space::SM)
}

/// A one-pixel horizontal rule.
pub fn divider() -> gpui::Div {
    div().h(px(1.)).w_full().bg(DIVIDER)
}

/// An uppercase section label, for grouping a sidebar or a form.
pub fn section_label(label: impl Into<SharedString>) -> gpui::Div {
    div()
        .px(space::SM)
        .pt(space::SM)
        .pb(space::XXS)
        .text_size(text::SMALL)
        .line_height(text::SMALL_LINE)
        .text_color(TEXT_TERTIARY)
        .child(label.into())
}

/// The colour Ant gives each HTTP method. Distinct hues, not shades of one.
pub fn method_color(method: &str) -> Rgba {
    match method.to_ascii_uppercase().as_str() {
        "GET" => hue::GREEN_6,
        "POST" => hue::GOLD_6,
        "PUT" => blue::S6,
        "PATCH" => hue::PURPLE_6,
        "DELETE" => hue::RED_5,
        "HEAD" => hue::CYAN_6,
        "OPTIONS" => hue::GEEKBLUE_6,
        _ => hue::MAGENTA_6,
    }
}

/// Status-code colour, following Ant's success/info/warning/error roles.
pub fn status_color(status: u16) -> Rgba {
    match status {
        200..=299 => SUCCESS,
        300..=399 => INFO,
        400..=499 => WARNING,
        _ => DANGER,
    }
}

// ---------------------------------------------------------------------------
// Compatibility aliases.
// ---------------------------------------------------------------------------
// ponytail: the old flat names, so every view keeps compiling while it is
// migrated onto the tokens above. Delete each one as its last call site goes.

pub const BG: Rgba = SURFACE;
pub const BG_PANEL: Rgba = RAISED;
pub const BG_RAIL: Rgba = SUNKEN;
pub const BG_HOVER: Rgba = FILL;
pub const BG_ACTIVE: Rgba = FILL_STRONG;
pub const TEXT_DIM: Rgba = TEXT_TERTIARY;
