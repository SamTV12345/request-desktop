//! Design tokens, from the user's **Organic** design system on claude.ai/design
//! (project "App mit Requests, Tools und Tracking").
//!
//! Values are verbatim from `_ds/organic-…/styles.css` for the light ground and
//! from the `.theme-dark` override in `App Designs.dc.html` for the dark one.
//! Both grounds are specified by the design, which is why this is a runtime
//! [`Theme`] rather than a set of constants.
//!
//! Rules for using this module:
//!
//! * **Never write a literal colour or size in a view.** Read it off the theme,
//!   or add a field here so it exists once.
//! * **The scale is not a 4-px grid.** Organic steps by 4.4 px (`--space-1`),
//!   so `space::*` values look odd and must still be used verbatim — rounding
//!   them to 4 breaks their rhythm against the radii.
//! * **Small controls are pills.** `styles.css` ends by overriding buttons,
//!   tags, segmented controls and inputs to `999px`, and cards to
//!   `--radius-lg * 1.15`. That is the system's signature; a 2-px corner
//!   anywhere reads as a different product.
#![allow(dead_code)]

use gpui::{App, Global, ParentElement, Pixels, Rgba, SharedString, Styled, div, px, svg};

const fn c(hex: u32) -> Rgba {
    let [_, r, g, b] = hex.to_be_bytes();
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

/// The design expresses dividers and washes as `color-mix(… N%, transparent)`,
/// i.e. the ink colour at reduced alpha rather than a separate grey.
pub const fn alpha(base: Rgba, a: f32) -> Rgba {
    Rgba { a, ..base }
}

/// A nine-step tonal ramp. The dark ground inverts the order, so step 100 is
/// always "closest to the ground" and 900 "furthest from it".
#[derive(Clone, Copy)]
pub struct Ramp {
    pub s100: Rgba,
    pub s200: Rgba,
    pub s300: Rgba,
    pub s400: Rgba,
    pub s500: Rgba,
    pub s600: Rgba,
    pub s700: Rgba,
    pub s800: Rgba,
    pub s900: Rgba,
}

#[derive(Clone, Copy)]
pub struct Theme {
    pub dark: bool,
    /// Window ground.
    pub bg: Rgba,
    /// Cards, inputs, raised areas.
    pub surface: Rgba,
    pub text: Rgba,
    pub accent: Rgba,
    /// The olive secondary, for a second categorical role.
    pub accent_2: Rgba,
    pub divider: Rgba,
    pub neutral: Ramp,
    pub accent_ramp: Ramp,
    pub accent_2_ramp: Ramp,
}

/// `--space-*`, verbatim. Not a 4-px grid — see the module docs.
pub mod space {
    use gpui::{Pixels, px};
    pub const S1: Pixels = px(4.4);
    pub const S2: Pixels = px(8.8);
    pub const S3: Pixels = px(13.2);
    pub const S4: Pixels = px(17.6);
    /// 5 × 4.4. Absent from `--space-*` but used by the mockups (the title
    /// bar's left inset), so it is a real step rather than a rounded 22.
    pub const S5: Pixels = px(22.);
    pub const S6: Pixels = px(26.4);
    pub const S8: Pixels = px(35.2);

    // Migration shim: the previous scale was Ant's XXS..XXL. Mapped onto the
    // nearest Organic step so views keep building; delete as each file moves.
    pub const XXS: Pixels = S1;
    pub const XS: Pixels = S2;
    pub const SM: Pixels = S3;
    pub const MD: Pixels = S4;
    pub const LG: Pixels = px(22.);
    pub const XL: Pixels = S6;
    pub const XXL: Pixels = S8;
}

/// `--radius-*`, plus the two overrides `styles.css` applies at the end.
pub mod radius {
    use gpui::{Pixels, px};
    pub const SM: Pixels = px(8.);
    pub const MD: Pixels = px(16.);
    pub const LG: Pixels = px(28.);
    /// Buttons, tags, segmented controls and inputs.
    pub const PILL: Pixels = px(999.);
    /// Cards and dialogs: `--radius-lg * 1.15`.
    pub const CARD: Pixels = px(32.2);

    // Migration shim. NOTE: `XS` was Ant's 2-px corner; Organic puts every
    // small control on a pill, so a view still using XS looks like the old
    // system even once its colours are right. This is the alias that must go
    // first, not last.
    pub const XS: Pixels = PILL;
}

/// Type sizes from the system's element rules.
pub mod text {
    use gpui::{Pixels, px};
    pub const H1: Pixels = px(42.);
    pub const H2: Pixels = px(32.);
    pub const H3: Pixels = px(25.);
    pub const H4: Pixels = px(20.);
    pub const H5: Pixels = px(16.);
    /// `h6`: uppercase and letter-spaced in the system — a section label.
    pub const H6: Pixels = px(13.);
    /// `body`: 15px / 1.55.
    pub const BODY: Pixels = px(15.);
    pub const BODY_LINE: Pixels = px(23.25);
    /// Buttons and inputs share 14px so they align side by side.
    pub const CONTROL: Pixels = px(14.);
    pub const CARD_TITLE: Pixels = px(17.);
    /// `.card-body`, `.dialog-body`, `.seg-opt`.
    pub const SMALL: Pixels = px(13.);
    /// `.field > label`.
    pub const LABEL: Pixels = px(12.);
    /// `.tag`, `.card-meta`, `figcaption`.
    pub const TINY: Pixels = px(11.);
    /// `.card-kicker`.
    pub const KICKER: Pixels = px(10.);

    // Migration shim: the previous set carried a `_LINE` partner per size and a
    // DISPLAY step. Organic states leading as a ratio (1.12 headings, 1.55
    // body), so these are those ratios applied.
    pub const H3_LINE: Pixels = px(28.);
    pub const H4_LINE: Pixels = px(22.4);
    pub const H5_LINE: Pixels = px(18.);
    pub const SMALL_LINE: Pixels = px(20.);
    pub const DISPLAY: Pixels = px(48.);
    pub const DISPLAY_LINE: Pixels = px(54.);
}

/// Control geometry from `.input`, `.btn` and `.btn-icon`.
pub mod control {
    use gpui::{Pixels, px};
    /// `.input` and `.btn` minimum height.
    pub const MD: Pixels = px(36.);
    /// `.btn-icon` is a 36-px square.
    pub const ICON: Pixels = px(36.);

    // Migration shim: Organic states one control height, not three.
    pub const SM: Pixels = px(28.);
    pub const LG: Pixels = px(44.);
}

/// Fixed panel and column widths. App layout, not part of the design system.
pub mod size {
    use gpui::{Pixels, px};
    /// Icon rail. 104 in the mockups — was 176 before the design was read.
    pub const RAIL: Pixels = px(104.);
    /// A rail tile and its corner.
    pub const TILE_W: Pixels = px(76.);
    pub const TILE_H: Pixels = px(74.);
    pub const TILE_RADIUS: Pixels = px(24.);
    pub const RAIL_ICON: Pixels = px(22.);
    /// Custom title bar, its app mark, and its window controls.
    pub const TITLEBAR: Pixels = px(52.);
    pub const MARK: Pixels = px(22.);
    pub const WINDOW_CTRL_W: Pixels = px(40.);
    pub const WINDOW_CTRL_H: Pixels = px(32.);
    /// The Time screen's left column: a *starting* width, capped by the
    /// container. [`SIDEBAR`] doubles as its minimum.
    pub const COLUMN: Pixels = px(392.);

    // -- floors and starting sizes for flexible layout --
    // Measured, not guessed: a layout probe mounted each pane at 1280, 1000,
    // 760, 560, 420 and 300 px and read the real geometry back.

    /// Floor under a card or editor block. The old floors summed to more than a
    /// narrow pane had, which pushed siblings off screen.
    pub const CARD_MIN_H: Pixels = px(70.4);
    /// Minimum width of a URL or search field before its row is allowed to wrap.
    pub const URL_MIN: Pixels = px(140.8);
    /// Floor under a flexible table column. Without it the task column
    /// collapsed to zero at 760 px.
    pub const TABLE_COL_MIN_W: Pixels = px(70.);
    /// Starting size of a grid cell, with its shrink and growth bounds.
    pub const GRID_CELL: Pixels = px(246.4);
    pub const GRID_CELL_MIN: Pixels = px(140.8);
    pub const GRID_CELL_MAX: Pixels = px(316.8);
    pub const SIDEBAR: Pixels = px(264.);
    pub const SIDEBAR_WIDE: Pixels = px(316.);
    pub const METHOD: Pixels = px(62.);
    pub const LABEL: Pixels = px(158.);
    pub const TAB_TITLE: Pixels = px(140.);
}

pub const FONT_HEADING: &str = "Caprasimo";
pub const FONT_BODY: &str = "Figtree";
/// The design doc loads JetBrains Mono alongside the system fonts, for the
/// places where alignment carries meaning: request and response bodies, tool
/// output, timer digits.
pub const FONT_MONO: &str = "JetBrains Mono";

impl Theme {
    /// Light ground — `:root` in `styles.css`.
    pub fn light() -> Self {
        let text = c(0x201E1D);
        Self {
            dark: false,
            bg: c(0xF5EAD8),
            surface: c(0xEBDDC5),
            text,
            accent: c(0xC67139),
            accent_2: c(0x7A8A5E),
            divider: alpha(text, 0.16),
            neutral: Ramp {
                s100: c(0xF9F4ED),
                s200: c(0xEEE7DB),
                s300: c(0xDCD3C4),
                s400: c(0xC0B6A5),
                s500: c(0xA19786),
                s600: c(0x82796A),
                s700: c(0x645C50),
                s800: c(0x474238),
                s900: c(0x2E2B25),
            },
            accent_ramp: Ramp {
                s100: c(0xFFF2EB),
                s200: c(0xFFE1D0),
                s300: c(0xFFC6A5),
                s400: c(0xF6A06B),
                s500: c(0xD67F48),
                s600: c(0xB2622D),
                s700: c(0x8C491A),
                s800: c(0x643312),
                s900: c(0x402310),
            },
            accent_2_ramp: Ramp {
                s100: c(0xF0FAE1),
                s200: c(0xE1EECC),
                s300: c(0xCCDBB2),
                s400: c(0xAEBF92),
                s500: c(0x8FA073),
                s600: c(0x728157),
                s700: c(0x56633F),
                s800: c(0x3D472B),
                s900: c(0x272E1B),
            },
        }
    }

    /// Dark ground — the `.theme-dark` override in `App Designs.dc.html`. Its
    /// ramps are the light ones reversed, which is how the design defines them.
    pub fn dark() -> Self {
        let text = c(0xF3E8D6);
        let light = Self::light();
        Self {
            dark: true,
            bg: c(0x221E19),
            surface: c(0x2E2921),
            text,
            accent: c(0xE08D54),
            accent_2: c(0x9FB181),
            divider: alpha(text, 0.18),
            neutral: reverse(light.neutral),
            accent_ramp: reverse(light.accent_ramp),
            accent_2_ramp: reverse(light.accent_2_ramp),
        }
    }

    // -- derived roles, so views cannot re-derive them inconsistently --

    /// `.text-muted` — ink at 55%.
    pub fn text_muted(&self) -> Rgba {
        alpha(self.text, 0.55)
    }

    /// `.field > label` — 70%.
    pub fn text_label(&self) -> Rgba {
        alpha(self.text, 0.70)
    }

    /// `.card-meta` — 50%, the quietest readable step.
    pub fn text_meta(&self) -> Rgba {
        alpha(self.text, 0.50)
    }

    pub fn text_disabled(&self) -> Rgba {
        alpha(self.text, 0.30)
    }

    /// Foreground on an accent fill: the system uses the ground colour.
    pub fn on_accent(&self) -> Rgba {
        self.bg
    }

    /// `.btn-secondary:hover` — ink at 7%.
    pub fn hover(&self) -> Rgba {
        alpha(self.text, 0.07)
    }

    /// `.btn-secondary:active` — ink at 14%.
    pub fn active(&self) -> Rgba {
        alpha(self.text, 0.14)
    }

    /// `.btn-ghost:hover` — accent at 10%. Also the selected wash for chips.
    pub fn accent_wash(&self) -> Rgba {
        alpha(self.accent, 0.10)
    }

    pub fn accent_wash_strong(&self) -> Rgba {
        alpha(self.accent, 0.18)
    }

    /// `::selection` — accent at 30%.
    pub fn selection(&self) -> Rgba {
        alpha(self.accent, 0.30)
    }

    /// `.input:hover` border — ink at 45%.
    pub fn border_hover(&self) -> Rgba {
        alpha(self.text, 0.45)
    }

    /// Focus is an accent outline in this system.
    pub fn focus(&self) -> Rgba {
        self.accent
    }

    /// `.tag-*`: a tint from one end of a ramp with the far end as ink.
    pub fn tag_accent(&self) -> (Rgba, Rgba) {
        (self.accent_ramp.s100, self.accent_ramp.s800)
    }

    pub fn tag_accent_2(&self) -> (Rgba, Rgba) {
        (self.accent_2_ramp.s100, self.accent_2_ramp.s800)
    }

    pub fn tag_neutral(&self) -> (Rgba, Rgba) {
        (self.neutral.s100, self.neutral.s800)
    }

    /// Organic has no status palette, so these are the closest roles it does
    /// define: olive for good, the accent ramp for caution, and a red kept in
    /// the same warm family rather than a generic one that would look imported.
    pub fn success(&self) -> Rgba {
        self.accent_2
    }

    pub fn warning(&self) -> Rgba {
        self.accent_ramp.s500
    }

    pub fn danger(&self) -> Rgba {
        if self.dark { c(0xE07A6B) } else { c(0xA8402C) }
    }

    pub fn danger_wash(&self) -> Rgba {
        alpha(self.danger(), 0.12)
    }

    /// The success and warning washes, at the 16% the system's tokens use for
    /// both. Their counterpart for danger is 12%, which is the design's own
    /// asymmetry and not a slip here.
    pub fn success_wash(&self) -> Rgba {
        alpha(self.success(), 0.16)
    }

    pub fn warning_wash(&self) -> Rgba {
        alpha(self.warning(), 0.16)
    }
}

fn reverse(r: Ramp) -> Ramp {
    Ramp {
        s100: r.s900,
        s200: r.s800,
        s300: r.s700,
        s400: r.s600,
        s500: r.s500,
        s600: r.s400,
        s700: r.s300,
        s800: r.s200,
        s900: r.s100,
    }
}

/// The active theme, as a gpui global so every window agrees.
#[derive(Clone, Copy)]
pub struct ActiveTheme(pub Theme);

impl Global for ActiveTheme {}

/// The current theme. Defaults to the dark ground when nothing was set, since
/// that is what the app shipped with.
pub fn theme(cx: &App) -> Theme {
    cx.try_global::<ActiveTheme>()
        .map(|t| t.0)
        .unwrap_or_else(Theme::dark)
}

pub fn set_theme(dark: bool, cx: &mut App) {
    cx.set_global(ActiveTheme(if dark {
        Theme::dark()
    } else {
        Theme::light()
    }));
}

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

/// `.card` — a surface block with the system's generous corner.
pub fn card(t: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(space::S2)
        .p(space::S3)
        .rounded(radius::CARD)
        .bg(t.surface)
}

/// `.checkbox` — the box alone, without a label.
///
/// A boolean is a checkbox, not a two-option segment: a `false | true` track
/// asks the reader to compare two words where a tick answers at a glance, and it
/// cannot be laid out as a grid the way a permission matrix needs. The label is
/// the caller's, because a form puts it beside the box and a matrix puts it in
/// the row and column headers.
pub fn check_box(t: &Theme, checked: bool) -> gpui::Div {
    let box_ = div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(space::S4)
        .rounded(radius::SM)
        .border_1()
        .border_color(if checked { t.accent } else { t.divider });
    if !checked {
        return box_;
    }
    // The tick is tinted from the accent's own ink, so it stays legible on
    // either ground; gpui never inherits a colour into an `svg`.
    box_.bg(t.accent)
        .child(icon_tinted("check", text::SMALL, t.on_accent()))
}

/// `.hr`
pub fn divider(t: &Theme) -> gpui::Div {
    div().h(px(1.)).w_full().bg(t.divider)
}

/// `h6` as the system defines it: uppercase, letter-spaced, quiet.
pub fn section_label(t: &Theme, label: impl Into<SharedString>) -> gpui::Div {
    div()
        .px(space::S3)
        .pt(space::S3)
        .pb(space::S1)
        .text_size(text::H6)
        .font_family(FONT_HEADING)
        .text_color(t.text_muted())
        .child(label.into())
}

/// HTTP method colours from the two ramps the system provides. Organic has no
/// categorical palette, so methods are separated by family and step rather than
/// by a borrowed rainbow.
pub fn method_color(t: &Theme, method: &str) -> Rgba {
    match method.to_ascii_uppercase().as_str() {
        "GET" => t.accent_2,
        "POST" => t.accent,
        "PUT" => t.accent_2_ramp.s600,
        "PATCH" => t.accent_ramp.s600,
        "DELETE" => t.danger(),
        "HEAD" => t.neutral.s600,
        "OPTIONS" => t.neutral.s500,
        _ => t.text_muted(),
    }
}

pub fn status_color(t: &Theme, status: u16) -> Rgba {
    match status {
        200..=299 => t.success(),
        300..=399 => t.accent_2_ramp.s600,
        400..=499 => t.warning(),
        _ => t.danger(),
    }
}

// ---------------------------------------------------------------------------
// Migration shim.
// ---------------------------------------------------------------------------
// ponytail: the previous token set was a flat list of constants, and every view
// still reads it that way. These are the Organic **dark** values under the old
// names, so the tree keeps building while each file moves onto `Theme`.
//
// They are a dead end by construction: a constant cannot switch ground, so a
// view still using these will not follow a light/dark change. Delete each name
// as its last call site goes, and the shim disappears with them.

pub const LAYOUT: Rgba = c(0x221E19);
pub const SURFACE: Rgba = c(0x221E19);
pub const RAISED: Rgba = c(0x2E2921);
pub const SUNKEN: Rgba = c(0x221E19);
pub const BORDER: Rgba = alpha(c(0xF3E8D6), 0.18);
pub const DIVIDER: Rgba = alpha(c(0xF3E8D6), 0.18);
pub const FILL: Rgba = alpha(c(0xF3E8D6), 0.07);
pub const FILL_STRONG: Rgba = alpha(c(0xF3E8D6), 0.14);
pub const MASK: Rgba = alpha(c(0x000000), 0.5);
pub const TEXT: Rgba = c(0xF3E8D6);
pub const TEXT_SECONDARY: Rgba = alpha(c(0xF3E8D6), 0.70);
pub const TEXT_TERTIARY: Rgba = alpha(c(0xF3E8D6), 0.55);
pub const TEXT_DISABLED: Rgba = alpha(c(0xF3E8D6), 0.30);
pub const TEXT_ON_ACCENT: Rgba = c(0x221E19);
pub const ACCENT: Rgba = c(0xE08D54);
pub const ACCENT_HOVER: Rgba = c(0xF6A06B);
pub const ACCENT_ACTIVE: Rgba = c(0xD67F48);
pub const ACCENT_WASH: Rgba = alpha(c(0xE08D54), 0.10);
pub const SUCCESS: Rgba = c(0x9FB181);
pub const SUCCESS_WASH: Rgba = alpha(c(0x9FB181), 0.16);
pub const WARNING: Rgba = c(0xD67F48);
pub const WARNING_WASH: Rgba = alpha(c(0xD67F48), 0.16);
pub const DANGER: Rgba = c(0xE07A6B);
pub const DANGER_HOVER: Rgba = c(0xE07A6B);
pub const DANGER_ACTIVE: Rgba = c(0xE07A6B);
pub const DANGER_WASH: Rgba = alpha(c(0xE07A6B), 0.12);
pub const INFO: Rgba = c(0xE08D54);
pub const FOCUS: Rgba = c(0xE08D54);
pub const TRANSPARENT: Rgba = alpha(c(0x000000), 0.);
pub const HAIRLINE: Pixels = px(1.);
pub const DOT: Pixels = px(8.);
/// The old flat names for the two type roles views still reference.
pub const FONT_UI: &str = FONT_BODY;
pub const BG: Rgba = c(0x221E19);
pub const BG_PANEL: Rgba = c(0x2E2921);
pub const BG_RAIL: Rgba = c(0x221E19);
pub const BG_HOVER: Rgba = alpha(c(0xF3E8D6), 0.07);
pub const BG_ACTIVE: Rgba = alpha(c(0xF3E8D6), 0.14);
pub const TEXT_DIM: Rgba = alpha(c(0xF3E8D6), 0.55);

pub const fn wash(color: Rgba) -> Rgba {
    alpha(color, 0.16)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shim must agree with the dark ground it stands in for, or a
    /// half-migrated screen shows two different darks side by side.
    #[test]
    fn the_migration_shim_matches_the_dark_ground() {
        let d = Theme::dark();
        assert_eq!(
            (RAISED.r, RAISED.g, RAISED.b),
            (d.surface.r, d.surface.g, d.surface.b)
        );
        assert_eq!((TEXT.r, TEXT.g, TEXT.b), (d.text.r, d.text.g, d.text.b));
        assert_eq!(
            (ACCENT.r, ACCENT.g, ACCENT.b),
            (d.accent.r, d.accent.g, d.accent.b)
        );
        assert_eq!((SUCCESS.r, SUCCESS.g), (d.success().r, d.success().g));
        assert_eq!(BORDER.a, d.divider.a);
        assert_eq!(TEXT_ON_ACCENT.r, d.on_accent().r);
    }

    #[test]
    fn both_grounds_are_the_values_the_design_specifies() {
        let l = Theme::light();
        assert_eq!((l.bg.r * 255.).round() as u32, 0xF5);
        assert_eq!((l.accent.r * 255.).round() as u32, 0xC6);
        assert!(!l.dark);

        let d = Theme::dark();
        assert_eq!((d.bg.r * 255.).round() as u32, 0x22);
        assert_eq!((d.accent.r * 255.).round() as u32, 0xE0);
        assert!(d.dark);
    }

    /// The dark ground defines its ramps as the light ones reversed, so step 100
    /// stays "nearest the ground" in both. If that inverts, every tint flips
    /// from subtle to glaring.
    #[test]
    fn dark_ramps_are_the_light_ramps_reversed() {
        let l = Theme::light();
        let d = Theme::dark();
        assert_eq!(d.neutral.s100.r, l.neutral.s900.r);
        assert_eq!(d.neutral.s900.r, l.neutral.s100.r);
        assert_eq!(d.accent_ramp.s100.r, l.accent_ramp.s900.r);
        // The middle step is its own mirror.
        assert_eq!(d.accent_ramp.s500.r, l.accent_ramp.s500.r);
    }

    /// On a light ground the ink washes must be dark, on a dark ground light, or
    /// hover states vanish into the surface.
    #[test]
    fn washes_follow_the_ink_of_their_ground() {
        let l = Theme::light();
        assert!(l.hover().r < 0.5, "light-ground hover must be dark ink");
        assert!(l.hover().a > 0.0 && l.hover().a < 0.2);

        let d = Theme::dark();
        assert!(d.hover().r > 0.5, "dark-ground hover must be light ink");
    }

    /// Text on an accent fill is the ground colour here; if that ever equals the
    /// accent the label disappears.
    #[test]
    fn accent_foreground_contrasts_with_the_accent() {
        for t in [Theme::light(), Theme::dark()] {
            let fg = t.on_accent();
            let delta = (fg.r - t.accent.r).abs() + (fg.g - t.accent.g).abs();
            assert!(delta > 0.3, "on_accent is too close to accent");
        }
    }

    #[test]
    fn methods_and_statuses_are_distinguishable() {
        let t = Theme::dark();
        let get = method_color(&t, "GET");
        let post = method_color(&t, "POST");
        assert_ne!((get.r, get.g, get.b), (post.r, post.g, post.b));
        // An unknown method must not silently look like a known one.
        let odd = method_color(&t, "PURGE");
        assert_ne!((odd.r, odd.g, odd.b), (get.r, get.g, get.b));

        assert_eq!(status_color(&t, 204).r, t.success().r);
        assert_eq!(status_color(&t, 500).r, t.danger().r);
    }

    #[test]
    fn the_scale_is_the_designs_own_and_not_rounded_to_four() {
        // 4.4-px steps: rounding these to a 4-px grid would break their rhythm
        // against the radii, so the odd numbers are deliberate.
        assert_eq!(space::S1, px(4.4));
        assert_eq!(space::S3, px(13.2));
        assert_eq!(radius::PILL, px(999.));
        assert_eq!(radius::CARD, px(32.2));
    }
}
