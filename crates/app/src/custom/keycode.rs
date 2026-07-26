//! Keycode info. `Kind::Custom` because the input is a live key event, not a
//! form field.
//!
//! Upstream reads a DOM `KeyboardEvent` and shows `key`, `keyCode`, `code`,
//! `location` and the modifier flags. GPUI hands us a [`Keystroke`] instead: a
//! platform-normalised key name, the character that would have been typed, and
//! the modifier flags. `keyCode`, `code` and `location` are DOM concepts that
//! GPUI does not surface at all — so this panel reports them as unavailable
//! rather than inventing numbers from a lookup table that would be wrong for
//! every non-US layout.
use super::{Note, chip, header, meta_row, note};
use crate::theme;
use gpui::{
    Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, Keystroke, MouseButton,
    ParentElement, SharedString, Styled, Window, div, prelude::*,
};

/// Enough to see a chord you just fumbled, bounded so a leaned-on key cannot
/// grow the pane without limit.
const HISTORY: usize = 10;

/// What one keystroke looks like on screen. Derived from `Keystroke` only —
/// every string here comes from something GPUI actually reported.
#[derive(Clone, PartialEq, Debug)]
struct Shown {
    /// GPUI's normalised key name, e.g. `a`, `enter`, `f5`.
    key: String,
    /// The character that would have been typed, if any. This is the closest
    /// thing GPUI has to the DOM's `event.key`.
    key_char: Option<String>,
    /// `ctrl+shift`-style summary, empty when no modifier was held.
    modifiers: String,
    /// The keystroke as GPUI would write it in a keymap, e.g. `ctrl-shift-p`.
    /// `Keystroke::unparse`, not `Display`: `Display` prettifies (uppercases
    /// single characters, uses ⌘-style glyphs on macOS), which is not what you
    /// would paste into a keymap.
    raw: String,
    /// Whether the platform reported this as an auto-repeat (DOM `event.repeat`).
    held: bool,
}

/// Every modifier GPUI reports, in a stable order, named as the platform names
/// them: `platform` is cmd on macOS, win on Windows. The chip row and the
/// summary string both come from this, so they cannot disagree.
const MODIFIERS: [&str; 5] = [
    "ctrl",
    "alt",
    "shift",
    if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "win"
    },
    "fn",
];

fn describe(keystroke: &Keystroke, is_held: bool) -> Shown {
    let m = keystroke.modifiers;
    let mut modifiers = Vec::new();
    for (on, name) in [m.control, m.alt, m.shift, m.platform, m.function]
        .into_iter()
        .zip(MODIFIERS)
    {
        if on {
            modifiers.push(name);
        }
    }
    Shown {
        key: keystroke.key.clone(),
        key_char: keystroke.key_char.clone(),
        modifiers: modifiers.join(" + "),
        raw: keystroke.unparse(),
        held: is_held,
    }
}

/// Newest first, capped at [`HISTORY`].
fn push(history: &mut Vec<Shown>, shown: Shown) {
    history.insert(0, shown);
    history.truncate(HISTORY);
}

pub struct Pane {
    focus_handle: FocusHandle,
    /// Newest first; `first()` is the keystroke shown in detail.
    history: Vec<Shown>,
}

impl Pane {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        // Without focus the panel receives nothing, which looks broken, so take
        // it as soon as the tool is opened.
        focus_handle.focus(window);
        Self {
            focus_handle,
            history: Vec::new(),
        }
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        push(&mut self.history, describe(&event.keystroke, event.is_held));
        cx.notify();
    }
}

impl Focusable for Pane {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        let last = self.history.first().cloned();

        let banner = match (&last, focused) {
            (Some(shown), _) => shown.key_char.clone().unwrap_or_else(|| shown.key.clone()),
            (None, true) => "Press any key".into(),
            (None, false) => "Click here first".into(),
        };
        let hint = if focused {
            "Press the key on your keyboard you want to get info about."
        } else {
            "This panel is not focused, so it receives no key events. Click it to capture keys."
        };

        // ponytail: `for` loops, not `map` — the rows are built into vectors so
        // the reported and the unavailable groups stay separate.
        let mut rows = Vec::new();
        match &last {
            Some(s) => {
                rows.push(meta_row("key (gpui)", s.key.clone(), false));
                rows.push(meta_row(
                    "typed character",
                    s.key_char.clone().unwrap_or_else(|| "—".into()),
                    s.key_char.is_none(),
                ));
                rows.push(meta_row("keystroke (keymap)", s.raw.clone(), false));
                rows.push(meta_row("auto-repeat", s.held.to_string(), !s.held));
            }
            None => rows.push(meta_row("key (gpui)", "waiting for a keystroke", true)),
        }

        // Modifier state as chips: which ones are down is a glance, not a
        // sentence to parse.
        let held = last
            .as_ref()
            .map(|s| s.modifiers.clone())
            .unwrap_or_default();
        let mut modifier_chips = Vec::new();
        for name in MODIFIERS {
            modifier_chips.push(chip(name, held.split(" + ").any(|m| m == name)));
        }

