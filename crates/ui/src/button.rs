//! Every pressable thing, in one family.
//!
//! This module exists because there were **five** button functions across three
//! files: `button` and `primary_button` in `main.rs`, `button` again with a `Tone`
//! in `custom/mod.rs`, `primary_button` again and `pill_button` in `track.rs`, and
//! `secondary_button` in `year_strip.rs`. They differed in small ways nobody chose
//! — a hover that lifted the border in one and only the fill in another — so the
//! same button looked slightly different depending on which screen it was on.
//!
//! One shape, one hover, one disabled state, and a [`Tone`] to say what it means.
use crate::icon::icon_tinted;
use crate::theme::Theme;
use crate::tokens::{control, radius, space, text};
use gpui::{
    ClickEvent, Div, ElementId, InteractiveElement as _, ParentElement as _, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, div, prelude::*,
};

/// What a button means, mapped onto the system's `.btn` variants.
///
/// `Primary` is `.btn-primary` — the accent fill, one per view at most. `Default`
/// is `.btn-secondary`, the surface fill every other button uses. `Ghost` has no
/// fill until hovered, for the ones that sit inside a row of content. `Danger` is
/// the same solid shape in the danger role.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Tone {
    #[default]
    Default,
    Primary,
    Ghost,
    Danger,
}

/// The standard button: optional icon, label, the system's one control height, and
/// a pill corner — `.btn` is `999px` in this design.
///
/// Props rather than positional arguments, for the reason [`crate::search_bar`]
/// spells out: this took seven, of which two were a bare `Tone` and a bare `bool`
/// that no call site could read back.
///
/// ```ignore
/// Button::new("save-file", "Save as…")
///     .icon("save")
///     .tone(Tone::Primary)
///     .enabled(ready)
///     .render(t, cx.listener(|this, _, _, cx| this.save(cx)))
/// ```
#[derive(Debug, Clone)]
pub struct Button {
    pub id: ElementId,
    pub label: SharedString,
    /// The glyph left of the label. `None` is a label-only pill, not an empty gap —
    /// the two buttons in the app that carry no icon simply never call `icon`.
    pub icon: Option<&'static str>,
    pub tone: Tone,
    /// A disabled button keeps its shape and loses its affordances: no cursor
    /// change, no hover, no click handler at all. Registering the handler and
    /// ignoring the event would leave a button that looks pressable and is not.
    pub enabled: bool,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            tone: Tone::Default,
            enabled: true,
        }
    }

    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn render(
        self,
        t: Theme,
        on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let (bg, ink, border) = match (self.enabled, self.tone) {
            (false, _) => (t.hover(), t.text_disabled(), t.divider),
            (true, Tone::Primary) => (t.accent, t.on_accent(), t.accent),
            (true, Tone::Danger) => (t.danger(), t.on_accent(), t.danger()),
            (true, Tone::Default) => (t.surface, t.text, t.divider),
            (true, Tone::Ghost) => (
                crate::theme::alpha(t.divider, 0.),
                t.text_muted(),
                crate::theme::alpha(t.divider, 0.),
            ),
        };
        let mut el = div()
            .id(self.id)
            .flex()
            // A button is sized by its label: it must not be squeezed narrower than
            // it, and the label must not wrap inside a pill.
            .flex_none()
            .whitespace_nowrap()
            .items_center()
            .justify_center()
            .gap(space::S1)
            .h(control::MD)
            .px(space::S4)
            .rounded(radius::PILL)
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(text::CONTROL)
            .text_color(ink);
        if let Some(icon) = self.icon {
            el = el.child(icon_tinted(icon, text::CONTROL, ink));
        }
        el = el.child(self.label);
        if !self.enabled {
            return el;
        }
        el = el.cursor_pointer();
        let (accent_hover, accent_active) = (t.accent_ramp.s400, t.accent_ramp.s600);
        el = match self.tone {
            Tone::Primary => el
                .hover(|s| s.bg(accent_hover).border_color(accent_hover))
                .active(|s| s.bg(accent_active).border_color(accent_active)),
            // No danger ramp in this system, so the state change is the fill dropping
            // to its wash rather than a second red.
            Tone::Danger => el
                .hover(|s| s.bg(t.danger_wash()).text_color(t.danger()))
                .active(|s| s.bg(t.danger()).text_color(t.on_accent())),
            Tone::Default => el
                .hover(|s| s.bg(t.hover()).border_color(t.border_hover()))
                .active(|s| s.bg(t.active())),
            Tone::Ghost => el
                .hover(|s| s.bg(t.accent_wash()).text_color(t.accent))
                .active(|s| s.bg(t.active())),
        };
        el.on_click(on_click)
    }
}

