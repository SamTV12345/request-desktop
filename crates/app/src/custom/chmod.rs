//! chmod-calculator: the nine permission bits, as the 3×3 grid they are.
//!
//! The generic form can render this tool's nine `Bool` fields, and did — nine
//! rows stacked down the page, each a `false | true` pill, with the octal, the
//! symbolic form and the command joined into one string inside a text area. Every
//! part of that fights the tool: the nine bits are a matrix whose *shape* carries
//! the meaning (this row is the owner, that column is write), and three results
//! wedged into one text box cannot be copied one at a time.
//!
//! So: a real grid with real checkboxes, a header row and a header column, and
//! each of the three answers on its own copyable line. The bit weights and the
//! `rwx` ordering stay in `rp_devtools::tools::web::chmod_forms` — this panel
//! only draws.
use super::{Note, header, label_cell};
use crate::i18n;
use gpui::{
    ClipboardItem, Context, IntoElement, ParentElement as _, SharedString, Styled as _, Window,
    div, prelude::*,
};
use rp_devtools::tools::web::chmod_forms;
use rp_ui::button::check_box;
use rp_ui::icon::icon_tinted;
use rp_ui::layout::{card, section_label};
use rp_ui::theme::{self, FONT_MONO, Theme};
use rp_ui::tokens::{control, radius, size, space, text};

/// Display labels for the two axes. Their own constants because the registry's
/// are lowercase keys used to build field names, not headings — but they must
/// stay in the registry's order, which `the_grid_axes_are_the_order_chmod_forms_expects`
/// checks against it.
const GROUPS: [&str; 3] = ["Owner", "Group", "Public"];
const SCOPES: [&str; 3] = ["Read", "Write", "Execute"];

pub struct Pane {
    /// `[owner, group, public][read, write, execute]`, the order `chmod_forms`
    /// expects. A fixed array rather than a map: the grid is 3×3 by definition,
    /// and an index that cannot exist needs no handling.
    bits: [[bool; 3]; 3],
    status: SharedString,
}