        // Honest about the gaps rather than guessing a number: deliberately
        // unavailable, with the reason kept quiet underneath.
        let mut missing = Vec::new();
        for field in ["keyCode", "code", "location"] {
            missing.push(meta_row(field, "unavailable", true));
        }

        let mut recent = Vec::new();
        for shown in self.history.iter().skip(1) {
            recent.push(chip(shown.raw.clone(), false));
        }

        div()
            .key_context("KeycodeInfo")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus_handle.focus(window);
                    cx.notify();
                }),
            )
            .flex()
            .flex_col()
            .gap(theme::space::MD)
            .size_full()
            .p(theme::space::MD)
            .overflow_hidden()
            .child(header(
                "Keycode info",
                "Find the key name and modifiers of any pressed key.",
            ))
            // The hero: what you just pressed, and whether this panel is even
            // listening.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(theme::space::XS)
                    .py(theme::space::XXL)
                    .bg(if focused {
                        theme::ACCENT_WASH
                    } else {
                        theme::SUNKEN
                    })
                    .border_1()
                    .border_color(if focused {
                        theme::ACCENT
                    } else {
                        theme::BORDER
                    })
                    .rounded(theme::radius::SM)
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(theme::text::H3 * 1.5)
                            .line_height(theme::text::H3 * 2.)
                            .text_color(if last.is_some() {
                                theme::TEXT
                            } else {
                                theme::TEXT_TERTIARY
                            })
                            .child(SharedString::from(banner)),
                    )
                    .child(div().flex().gap(theme::space::XXS).children(modifier_chips))
                    .child(
                        div()
                            .text_size(theme::text::SMALL)
                            .line_height(theme::text::SMALL_LINE)
                            .text_color(if focused {
                                theme::TEXT_SECONDARY
                            } else {
                                theme::WARNING
                            })
                            .child(hint),
                    ),
            )
            .child(
                theme::card()
                    .id("keycode-fields")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .p_0()
                    .overflow_y_scroll()
                    .child(theme::section_label("Reported by gpui"))
                    .child(div().flex().flex_col().px(theme::space::SM).children(rows))
                    .child(theme::divider().mt(theme::space::XS))
                    .child(theme::section_label("Not available on this platform"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .px(theme::space::SM)
                            .children(missing),
                    )
                    .child(
                        div()
                            .px(theme::space::SM)
                            .pb(theme::space::SM)
                            .pt(theme::space::XXS)
                            .text_size(theme::text::SMALL)
                            .line_height(theme::text::SMALL_LINE)
                            .text_color(theme::TEXT_TERTIARY)
                            .child(
                                "These are DOM concepts. GPUI reports a normalised key name, not \
                                 a keyCode, a physical code or which side of the keyboard was \
                                 used — so this panel says so instead of inventing a number that \
                                 would be wrong on most layouts.",
                            ),
                    ),
            )
            .when(!recent.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme::space::XXS)
                        .child(theme::section_label("Recent").px_0().pt_0())
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(theme::space::XXS)
                                .children(recent),
                        ),
                )
            })
            .child(note(
                SharedString::from(format!(
                    "{} of the last {HISTORY} keystrokes kept.",
                    self.history.len()
                )),
                Note::Info,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    fn ks(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.into(),
            key_char: key_char.map(Into::into),
        }
    }

    #[test]
    fn plain_key() {
        let s = describe(&ks("a", Some("a"), Modifiers::none()), false);
        assert_eq!(s.key, "a");
        assert_eq!(s.key_char.as_deref(), Some("a"));
        assert_eq!(s.modifiers, "");
        assert_eq!(s.raw, "a");
        assert!(!s.held);
    }

    #[test]
    fn named_key_without_typed_character() {
        let s = describe(&ks("enter", None, Modifiers::none()), true);
        assert_eq!(s.key, "enter");
        assert_eq!(s.key_char, None);
        assert!(s.held);
    }

    #[test]
    fn modifiers_are_listed_in_a_stable_order() {
        let mods = Modifiers {
            control: true,
            alt: true,
            shift: true,
            platform: false,
            function: false,
        };
        let s = describe(&ks("p", None, mods), false);
        assert_eq!(s.modifiers, "ctrl + alt + shift");
        // The raw form is GPUI's own keymap spelling, not ours.
        assert_eq!(s.raw, ks("p", None, mods).unparse());
        assert!(s.raw.contains("ctrl-") && s.raw.ends_with('p'));
    }

    #[test]
    fn history_keeps_newest_first_and_caps() {
        let mut history = Vec::new();
        for i in 0..HISTORY + 5 {
            let key = i.to_string();
            push(
                &mut history,
                describe(&ks(&key, None, Modifiers::none()), false),
            );
        }
        assert_eq!(history.len(), HISTORY);
        assert_eq!(history[0].key, (HISTORY + 4).to_string());
        assert_eq!(history[HISTORY - 1].key, "5");
    }
}
