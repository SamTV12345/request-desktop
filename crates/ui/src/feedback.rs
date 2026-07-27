//! What the app says to the reader: headings, hints, status notes, banners.
//!
//! `note` existed in two places with two different sets of tones. One set, here.
use crate::icon::icon_tinted;
use crate::theme::{self, Theme};
use crate::tokens::{radius, space, text};
use gpui::{Div, ParentElement as _, SharedString, Styled as _, div};

/// How the footer note reads: neutral, done, or gone wrong.
#[derive(Clone, Copy, PartialEq)]
pub enum Note {
    Info,
    Good,
    Bad,
}

pub fn heading(t: Theme, label: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::FONT_HEADING)
        .text_size(text::H5)
        .text_color(t.text)
        .child(label.into())
}
pub fn hint(t: Theme, label: impl Into<SharedString>) -> Div {
    div()
        .text_size(text::SMALL)
        .text_color(t.text_muted())
        .child(label.into())
}
/// The failure line. A store error is an error; no store at all is not the
/// user's fault, so it explains itself instead of shouting.
pub fn error_banner(t: Theme, message: SharedString, calm: bool) -> Div {
    // `calm` rather than a look at the message: this crate must not know which
    // strings the app considers routine. "No database" is expected on a first run
    // and is not an error; a parse failure is. The caller knows which it has.
    let ink = if calm { t.text_muted() } else { t.danger() };
    div()
        .flex()
        .items_start()
        .gap(space::S2)
        .p(space::S3)
        .rounded(radius::MD)
        .bg(if calm { t.hover() } else { t.danger_wash() })
        .text_size(text::SMALL)
        .text_color(ink)
        .child(icon_tinted("alert", text::BODY, ink))
        .child(if calm {
            div().flex_1().child(
                div()
                    .child("Time tracking is unavailable: the database could not be opened.")
                    .child(hint(
                        t,
                        "Everything else keeps working; nothing here can be saved.",
                    )),
            )
        } else {
            div().flex_1().child(message)
        })
}
/// The footer line every pane ends with. An error looks like an error.
pub fn note(t: Theme, message: SharedString, kind: Note) -> Div {
    let (ink, icon) = match kind {
        Note::Info => (t.text_meta(), None),
        Note::Good => (t.success(), Some("check")),
        Note::Bad => (t.danger(), Some("alert")),
    };
    let mut row = div()
        .flex()
        .items_start()
        .gap(space::S1)
        .text_size(text::SMALL)
        .text_color(ink);
    if let Some(icon) = icon {
        row = row.child(icon_tinted(icon, text::SMALL, ink));
    }
    row.child(message)
}
