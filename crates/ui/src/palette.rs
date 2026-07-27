//! The command palette: a centred overlay listing rows, grouped, one selected.
//!
//! Prop-driven in the shape [`crate::search_bar`] documents — every field `pub`,
//! `new` plus chainable setters, handlers passed to `render`. It knows nothing about
//! what a row *is*: it takes `{ group, label, detail, icon }` and reports the index
//! that was chosen. Mapping that index back to "open this request" or "switch to
//! that section" is the application's job, which is what keeps this crate free of
//! `rp_core` and `rp_devtools`.
//!
//! # Keys
//!
//! The bindings live here rather than at the call site because the actions do:
//! [`bind_keys`] takes the [`Chord`] that opens the palette, so the label the
//! search bar shows and the binding that works come from one definition. The
//! movement keys are scoped to [`CONTEXT`], which only exists while the palette is
//! rendered, so Up/Down/Enter keep their normal meaning everywhere else.
use crate::icon::icon_tinted;
use crate::layout::section_label;
use crate::shortcut::Chord;
use crate::theme::{Theme, alpha};
use crate::tokens::{control, radius, space, text};
use gpui::{
    AnyElement, App, ElementId, InteractiveElement as _, KeyBinding, MouseDownEvent,
    ParentElement as _, Pixels, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    actions, div, point, prelude::*, px,
};
use std::rc::Rc;

/// The key context the movement bindings are scoped to. Set on the overlay, so it
/// is in the dispatch tree exactly while the palette is open.
pub const CONTEXT: &str = "CommandPalette";

actions!(
    command_palette,
    [
        /// Open the palette, or close it if it is already open.
        Toggle,
        Dismiss,
        SelectNext,
        SelectPrev,
        Confirm,
    ]
);

/// Register the palette's bindings. `open` is the chord the search bar advertises.
///
/// `Toggle` is bound with no context so it works wherever focus happens to be —
/// including inside a text field, which is where it usually is.
pub fn bind_keys(cx: &mut App, open: Chord) {
    cx.bind_keys([
        KeyBinding::new(&open.keystroke(), Toggle, None),
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("up", SelectPrev, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
    ]);
}

/// One offered thing.
///
/// `detail` is not decoration: two rows that read the same and do different things
/// is the failure this component exists to avoid, so whatever tells them apart —
/// the file a request came from, what a tool does — goes here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The heading this row sits under. Rows are grouped in the order given; a
    /// heading is drawn each time this changes, so callers keep a group contiguous.
    pub group: SharedString,
    pub label: SharedString,
    pub detail: SharedString,
    /// A name from `crates/ui/src/assets.rs`. Anything else paints nothing.
    pub icon: &'static str,
}

impl Row {
    pub fn new(group: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            group: group.into(),
            label: label.into(),
            detail: SharedString::default(),
            icon: "file",
        }
    }

    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = detail.into();
        self
    }

    pub fn icon(mut self, icon: &'static str) -> Self {
        self.icon = icon;
        self
    }
}

/// Keep `selected` inside `len`.
///
/// Called on every render, not only when the query changes: an index left over
/// from a longer list points past the end of a shorter one, and Enter on a stale
/// index opens whatever happens to be there instead — or nothing.
pub fn clamp_selection(selected: usize, len: usize) -> usize {
    match len {
        0 => 0,
        len => selected.min(len - 1),
    }
}

/// One step of the selection, wrapping at both ends.
///
/// Wrapping rather than stopping: the list is short and reaching the last row by
/// pressing Up once is what every palette does.
pub fn step_selection(selected: usize, len: usize, down: bool) -> usize {
    if len == 0 {
        return 0;
    }
    let at = clamp_selection(selected, len);
    if down {
        (at + 1) % len
    } else {
        (at + len - 1) % len
    }
}

/// The width of the card. The design has no token for a dialog width; this is
/// wide enough for a request name plus its method and file on one line.
const CARD_W: Pixels = px(616.);
/// How far down the card sits. Not vertically centred: a palette that grows and
/// shrinks with its list must keep its field still, and anchoring the top is what
/// does that. Roughly the title bar plus a gap.
const CARD_TOP: Pixels = px(88.);
/// Ceiling on the list, so 86 tools scroll instead of running off the screen.
const LIST_MAX_H: Pixels = px(396.);

pub struct Palette {
    pub id: ElementId,
    pub rows: Vec<Row>,
    pub selected: usize,
    /// Shown in place of the list when nothing matched. A blank card looks broken.
    pub empty: SharedString,
}

