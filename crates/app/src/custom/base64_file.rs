//! base64-file-converter: file -> `data:` URI, and base64 -> file.
//!
//! Upstream renders both directions at once (no mode toggle) and this pane keeps
//! that. It diverges in one place: the decode direction writes a real file
//! through the save dialog instead of triggering a browser download.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::{Note, Tone, button, header, meta_row, note};
use crate::editor::Editor;
use crate::theme::{self, radius, size, space, text};
use gpui::{
    AppContext, ClipboardItem, Context, Entity, ExternalPaths, Image, ImageFormat,
    PathPromptOptions, SharedString, Window, div, img, prelude::*,
};

/// ponytail: a file this size already makes a 21 MB data URI; the editor shapes
/// text per visible line so it survives it, but the clipboard round-trip is the
/// real limit. Raise it when someone actually needs to.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Pure logic
// ---------------------------------------------------------------------------

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding. ponytail: hand-rolled because the `base64`
/// crate is a dependency of `rp-devtools`, not of this crate, and adding a
/// dependency for 15 lines is worse than the 15 lines.
fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b1 = chunk[0] as u32;
        let b2 = *chunk.get(1).unwrap_or(&0) as u32;
        let b3 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b1 << 16) | (b2 << 8) | b3;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Decode base64, tolerating a `data:…;base64,` prefix, whitespace, missing
/// padding and the URL-safe alphabet — the same latitude `rp_devtools` gives its
/// string converter, since pasted base64 arrives wrapped and trimmed.
fn b64_decode(input: &str) -> Result<Vec<u8>, String> {
    let s = input.trim();
    let s = s
        .strip_prefix("data:")
        .and_then(|rest| rest.split_once(";base64,"))
        .map_or(s, |(_, body)| body);

    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for ch in s.chars() {
        if ch.is_whitespace() || ch == '=' {
            continue;
        }
        let v = match ch {
            'A'..='Z' => ch as u32 - 'A' as u32,
            'a'..='z' => ch as u32 - 'a' as u32 + 26,
            '0'..='9' => ch as u32 - '0' as u32 + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            _ => return Err(format!("invalid base64: {ch:?} is not a base64 character")),
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    if out.is_empty() {
        return Err("nothing to decode — paste base64 above".into());
    }
    Ok(out)
}

/// Mime from the file extension, over `rp_devtools`' `mime_guess` table — the
/// same data its `mime-types` tool formats. `application/octet-stream` is the
/// honest answer for a file we cannot identify.
fn mime_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    rp_devtools::tools::web::mime_for_extension(&ext).unwrap_or("application/octet-stream")
}

/// Extension to suggest when saving a decoded `data:` URI back to disk.
fn ext_for_mime(mime: &str) -> Option<&'static str> {
    rp_devtools::tools::web::extensions_for_mime(mime)
        .first()
        .copied()
}

/// The mime type declared by a `data:<mime>;base64,` prefix, if there is one.
fn mime_of_data_uri(input: &str) -> Option<&str> {
    input
        .trim()
        .strip_prefix("data:")?
        .split_once(";base64,")
        .map(|(mime, _)| mime)
}