impl Pane {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            // 644, the default a freshly written file gets — a more useful
            // starting point than all-off, which reads as "nobody may do
            // anything" and is a permission set nobody wants.
            bits: [
                [true, true, false],
                [true, false, false],
                [true, false, false],
            ],
            status: "Tick the permissions; the octal and symbolic forms follow.".into(),
        }
    }

    fn copy(&mut self, what: &'static str, value: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.status = format!("{what} copied to the clipboard.").into();
        cx.notify();
    }

    /// One result line: label, value in mono, and a copy button for that value
    /// alone — which is the part the text area could not do.
    fn result_row(
        &self,
        t: Theme,
        label: &'static str,
        value: String,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let to_copy = value.clone();
        div()
            .flex()
            .items_center()
            .gap(space::S3)
            .min_w_0()
            .child(label_cell(t, label))
            .child(
                div()
                    .id(SharedString::from(format!("chmod-value-{label}")))
                    .flex_1()
                    .min_w_0()
                    .overflow_x_scroll()
                    .font_family(FONT_MONO)
                    .text_size(text::CONTROL)
                    .text_color(t.text)
                    .child(SharedString::from(value)),
            )
            .child(
                div()
                    .id(SharedString::from(format!("chmod-copy-{label}")))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(space::S1)
                    .h(control::MD)
                    .px(space::S3)
                    .rounded(radius::PILL)
                    .text_size(text::SMALL)
                    .text_color(t.text_muted())
                    .cursor_pointer()
                    .hover(|s| s.bg(t.accent_wash()).text_color(t.accent))
                    .child(icon_tinted("copy", text::SMALL, t.text_muted()))
                    .child(i18n::text(cx, "copy"))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.copy(label, to_copy.clone(), cx)),
                    ),
            )
    }

    /// One cell of the matrix: the box, centred in its column so the three
    /// columns line up down the grid.
    fn cell(
        &self,
        t: Theme,
        group: usize,
        scope: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let checked = self.bits[group][scope];
        div()
            .id(SharedString::from(format!("chmod-{group}-{scope}")))
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .justify_center()
            .h(control::MD)
            .rounded(radius::MD)
            .cursor_pointer()
            .hover(|s| s.bg(t.hover()))
            .child(check_box(t, checked))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.bits[group][scope] = !this.bits[group][scope];
                // What changed is already visible in the grid; the note says what
                // the whole thing now means.
                let (octal, symbolic) = chmod_forms(this.bits);
                this.status = format!("chmod {octal} — {symbolic}").into();
                cx.notify();
            }))
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        let (octal, symbolic) = chmod_forms(self.bits);

        // Header row first: the three column labels over the three columns, with
        // an empty cell above the row labels so the columns align.
        let mut grid = div().flex().flex_col().gap(space::S1).child({
            let mut head = div()
                .flex()
                .items_center()
                .gap(space::S2)
                .child(div().flex_none().w(size::LABEL).max_w_1_2());
            for scope in SCOPES {
                head = head.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_center()
                        .text_size(text::LABEL)
                        .text_color(t.text_label())
                        .child(scope),
                );
            }
            head
        });
        for (group, name) in GROUPS.iter().enumerate() {
            let mut row = div().flex().items_center().gap(space::S2).child(
                div()
                    .flex_none()
                    .w(size::LABEL)
                    .max_w_1_2()
                    .truncate()
                    .text_size(text::CONTROL)
                    .text_color(t.text)
                    .child(*name),
            );
            for scope in 0..SCOPES.len() {
                row = row.child(self.cell(t, group, scope, cx));
            }
            grid = grid.child(row);
        }

        div()
            .flex()
            .flex_col()
            .gap(space::S4)
            .size_full()
            .p(space::S4)
            .overflow_hidden()
            .child(header(
                t,
                "Chmod calculator",
                i18n::text(cx, "description-chmod"),
            ))
            .child(
                card(t)
                    .flex()
                    .flex_col()
                    .gap(space::S2)
                    .child(section_label(t, "Permissions").px_0().pt_0())
                    .child(grid),
            )
            .child(
                card(t)
                    .flex()
                    .flex_col()
                    .gap(space::S2)
                    .child(section_label(t, "Result").px_0().pt_0())
                    // The octal on its own, big: it is the number you actually
                    // came for, and a 12pt line in a text area buried it.
                    .child(
                        div()
                            .font_family(FONT_MONO)
                            .text_size(text::H3)
                            .text_color(t.accent)
                            .child(SharedString::from(octal.clone())),
                    )
                    .child(self.result_row(t, "Octal", octal.clone(), cx))
                    .child(self.result_row(t, "Symbolic", symbolic, cx))
                    .child(self.result_row(t, "Command", format!("chmod {octal} path"), cx)),
            )
            .child(Note::new(self.status.clone()).render(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the axis-order test needs the registry's own constants.
    use rp_devtools::tools::web::{CHMOD_GROUPS, CHMOD_SCOPES};

    /// The grid's index order has to be the one `chmod_forms` reads, or every
    /// answer is a transposition of the right one — which a `[[bool; 3]; 3]`
    /// cannot make a type error, so it is a test.
    #[test]
    fn the_grid_axes_are_the_order_chmod_forms_expects() {
        // Owner-read alone is 400, not 004 and not 100.
        let mut bits = [[false; 3]; 3];
        bits[0][0] = true;
        assert_eq!(chmod_forms(bits), ("400".into(), "r--------".into()));

        // Public-execute alone is the far corner.
        let mut bits = [[false; 3]; 3];
        bits[2][2] = true;
        assert_eq!(chmod_forms(bits), ("001".into(), "--------x".into()));

        // And the labels are in the same order as the axes they head.
        // The headings this panel draws must name the registry's axes, in the
        // registry's order — otherwise the grid is labelled for one layout and
        // computed for another.
        assert_eq!(GROUPS.map(str::to_lowercase), CHMOD_GROUPS);
        assert_eq!(
            SCOPES.map(str::to_lowercase),
            CHMOD_SCOPES.map(|(label, _)| label)
        );
        assert_eq!(CHMOD_SCOPES.map(|(_, bit)| bit), [4, 2, 1]);
    }

    #[test]
    fn the_default_is_644() {
        let pane = Pane {
            bits: [
                [true, true, false],
                [true, false, false],
                [true, false, false],
            ],
            status: "".into(),
        };
        assert_eq!(chmod_forms(pane.bits), ("644".into(), "rw-r--r--".into()));
    }

    #[test]
    fn every_bit_is_reachable_and_none_is_shared() {
        // Each of the nine cells must move exactly one digit, or two cells are
        // wired to the same bit.
        for group in 0..3 {
            for scope in 0..3 {
                let mut bits = [[false; 3]; 3];
                bits[group][scope] = true;
                let (octal, _) = chmod_forms(bits);
                let digits: Vec<u32> = octal.chars().map(|c| c.to_digit(10).unwrap()).collect();
                assert_eq!(digits.len(), 3);
                assert_eq!(
                    digits.iter().filter(|d| **d != 0).count(),
                    1,
                    "cell {group},{scope} touched more than one digit: {octal}"
                );
                assert_eq!(digits[group], [4, 2, 1][scope], "cell {group},{scope}");
            }
        }
        // All nine on is 777.
        assert_eq!(
            chmod_forms([[true; 3]; 3]),
            ("777".into(), "rwxrwxrwx".into())
        );
        assert_eq!(
            chmod_forms([[false; 3]; 3]),
            ("000".into(), "---------".into())
        );
    }
}
