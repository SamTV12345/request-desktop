//! How a value is shown: table columns and rows, label/value pairs, chips, tags.
//!
//! `chip` lived twice — once in `custom/mod.rs` as an on/off pill, once in
//! `track.rs` as a duration-plus-name pill — with different paddings. Both shapes
//! are here now under names that say which is which, so a third screen picks one
//! instead of writing a fourth.
use crate::theme::{self, Theme, method_color};
use crate::tokens::{radius, size, space, text};
use gpui::{
    Div, IntoElement, ParentElement as _, Pixels, SharedString, Stateful, Styled as _, div,
    prelude::*,
};

/// One aligned column of the entry table, in the header and in every row.
///
/// The width is a starting size: the cell may shrink to half of it before the
/// flexible task column gives up any more, and its content ellipsizes rather
/// than spilling into the neighbouring column. Both the header and the rows go
/// through here, so a change cannot align one and not the other.
pub fn col(w: Pixels) -> Div {
    div()
        .flex_basis(w)
        .w(w)
        .min_w(w * 0.5)
        .flex_shrink()
        .truncate()
}
/// A `.table` header row: the column names, quiet and letter-spaced-small.
pub fn table_head(t: Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(space::S3)
        .px(space::S3)
        .pb(space::S1)
        // The rows carry a 1-px border so a running row can go dashed without
        // changing height; the header needs the same one or every column sits a
        // pixel off its heading.
        .border_1()
        .border_color(theme::alpha(t.divider, 0.))
        .text_size(text::LABEL)
        .text_color(t.text_meta())
}
/// A `.table` body row. Clickable, so it carries an id and a hover wash; the
/// corner is the large radius rather than a pill, because a row is not a
/// control.
pub fn table_row(t: Theme, id: SharedString) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        // The same gap and inset as [`table_head`], and not a wider one: the
        // columns shrink by the space their line is missing, so a row with more
        // chrome than its header shrinks further and stops lining up under it.
        // Measured at 760 px the head still had its full 104-px WHEN column
        // while the rows were down to 78.
        .gap(space::S3)
        .px(space::S3)
        .py(space::S3)
        .rounded(radius::LG)
        // A transparent border, so switching a running row to dashed cannot
        // change the row's height.
        .border_1()
        .border_color(theme::alpha(t.divider, 0.))
        .hover(|s| s.bg(t.hover()))
}
/// The label column shared by every `label   value` row in these panels.
///
/// `size::LABEL` is a *starting* width, not a rigid one: the cap is half the row,
/// so in a narrow pane the label gives way to the value instead of leaving it a
/// sliver. The cap is relative, so every row in one container lands on the same
/// width and the column still aligns; the label itself ellipsizes.
pub fn label_cell(t: Theme, label: &'static str) -> Div {
    div()
        .flex_none()
        .w(size::LABEL)
        .max_w_1_2()
        .truncate()
        .text_size(text::LABEL)
        .text_color(t.text_label())
        .child(label)
}
/// `label   value`, two columns on one grid across all five panels: label quiet
/// and proportional in the label column, value in mono so digits and paths line
/// up down the column.
pub fn meta_row(t: Theme, label: &'static str, value: impl Into<SharedString>, muted: bool) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(space::S3)
        .py(space::S1)
        .child(label_cell(t, label))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .font_family(theme::FONT_MONO)
                .text_size(text::CONTROL)
                .text_color(if muted { t.text_disabled() } else { t.text })
                .child(value.into()),
        )
}
/// `.tag` — a pill. `on` lights it up in the accent wash; off it is the
/// system's neutral tag.
pub fn chip(t: Theme, label: impl Into<SharedString>, on: bool) -> Div {
    let (bg, ink, border) = if on {
        (t.accent_wash(), t.accent, t.accent)
    } else {
        let (bg, ink) = t.tag_neutral();
        (bg, ink, t.divider)
    };
    div()
        .flex()
        .items_center()
        .px(space::S2)
        .py(space::S1)
        .rounded(radius::PILL)
        .border_1()
        .border_color(border)
        .bg(bg)
        .font_family(theme::FONT_MONO)
        .text_size(text::TINY)
        .text_color(ink)
        .child(label.into())
}
/// A method badge: the method's own hue on a wash of itself, at a fixed width
/// so the names to its right form a column.
pub fn method_badge(t: Theme, method: &str) -> impl IntoElement + use<> {
    let color = method_color(t, method);
    div()
        .flex_none()
        .w(size::METHOD)
        .rounded(radius::PILL)
        // `theme` has the accent wash as a token but no general "tint this hue"
        // helper, and there is one badge per method — so the strength is taken
        // from the token instead of a second set of hard-coded colours.
        .bg(theme::alpha(color, t.accent_wash_strong().a))
        .text_size(text::TINY)
        .text_color(color)
        .text_center()
        .child(SharedString::from(method.to_ascii_uppercase()))
}
