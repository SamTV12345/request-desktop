//! Bespoke panes for the tools the generic form cannot express.
//!
//! `rp_devtools` marks these `Kind::Custom` because a webcam, a live timer or a
//! file picker is not `fields in -> text out`. Clicking one used to produce
//! "needs a dedicated UI — not wired up yet", which is worse than not listing
//! it: the tool looks present and does nothing. Each module here is the real
//! panel for one such tool.
use crate::theme;
use gpui::{
    AnyView, App, AppContext as _, ClickEvent, Div, InteractiveElement as _, ParentElement as _,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Window, div,
};

mod base64_file;
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
pub fn pane(id: &str, window: &mut Window, cx: &mut App) -> Option<AnyView> {
    match id {
        "base64-file-converter" => Some(cx.new(base64_file::Pane::new).into()),
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

/// A button's weight, Ant-style: one primary action per group, destructive in
/// red, everything else quiet.
#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Default,
    Primary,
    Danger,
}

/// The standard button: icon, label, one control height, visibly distinct
/// hover/active/disabled states.
fn button(
    id: &'static str,
    label: impl Into<SharedString>,
    icon: &'static str,
    tone: Tone,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (bg, ink, border) = match (enabled, tone) {
        (false, _) => (theme::FILL, theme::TEXT_DISABLED, theme::BORDER),
        (true, Tone::Primary) => (theme::ACCENT, theme::TEXT_ON_ACCENT, theme::ACCENT),
        (true, Tone::Danger) => (theme::DANGER, theme::TEXT_ON_ACCENT, theme::DANGER),
        (true, Tone::Default) => (theme::RAISED, theme::TEXT, theme::BORDER),
    };
    let mut el = div()
        .id(id)
        .flex()
        .items_center()
        .gap(theme::space::XXS)
        .h(theme::control::MD)
        .px(theme::space::SM)
        .rounded(theme::radius::XS)
        .border_1()
        .border_color(border)
        .bg(bg)
        .text_size(theme::text::BODY)
        .line_height(theme::text::BODY_LINE)
        .text_color(ink)
        .child(theme::icon_sized(icon, theme::text::BODY).text_color(ink))
        .child(label.into());
    if enabled {
        el = el.cursor_pointer();
        el = match tone {
            Tone::Primary => el
                .hover(|s| s.bg(theme::ACCENT_HOVER).border_color(theme::ACCENT_HOVER))
                .active(|s| {
                    s.bg(theme::ACCENT_ACTIVE)
                        .border_color(theme::ACCENT_ACTIVE)
                }),
            Tone::Danger => el
                .hover(|s| s.bg(theme::DANGER_HOVER).border_color(theme::DANGER_HOVER))
                .active(|s| s.bg(theme::hue::RED_6).border_color(theme::hue::RED_6)),
            Tone::Default => el
                .hover(|s| s.border_color(theme::ACCENT).text_color(theme::ACCENT))
                .active(|s| s.bg(theme::FILL_STRONG)),
        };
        el = el.on_click(on_click);
    }
    el
}

/// Title plus one line of what the panel is for. Every pane opens with this.
fn header(title: &'static str, subtitle: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(theme::space::XXS)
        .child(
            div()
                .text_size(theme::text::H5)
                .line_height(theme::text::H5_LINE)
                .text_color(theme::TEXT)
                .child(title),
        )
        .child(
            div()
                .text_size(theme::text::SMALL)
                .line_height(theme::text::SMALL_LINE)
                .text_color(theme::TEXT_SECONDARY)
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
fn note(message: SharedString, kind: Note) -> Div {
    let (ink, icon) = match kind {
        Note::Info => (theme::TEXT_TERTIARY, None),
        Note::Good => (theme::SUCCESS, Some("check")),
        Note::Bad => (theme::DANGER, Some("alert")),
    };
    let mut row = div()
        .flex()
        .items_start()
        .gap(theme::space::XXS)
        .text_size(theme::text::SMALL)
        .line_height(theme::text::SMALL_LINE)
        .text_color(ink);
    if let Some(icon) = icon {
        row = row.child(theme::icon_sized(icon, theme::text::SMALL).text_color(ink));
    }
    row.child(message)
}

/// Width of the label column in [`meta_row`]. Five spacing steps, so the values
/// of every panel line up on the same grid.
fn label_column() -> gpui::Pixels {
    theme::space::XXL * 5.
}

/// `label   value` — label quiet and proportional, value in mono so digits and
/// paths line up down the column.
fn meta_row(label: &'static str, value: impl Into<SharedString>, muted: bool) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(theme::space::SM)
        .child(
            div()
                .w(label_column())
                .flex_none()
                .text_size(theme::text::SMALL)
                .line_height(theme::text::BODY_LINE)
                .text_color(theme::TEXT_SECONDARY)
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .font_family(theme::FONT_MONO)
                .text_size(theme::text::BODY)
                .line_height(theme::text::BODY_LINE)
                .text_color(if muted {
                    theme::TEXT_DISABLED
                } else {
                    theme::TEXT
                })
                .child(value.into()),
        )
}

/// A pill. `on` lights it up in the accent; off it is a quiet outline.
fn chip(text: impl Into<SharedString>, on: bool) -> Div {
    let (bg, ink, border) = if on {
        (theme::ACCENT_WASH, theme::ACCENT, theme::ACCENT)
    } else {
        (theme::FILL, theme::TEXT_TERTIARY, theme::BORDER)
    };
    div()
        .flex()
        .items_center()
        .h(theme::control::SM)
        .px(theme::space::XS)
        .rounded(theme::radius::PILL)
        .border_1()
        .border_color(border)
        .bg(bg)
        .font_family(theme::FONT_MONO)
        .text_size(theme::text::SMALL)
        .text_color(ink)
        .child(text.into())
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
