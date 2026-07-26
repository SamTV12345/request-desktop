//! Bespoke panes for the tools the generic form cannot express.
//!
//! `rp_devtools` marks these `Kind::Custom` because a webcam, a live timer or a
//! file picker is not `fields in -> text out`. Clicking one used to produce
//! "needs a dedicated UI — not wired up yet", which is worse than not listing
//! it: the tool looks present and does nothing. Each module here is the real
//! panel for one such tool.
use crate::theme::{self, Theme, control, radius, size, space, text};
use gpui::{
    AnyView, App, AppContext as _, ClickEvent, Div, InteractiveElement as _, ParentElement as _,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Window, div,
};

mod base64_file;
mod bcrypt;
mod chmod;
mod chronometer;
mod device_info;
mod emoji_picker;
mod keycode;

/// Tools that have a bespoke pane here. The sidebar uses this to mark the ones
/// that do not, so a user is not invited to click something inert.
pub const HAS_PANE: &[&str] = &[
    "base64-file-converter",
    "chronometer",
    "device-information",
    "emoji-picker",
    "keycode-info",
];

/// Build the bespoke pane for `id`, if one exists.
///
/// Not only for `Kind::Custom` tools: `bcrypt` is a perfectly ordinary
/// `fields in -> text out` entry that the generic form nevertheless got wrong
/// (random salts, and a verdict rendered as prose), so it is answered here.
/// Which is why it is absent from [`HAS_PANE`] — that list is about which
/// `Kind::Custom` tools would otherwise be inert.
pub fn pane(id: &str, window: &mut Window, cx: &mut App) -> Option<AnyView> {
    match id {
        "base64-file-converter" => Some(cx.new(base64_file::Pane::new).into()),
        "bcrypt" => Some(cx.new(bcrypt::Pane::new).into()),
        "chmod-calculator" => Some(cx.new(chmod::Pane::new).into()),
        "chronometer" => Some(cx.new(chronometer::Pane::new).into()),
        "device-information" => Some(cx.new(device_info::Pane::new).into()),
        "emoji-picker" => Some(cx.new(emoji_picker::Pane::new).into()),
        "keycode-info" => Some(cx.new(|cx| keycode::Pane::new(window, cx)).into()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Shared chrome
// ---------------------------------------------------------------------------
// ponytail: five panels needed the same button, the same header and the same
// label/value row. One copy here, on the tokens in `theme`, is why they now
// look like one app. Nothing here is public: these are only for the panes.

/// A button's weight, mapped onto the system's `.btn` variants: `Primary` is
/// `.btn-primary`, `Default` is `.btn-secondary`, `Danger` is the same solid
/// shape in the danger role.
///
/// ponytail: no `Ghost` or `Icon` tone. The system has `.btn-ghost` and
/// `.btn-icon`, but none of the five panels wants a button without a label —
/// add them when one does.
#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Default,
    Primary,
    Danger,
}

/// The standard button: icon, label, the system's one control height, and a
/// pill corner — `.btn` is `999px` in this design.
fn button(
    t: &Theme,
    id: &'static str,
    label: impl Into<SharedString>,
    icon: &'static str,
    tone: Tone,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (bg, ink, border) = match (enabled, tone) {
        (false, _) => (t.hover(), t.text_disabled(), t.divider),
        (true, Tone::Primary) => (t.accent, t.on_accent(), t.accent),
        (true, Tone::Danger) => (t.danger(), t.on_accent(), t.danger()),
        (true, Tone::Default) => (t.surface, t.text, t.divider),
    };
    let (accent_hover, accent_active) = (t.accent_ramp.s400, t.accent_ramp.s600);
    let mut el = div()
        .id(id)
        .flex()
        // A button is sized by its label: it must not be squeezed narrower than
        // it, and the label must not wrap inside a pill.
        .flex_none()
        .whitespace_nowrap()
        .items_center()
        .justify_center()
        .gap(space::S1)
        .h(control::MD)
        .px(space::S4)
        .rounded(radius::PILL)
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_size(text::CONTROL)
        .text_color(ink)
        .child(theme::icon_tinted(icon, text::CONTROL, ink))
        .child(label.into());
    if enabled {
        el = el.cursor_pointer();
        el = match tone {
            Tone::Primary => el
                .hover(|s| s.bg(accent_hover).border_color(accent_hover))
                .active(|s| s.bg(accent_active).border_color(accent_active)),
            // No danger ramp in this system, so the state change is the fill
            // dropping to its wash rather than a second red.
            Tone::Danger => el
                .hover(|s| s.bg(t.danger_wash()).text_color(t.danger()))
                .active(|s| s.bg(t.danger()).text_color(t.on_accent())),
            Tone::Default => el
                .hover(|s| s.bg(t.hover()).border_color(t.border_hover()))
                .active(|s| s.bg(t.active())),
        };
        el = el.on_click(on_click);
    }
    el
}

/// Title plus one line of what the panel is for. Every pane opens with this.
/// The title takes the heading face; the system reserves it for headings.
fn header(t: &Theme, title: &'static str, subtitle: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(space::S1)
        .child(
            div()
                .font_family(theme::FONT_HEADING)
                .text_size(text::H4)
                .text_color(t.text)
                .child(title),
        )
        .child(
            div()
                .text_size(text::SMALL)
                .text_color(t.text_muted())
                .child(subtitle),
        )
}

/// How the footer note reads: neutral, done, or gone wrong.
#[derive(Clone, Copy, PartialEq)]
enum Note {
    Info,
    Good,
    Bad,
}

/// The footer line every pane ends with. An error looks like an error.
fn note(t: &Theme, message: SharedString, kind: Note) -> Div {
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
        row = row.child(theme::icon_tinted(icon, text::SMALL, ink));
    }
    row.child(message)
}

/// The label column shared by every `label   value` row in these panels.
///
/// `size::LABEL` is a *starting* width, not a rigid one: the cap is half the row,
/// so in a narrow pane the label gives way to the value instead of leaving it a
/// sliver. The cap is relative, so every row in one container lands on the same
/// width and the column still aligns; the label itself ellipsizes.
fn label_cell(t: &Theme, label: &'static str) -> Div {
    div()
        .flex_none()
        .w(size::LABEL)
        .max_w_1_2()
        .truncate()
        .text_size(text::LABEL)
        .text_color(t.text_label())
        .child(label)
}

/// `label   value`, two columns on one grid across all five panels: label quiet
/// and proportional in the label column, value in mono so digits and paths line
/// up down the column.
fn meta_row(t: &Theme, label: &'static str, value: impl Into<SharedString>, muted: bool) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(space::S3)
        .py(space::S1)
        .child(label_cell(t, label))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .font_family(theme::FONT_MONO)
                .text_size(text::CONTROL)
                .text_color(if muted { t.text_disabled() } else { t.text })
                .child(value.into()),
        )
}

