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
    AnyElement, App, ElementId, FontWeight, HighlightStyle, InteractiveElement as _, KeyBinding,
    ListState, MouseDownEvent, ParentElement as _, Pixels, SharedString, Styled as _, StyledText,
    Window, actions, div, point, prelude::*, px,
};
use std::ops::Range;
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

/// How well a query matched, and where.
///
/// `score` is comparable only against other matches of the *same* query; higher is
/// better. `ranges` are byte ranges into the haystack, in order and merged where
/// the matched characters were adjacent, which is the shape
/// [`gpui::StyledText::with_highlights`] wants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Match {
    pub score: i32,
    pub ranges: Vec<Range<usize>>,
}

/// A match at the very start of the label — what the user is most likely typing.
const AT_START: i32 = 8;
/// A match at a word start: after a space, `-`, `_`, `.` or `/`.
const AT_WORD: i32 = 6;
/// A match on the capital of a camelCase hump, so `sc` finds "statusCode".
const AT_HUMP: i32 = 4;
/// A match immediately after the previous one — a run reads as a real substring.
const IN_RUN: i32 = 5;

fn same_letter(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Fuzzy-match `query` against `haystack`, case-insensitively.
///
/// A *subsequence* match rather than a substring one: `csv` finds "Copy as CSV"
/// and `htst` finds "HTTP status codes". `None` means the query's characters do not
/// appear in order.
///
/// An empty (or all-whitespace) query matches everything with score 0 and nothing
/// highlighted, which is what lets the palette open onto its whole list.
///
/// No gpui in the signature on purpose — the ranking is the behaviour worth
/// testing, and nothing inside a `render` can be tested in this workspace.
pub fn fuzzy(haystack: &str, query: &str) -> Option<Match> {
    let query = query.trim();
    let mut m = Match::default();
    if query.is_empty() {
        return Some(m);
    }
    // ponytail: leftmost-greedy. It never misses a match that exists — for
    // subsequence *existence* greedy is complete — but it can score one below the
    // best alignment ("ab" in "a-ab" takes the leading `a`). A Smith-Waterman pass
    // is the upgrade if the ordering ever reads wrong; over palette-length labels
    // it does not.
    let mut needle = query.chars().peekable();
    // The haystack character before the one being looked at, for the boundary and
    // hump bonuses. `None` means we are at the start.
    let mut prev: Option<char> = None;
    let mut run_end: Option<usize> = None;
    for (at, ch) in haystack.char_indices() {
        let Some(&n) = needle.peek() else { break };
        if !same_letter(ch, n) {
            prev = Some(ch);
            continue;
        }
        needle.next();
        let end = at + ch.len_utf8();
        m.score +=
            1 + match prev {
                None => AT_START,
                Some(p) if !p.is_alphanumeric() => AT_WORD,
                Some(p) if p.is_lowercase() && ch.is_uppercase() => AT_HUMP,
                _ => 0,
            } + if run_end == Some(at) { IN_RUN } else { 0 };
        match m.ranges.last_mut() {
            Some(r) if r.end == at => r.end = end,
            _ => m.ranges.push(at..end),
        }
        run_end = Some(end);
        prev = Some(ch);
    }
    // Anything left over never appeared.
    needle.next().is_none().then_some(m)
}

/// Whether row `i` is the first of its group, and so draws the group's label above
/// itself.
///
/// The label lives *inside* the leading row rather than beside it as a sibling.
/// That is what keeps one list item equal to one row: the palette is virtualised,
/// and `list()` indexes by item, so a heading that was its own item would make the
/// row index and the item index drift apart by one per group — the selection would
/// then scroll to a point short of itself by exactly the number of groups above it.
fn leads_group(rows: &[Row], i: usize) -> bool {
    match i.checked_sub(1) {
        None => true,
        Some(prev) => rows[prev].group != rows[i].group,
    }
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
    /// Byte ranges of `label` the query matched — [`Match::ranges`], passed through.
    /// Empty means no emphasis, which is also what an empty query gets.
    pub highlight: Vec<Range<usize>>,
}

impl Row {
    pub fn new(group: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            group: group.into(),
            label: label.into(),
            detail: SharedString::default(),
            icon: "file",
            highlight: Vec::new(),
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

    pub fn highlight(mut self, highlight: Vec<Range<usize>>) -> Self {
        self.highlight = highlight;
        self
    }
}

/// The label, with the matched characters picked out.
///
/// `StyledText::with_highlights` resolves the *unhighlighted* runs against the
/// ambient text style at layout time (checked in gpui 0.2.2's
/// `StyledText::request_layout`, which is where the delayed highlights are turned
/// into runs), so the rest of the label keeps the `text_color` the row set and
/// `.truncate()` on the wrapper still clips it.
fn labelled(label: &SharedString, ranges: &[Range<usize>], style: HighlightStyle) -> AnyElement {
    // `compute_runs` subtracts the previous run's end from the next range's start,
    // so an overlapping or backwards range underflows and panics. `highlight` is a
    // `pub` field, so that cannot be assumed away.
    let mut clean: Vec<(Range<usize>, HighlightStyle)> = Vec::with_capacity(ranges.len());
    let mut end = 0;
    for r in ranges {
        if r.start >= end
            && r.start < r.end
            && r.end <= label.len()
            && label.is_char_boundary(r.start)
            && label.is_char_boundary(r.end)
        {
            end = r.end;
            clean.push((r.clone(), style));
        }
    }
    if clean.is_empty() {
        return label.clone().into_any_element();
    }
    StyledText::new(label.clone())
        .with_highlights(clean)
        .into_any_element()
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
    /// Shared, not owned: `list()`'s item closure is `'static` and cannot borrow the
    /// palette, which is rebuilt every frame anyway.
    pub rows: Rc<Vec<Row>>,
    pub selected: usize,
    /// Shown in place of the list when nothing matched. A blank card looks broken.
    pub empty: SharedString,
    /// Row heights and scroll offset. Owned by the caller because the palette is
    /// rebuilt every frame and state made here would forget itself immediately.
    ///
    /// Whoever moves `selected` is the one that calls
    /// [`gpui::ListState::scroll_to_reveal_item`] — not this `render`, which runs on
    /// wheel scrolls too and would drag the list back to the selection on every
    /// notch.
    pub list: ListState,
}

impl Palette {
    pub fn new(id: impl Into<ElementId>, list: ListState) -> Self {
        Self {
            id: id.into(),
            rows: Rc::default(),
            selected: 0,
            empty: "No matches".into(),
            list,
        }
    }

    pub fn rows(mut self, rows: Rc<Vec<Row>>) -> Self {
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
        // The matched characters, in the accent and a shade heavier. Not a
        // background wash: the selected row already has one, and two overlapping
        // washes read as a rendering fault.
        let hit_style = HighlightStyle {
            color: Some(t.accent.into()),
            font_weight: Some(FontWeight::SEMIBOLD),
            ..Default::default()
        };
        let list = if self.rows.is_empty() {
            div()
                .px(space::S3)
                .py(space::S2)
                .text_color(t.text_meta())
                .child(self.empty)
                .into_any_element()
        } else {
            // Virtualised: an untyped palette is an index of everything — 142 tools
            // plus every request and environment — and building all of it on every
            // keystroke is work the card has no room to show.
            let rows = self.rows.clone();
            gpui::list(self.list.clone(), move |i, _window, _cx| {
                let Some(row) = rows.get(i) else {
                    // The count comes from `rows`, so this cannot happen — and a
                    // render path is the wrong place to prove it with a panic.
                    return div().into_any_element();
                };
                let on_choose = on_choose.clone();
                let ink = if i == selected {
                    t.text
                } else {
                    t.text_muted()
                };
                let line = div()
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
                                    .child(labelled(&row.label, &row.highlight, hit_style)),
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
                    .on_click(move |_, window, cx| on_choose(i, window, cx));
                // The group's label rides along with the row that leads it, so one
                // item stays one row. See `leads_group`.
                if leads_group(&rows, i) {
                    div()
                        .flex()
                        .flex_col()
                        .child(section_label(t, row.group.clone()))
                        .child(line)
                        .into_any_element()
                } else {
                    line.into_any_element()
                }
            })
            .max_h(LIST_MAX_H)
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

    /// The builder needs one; these tests never render, so the count is irrelevant.
    fn state() -> ListState {
        ListState::new(0, gpui::ListAlignment::Top, px(0.))
    }

    /// The quiet defaults: no rows, the first one selected, and a message rather
    /// than an empty box when nothing matches.
    #[test]
    fn the_defaults_are_the_quiet_ones() {
        let p = Palette::new("palette", state());
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
        let p = Palette::new("palette", state())
            .rows(Rc::new(vec![
                Row::new("Tools", "Base64").detail("Encode").icon("wrench"),
            ]))
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

    /// The matched slices, as text — what the highlight will actually emphasise.
    fn hit(haystack: &str, query: &str) -> Option<Vec<String>> {
        Some(
            fuzzy(haystack, query)?
                .ranges
                .into_iter()
                .map(|r| haystack[r].to_string())
                .collect(),
        )
    }

    fn score(haystack: &str, query: &str) -> i32 {
        fuzzy(haystack, query).expect("must match").score
    }

    /// The whole point of not being a substring filter: initials find the thing.
    #[test]
    fn a_query_may_skip_characters() {
        assert_eq!(hit("Copy as CSV", "csv").unwrap(), ["C", "s", "V"]);
        assert_eq!(hit("HTTP status codes", "htst").unwrap(), ["HT", "st"]);
        // A plain substring still matches, as one run.
        assert_eq!(hit("HTTP status codes", "status").unwrap(), ["status"]);
        // Case is ignored in both directions.
        assert!(fuzzy("Base64 Encode", "BASE64").is_some());
        assert!(fuzzy("BASE64 ENCODE", "base64").is_some());
    }

    /// Out of order is not a match, or the palette matches everything.
    #[test]
    fn characters_out_of_order_do_not_match() {
        assert!(fuzzy("Copy as CSV", "vsc").is_none());
        assert!(fuzzy("Get films", "zzqq").is_none());
        // One character too many, even though the rest is there.
        assert!(fuzzy("csv", "csvv").is_none());
    }

    /// The empty query is the palette's index view: everything, nothing emphasised.
    #[test]
    fn an_empty_query_matches_with_nothing_highlighted() {
        for query in ["", "   ", "\t\n"] {
            let m = fuzzy("Anything at all", query).expect("must match");
            assert_eq!(m.score, 0);
            assert!(m.ranges.is_empty(), "nothing to emphasise for {query:?}");
        }
    }

    /// The ranking rules, each against its own counter-example.
    #[test]
    fn a_prefix_beats_a_word_start_beats_a_scattered_hit() {
        assert!(
            score("Token exchange", "token") > score("Refresh token", "token"),
            "a prefix outranks a word start"
        );
        assert!(
            score("Refresh token", "token") > score("Retokenise", "token"),
            "a word start outranks a hit in the middle of a word"
        );
        assert!(
            score("status codes", "stco") > score("stack of coats", "stco"),
            "one run plus a word start beats two scattered pairs"
        );
        // camelCase counts as a word start, so initials work on joined names.
        assert!(score("statusCode", "sc") > score("stack", "sc"));
    }

    /// Ties go to the shorter label, but that is the *caller's* sort — the score
    /// says nothing about length, so this pins the contract the caller relies on.
    #[test]
    fn equal_matches_score_equally_regardless_of_length() {
        assert_eq!(
            score("Get", "get"),
            score("Get films from a very long list", "get")
        );
    }

    /// Adjacent matches are one range, not one per character — a highlight drawn
    /// per character shows seams between the letters.
    #[test]
    fn adjacent_matches_merge_into_one_range() {
        let m = fuzzy("HTTP status codes", "htstat").expect("must match");
        assert_eq!(
            m.ranges,
            vec![0..2, 5..9],
            "two runs, not six single characters"
        );
    }

    /// Byte ranges, not character counts: a multi-byte label must still slice.
    #[test]
    fn the_ranges_are_byte_ranges_that_slice_cleanly() {
        let label = "Grüße löschen";
        let m = fuzzy(label, "gl").expect("must match");
        let cut: Vec<&str> = m.ranges.iter().map(|r| &label[r.clone()]).collect();
        assert_eq!(cut, ["G", "l"]);
    }

    /// One list item is one row, so the group label has to ride along with the row
    /// that leads its group — the drift this replaces is what scrolled the selection
    /// short by one per group above it.
    #[test]
    fn only_the_first_row_of_a_group_carries_its_label() {
        let rows = vec![
            Row::new("Requests", "a"),
            Row::new("Requests", "b"),
            Row::new("Tools", "c"),
            Row::new("Sections", "d"),
        ];
        assert!(leads_group(&rows, 0), "the very first row always leads");
        assert!(!leads_group(&rows, 1), "same group as the row above it");
        assert!(leads_group(&rows, 2));
        assert!(leads_group(&rows, 3), "a group of one still gets its label");
    }
}
