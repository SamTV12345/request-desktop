//! bcrypt: hash a string, and check a string against a hash.
//!
//! The generic form could technically run this tool — it is `fields in -> text
//! out` like the other 77 — and it was wrong in two ways that no amount of
//! tuning the form could fix:
//!
//! 1. **bcrypt salts randomly.** The form re-runs its tool whenever an input
//!    notifies, which includes clicking into a field, so the hash changed while
//!    you were only looking at it. `ToolPane` no longer recomputes for identical
//!    input, which stops the flicker everywhere; here the hash is produced by an
//!    explicit press instead, because a *new* hash for the same password is a
//!    thing you ask for, not something that should happen behind your back.
//! 2. **A comparison is a verdict, not prose.** The form can only emit one
//!    string, so the answer arrived as the line `Compare string with hash: No`
//!    inside a text area — with "No" also covering "that is not a bcrypt hash".
//!    Here it is a badge, and a malformed hash says so.
use super::{Note, Tone, button, header, note};
use gpui::{
    AppContext, ClipboardItem, Context, Entity, IntoElement, ParentElement as _, SharedString,
    Styled as _, Window, div, prelude::*,
};
use rp_devtools::tools::crypto::{bcrypt_hash_with_rounds, bcrypt_verify};
use rp_ui::icon::icon_tinted;
use rp_ui::input::TextInput;
use rp_ui::layout::{card, section_label};
use rp_ui::theme::{self, FONT_MONO, Theme};
use rp_ui::tokens::{control, radius, size, space, text};

/// What the comparison can say. `Malformed` is its own answer: reporting an
/// input that is not a bcrypt hash as "does not match" sends you looking for the
/// wrong problem — a typo in the password rather than in the hash.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Verdict {
    /// Nothing to compare yet.
    Idle,
    Matches,
    Differs,
    Malformed,
}

/// The comparison, as a verdict rather than a "Yes"/"No" line.
///
/// Pure over the two strings, so every case is testable without a window —
/// which is the only way to test anything in this pane.
fn compare(candidate: &str, hash: &str) -> Verdict {
    let hash = hash.trim();
    if hash.is_empty() {
        return Verdict::Idle;
    }
    match bcrypt_verify(candidate, hash) {
        Ok(true) => Verdict::Matches,
        Ok(false) => Verdict::Differs,
        Err(_) => Verdict::Malformed,
    }
}

/// How many rounds the field is asking for. Empty or nonsense reads as the
/// bcryptjs default rather than refusing to hash at all.
fn rounds_of(field: &str) -> i64 {
    field.trim().parse().unwrap_or(10)
}

pub struct Pane {
    text: Entity<TextInput>,
    rounds: Entity<TextInput>,
    /// The last hash produced, or the reason there is none. Held rather than
    /// recomputed, because recomputing it would produce a different hash.
    hash: Result<String, String>,
    candidate: Entity<TextInput>,
    against: Entity<TextInput>,
    status: SharedString,
    failed: bool,
}

impl Pane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let text = cx.new(|cx| TextInput::new(cx, "the string to hash"));
        let rounds = cx.new(|cx| {
            let mut i = TextInput::new(cx, "10");
            i.set_text("10", cx);
            i
        });
        let candidate = cx.new(|cx| TextInput::new(cx, "the string to check"));
        let against = cx.new(|cx| TextInput::new(cx, "$2b$10$…"));
        // Only the comparison follows the typing: `bcrypt_verify` is
        // deterministic and reads its cost from the hash, so it is honest to run
        // live. The hash deliberately does not.
        for input in [&candidate, &against] {
            cx.observe(input, |_, _, cx| cx.notify()).detach();
        }
        Self {
            text,
            rounds,
            hash: Err(String::new()),
            candidate,
            against,
            status: "Press Hash to salt and hash the string above.".into(),
            failed: false,
        }
    }

    fn hash_now(&mut self, cx: &mut Context<Self>) {
        let text = self.text.read(cx).text().to_string();
        let rounds = rounds_of(self.rounds.read(cx).text());
        // Cost 15 is already ~10 seconds; anything the rule allows is quick
        // enough to run inline, and anything slower it refuses outright.
        match bcrypt_hash_with_rounds(&text, rounds) {
            Ok(hash) => {
                self.hash = Ok(hash);
                self.status = format!(
                    "Hashed with {} salt rounds. Press Hash again for a different salt.",
                    rounds.clamp(4, 31)
                )
                .into();
                self.failed = false;
            }
            Err(e) => {
                let e = format!("{e:#}");
                self.hash = Err(e.clone());
                self.status = e.into();
                self.failed = true;
            }
        }
        cx.notify();
    }

    fn copy_hash(&mut self, cx: &mut Context<Self>) {
        let Ok(hash) = &self.hash else {
            self.status = "Nothing to copy yet — press Hash first.".into();
            self.failed = true;
            cx.notify();
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(hash.clone()));
        self.status = "Hash copied to the clipboard.".into();
        self.failed = false;
        cx.notify();
    }

    /// `label` above a field, the rhythm the generic form uses.
    fn field(t: Theme, label: &'static str, input: impl IntoElement) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap(space::S1)
            .min_w_0()
            .child(
                div()
                    .text_size(text::LABEL)
                    .text_color(t.text_label())
                    .child(label),
            )
            .child(input)
    }
}