fn data_uri(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", b64_encode(bytes))
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Name to suggest in the save dialog, derived from the pasted URI's mime type.
fn suggested_name(input: &str) -> String {
    let ext = mime_of_data_uri(input).and_then(ext_for_mime);
    match ext {
        Some(ext) => format!("file.{ext}"),
        None => "file.bin".to_string(),
    }
}

/// Where the save dialog should open. ponytail: home, falling back to the temp
/// dir — no "last used directory" memory until someone misses it.
fn default_directory() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

/// Read a file, refusing anything over the ceiling instead of stalling on it.
fn read_capped(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if meta.len() > MAX_BYTES {
        return Err(format!(
            "{} is {} — over the {} limit for base64 in this panel",
            path.display(),
            human_size(meta.len()),
            human_size(MAX_BYTES)
        ));
    }
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Everything the encode direction needs from disk. One struct so the whole read
/// — including the base64 and the preview decode — runs on the background
/// executor, whether the path came from the dialog or from a drop.
struct Loaded {
    name: String,
    size: u64,
    mime: &'static str,
    uri: String,
    preview: Preview,
}

fn encode_file(path: &Path) -> Result<Loaded, String> {
    let bytes = read_capped(path)?;
    let mime = mime_for_path(path);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Ok(Loaded {
        size: bytes.len() as u64,
        uri: data_uri(mime, &bytes),
        preview: preview_of(Some(mime), &bytes),
        name,
        mime,
    })
}

// ---------------------------------------------------------------------------
// Image preview
// ---------------------------------------------------------------------------

/// What to put in the preview slot. `Image` holds an `Arc<Image>`, which is what
/// gpui's asset cache keys on: it decodes the bytes once, off the main thread,
/// and every later frame is a cache hit. A decode failure inside gpui surfaces
/// as the element's fallback, not a panic.
enum Preview {
    None,
    Image {
        image: Arc<Image>,
        label: SharedString,
    },
    Unsupported(SharedString),
}

/// Sniff the formats gpui can decode from memory — `gpui::ImageFormat`, which is
/// narrower than what the `image` crate handles. Header-only: a truncated or
/// corrupt file of a known format still matches here and fails later in the
/// decoder, where it degrades to the fallback message.
fn sniff_image_format(bytes: &[u8]) -> Option<ImageFormat> {
    let at = |i: usize, sig: &[u8]| bytes.len() >= i + sig.len() && &bytes[i..i + sig.len()] == sig;
    if at(0, b"\x89PNG\r\n\x1a\n") {
        return Some(ImageFormat::Png);
    }
    if at(0, b"\xff\xd8\xff") {
        return Some(ImageFormat::Jpeg);
    }
    if at(0, b"GIF87a") || at(0, b"GIF89a") {
        return Some(ImageFormat::Gif);
    }
    if at(0, b"RIFF") && at(8, b"WEBP") {
        return Some(ImageFormat::Webp);
    }
    if at(0, b"BM") {
        return Some(ImageFormat::Bmp);
    }
    if at(0, b"II*\0") || at(0, b"MM\0*") {
        return Some(ImageFormat::Tiff);
    }
    // SVG has no magic number, so look for the root tag near the start.
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]);
    let head = head.trim_start_matches('\u{feff}').trim_start();
    if (head.starts_with("<?xml") || head.starts_with("<!DOCTYPE svg") || head.starts_with("<svg"))
        && head.contains("<svg")
    {
        return Some(ImageFormat::Svg);
    }
    None
}

/// Build the preview once, from bytes we already hold. Only copies the bytes when
/// they really are a format gpui can render.
fn preview_of(mime: Option<&str>, bytes: &[u8]) -> Preview {
    match sniff_image_format(bytes) {
        Some(format) => Preview::Image {
            image: Arc::new(Image::from_bytes(format, bytes.to_vec())),
            label: format!(
                "{} — {}",
                format.mime_type(),
                human_size(bytes.len() as u64)
            )
            .into(),
        },
        None => Preview::Unsupported(
            format!(
                "No preview — {} is not an image format this panel can render.",
                mime.unwrap_or("this content")
            )
            .into(),
        ),
    }
}

// ---------------------------------------------------------------------------
// Drops
// ---------------------------------------------------------------------------

/// Which of the dropped paths to encode. ponytail: one file at a time, because
/// the pane shows exactly one data URI — taking the first of several would
/// silently ignore the rest, so refuse and say why.
fn dropped_path(paths: &[PathBuf]) -> Result<PathBuf, String> {
    match paths {
        [] => Err("nothing was dropped".into()),
        [one] if one.is_dir() => Err(format!(
            "{} is a folder — drop a single file",
            one.display()
        )),
        [one] => Ok(one.clone()),
        many => Err(format!(
            "{} files dropped — this panel encodes one file at a time",
            many.len()
        )),
    }
}

// ---------------------------------------------------------------------------
// Pane
// ---------------------------------------------------------------------------

/// What the metadata row shows about the file that was encoded. Three fields
/// rather than one joined string, so they can be laid out as a row of labelled
/// values instead of a sentence.
struct Picked {
    name: SharedString,
    size: SharedString,
    mime: SharedString,
}

pub struct Pane {
    /// `data:` URI of the picked file, read-only so it can be copied out.
    encoded: Entity<Editor>,
    /// Paste target for the decode direction.
    to_decode: Entity<Editor>,
    /// Name / size / mime of the picked file, or nothing picked yet.
    picked: Option<Picked>,
    /// Decoded once when the content changes, never per render.
    preview: Preview,
    /// Last thing that happened, good or bad. Errors set `failed`.
    status: SharedString,
    failed: bool,
    busy: bool,
}

