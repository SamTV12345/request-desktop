//! emoji-picker: searchable grid, click to copy.
//!
//! The table lives in `rp_devtools::tools::text::{EMOJIS, emoji_search}` — this
//! pane is only the surface. Divergence from upstream: it-tools shows a JS-style
//! `😀` surrogate escape; a desktop tool is far likelier to want a
//! Rust/ES6 `\u{1F600}`, so that is what the escape chip copies.
//!
//! # Why this is a `uniform_list` and not a wrapping flex box
//!
//! It was the latter, and scrolling it was visibly slow for 126 emoji. The count
//! was never the problem — rendering all of them *per frame* was. A
//! `flex_wrap` + `overflow_y_scroll` container builds, lays out and measures
//! every child whether it is on screen or not, and scrolling re-renders the view
//! on every wheel event. Per frame that meant ~500 interactive elements with
//! their hitboxes, ~380 text runs to measure, and a `format!` for every element
//! id and label.
//!
//! `uniform_list` asks only for the rows in view — about a dozen cards instead of
//! 126 — so all of that cost becomes proportional to the window rather than to
//! the table. The price is that rows must be a **fixed height**, which is why a
//! card truncates rather than growing, and that the grid needs to know its own
//! column count: see [`Pane::grid_width`].
use rp_ui::icon::icon_tinted;
use rp_ui::theme::{self, FONT_MONO, Theme};
use rp_ui::tokens::{control, radius, size, space, text};

use super::{Note, NoteKind, header};
use gpui::{
    ClipboardItem, Context, Entity, Pixels, SharedString, WeakEntity, Window, canvas, div,
    prelude::*, px, uniform_list,
};
use rp_devtools::tools::text::emoji_search;
use rp_ui::input::TextInput;

/// `U+1F1E9 U+1F1EA` — every codepoint in the (possibly multi-scalar) emoji.
fn code_points(emoji: &str) -> String {
    emoji
        .chars()
        .map(|c| format!("U+{:04X}", c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `\u{1F1E9}\u{1F1EA}` — pasteable into Rust or modern JS source.
fn unicode_escape(emoji: &str) -> String {
    emoji
        .chars()
        .map(|c| format!("\\u{{{:X}}}", c as u32))
        .collect()
}

/// How many cards fit across `width`, counting the gap between them.
///
/// Measured against `GRID_CELL`, the cell's *starting* width, not `GRID_CELL_MIN`
/// — that is the floor a card may be squeezed to, and dividing by it packed in as
/// many minimum-width columns as would fit, so every card came out at its
/// narrowest. A column is added only once a whole cell's worth of room appears,
/// and the cards then share the remainder, which is what the wrapping layout did
/// before it was virtualised.
///
/// Always at least one: a pane narrower than a single card shows one card that
/// overflows, which is better than a division by zero or an empty grid.
fn columns_for(width: Pixels) -> usize {
    let cell = size::GRID_CELL + space::S2;
    if width <= px(0.) {
        // Nothing measured yet — first frame. One column is the safe guess: it is
        // never *wrong*, only narrow, and the next frame has the real width.
        return 1;
    }
    (((width + space::S2) / cell).floor() as usize).max(1)
}

/// A row's fixed height: one card plus the gap below it.
///
/// `uniform_list` measures the first row and assumes the rest match, so these are
/// heights and not floors — the name and both tags truncate instead of growing
/// the card. Stated as a sum rather than left to padding, because a card sized
/// against its parent could then overflow the row it was measured into.
fn row_height() -> Pixels {
    size::CARD_MIN_H + space::S2
}

pub struct Pane {
    search: Entity<TextInput>,
    status: SharedString,
    /// Whether the status line is a copy confirmation rather than the standing
    /// instruction.
    copied: bool,
    /// Width of the grid, as measured during the last frame's prepaint.
    ///
    /// `uniform_list` hands its item closure a row range and nothing else — no
    /// bounds — so the closure cannot work out how many columns fit. A zero-height
    /// `canvas` in the same box reports the width, and the next frame uses it.
    /// That is one frame behind while a resize is being dragged, which is not
    /// visible; deriving it from the window width instead would hard-code the rail
    /// and sidebar widths into this file.
    grid_width: Pixels,
}

impl Pane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new(cx, "Search emojis (name or keyword)"));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            search,
            status: "Click an emoji to copy it.".into(),
            copied: false,
            grid_width: px(0.),
        }
    }

    fn copy(&mut self, what: &str, text: String, cx: &mut Context<Self>) {
        self.status = format!("Copied {what} {text}").into();
        self.copied = true;
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        cx.notify();
    }
}

