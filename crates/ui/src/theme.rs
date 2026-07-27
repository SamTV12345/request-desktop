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

use gpui::{App, Global, Pixels, Rgba, px};

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
    /// The window's chrome — the title row, the rail, the tab strip. A ground of
    /// its own, *recessed* rather than raised: on the dark ground the mockups give
    /// it `#1b1814`, darker than `bg`, so an active tab drawn in `bg` reads as
    /// lifted out of the chrome. Not a derived alpha, because it has to be opaque
    /// and it is not the same distance from `bg` as `surface` is.
    pub chrome: Rgba,
    pub text: Rgba,
    pub accent: Rgba,
    /// The olive secondary, for a second categorical role.
    pub accent_2: Rgba,
    pub divider: Rgba,
    pub neutral: Ramp,
    pub accent_ramp: Ramp,
    pub accent_2_ramp: Ramp,
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
            // The mockups are dark-ground only, so the light chrome is the step
            // the light ramp already defines at the same remove from `bg` that
            // `#1b1814` sits from the dark one: neutral-300.
            chrome: c(0xDCD3C4),
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
            chrome: c(0x1B1814),
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

pub fn method_color(t: Theme, method: &str) -> Rgba {
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

pub fn status_color(t: Theme, status: u16) -> Rgba {
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
    use crate::tokens::{radius, space};

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

    /// Chrome is recessed on both grounds: *darker* than `bg`, where `surface` is
    /// the raised one. Darker rather than "further from the ink", because on a
    /// light ground darker is nearer the ink — the direction is the shadow's, not
    /// the contrast's. If chrome collapses onto `bg` the tab strip and the active
    /// tab in it become one shape.
    #[test]
    fn chrome_is_a_ground_of_its_own_and_recessed_on_both() {
        for t in [Theme::light(), Theme::dark()] {
            assert_ne!(t.chrome.r, t.bg.r, "chrome must not be bg");
            assert_ne!(t.chrome.r, t.surface.r, "chrome is not the raised surface");
            assert!(t.chrome.r < t.bg.r, "chrome sits under bg, not over it");
            assert!(t.chrome.g < t.bg.g && t.chrome.b < t.bg.b);
        }
        // And the dark one is the mockups' own value.
        assert_eq!((Theme::dark().chrome.r * 255.).round() as u32, 0x1B);
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
        let get = method_color(t, "GET");
        let post = method_color(t, "POST");
        assert_ne!((get.r, get.g, get.b), (post.r, post.g, post.b));
        // An unknown method must not silently look like a known one.
        let odd = method_color(t, "PURGE");
        assert_ne!((odd.r, odd.g, odd.b), (get.r, get.g, get.b));

        assert_eq!(status_color(t, 204).r, t.success().r);
        assert_eq!(status_color(t, 500).r, t.danger().r);
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
