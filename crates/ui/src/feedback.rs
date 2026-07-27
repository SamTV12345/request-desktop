//! What the app says to the reader: headings, hints, status notes, banners.
//!
//! `note` existed in two places with two different sets of tones. One set, here.
use crate::icon::icon_tinted;
use crate::theme::{self, Theme};
use crate::tokens::{radius, space, text};
use gpui::{Div, ParentElement as _, SharedString, Styled as _, div};

/// How the footer note reads: neutral, done, or gone wrong.
///
/// `NoteKind` and not `Note`, because [`Note`] is now the note itself. The kind is
/// one of the note's props, and the two wanting the same name is how you end up
/// reading `note(t, msg, Note::Bad)` and wondering which is the component.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NoteKind {
    #[default]
    Info,
    Good,
    Bad,
}

/// `heading` and [`hint`] stay plain functions: a label is not an option, and
/// wrapping `hint(t, "…")` in a builder would be three lines to say one.
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
///
/// ```ignore
/// ErrorBanner::new(e).calm(e.as_ref() == NO_STORE).render(t)
/// ```
#[derive(Debug, Clone)]
pub struct ErrorBanner {
    pub message: SharedString,
    /// A prop rather than a look at the message: this crate must not know which
    /// strings the app considers routine. "No database" is expected on a first run
    /// and is not an error; a parse failure is. The caller knows which it has.
    ///
    /// A calm banner explains itself and **ignores `message`** — the string that
    /// reached it is the store's, and the reader needs the consequence instead.
    pub calm: bool,
}

impl ErrorBanner {
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
            calm: false,
        }
    }

    pub fn calm(mut self, calm: bool) -> Self {
        self.calm = calm;
        self
    }

    pub fn render(self, t: Theme) -> Div {
        let ink = if self.calm {
            t.text_muted()
        } else {
            t.danger()
        };
        div()
            .flex()
            .items_start()
            .gap(space::S2)
            .p(space::S3)
            .rounded(radius::MD)
            .bg(if self.calm {
                t.hover()
            } else {
                t.danger_wash()
            })
            .text_size(text::SMALL)
            .text_color(ink)
            .child(icon_tinted("alert", text::BODY, ink))
            .child(if self.calm {
                div().flex_1().child(
                    div()
                        .child("Time tracking is unavailable: the database could not be opened.")
                        .child(hint(
                            t,
                            "Everything else keeps working; nothing here can be saved.",
                        )),
                )
            } else {
                div().flex_1().child(self.message)
            })
    }
}

/// The footer line every pane ends with. An error looks like an error.
///
/// ```ignore
/// Note::new(self.status.clone())
///     .kind(if self.failed { NoteKind::Bad } else { NoteKind::Info })
///     .render(t)
/// ```
#[derive(Debug, Clone)]
pub struct Note {
    pub message: SharedString,
    pub kind: NoteKind,
}

impl Note {
    pub fn new(message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
            kind: NoteKind::Info,
        }
    }

    pub fn kind(mut self, kind: NoteKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn render(self, t: Theme) -> Div {
        let (ink, icon) = match self.kind {
            NoteKind::Info => (t.text_meta(), None),
            NoteKind::Good => (t.success(), Some("check")),
            NoteKind::Bad => (t.danger(), Some("alert")),
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
        row.child(self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Info` and not-calm: a note nobody classified is a status line, and a banner
    /// nobody excused is a real error. Getting either default backwards would either
    /// paint every footer red or swallow a genuine failure into grey.
    #[test]
    fn the_defaults_are_the_quiet_ones() {
        assert_eq!(Note::new("saved").kind, NoteKind::Info);
        assert!(!ErrorBanner::new("boom").calm);
    }

    #[test]
    fn the_builder_sets_only_what_it_is_given() {
        let bad = Note::new("could not decode").kind(NoteKind::Bad);
        assert_eq!(bad.message.as_ref(), "could not decode");
        assert_eq!(bad.kind, NoteKind::Bad);

        let banner = ErrorBanner::new("no database").calm(true);
        assert_eq!(banner.message.as_ref(), "no database");
        assert!(banner.calm);
    }
}
