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
/// `icon` takes a name or `None`: `Some("copy")` at twenty call sites is noise, and
/// the two buttons that carry no icon should not have to spell it differently from
/// the eighteen that do.
///
/// A disabled button keeps its shape and loses its affordances: no cursor change,
/// no hover, no click handler at all. Registering the handler and ignoring the
/// event would leave a button that looks pressable and is not.
pub fn button(
    t: Theme,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: impl Into<Option<&'static str>>,
    tone: Tone,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    let (bg, ink, border) = match (enabled, tone) {
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
        .id(id)
        .flex()
        // A button is sized by its label: it must not be squeezed narrower than it,
        // and the label must not wrap inside a pill.
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
    if let Some(icon) = icon.into() {
        el = el.child(icon_tinted(icon, text::CONTROL, ink));
    }
    el = el.child(label.into());
    if !enabled {
        return el;
    }
    el = el.cursor_pointer();
    let (accent_hover, accent_active) = (t.accent_ramp.s400, t.accent_ramp.s600);
    el = match tone {
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

/// `button` with [`Tone::Primary`], for the one action a view is about.
pub fn primary(
    t: Theme,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    icon: impl Into<Option<&'static str>>,
    on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    button(t, id, label, icon, Tone::Primary, true, on_click)
}

/// A square button carrying only an icon — a title-bar gear, a close cross.
///
/// Its own function rather than `button` with an empty label: the width comes from
/// the height instead of from the text, and a pill with no label needs no `px`.
/// ponytail: no `enabled` flag. Nothing in the app wants a disabled icon button,
/// and the two states it would add are two more things to style. Give it one when
/// a caller actually needs it.
pub fn icon_button(
    t: Theme,
    id: impl Into<ElementId>,
    glyph: &'static str,
    on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    let ink = t.text_muted();
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(control::MD)
        .rounded(radius::PILL)
        .child(icon_tinted(glyph, text::CONTROL, ink))
        .cursor_pointer()
        .hover(|s| s.bg(t.hover()))
        .active(|s| s.bg(t.active()))
        .on_click(on_click)
}

/// One pill in a segmented control, and the trough they sit in.
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
