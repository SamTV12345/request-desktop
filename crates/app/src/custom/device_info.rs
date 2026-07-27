//! device-information, deliberately reinterpreted.
//!
//! Upstream reports the *browser*: user agent, screen orientation, `navigator`
//! flags. None of that exists here, and faking it would be worse than useless —
//! a bug report saying "Chrome 0.0" is a lie. So this pane reports the
//! **desktop environment** the app is actually running in: OS, architecture,
//! CPU count, app build, window and display geometry, and the directories the
//! app resolves. That is the same job upstream does (tell me where this is
//! running so I can file it), against the runtime we actually have.
//!
//! Everything comes from `std` and gpui; no new dependencies. The Rust target
//! triple is *not* included: reaching it needs a `build.rs` to bake `TARGET`
//! in, and os/arch/family/pointer-width already say the same thing.
use super::{Note, Tone, button, header, meta_row, note};
use gpui::{ClipboardItem, Context, Entity, SharedString, Window, div, prelude::*};
use rp_ui::input::Editor;
use rp_ui::layout::{card, divider, section_label};
use rp_ui::theme;
use rp_ui::tokens::{size, space};

/// The bits of the report that need a live window; everything else is `std`.
/// Split out so the text assembly is testable without one.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
struct Geometry {
    /// Logical window size, in gpui pixels.
    window: (f32, f32),
    scale: f32,
    /// Logical size of the display holding the window, if gpui knows it.
    display: Option<(f32, f32)>,
    displays: usize,
}

fn dir(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| "(not set)".into())
}

/// One heading and its `label -> value` rows. The single source of truth: the
/// grouped view renders it directly and [`info_text`] is only a formatter for
/// it, so the panel and the copyable report can never drift apart.
type Group = (&'static str, Vec<(&'static str, String)>);

fn groups(geo: &Geometry) -> Vec<Group> {
    let (w, h) = geo.window;
    vec![
        (
            "App",
            vec![
                (
                    "app",
                    format!("request-pilot {}", env!("CARGO_PKG_VERSION")),
                ),
                (
                    "build",
                    if cfg!(debug_assertions) {
                        "debug"
                    } else {
                        "release"
                    }
                    .to_string(),
                ),
            ],
        ),
        (
            "System",
            vec![
                ("os", std::env::consts::OS.to_string()),
                ("arch", std::env::consts::ARCH.to_string()),
                ("family", std::env::consts::FAMILY.to_string()),
                ("pointer width", format!("{} bit", usize::BITS)),
                (
                    "endianness",
                    if cfg!(target_endian = "little") {
                        "little"
                    } else {
                        "big"
                    }
                    .to_string(),
                ),
                (
                    "cpu threads",
                    match std::thread::available_parallelism() {
                        Ok(n) => n.to_string(),
                        Err(e) => format!("(unavailable: {e})"),
                    },
                ),
            ],
        ),
        (
            "Window",
            vec![
                ("window", format!("{w:.0} x {h:.0} logical px")),
                ("scale factor", format!("{:.2}", geo.scale)),
                (
                    "window (physical)",
                    format!("{:.0} x {:.0} px", w * geo.scale, h * geo.scale),
                ),
                (
                    "display",
                    match geo.display {
                        Some((dw, dh)) => format!("{dw:.0} x {dh:.0} logical px"),
                        None => "(unknown)".into(),
                    },
                ),
                ("displays", geo.displays.to_string()),
            ],
        ),
        (
            "Paths",
            vec![
                (
                    "home",
                    std::env::var("USERPROFILE")
                        .or_else(|_| std::env::var("HOME"))
                        .unwrap_or_else(|_| "(not set)".into()),
                ),
                ("temp", std::env::temp_dir().display().to_string()),
                (
                    "working dir",
                    std::env::current_dir()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|e| format!("(unavailable: {e})")),
                ),
                (
                    "executable",
                    std::env::current_exe()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|e| format!("(unavailable: {e})")),
                ),
            ],
        ),
        (
            "Environment",
            vec![(
                "shell/locale",
                format!("{} / {}", dir("SHELL"), dir("LANG")),
            )],
        ),
    ]
}

/// The report as text, key column padded so it stays readable in a bug report.
fn info_text(geo: &Geometry) -> String {
    let mut out = String::new();
    for (i, (_, rows)) in groups(geo).iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for (key, value) in rows {
            out.push_str(&format!("{key:<20}{value}\n"));
        }
    }
    out
}

