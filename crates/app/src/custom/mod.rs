//! Bespoke panes for the tools the generic form cannot express.
//!
//! `rp_devtools` marks these `Kind::Custom` because a webcam, a live timer or a
//! file picker is not `fields in -> text out`. Clicking one used to produce
//! "needs a dedicated UI — not wired up yet", which is worse than not listing
//! it: the tool looks present and does nothing. Each module here is the real
//! panel for one such tool.
use gpui::{AnyView, App, AppContext as _, Window};

// The shared chrome the five panels used to keep here — button, header, note,
// label_cell, meta_row, chip — is `rp_ui`'s now. Each panel imports what it draws
// with; this file is back to being a registry.
pub(crate) use rp_ui::button::{Button, Tone};
pub(crate) use rp_ui::data::{Chip, MetaRow, label_cell};
pub(crate) use rp_ui::feedback::{Note, NoteKind};
pub(crate) use rp_ui::layout::header;

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
