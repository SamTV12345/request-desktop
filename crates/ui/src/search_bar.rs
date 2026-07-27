//! The "jump to anything" bar from the redesign's title row.
//!
//! # The props pattern
//!
//! This is the shape every component with an optional argument now has, and the
//! reason is what it replaced: `button` took seven positional arguments, so a call
//! site read `button(t, "id", "Save", "save", Tone::Primary, true, cb)` and nobody
//! could tell `true` from the next `bool` without opening the definition. Adding an
//! eighth option meant touching every call. [`crate::button::Button`],
//! [`crate::data::Chip`], [`MetaRow`](crate::data::MetaRow), [`Note`](crate::feedback::Note)
//! and [`ErrorBanner`](crate::feedback::ErrorBanner) follow it.
//!
//! Components with nothing optional to name stay functions — `check_box(t, checked)`,
//! `table_row(t, id)`, `hint(t, label)`. Props pay for themselves against optional
//! arguments, not against arguments.
//!
//! A props struct fixes both. Only `t` and the id are required; everything else has
//! a default and is set by name:
//!
//! ```ignore
//! SearchBar::new("palette")
//!     .placeholder("Jump to anything")
//!     .chord(Chord::primary("K"))
//!     .wide(true)
//!     .render(t, cx.listener(|this, _, _, cx| this.open_palette(cx)))
//! ```
//!
//! Every field is `pub`, so a caller that wants to build the struct literally and
//! keep it in state can, and the builder methods are sugar over that rather than
//! the only door.
use crate::icon::icon_tinted;
use crate::shortcut::Chord;
use crate::theme::{Theme, alpha};
use crate::tokens::{radius, space, text};
use gpui::{
    ClickEvent, Div, ElementId, InteractiveElement as _, ParentElement as _, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, div, prelude::*,
};

/// A read-only bar that *opens* a search, rather than being one.
///
/// It is deliberately not a text field: the redesign puts it in the title row,
/// where a focusable input would compete with the window's own drag region and
/// steal the caret on every click near the top of the window. Pressing it — or its
/// shortcut — opens the real palette.
#[derive(Debug, Clone)]
pub struct SearchBar {
    pub id: ElementId,
    pub placeholder: SharedString,
    /// The shortcut shown on the right. `None` hides the chip entirely rather than
    /// showing an empty one.
    pub chord: Option<Chord>,
    /// Fill the row it sits in instead of taking only its content width. The title
    /// row wants the compact form; a pane header wants the wide one.
    pub wide: bool,
    /// Show it as already active, for when the palette it opens is open.
    pub active: bool,
}

impl SearchBar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            placeholder: "Search".into(),
            chord: None,
            wide: false,
            active: false,
        }
    }

    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    pub fn chord(mut self, chord: Chord) -> Self {
        self.chord = Some(chord);
        self
    }

    pub fn wide(mut self, wide: bool) -> Self {
        self.wide = wide;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn render(
        self,
        t: Theme,
        on_click: impl Fn(&ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        // The design's own ground: text at 6%, which is a shade under `t.hover()`
        // and reads as an inset rather than a raised control.
        let ground = alpha(t.text, 0.06);
        let mut el = div()
            .id(self.id)
            .flex()
            .items_center()
            .gap(space::S2)
            // The design's asymmetric inset: tighter on the icon side, looser on
            // the shortcut side, so the chip does not touch the edge.
            .pl(space::S3)
            .pr(space::S4)
            .py(space::S1 * 0.75)
            .rounded(radius::PILL)
            .bg(if self.active { t.active() } else { ground })
            .text_size(text::SMALL)
            .text_color(t.text_muted())
            .cursor_pointer()
            .hover(|s| s.bg(t.hover()).text_color(t.text))
            .child(icon_tinted("search", text::SMALL, t.text_muted()))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    // `flex_1` only when wide: in the title row the bar should be
                    // as wide as its text, not push the window controls aside.
                    .when(self.wide, |d| d.flex_1())
                    .child(self.placeholder),
            );
        if let Some(chord) = self.chord {
            el = el.child(
                div()
                    .flex_none()
                    .font_family(crate::theme::FONT_MONO)
                    .text_color(alpha(t.text, 0.65))
                    .child(SharedString::from(chord.label())),
            );
        }
        if self.wide {
            el = el.flex_1().min_w_0();
        } else {
            el = el.flex_none();
        }
        el.on_click(on_click)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcut::Platform;

    /// The props carry what the caller set and nothing else — the defaults are the
    /// quiet ones, so a bar with no chord shows no chip rather than an empty pill.
    #[test]
    fn the_defaults_are_the_quiet_ones() {
        let bar = SearchBar::new("x");
        assert_eq!(bar.placeholder.as_ref(), "Search");
        assert!(bar.chord.is_none(), "no chord means no chip");
        assert!(!bar.wide);
        assert!(!bar.active);
    }

    #[test]
    fn the_builder_sets_only_what_it_is_given() {
        let bar = SearchBar::new("palette")
            .placeholder("Jump to anything")
            .chord(Chord::primary("K"))
            .wide(true);
        assert_eq!(bar.placeholder.as_ref(), "Jump to anything");
        assert!(bar.wide);
        assert!(!bar.active, "not set, so still the default");
        // The chip's text is the platform's, not a hard-coded one.
        let chord = bar.chord.expect("chord");
        assert_eq!(chord.label_for(Platform::Mac), "⌘K");
        assert_eq!(chord.label_for(Platform::Windows), "Ctrl+K");
    }
}