pub struct Pane {
    /// Read-only so the whole report can be selected and pasted into a bug report.
    view: Entity<Editor>,
    /// Last geometry rendered, so the editor is only rewritten when it changes
    /// (rewriting drops the user's selection).
    geo: Option<Geometry>,
    status: SharedString,
    /// Set once the report has been copied, so the note reads as a success
    /// rather than as another instruction.
    copied: bool,
}

impl Pane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            view: cx.new(|cx| Editor::new(cx, "device information").read_only()),
            geo: None,
            status: "Select any of it, or copy the lot.".into(),
            copied: false,
        }
    }

    fn copy_all(&mut self, cx: &mut Context<Self>) {
        let text = self.view.read(cx).text().to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.status = "Copied the report to the clipboard.".into();
        self.copied = true;
        cx.notify();
    }
}

impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        // `new` only gets a `Context`, so window geometry is read here — it is
        // also the only place that sees a resize.
        let size = window.viewport_size();
        let geo = Geometry {
            window: (f32::from(size.width), f32::from(size.height)),
            scale: window.scale_factor(),
            display: window.display(cx).map(|d| {
                let b = d.bounds();
                (f32::from(b.size.width), f32::from(b.size.height))
            }),
            displays: cx.displays().len(),
        };
        if self.geo != Some(geo) {
            self.geo = Some(geo);
            let text = info_text(&geo);
            self.view.update(cx, |v, cx| v.set_text(text, cx));
        }

        // ponytail: a `for` loop, not `map` — a closure would have to capture
        // the group list by reference across the divider pushes.
        let mut sections = Vec::new();
        for (i, (heading, rows)) in groups(&geo).into_iter().enumerate() {
            if i > 0 {
                sections.push(divider(t).into_any_element());
            }
            let group = div().flex().flex_col().child(section_label(t, heading));
            let mut body = div().flex().flex_col().px(space::S3);
            for (label, value) in rows {
                body = body.child(meta_row(t, label, value, false));
            }
            sections.push(group.child(body).pb(space::S3).into_any_element());
        }

        div()
            .flex()
            .flex_col()
            .gap(space::S4)
            .size_full()
            .p(space::S4)
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(space::S4)
                    .child(
                        header(
                            t,
                            "Device information",
                            "The desktop environment this build is running in — upstream's \
                             browser report has no meaning here.",
                        )
                        // The subtitle wraps; the button keeps its size.
                        .flex_1()
                        .min_w_0(),
                    )
                    .child(button(
                        t,
                        "copy-device-info",
                        "Copy all",
                        "copy",
                        Tone::Primary,
                        true,
                        cx.listener(|this, _, _, cx| this.copy_all(cx)),
                    )),
            )
            .child(
                card(t)
                    .id("device-info-groups")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .p_0()
                    .overflow_y_scroll()
                    .children(sections),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::S1)
                    // The report shares the leftover height with the group list
                    // instead of reserving a fixed slab of it.
                    .flex_1()
                    .min_h(size::CARD_MIN_H)
                    .max_h(space::S8 * 5.)
                    .child(section_label(t, "Copyable report").px_0().pt_0())
                    // Kept as a real editor: selecting a couple of lines out of
                    // it is how this ends up in a bug report.
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .child(self.view.clone()),
                    ),
            )
            .child(note(
                t,
                self.status.clone(),
                if self.copied { Note::Good } else { Note::Info },
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_covers_the_environment_and_the_window() {
        let geo = Geometry {
            window: (1280.0, 800.0),
            scale: 1.5,
            display: Some((2560.0, 1440.0)),
            displays: 2,
        };
        let text = info_text(&geo);
        for expected in [
            concat!("request-pilot ", env!("CARGO_PKG_VERSION")),
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::env::consts::FAMILY,
            "1280 x 800 logical px",
            "scale factor        1.50",
            "1920 x 1200 px", // physical = logical * scale
            "2560 x 1440 logical px",
            "displays            2",
            "cpu threads",
            "temp",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
        }
        // no key is left with an empty value
        for l in text.lines().filter(|l| !l.is_empty()) {
            assert!(l.len() > 20, "empty value: {l:?}");
        }
    }

    #[test]
    fn unknown_display_is_reported_not_guessed() {
        let text = info_text(&Geometry::default());
        assert!(text.contains("display             (unknown)"), "{text}");
        assert!(text.contains("displays            0"), "{text}");
        // a zero-size window must not produce NaN or a panic
        assert!(text.contains("0 x 0 logical px"), "{text}");
    }
}