/// `.tag` — a pill. `on` lights it up in the accent wash; off it is the
/// system's neutral tag.
fn chip(t: &Theme, label: impl Into<SharedString>, on: bool) -> Div {
    let (bg, ink, border) = if on {
        (t.accent_wash(), t.accent, t.accent)
    } else {
        let (bg, ink) = t.tag_neutral();
        (bg, ink, t.divider)
    };
    div()
        .flex()
        .items_center()
        .px(space::S2)
        .py(space::S1)
        .rounded(radius::PILL)
        .border_1()
        .border_color(border)
        .bg(bg)
        .font_family(theme::FONT_MONO)
        .text_size(text::TINY)
        .text_color(ink)
        .child(label.into())
}

/// Why a `Kind::Custom` tool has no pane, for the ones that genuinely cannot
/// have one here. Shown instead of a vague apology.
///
/// ponytail: these three stay listed rather than being deleted from the
/// registry, so the it-tools inventory remains honest about what exists and
/// what this build cannot do.
pub fn unavailable_reason(id: &str) -> &'static str {
    match id {
        "camera-recorder" => {
            "Needs camera capture. GPUI exposes no camera API, so there is no way to \
             implement this without a platform-specific video stack."
        }
        "html-wysiwyg-editor" => {
            "Needs a rich-text editing surface. The editor in this app is plain text; \
             rich text means inline formatting spans, which is a project of its own."
        }
        "pdf-signature-checker" => {
            "Needs PDF parsing plus PKCS#7 certificate verification. No dependency for \
             either is available in this crate."
        }
        _ => "No dedicated panel for this tool yet.",
    }
}
