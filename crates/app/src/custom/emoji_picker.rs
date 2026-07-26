//! emoji-picker: searchable grid, click to copy.
//!
//! The table lives in `rp_devtools::tools::text::{EMOJIS, emoji_search}` — this
//! pane is only the surface. Divergence from upstream: it-tools shows a JS-style
//! `😀` surrogate escape; a desktop tool is far likelier to want a
//! Rust/ES6 `\u{1F600}`, so that is what the escape chip copies.
use super::{Note, header, note};
use crate::text_input::TextInput;
use crate::theme;
use gpui::{ClipboardItem, Context, Entity, SharedString, Window, div, prelude::*};
use rp_devtools::tools::text::emoji_search;

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

pub struct Pane {
    search: Entity<TextInput>,
    status: SharedString,
    /// Whether the status line is a copy confirmation rather than the standing
    /// instruction.
    copied: bool,
}

impl Pane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| TextInput::new(cx, "Search emojis (name or keyword)"));
        cx.observe(&search, |_, _, cx| cx.notify()).detach();
        Self {
            search,
            status: "Click an emoji to copy it.".into(),
            copied: false,
        }
    }

    fn copy(&mut self, what: &str, text: String, cx: &mut Context<Self>) {
        self.status = format!("Copied {what} {text}").into();
        self.copied = true;
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        cx.notify();
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search.read(cx).text().to_string();
        let hits = emoji_search(&query);

        // ponytail: a `for` loop, not `map` — the closure would have to capture
        // `&mut Context` for the click listeners, which does not compile.
        let mut cards = Vec::with_capacity(hits.len());
        for (emoji, name, _keywords) in &hits {
            let emoji: &'static str = emoji;
            let points = code_points(emoji);
            let escape = unicode_escape(emoji);
            cards.push(
                div()
                    .id(SharedString::from(format!("cell-{points}")))
                    .flex()
                    .items_center()
                    .gap(theme::space::XS)
                    // Uniform cells: same width, same height, so the wrap reads
                    // as a grid rather than as a paragraph of boxes.
                    .w(theme::space::XXL * 7.)
                    .h(theme::space::XXL * 2.)
                    .p(theme::space::XS)
                    .rounded(theme::radius::SM)
                    .border_1()
                    .border_color(theme::BORDER)
                    .bg(theme::RAISED)
                    .hover(|s| s.border_color(theme::ACCENT).bg(theme::FILL))
                    .child(
                        // Target 1: the emoji itself, the big obvious one.
                        div()
                            .id(SharedString::from(format!("emoji-{points}")))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(theme::control::LG)
                            .flex_none()
                            .rounded(theme::radius::SM)
                            .bg(theme::SUNKEN)
                            .text_size(theme::text::H3)
                            .line_height(theme::text::H3_LINE)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::ACCENT_WASH))
                            .active(|s| s.bg(theme::FILL_STRONG))
                            .child(emoji)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy("emoji", emoji.to_string(), cx)
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(theme::space::XXS)
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(theme::text::SMALL)
                                    .line_height(theme::text::SMALL_LINE)
                                    .text_color(theme::TEXT_SECONDARY)
                                    .overflow_hidden()
                                    .child(*name),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(theme::space::XXS)
                                    .font_family(theme::FONT_MONO)
                                    .text_size(theme::text::SMALL)
                                    // Target 2: code points — a quiet sunken
                                    // chip. Target 3: the escape — accent-tinted.
                                    // Three targets, three looks.
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("points-{points}")))
                                            .px(theme::space::XXS)
                                            .rounded(theme::radius::XS)
                                            .bg(theme::SUNKEN)
                                            .text_color(theme::TEXT_TERTIARY)
                                            .overflow_hidden()
                                            .cursor_pointer()
                                            .hover(|s| s.text_color(theme::TEXT))
                                            .child(points.clone())
                                            .on_click(cx.listener({
                                                let points = points.clone();
                                                move |this, _, _, cx| {
                                                    this.copy("code points", points.clone(), cx)
                                                }
                                            })),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!("escape-{points}")))
                                            .px(theme::space::XXS)
                                            .rounded(theme::radius::XS)
                                            .bg(theme::ACCENT_WASH)
                                            .text_color(theme::ACCENT)
                                            .overflow_hidden()
                                            .cursor_pointer()
                                            .hover(|s| {
                                                s.bg(theme::ACCENT)
                                                    .text_color(theme::TEXT_ON_ACCENT)
                                            })
                                            .child(escape.clone())
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.copy("escape", escape.clone(), cx)
                                            })),
                                    ),
                            ),
                    ),
            );
        }

        let count: SharedString = if hits.is_empty() {
            "No emoji matches that search.".into()
        } else {
            format!("{} emoji", hits.len()).into()
        };

        div()
            .flex()
            .flex_col()
            .gap(theme::space::MD)
            .size_full()
            .p(theme::space::MD)
            .overflow_hidden()
            .child(header(
                "Emoji picker",
                "Click the emoji, its code points or its escape to copy.",
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space::XS)
                    .child(
                        theme::icon_sized("search", theme::text::BODY)
                            .text_color(theme::TEXT_TERTIARY),
                    )
                    .child(self.search.clone())
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme::text::SMALL)
                            .line_height(theme::text::SMALL_LINE)
                            .text_color(theme::TEXT_TERTIARY)
                            .child(count),
                    ),
            )
            .child(
                // overflow_y_scroll needs an id: it lives on StatefulInteractiveElement.
                div()
                    .id("emoji-grid")
                    .flex()
                    .flex_wrap()
                    .gap(theme::space::XS)
                    .flex_1()
                    .overflow_y_scroll()
                    .children(cards),
            )
            .child(note(
                self.status.clone(),
                if self.copied { Note::Good } else { Note::Info },
            ))
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
}