impl Pane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            encoded: cx.new(|cx| Editor::new(cx, "data:…;base64,… will appear here").read_only()),
            to_decode: cx.new(|cx| Editor::new(cx, "paste base64 or a data: URI here")),
            picked: None,
            preview: Preview::None,
            status: "Pick or drop a file to encode, or paste base64 and save it.".into(),
            failed: false,
            busy: false,
        }
    }

    fn say(&mut self, message: impl Into<SharedString>, failed: bool, cx: &mut Context<Self>) {
        self.status = message.into();
        self.failed = failed;
        self.busy = false;
        cx.notify();
    }

    fn pick_file(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        self.busy = true;
        self.status = "Waiting for the file dialog…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let picked = match rx.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(e)) => {
                    this.update(cx, |this, cx| this.say(format!("{e:#}"), true, cx))
                        .ok();
                    return;
                }
                Err(_) => {
                    this.update(cx, |this, cx| {
                        this.say("the file dialog closed unexpectedly", true, cx)
                    })
                    .ok();
                    return;
                }
            };
            let Some(path) = picked else {
                this.update(cx, |this, cx| {
                    this.say("Cancelled — no file picked.", false, cx)
                })
                .ok();
                return;
            };
            this.update(cx, |this, cx| this.load_path(path, cx)).ok();
        })
        .detach();
    }

    /// The one read/encode path: the dialog and the drop target both end here, so
    /// the size ceiling, the background read, the mime, the preview and the error
    /// reporting are the same either way.
    fn load_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.busy = true;
        self.status = format!("Reading {}…", path.display()).into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            // Reading, base64-encoding and sniffing megabytes off the main
            // thread; the UI stays live while it happens.
            let result = cx.background_spawn(async move { encode_file(&path) }).await;
            this.update(cx, |this, cx| match result {
                Ok(loaded) => {
                    let chars = loaded.uri.len();
                    this.picked = Some(Picked {
                        name: loaded.name.clone().into(),
                        size: human_size(loaded.size).into(),
                        mime: loaded.mime.into(),
                    });
                    this.preview = loaded.preview;
                    this.encoded.update(cx, |e, cx| e.set_text(loaded.uri, cx));
                    this.say(
                        format!("Encoded {} ({chars} characters).", loaded.name),
                        false,
                        cx,
                    );
                }
                Err(e) => {
                    this.picked = None;
                    this.preview = Preview::None;
                    this.encoded.update(cx, |e, cx| e.set_text("", cx));
                    this.say(e, true, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn drop_files(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        match dropped_path(paths.paths()) {
            Ok(path) => self.load_path(path, cx),
            Err(e) => self.say(e, true, cx),
        }
    }

    /// Preview the pasted base64 without writing it anywhere. ponytail: a button
    /// rather than live-on-keystroke, because the editor has no change event and
    /// re-decoding megabytes per keypress would be worse than one click.
    fn preview_decoded(&mut self, cx: &mut Context<Self>) {
        let input = self.to_decode.read(cx).text().to_string();
        match b64_decode(&input) {
            Ok(bytes) => {
                self.preview = preview_of(mime_of_data_uri(&input), &bytes);
                let message = match &self.preview {
                    Preview::Image { label, .. } => format!("Previewing {label}."),
                    Preview::Unsupported(m) => m.to_string(),
                    Preview::None => String::new(),
                };
                self.picked = None;
                self.say(message, false, cx);
            }
            Err(e) => {
                self.preview = Preview::None;
                self.say(e, true, cx);
            }
        }
    }

    fn copy_encoded(&mut self, cx: &mut Context<Self>) {
        let text = self.encoded.read(cx).text().to_string();
        if text.is_empty() {
            self.say("Nothing to copy yet — pick a file first.", true, cx);
            return;
        }
        let len = text.len();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.say(
            format!("Copied {len} characters to the clipboard."),
            false,
            cx,
        );
    }

    fn save_decoded(&mut self, cx: &mut Context<Self>) {
        let input = self.to_decode.read(cx).text().to_string();
        // Decode before opening the dialog, so bad input is reported in place
        // instead of after the user has chosen a name.
        let bytes = match b64_decode(&input) {
            Ok(bytes) => bytes,
            Err(e) => {
                self.say(e, true, cx);
                return;
            }
        };
        let name = suggested_name(&input);
        let rx = cx.prompt_for_new_path(&default_directory(), Some(&name));
        self.busy = true;
        self.status = "Waiting for the save dialog…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let target = match rx.await {
                Ok(Ok(Some(path))) => path,
                Ok(Ok(None)) => {
                    this.update(cx, |this, cx| {
                        this.say("Cancelled — nothing written.", false, cx)
                    })
                    .ok();
                    return;
                }
                Ok(Err(e)) => {
                    this.update(cx, |this, cx| this.say(format!("{e:#}"), true, cx))
                        .ok();
                    return;
                }
                Err(_) => {
                    this.update(cx, |this, cx| {
                        this.say("the save dialog closed unexpectedly", true, cx)
                    })
                    .ok();
                    return;
                }
            };
            let size = bytes.len() as u64;
            let shown = target.display().to_string();
            let written = cx
                .background_spawn(async move {
                    std::fs::write(&target, &bytes).map_err(|e| e.to_string())
                })
                .await;
            this.update(cx, |this, cx| match written {
                Ok(()) => this.say(format!("Wrote {} to {shown}.", human_size(size)), false, cx),
                Err(e) => this.say(format!("{shown}: {e}"), true, cx),
            })
            .ok();
        })
        .detach();
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        // Dialogs and background reads disable the controls that would start a
        // second one, instead of queueing surprises.
        let ready = !self.busy;
        // Built here, from state decoded when the content last changed — the
        // element only ever hands gpui a cached `Arc<Image>`.
        let preview = match &self.preview {
            Preview::None => None,
            Preview::Image { image, label } => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::S1)
                    // Shares the pane's height with the two cards: a floor so
                    // the image stays recognisable, a ceiling so it never
                    // pushes them off a short window.
                    .flex_1()
                    .min_h(size::CARD_MIN_H)
                    .max_h(space::S8 * 5.)
                    .child(
                        div()
                            .text_size(text::SMALL)
                            .text_color(t.text_muted())
                            .child(label.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .p(space::S2)
                            .rounded(radius::MD)
                            .border_1()
                            .border_color(t.divider)
                            // A mid neutral behind the frame, so a transparent
                            // PNG is legible on either ground.
                            .bg(t.neutral.s300)
                            // The id gives the element frame state, which is what
                            // steps an animated GIF along.
                            .child(
                                img(image.clone())
                                    .size_full()
                                    .id("preview-image")
                                    .with_fallback(move || {
                                        note(
                                            &t,
                                            "Could not decode these bytes as an image — they may \
                                             be truncated or corrupt."
                                                .into(),
                                            Note::Bad,
                                        )
                                        .into_any_element()
                                    }),
                            ),
                    )
                    .into_any_element(),
            ),
            Preview::Unsupported(message) => Some(
                note(&t, message.clone(), Note::Info)
                    .px(space::S3)
                    .into_any_element(),
            ),
        };

        let picked = self.picked.as_ref().map(|p| {
            div()
                .flex()
                .flex_col()
                .px(space::S3)
                .py(space::S2)
                .rounded(radius::MD)
                .bg(t.bg)
                .child(meta_row(&t, "name", p.name.clone(), false))
                .child(meta_row(&t, "size", p.size.clone(), false))
                .child(meta_row(&t, "mime", p.mime.clone(), false))
        });

        div()
            .flex()
            .flex_col()
            .gap(space::S4)
            .size_full()
            .p(space::S4)
            .overflow_hidden()
            .child(header(
                &t,
                "Base64 file converter",
                "Encode a file to a data URI, or write base64 back to a file.",
            ))
            .when_some(preview, |d, preview| d.child(preview))
            // ---- file -> base64. The whole section is the drop target.
            .child(
                theme::card(&t)
                    .id("encode-drop")
                    .flex()
                    .flex_col()
                    .gap(space::S2)
                    .flex_1()
                    // A floor, not a slab: two of these plus a preview at
                    // 4 × S8 each no longer fit a short window.
                    .min_h(size::CARD_MIN_H)
                    // At rest this reads as a drop target: a dashed accent-tinted
                    // edge on a washed ground, not a plain card that only
                    // reacts once something is already over it.
                    .border_2()
                    .border_dashed()
                    .border_color(t.accent_wash_strong())
                    .bg(t.accent_wash())
                    .on_drop(
                        cx.listener(|this, paths: &ExternalPaths, _, cx| {
                            this.drop_files(paths, cx)
                        }),
                    )
                    .drag_over::<ExternalPaths>(move |style, _, _, _| {
                        style.border_color(t.accent).bg(t.accent_wash_strong())
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            // The buttons drop under the label rather than off
                            // the card when the pane is narrow.
                            .flex_wrap()
                            .gap(space::S3)
                            .child(theme::section_label(&t, "File to base64").px_0().pt_0())
                            .child(
                                div()
                                    .flex()
                                    .gap(space::S2)
                                    .child(button(
                                        &t,
                                        "pick-file",
                                        "Choose file…",
                                        "folder",
                                        Tone::Primary,
                                        ready,
                                        cx.listener(|this, _, _, cx| this.pick_file(cx)),
                                    ))
                                    .child(button(
                                        &t,
                                        "copy-base64",
                                        "Copy",
                                        "copy",
                                        Tone::Default,
                                        ready,
                                        cx.listener(|this, _, _, cx| this.copy_encoded(cx)),
                                    )),
                            ),
                    )
                    // The drop target says so at rest, not only while a file is
                    // hovering over it.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::S2)
                            .text_size(text::SMALL)
                            .text_color(t.text_muted())
                            .child(theme::icon_tinted("file", text::H5, t.accent))
                            .child(SharedString::from(format!(
                                "Drop a single file anywhere in this box, or choose one — up to {}.",
                                human_size(MAX_BYTES)
                            ))),
                    )
                    .when_some(picked, |d, picked| d.child(picked))
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .child(self.encoded.clone()),
                    ),
            )
            // ---- base64 -> file
            .child(
                theme::card(&t)
                    .flex()
                    .flex_col()
                    .gap(space::S2)
                    .flex_1()
                    .min_h(size::CARD_MIN_H)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .flex_wrap()
                            .gap(space::S3)
                            .child(theme::section_label(&t, "Base64 to file").px_0().pt_0())
                            .child(
                                div()
                                    .flex()
                                    .gap(space::S2)
                                    .child(button(
                                        &t,
                                        "preview-base64",
                                        "Preview",
                                        "file",
                                        Tone::Default,
                                        ready,
                                        cx.listener(|this, _, _, cx| this.preview_decoded(cx)),
                                    ))
                                    .child(button(
                                        &t,
                                        "save-file",
                                        "Save as…",
                                        "save",
                                        Tone::Primary,
                                        ready,
                                        cx.listener(|this, _, _, cx| this.save_decoded(cx)),
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .child(self.to_decode.clone()),
                    ),
            )
            .child(note(
                &t,
                self.status.clone(),
                if self.failed { Note::Bad } else { Note::Info },
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_rfc4648() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(b64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
        // every byte value round-trips
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(b64_decode(&b64_encode(&all)).unwrap(), all);
    }

    #[test]
    fn decode_reports_errors_instead_of_panicking() {
        assert_eq!(b64_decode("Zm9vYmFy").unwrap(), b"foobar");
        // missing padding, embedded whitespace and URL-safe alphabet all pass
        assert_eq!(b64_decode("Zm9v YmFy\n").unwrap(), b"foobar");
        assert_eq!(b64_decode("Zg").unwrap(), b"f");
        assert_eq!(b64_decode("-_8=").unwrap(), vec![0xfb, 0xff]);
        // and the failures are messages, not panics
        assert!(b64_decode("").unwrap_err().contains("nothing to decode"));
        assert!(b64_decode("   \n ").is_err());
        assert!(b64_decode("Zm9v!!").unwrap_err().contains("invalid base64"));
        assert!(b64_decode("data:text/plain;base64,").is_err());
    }

    #[test]
    fn data_uri_carries_mime_and_survives_a_round_trip() {
        let uri = data_uri("text/plain", b"hello");
        assert_eq!(uri, "data:text/plain;base64,aGVsbG8=");
        assert_eq!(mime_of_data_uri(&uri), Some("text/plain"));
        assert_eq!(b64_decode(&uri).unwrap(), b"hello");

        assert_eq!(mime_of_data_uri("aGVsbG8="), None);
        assert_eq!(mime_of_data_uri("data:text/plain,hello"), None);

        assert_eq!(mime_for_path(Path::new("a/b/c.PNG")), "image/png");
        assert_eq!(
            mime_for_path(Path::new("x.unknown")),
            "application/octet-stream"
        );
        assert_eq!(
            mime_for_path(Path::new("noext")),
            "application/octet-stream"
        );

        assert_eq!(
            suggested_name(&data_uri("application/pdf", b"x")),
            "file.pdf"
        );
        assert_eq!(suggested_name("aGVsbG8="), "file.bin");
        assert_eq!(suggested_name(&data_uri("weird/thing", b"x")), "file.bin");
    }

    #[test]
    fn oversized_and_missing_files_report_rather_than_hang() {
        let dir = std::env::temp_dir().join("rp-base64-file-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("small.txt");
        std::fs::write(&file, b"hello").unwrap();
        assert_eq!(read_capped(&file).unwrap(), b"hello");
        assert!(read_capped(&dir).unwrap_err().contains("not a file"));
        assert!(read_capped(&dir.join("missing")).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Smallest real PNG: 1x1, so the header sniff and a decode both have
    /// something honest to work with.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn sniffs_the_formats_gpui_can_decode() {
        assert_eq!(sniff_image_format(PNG_1X1), Some(ImageFormat::Png));
        // A truncated header still sniffs as PNG — the decoder rejects it later —
        // but a header shorter than the magic number does not match at all.
        assert_eq!(sniff_image_format(&PNG_1X1[..20]), Some(ImageFormat::Png));
        assert_eq!(sniff_image_format(&PNG_1X1[..4]), None);
        assert_eq!(
            sniff_image_format(b"\xff\xd8\xffhello"),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(sniff_image_format(b"GIF89a...."), Some(ImageFormat::Gif));
        assert_eq!(
            sniff_image_format(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some(ImageFormat::Webp)
        );
        assert_eq!(sniff_image_format(b"BM\x00\x00"), Some(ImageFormat::Bmp));
        assert_eq!(sniff_image_format(b"II*\0rest"), Some(ImageFormat::Tiff));
        assert_eq!(
            sniff_image_format(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
            Some(ImageFormat::Svg)
        );
        assert_eq!(
            sniff_image_format(b"<?xml version=\"1.0\"?><svg/>"),
            Some(ImageFormat::Svg)
        );
        // Not images, and no panics on empty or non-utf8 input.
        assert_eq!(sniff_image_format(b""), None);
        assert_eq!(sniff_image_format(b"%PDF-1.7"), None);
        assert_eq!(sniff_image_format(b"<html><body>hi"), None);
        assert_eq!(sniff_image_format(&[0xff, 0xfe, 0x80, 0x81]), None);
    }

    #[test]
    fn preview_is_built_once_and_explains_itself_when_it_cannot_be() {
        match preview_of(Some("image/png"), PNG_1X1) {
            Preview::Image { image, label } => {
                assert_eq!(image.format(), ImageFormat::Png);
                assert_eq!(image.bytes(), PNG_1X1);
                assert!(label.contains("image/png"), "{label}");
            }
            _ => panic!("a real png should preview"),
        }
        // Truncated bytes still make an Image — the failure surfaces in gpui's
        // fallback, which is what keeps the rest of the pane alive.
        assert!(matches!(
            preview_of(Some("image/png"), &PNG_1X1[..20]),
            Preview::Image { .. }
        ));
        let Preview::Unsupported(message) = preview_of(Some("application/pdf"), b"%PDF-1.7") else {
            panic!("a pdf has no preview");
        };
        assert!(message.contains("application/pdf"), "{message}");
        let Preview::Unsupported(message) = preview_of(None, b"nope") else {
            panic!("unknown content has no preview");
        };
        assert!(message.contains("this content"), "{message}");
    }

    #[test]
    fn drops_take_exactly_one_file() {
        let dir = std::env::temp_dir().join("rp-base64-drop-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.txt");
        std::fs::write(&file, b"hi").unwrap();

        assert_eq!(dropped_path(std::slice::from_ref(&file)).unwrap(), file);
        assert!(
            dropped_path(&[])
                .unwrap_err()
                .contains("nothing was dropped")
        );
        assert!(
            dropped_path(std::slice::from_ref(&dir))
                .unwrap_err()
                .contains("folder")
        );
        let two = dropped_path(&[file.clone(), file.clone()]).unwrap_err();
        assert!(two.contains("2 files dropped"), "{two}");
        // A path that cannot be read is accepted here and reported by the read.
        let missing = dir.join("missing");
        assert_eq!(
            dropped_path(std::slice::from_ref(&missing)).unwrap(),
            missing
        );
        assert!(read_capped(&missing).is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sizes_read_in_human_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(MAX_BYTES), "16.0 MiB");
    }
}