/// A square button carrying only an icon — a title-bar gear, a close cross.
///
/// Its own type rather than a [`Button`] with an empty label: the width comes from
/// the height instead of from the text, and a pill with no label needs no `px`.
///
/// ponytail: no `tone` and no `enabled`. Every icon button in the app is the quiet
/// one, and nothing wants a disabled one — the states they would add are states to
/// style and to keep matching [`Button`]'s. Add a setter when a caller needs it;
/// that is exactly what the props shape makes cheap.
#[derive(Debug, Clone)]
pub struct IconButton {
    pub id: ElementId,
    pub glyph: &'static str,
}

impl IconButton {
    pub fn new(id: impl Into<ElementId>, glyph: &'static str) -> Self {
        Self {
            id: id.into(),
            glyph,
        }
    }

    pub fn render(
        self,
        t: Theme,
        on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let ink = t.text_muted();
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(control::MD)
            .rounded(radius::PILL)
            .child(icon_tinted(self.glyph, text::CONTROL, ink))
            .cursor_pointer()
            .hover(|s| s.bg(t.hover()))
            .active(|s| s.bg(t.active()))
            .on_click(on_click)
    }
}

/// One pill in a segmented control, and the trough they sit in.
///
/// Deliberately still a function of `(t, checked)` — as are [`check_box`] and
/// [`seg_track`]. There is nothing optional to name, so a props struct would add a
/// `new`, a `render` and a builder call to say what `seg_option(t, checked)` already
/// says. Props earn their keep against *optional* arguments, not against arguments.
///
/// The checked option is a *filled* pill on an unfilled track. It used to be a
/// surface fill on a hover-coloured track, which on the dark ground are within a
/// few percent of each other — you could not see which option was selected. Callers
/// that colour the selection themselves (the HTTP method segment) paint over `bg`
/// **after** this, which is why the fill is set here and not left to them.
pub fn seg_track(t: Theme) -> Div {
    div()
        .flex()
        // Never wraps: a segmented control split over two lines stops reading as
        // one track. It does shrink — the cells ellipsize rather than the far end
        // of the track being clipped away, which would make the last option
        // unreachable.
        .flex_shrink()
        .min_w_0()
        .items_center()
        .gap(space::S1)
        .h(control::MD)
        .p(space::S1)
        .rounded(radius::PILL)
        .bg(t.hover())
}

pub fn seg_option(t: Theme, checked: bool) -> Div {
    div()
        .flex()
        .items_center()
        .h(control::MD - space::S2)
        .px(space::S3)
        .rounded(radius::PILL)
        .text_size(text::SMALL)
        .min_w_0()
        .truncate()
        .cursor_pointer()
        .when(checked, |d| d.bg(t.accent).text_color(t.on_accent()))
        .when(!checked, |d| {
            d.text_color(t.text_muted())
                .hover(|s| s.bg(t.hover()).text_color(t.text))
        })
}

/// `.checkbox` — the box alone, without a label.
///
/// A boolean is a checkbox, not a two-option segment: a `false | true` track asks
/// the reader to compare two words where a tick answers at a glance, and it cannot
/// be laid out as a grid the way a permission matrix needs. The label is the
/// caller's, because a form puts it beside the box and a matrix puts it in the row
/// and column headers.
pub fn check_box(t: Theme, checked: bool) -> Div {
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
    // The tick is tinted from the accent's own ink, so it stays legible on either
    // ground; gpui never inherits a colour into an `svg`.
    box_.bg(t.accent)
        .child(icon_tinted("check", text::SMALL, t.on_accent()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain `new` is the secondary pill every screen is full of: no icon, no
    /// accent, and pressable — the seventh positional argument used to be `true` at
    /// every call site but four.
    #[test]
    fn the_defaults_are_the_quiet_ones() {
        let b = Button::new("x", "Save");
        assert!(b.icon.is_none(), "no icon means a label-only pill");
        assert_eq!(b.tone, Tone::Default);
        assert!(b.enabled, "a button nobody disabled is pressable");
    }

    #[test]
    fn the_builder_sets_only_what_it_is_given() {
        let b = Button::new("pick-file", "Choose file…")
            .icon("folder")
            .tone(Tone::Primary);
        assert_eq!(b.label.as_ref(), "Choose file…");
        assert_eq!(b.icon, Some("folder"));
        assert_eq!(b.tone, Tone::Primary);
        assert!(b.enabled, "not set, so still the default");
    }

    /// The one `false` that used to be indistinguishable from the `Tone` beside it
    /// now says which of the two it is.
    #[test]
    fn disabling_is_named() {
        assert!(!Button::new("copy", "Copy").enabled(false).enabled);
    }

    #[test]
    fn an_icon_button_carries_its_glyph() {
        assert_eq!(IconButton::new("drop-running", "trash").glyph, "trash");
    }
}