/// The verdict badge: the colour carries the answer, the icon repeats it for
/// anyone who cannot use the colour, and the words repeat it again.
fn badge(t: Theme, verdict: &Verdict) -> gpui::Div {
    let (label, icon, ink, bg) = match verdict {
        Verdict::Idle => (
            "Waiting for a hash",
            "clock",
            t.text_muted(),
            t.tag_neutral().0,
        ),
        Verdict::Matches => ("Matches", "check", t.success(), t.success_wash()),
        Verdict::Differs => ("Does not match", "close", t.danger(), t.danger_wash()),
        Verdict::Malformed => ("Not a bcrypt hash", "alert", t.warning(), t.warning_wash()),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(space::S2)
        .h(control::MD)
        .px(space::S4)
        .rounded(radius::PILL)
        .border_1()
        .border_color(ink)
        .bg(bg)
        .text_size(text::CONTROL)
        .text_color(ink)
        .child(icon_tinted(icon, text::CONTROL, ink))
        .child(label)
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        let verdict = compare(self.candidate.read(cx).text(), self.against.read(cx).text());
        let hashed = self.hash.is_ok();
        let shown: SharedString = match &self.hash {
            Ok(h) => h.clone().into(),
            Err(_) => "—".into(),
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
                "Bcrypt",
                "Hash a string with a fresh salt, or check a string against a hash.",
            ))
            // ---- hash
            .child(
                card(t)
                    .flex()
                    .flex_col()
                    .gap(space::S3)
                    .min_h(size::CARD_MIN_H)
                    .child(section_label(t, "Hash").px_0().pt_0())
                    .child(
                        // The two fields sit side by side until the pane is too
                        // narrow, then they stack instead of squeezing.
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(space::S3)
                            .child(
                                Self::field(t, "String", self.text.clone())
                                    .flex_1()
                                    .min_w(size::URL_MIN),
                            )
                            .child(
                                Self::field(t, "Salt rounds", self.rounds.clone())
                                    .flex_none()
                                    .w(size::METHOD),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(space::S2)
                            .child(button(
                                t,
                                "bcrypt-hash",
                                "Hash",
                                "play",
                                Tone::Primary,
                                true,
                                cx.listener(|this, _, _, cx| this.hash_now(cx)),
                            ))
                            .child(button(
                                t,
                                "bcrypt-copy",
                                "Copy",
                                "copy",
                                Tone::Default,
                                hashed,
                                cx.listener(|this, _, _, cx| this.copy_hash(cx)),
                            )),
                    )
                    // The hash itself, in mono and selectable-width rather than a
                    // text area: it is one line and it is the answer.
                    .child(
                        div()
                            .id("bcrypt-hash-value")
                            .w_full()
                            .min_w_0()
                            .overflow_x_scroll()
                            .px(space::S3)
                            .py(space::S2)
                            .rounded(radius::MD)
                            .bg(t.bg)
                            .font_family(FONT_MONO)
                            .text_size(text::CONTROL)
                            .text_color(if hashed { t.text } else { t.text_disabled() })
                            .child(shown),
                    ),
            )
            // ---- compare
            .child(
                card(t)
                    .flex()
                    .flex_col()
                    .gap(space::S3)
                    .min_h(size::CARD_MIN_H)
                    .child(
                        section_label(t, "Compare a string with a hash")
                            .px_0()
                            .pt_0(),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(space::S3)
                            .child(
                                Self::field(t, "String", self.candidate.clone())
                                    .flex_1()
                                    .min_w(size::URL_MIN),
                            )
                            .child(
                                Self::field(t, "Hash", self.against.clone())
                                    .flex_1()
                                    .min_w(size::URL_MIN),
                            ),
                    )
                    .child(badge(t, &verdict)),
            )
            .child(note(
                t,
                self.status.clone(),
                if self.failed { Note::Bad } else { Note::Info },
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verdict_tells_a_wrong_password_apart_from_a_broken_hash() {
        let hash = bcrypt_hash_with_rounds("secret", 4).expect("hash");
        assert_eq!(compare("secret", &hash), Verdict::Matches);
        assert_eq!(compare("Secret", &hash), Verdict::Differs);
        // Surrounding whitespace is what pasting a hash actually produces.
        assert_eq!(compare("secret", &format!("  {hash}\n")), Verdict::Matches);
        // An empty password against a real hash is a genuine mismatch, not idle.
        assert_eq!(compare("", &hash), Verdict::Differs);

        // Nothing to compare against yet.
        assert_eq!(compare("", ""), Verdict::Idle);
        assert_eq!(compare("secret", "   "), Verdict::Idle);

        // The case the old text output got wrong: not a bcrypt hash at all.
        assert_eq!(compare("secret", "not-a-hash"), Verdict::Malformed);
        assert_eq!(compare("secret", "$2b$10$too-short"), Verdict::Malformed);
    }

    /// The same string hashed twice must differ, which is exactly why the hash is
    /// produced on a press and never on a render.
    #[test]
    fn hashing_is_random_so_it_must_never_be_a_render_side_effect() {
        let a = bcrypt_hash_with_rounds("secret", 4).expect("hash");
        let b = bcrypt_hash_with_rounds("secret", 4).expect("hash");
        assert_ne!(a, b, "bcrypt salts randomly");
        assert_eq!(compare("secret", &a), Verdict::Matches);
        assert_eq!(compare("secret", &b), Verdict::Matches);
    }

    #[test]
    fn the_rounds_field_falls_back_instead_of_refusing() {
        assert_eq!(rounds_of("4"), 4);
        assert_eq!(rounds_of(" 12 "), 12);
        assert_eq!(rounds_of(""), 10);
        assert_eq!(rounds_of("lots"), 10);
        // Out of range is the hasher's business, and it reports it.
        assert_eq!(rounds_of("31"), 31);
        assert!(bcrypt_hash_with_rounds("x", 31).is_err());
        // Below the floor is clamped up, as bcryptjs does, rather than refused.
        assert!(bcrypt_hash_with_rounds("x", 1).is_ok());
    }
}