/// One emoji card: the emoji as a round icon button, its name, and its two
/// copyable tags.
///
/// A free function rather than a method, because `uniform_list`'s closure is
/// `'static` and cannot borrow the pane — hence the `WeakEntity` for the clicks,
/// which is also what makes a click safe after the tab has been closed.
fn card(t: Theme, pane: &WeakEntity<Pane>, emoji: &'static str, name: &'static str) -> gpui::Div {
    let points = code_points(emoji);
    let escape = unicode_escape(emoji);
    let (points_bg, points_ink) = t.tag_neutral();
    let (escape_bg, escape_ink) = t.tag_accent();
    // One copy path for all three targets, so the status line and the clipboard
    // write have a single implementation.
    let copy = |pane: WeakEntity<Pane>, what: &'static str, value: String| {
        move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
            pane.update(cx, |this, cx| this.copy(what, value.clone(), cx))
                .ok();
        }
    };
    div()
        .flex()
        .items_center()
        .gap(space::S2)
        // Equal shares of the row: the row has a fixed column count, so every
        // card is the same width and the row is flush by construction.
        .flex_1()
        .min_w_0()
        // The cap the wrapping layout also had: with one column in a wide pane,
        // a single card would otherwise be as wide as the whole grid.
        .max_w(size::GRID_CELL_MAX)
        .h(size::CARD_MIN_H)
        .p(space::S2)
        .rounded(radius::MD)
        .bg(t.surface)
        .child(
            // Target 1: the emoji itself, the big obvious one.
            div()
                .id(SharedString::from(format!("emoji-{points}")))
                .flex()
                .items_center()
                .justify_center()
                .size(control::ICON)
                .flex_none()
                .rounded(radius::PILL)
                .bg(t.bg)
                .text_size(text::H4)
                .line_height(text::H4)
                .cursor_pointer()
                .hover(|s| s.bg(t.accent_wash_strong()))
                .active(|s| s.bg(t.active()))
                .child(emoji)
                .on_click(copy(pane.clone(), "emoji", emoji.to_string())),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(space::S1)
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    // A long emoji name ellipsizes rather than being clipped
                    // mid-glyph.
                    div()
                        .text_size(text::SMALL)
                        .text_color(t.text_label())
                        .truncate()
                        .child(name),
                )
                .child(
                    div()
                        .flex()
                        .min_w_0()
                        .gap(space::S1)
                        .font_family(FONT_MONO)
                        .text_size(text::TINY)
                        // Target 2: code points, a neutral tag.
                        // Target 3: the escape, an accent tag.
                        .child(
                            div()
                                .id(SharedString::from(format!("points-{points}")))
                                .px(space::S2)
                                .rounded(radius::PILL)
                                .bg(points_bg)
                                .text_color(points_ink)
                                .min_w_0()
                                .truncate()
                                .cursor_pointer()
                                .hover(|s| s.bg(t.neutral.s300))
                                .child(points.clone())
                                .on_click(copy(pane.clone(), "code points", points)),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("escape-{escape}")))
                                .px(space::S2)
                                .rounded(radius::PILL)
                                .bg(escape_bg)
                                .text_color(escape_ink)
                                .min_w_0()
                                .truncate()
                                .cursor_pointer()
                                .hover(|s| s.bg(t.accent).text_color(t.on_accent()))
                                .child(escape.clone())
                                .on_click(copy(pane.clone(), "escape", escape)),
                        ),
                ),
        )
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        let query = self.search.read(cx).text().to_string();
        let hits = emoji_search(&query);
        let columns = columns_for(self.grid_width);
        let rows = hits.len().div_ceil(columns);

        let count: SharedString = if hits.is_empty() {
            "No emoji matches that search.".into()
        } else {
            format!("{} emoji", hits.len()).into()
        };

        let pane = cx.weak_entity();
        let grid = uniform_list("emoji-grid", rows, move |range, _window, _cx| {
            let mut out = Vec::with_capacity(range.len());
            for row in range {
                let mut line = div()
                    .flex()
                    .gap(space::S2)
                    .w_full()
                    .h(row_height())
                    .pb(space::S2);
                for column in 0..columns {
                    match hits.get(row * columns + column) {
                        Some((emoji, name, _keywords)) => {
                            line = line.child(card(t, &pane, emoji, name));
                        }
                        // The last row is short: hold the empty columns open so
                        // its cards keep the width the rows above gave them.
                        None => line = line.child(div().flex_1().min_w_0()),
                    }
                }
                out.push(line);
            }
            out
        })
        .h_full();

        // Measures the box the grid sits in. Zero height, so it takes no space,
        // and it only writes back a width that actually changed — otherwise every
        // frame would notify and ask for another one.
        let measure = {
            let pane = cx.weak_entity();
            canvas(
                move |bounds, _, cx| {
                    let width = bounds.size.width;
                    pane.update(cx, |this, cx| {
                        if columns_for(width) != columns_for(this.grid_width) {
                            this.grid_width = width;
                            cx.notify();
                        }
                    })
                    .ok();
                },
                |_, _, _, _| {},
            )
            .w_full()
            .h(px(0.))
        };

        div()
            .flex()
            .flex_col()
            .gap(space::S4)
            .size_full()
            .p(space::S4)
            .overflow_hidden()
            .child(header(
                t,
                "Emoji picker",
                "Click the emoji, its code points or its escape to copy.",
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .child(icon_tinted("search", text::CONTROL, t.text_muted()))
                    // TextInput asks for `w_full`, so it needs a flexible box of
                    // its own or it sizes against the whole row.
                    .child(div().flex().flex_1().min_w_0().child(self.search.clone()))
                    .child(
                        div()
                            .flex_none()
                            .text_size(text::SMALL)
                            .text_color(t.text_meta())
                            .child(count),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(measure)
                    .child(grid),
            )
            .child(
                Note::new(self.status.clone())
                    .kind(if self.copied {
                        NoteKind::Good
                    } else {
                        NoteKind::Info
                    })
                    .render(t),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_points_and_escapes_cover_multi_scalar_emoji() {
        assert_eq!(code_points("😀"), "U+1F600");
        assert_eq!(unicode_escape("😀"), "\\u{1F600}");
        // flag: two regional indicators, both must show up
        assert_eq!(code_points("🇩🇪"), "U+1F1E9 U+1F1EA");
        assert_eq!(unicode_escape("🇩🇪"), "\\u{1F1E9}\\u{1F1EA}");
        // variation selector is a codepoint too, and must not be dropped
        assert_eq!(code_points("☀️"), "U+2600 U+FE0F");
        assert_eq!(code_points(""), "");
    }

    /// The pane's only real logic is "what the search box yields is what renders".
    #[test]
    fn search_wiring_uses_the_shared_table() {
        let all = emoji_search("").len();
        assert!(all > 100, "table looks empty: {all}");
        assert!(emoji_search("rocket").iter().any(|(e, _, _)| *e == "🚀"));
        // untrimmed and mixed-case queries behave like the trimmed lowercase one
        assert_eq!(
            emoji_search("  ROCKET \n").len(),
            emoji_search("rocket").len()
        );
        assert!(emoji_search("zzz-no-such-emoji").is_empty());
        // every rendered card can build its labels without panicking
        for (emoji, name, _) in emoji_search("") {
            assert!(!code_points(emoji).is_empty(), "{name}");
            assert!(unicode_escape(emoji).starts_with("\\u{"), "{name}");
        }
    }

    /// The column count is what turns a flat list of hits into a grid, so an
    /// off-by-one here either drops the last cards or asks for an empty row.
    #[test]
    fn the_grid_never_asks_for_zero_columns_or_loses_a_card() {
        // Nothing measured yet, and pathological widths.
        assert_eq!(columns_for(px(0.)), 1);
        assert_eq!(columns_for(px(-50.)), 1);
        assert_eq!(columns_for(px(1.)), 1);

        let cell = size::GRID_CELL + space::S2;
        // Exactly n cells wide holds exactly n columns.
        for n in 1..8usize {
            let width = cell * (n as f32) - space::S2;
            assert_eq!(columns_for(width), n, "{n} cells wide");
            // A hair under drops to n-1 rather than overflowing the row.
            assert_eq!(
                columns_for(width - px(1.)),
                (n - 1).max(1),
                "{n} minus a hair"
            );
        }

        // Every hit lands in exactly one cell, for every plausible width.
        let hits = emoji_search("").len();
        for width in [px(0.), px(200.), px(640.), px(1280.), px(3840.)] {
            let columns = columns_for(width);
            let rows = hits.div_ceil(columns);
            assert!(rows * columns >= hits, "width {width:?} loses cards");
            assert!(
                rows == 0 || (rows - 1) * columns < hits,
                "width {width:?} asks for an empty trailing row"
            );
        }
    }
}