impl Palette {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            rows: Vec::new(),
            selected: 0,
            empty: "No matches".into(),
        }
    }

    pub fn rows(mut self, rows: Vec<Row>) -> Self {
        self.rows = rows;
        self
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn empty(mut self, empty: impl Into<SharedString>) -> Self {
        self.empty = empty.into();
        self
    }

    /// `field` is the query editor, owned by the caller — the palette holds no
    /// state of its own, so the text and its focus belong to whoever opens it.
    ///
    /// `on_choose` is handed the index into `rows`. `on_dismiss` fires on a click
    /// outside the card; Escape arrives as the [`Dismiss`] action instead, which the
    /// caller handles alongside the rest.
    pub fn render(
        self,
        t: Theme,
        window: &Window,
        field: AnyElement,
        on_choose: impl Fn(usize, &mut Window, &mut App) + 'static,
        on_dismiss: impl Fn(&mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let selected = clamp_selection(self.selected, self.rows.len());
        let on_choose = Rc::new(on_choose);
        // `for`, not `map`: the row builders capture per-row state and a lazy
        // iterator would keep the borrows alive past this scope.
        let mut children = Vec::new();
        let mut group: Option<&SharedString> = None;
        for (i, row) in self.rows.iter().enumerate() {
            if group != Some(&row.group) {
                children.push(section_label(t, row.group.clone()).into_any_element());
                group = Some(&row.group);
            }
            let on_choose = on_choose.clone();
            let ink = if i == selected {
                t.text
            } else {
                t.text_muted()
            };
            children.push(
                div()
                    .id(i)
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .flex_none()
                    .h(control::MD)
                    .px(space::S3)
                    .mx(space::S1)
                    .rounded(radius::PILL)
                    .when(i == selected, |d| d.bg(t.accent_wash()))
                    .hover(|s| s.bg(t.hover()))
                    .cursor_pointer()
                    .child(icon_tinted(row.icon, text::CONTROL, ink))
                    // `min_w_0` on this link and on the label inside it: without
                    // both, a long URL keeps the row at its intrinsic width and the
                    // detail hangs off the card.
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap(space::S2)
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(t.text)
                                    .child(row.label.clone()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(text::SMALL)
                                    .text_color(t.text_meta())
                                    .child(row.detail.clone()),
                            ),
                    )
                    .on_click(move |_, window, cx| on_choose(i, window, cx))
                    .into_any_element(),
            );
        }
        let list = if children.is_empty() {
            div()
                .px(space::S3)
                .py(space::S2)
                .text_color(t.text_meta())
                .child(self.empty)
                .into_any_element()
        } else {
            div()
                .id("palette-rows")
                .flex()
                .flex_col()
                .min_h_0()
                .max_h(LIST_MAX_H)
                .overflow_y_scroll()
                .pb(space::S1)
                .children(children)
                .into_any_element()
        };
        gpui::deferred(
            gpui::anchored().position(point(px(0.), px(0.))).child(
                // The dimmed ground. Sized to the viewport rather than
                // `size_full`, because an anchored element has no parent box to
                // be full of.
                div()
                    .w(window.viewport_size().width)
                    .h(window.viewport_size().height)
                    .flex()
                    .justify_center()
                    .pt(CARD_TOP)
                    .px(space::S4)
                    .bg(t.scrim())
                    .child(
                        div()
                            .id(self.id)
                            .key_context(CONTEXT)
                            // Or a click on the card itself counts as a click
                            // outside it and dismisses what was being aimed at.
                            .occlude()
                            .flex()
                            .flex_col()
                            .flex_none()
                            .min_h_0()
                            .w(CARD_W)
                            .max_w_full()
                            .p(space::S2)
                            .gap(space::S1)
                            .rounded(radius::MD)
                            .bg(t.surface)
                            .border_1()
                            .border_color(alpha(t.text, 0.16))
                            .shadow_lg()
                            .on_mouse_down_out(move |_: &MouseDownEvent, window, cx| {
                                on_dismiss(window, cx)
                            })
                            .child(field)
                            .child(list),
                    ),
            ),
        )
        // Above the tab menu, which is the only other deferred layer.
        .with_priority(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The quiet defaults: no rows, the first one selected, and a message rather
    /// than an empty box when nothing matches.
    #[test]
    fn the_defaults_are_the_quiet_ones() {
        let p = Palette::new("palette");
        assert!(p.rows.is_empty());
        assert_eq!(p.selected, 0);
        assert_eq!(p.empty.as_ref(), "No matches");
        // A row with nothing set carries no detail and a real icon name.
        let row = Row::new("Tools", "Base64");
        assert!(row.detail.is_empty());
        assert_eq!(row.icon, "file");
    }

    #[test]
    fn the_builder_sets_only_what_it_is_given() {
        let p = Palette::new("palette")
            .rows(vec![
                Row::new("Tools", "Base64").detail("Encode").icon("wrench"),
            ])
            .selected(3);
        assert_eq!(p.selected, 3, "held verbatim; clamping happens at render");
        assert_eq!(
            p.empty.as_ref(),
            "No matches",
            "not set, so still the default"
        );
        assert_eq!(p.rows[0].detail.as_ref(), "Encode");
        assert_eq!(p.rows[0].icon, "wrench");
    }

    /// A stale index is how Enter opens the wrong thing.
    #[test]
    fn the_selection_stays_inside_the_list() {
        assert_eq!(clamp_selection(0, 0), 0, "an empty list has no valid index");
        assert_eq!(clamp_selection(9, 0), 0);
        assert_eq!(
            clamp_selection(9, 3),
            2,
            "clamped to the last row, not dropped"
        );
        assert_eq!(clamp_selection(1, 3), 1);
    }

    #[test]
    fn stepping_wraps_at_both_ends() {
        assert_eq!(step_selection(0, 3, true), 1);
        assert_eq!(step_selection(2, 3, true), 0);
        assert_eq!(step_selection(0, 3, false), 2);
        assert_eq!(step_selection(1, 3, false), 0);
        // A step out of a list that has already shrunk lands somewhere real.
        assert_eq!(step_selection(9, 3, true), 0);
        assert_eq!(step_selection(9, 0, true), 0);
    }
}
