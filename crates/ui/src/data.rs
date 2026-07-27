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
///
/// Still a function, like [`col`], [`table_head`], [`label_cell`] and
/// [`method_badge`]: an id is not an option to name, and the row's *content* is
/// added by the caller with `.child`, which no props struct could take over.
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
///
/// ```ignore
/// MetaRow::new("auto-repeat", s.held.to_string()).muted(!s.held).render(t)
/// ```
#[derive(Debug, Clone)]
pub struct MetaRow {
    pub label: &'static str,
    pub value: SharedString,
    /// Grey the value out, for a row whose answer is "nothing yet" or "not this
    /// time". Only the value — the label stays a label.
    pub muted: bool,
}

impl MetaRow {
    pub fn new(label: &'static str, value: impl Into<SharedString>) -> Self {
        Self {
            label,
            value: value.into(),
            muted: false,
        }
    }

    pub fn muted(mut self, muted: bool) -> Self {
        self.muted = muted;
        self
    }

    pub fn render(self, t: Theme) -> Div {
        div()
            .flex()
            .items_baseline()
            .gap(space::S3)
            .py(space::S1)
            .child(label_cell(t, self.label))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .font_family(theme::FONT_MONO)
                    .text_size(text::CONTROL)
                    .text_color(if self.muted {
                        t.text_disabled()
                    } else {
                        t.text
                    })
                    .child(self.value),
            )
    }
}

/// `.tag` — a pill. Lit, it is the accent wash; unlit, the system's neutral tag.
///
/// The lit state is the option worth naming: `chip(t, name, held)` read as a pill
/// with a mystery flag, and a third of the call sites passed a literal `false`.
#[derive(Debug, Clone)]
pub struct Chip {
    pub label: SharedString,
    pub on: bool,
}

impl Chip {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            on: false,
        }
    }

    pub fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    pub fn render(self, t: Theme) -> Div {
        let (bg, ink, border) = if self.on {
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
            .child(self.label)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_quiet_ones() {
        assert!(!Chip::new("running").on, "a chip nobody lit is neutral");
        assert!(
            !MetaRow::new("name", "x").muted,
            "a value is ink by default"
        );
    }

    #[test]
    fn the_builder_sets_only_what_it_is_given() {
        let lit = Chip::new("shift").on(true);
        assert_eq!(lit.label.as_ref(), "shift");
        assert!(lit.on);

        let row = MetaRow::new("typed character", "—").muted(true);
        assert_eq!(row.label, "typed character");
        assert_eq!(row.value.as_ref(), "—");
        assert!(row.muted);
    }
}
