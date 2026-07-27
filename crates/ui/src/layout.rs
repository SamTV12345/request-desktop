//! The boxes things sit in: cards, dividers, section labels, sidebars, and the
//! "there is nothing here" state.
//!
//! `section_label` and the `section` that wrapped it existed twice — once in
//! `main.rs` with the heading face applied, once in `filters.rs` without — so the
//! same label was two different sizes on two screens. One of them is here.
use crate::fonts::fonts;
use crate::icon::icon_tinted;
use crate::theme::{self, Theme};
use crate::tokens::{control, radius, space, text};
use gpui::{
    App, IntoElement, ParentElement as _, Pixels, SharedString, Styled as _, div, prelude::*, px,
};

pub fn card(t: Theme) -> gpui::Div {
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
pub fn divider(t: Theme) -> gpui::Div {
    div().h(px(1.)).w_full().bg(t.divider)
}

/// `h6` as the system defines it: uppercase, letter-spaced, quiet.
pub fn section_label(t: Theme, label: impl Into<SharedString>) -> gpui::Div {
    div()
        .px(space::S3)
        .pt(space::S3)
        .pb(space::S1)
        .text_size(text::H6)
        .font_family(theme::FONT_HEADING)
        .text_color(t.text_muted())
        .child(label.into())
}

/// HTTP method colours from the two ramps the system provides. Organic has no
/// categorical palette, so methods are separated by family and step rather than
/// by a borrowed rainbow.
/// The sidebar column itself: both lists are the same box.
pub fn sidebar(t: Theme, width: Pixels) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(width)
        .h_full()
        .bg(t.bg)
        .border_r_1()
        .border_color(t.divider)
}
pub fn sidebar_row(t: Theme, selected: bool) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .gap(space::S2)
        .h(control::MD)
        .px(space::S3)
        .mx(space::S2)
        .rounded(radius::PILL)
        .text_size(text::BODY)
        .line_height(text::BODY_LINE)
        .when(selected, |d| d.bg(t.accent_wash()))
        .hover(|s| s.bg(t.hover()))
}
/// A centred icon, a one-line heading and a quieter hint — for every "there is
/// nothing here yet" in the shell.
pub fn empty_state(
    cx: &App,
    t: Theme,
    glyph: &'static str,
    heading: &'static str,
    hint: &'static str,
) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_1()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(space::S2)
        .p(space::S8)
        .child(icon_tinted(glyph, text::H3, t.text_disabled()))
        .child(
            div()
                .text_size(text::H4)
                .font_family(fonts(cx).heading)
                .text_color(t.text_label())
                .child(heading),
        )
        .child(
            div()
                .text_size(text::SMALL)
                .text_color(t.text_meta())
                .text_center()
                .child(hint),
        )
}

/// The title takes the heading face; the system reserves it for headings.
pub fn header(t: Theme, title: &'static str, subtitle: &'static str) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(space::S1)
        .child(
            div()
                .font_family(theme::FONT_HEADING)
                .text_size(text::H4)
                .text_color(t.text)
                .child(title),
        )
        .child(
            div()
                .text_size(text::SMALL)
                .text_color(t.text_muted())
                .child(subtitle),
        )
}
