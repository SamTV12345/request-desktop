//! Stopwatch. `Kind::Custom` because the output changes with the wall clock,
//! not with the inputs, so the `fields in -> text out` contract cannot express
//! it.
//!
//! The displayed value is always derived from `Instant`s — never accumulated
//! per tick — so the tick rate is a refresh rate, not the source of truth. A
//! dropped frame or a late timer costs nothing.
use super::{Note, Tone, button, chip, header, note};
use gpui::{Context, IntoElement, ParentElement, Styled, Task, Window, div, prelude::*};
use rp_ui::theme::{self, FONT_MONO};
use rp_ui::tokens::{radius, space, text};
use std::time::{Duration, Instant};

/// Refresh interval. Matches the displayed precision (centiseconds) so we do
/// not render digits we never update.
const TICK: Duration = Duration::from_millis(10);

/// Start/pause bookkeeping, with time injected so it is testable without
/// sleeping.
#[derive(Default)]
struct Clock {
    /// When the current run began, if running.
    started: Option<Instant>,
    /// Total of all completed runs.
    accumulated: Duration,
}

impl Clock {
    fn running(&self) -> bool {
        self.started.is_some()
    }

    fn elapsed_at(&self, now: Instant) -> Duration {
        match self.started {
            Some(start) => self.accumulated + now.saturating_duration_since(start),
            None => self.accumulated,
        }
    }

    fn start_at(&mut self, now: Instant) {
        if self.started.is_none() {
            self.started = Some(now);
        }
    }

    fn pause_at(&mut self, now: Instant) {
        if let Some(start) = self.started.take() {
            self.accumulated += now.saturating_duration_since(start);
        }
    }

    /// Reset stops the clock as well, matching upstream where Reset zeroes the
    /// counter while the raf loop keeps whatever state it had — except here
    /// leaving it running after a reset would mean a ticking task nobody asked
    /// for.
    fn reset(&mut self) {
        self.started = None;
        self.accumulated = Duration::ZERO;
    }
}

/// `[HH:]MM:SS.cc`, hours only once there are any — upstream's `formatMs`, with
/// centiseconds instead of milliseconds because that is what we refresh.
fn format(d: Duration) -> String {
    let cs = d.as_millis() / 10;
    let (cs, secs) = (cs % 100, cs / 100);
    let (s, mins) = (secs % 60, secs / 60);
    let (m, hours) = (mins % 60, mins / 60);
    if hours > 0 {
        format!("{hours:02}:{m:02}:{s:02}.{cs:02}")
    } else {
        format!("{m:02}:{s:02}.{cs:02}")
    }
}

pub struct Pane {
    clock: Clock,
    /// Held so dropping the pane cancels the refresh loop. `None` while
    /// paused — a stopwatch nobody started must not wake the executor.
    tick: Option<Task<()>>,
}

impl Pane {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            clock: Clock::default(),
            tick: None,
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if self.clock.running() {
            return;
        }
        self.clock.start_at(Instant::now());
        // The task holds a weak entity, so it also stops on its own if the pane
        // is dropped between ticks rather than notifying a dead entity.
        self.tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let still_running = this.update(cx, |this, cx| {
                    cx.notify();
                    this.clock.running()
                });
                // `Err` means the pane is gone; stop instead of spinning.
                if !matches!(still_running, Ok(true)) {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn pause(&mut self, cx: &mut Context<Self>) {
        self.clock.pause_at(Instant::now());
        self.tick = None;
        cx.notify();
    }

    fn reset(&mut self, cx: &mut Context<Self>) {
        self.clock.reset();
        self.tick = None;
        cx.notify();
    }
}

impl Render for Pane {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        let running = self.clock.running();
        let elapsed = format(self.clock.elapsed_at(Instant::now()));
        // Running versus stopped is legible from the container, not from the
        // button label: a live clock is accent-bordered on an accent wash with
        // full-strength digits, a stopped one is an inert surface.
        let (frame_border, frame_bg, digits) = if running {
            (t.accent, t.accent_wash_strong(), t.text)
        } else {
            (t.divider, t.surface, t.text_muted())
        };
        let toggle = if running {
            button(
                t,
                "chrono-pause",
                "Stop",
                "stop",
                Tone::Danger,
                true,
                cx.listener(|this, _, _, cx| this.pause(cx)),
            )
        } else {
            button(
                t,
                "chrono-start",
                "Start",
                "play",
                Tone::Primary,
                true,
                cx.listener(|this, _, _, cx| this.start(cx)),
            )
        };

        div()
            .flex()
            .flex_col()
            .gap(space::S4)
            .size_full()
            .p(space::S4)
            .child(header(
                t,
                "Chronometer",
                "Monitor the duration of a thing. Basically a stopwatch.",
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(space::S3)
                    .py(space::S8)
                    .bg(frame_bg)
                    .border_1()
                    .border_color(frame_border)
                    .rounded(radius::CARD)
                    .child(chip(
                        t,
                        if running { "running" } else { "stopped" },
                        running,
                    ))
                    .child(
                        // Mono, so the centiseconds do not shove the digits left
                        // and right as they roll over.
                        div()
                            .font_family(FONT_MONO)
                            .text_size(text::H1)
                            .line_height(text::H1)
                            .text_color(digits)
                            .child(elapsed),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(space::S2)
                    .justify_center()
                    .child(toggle)
                    .child(button(
                        t,
                        "chrono-reset",
                        "Reset",
                        "trash",
                        Tone::Default,
                        true,
                        cx.listener(|this, _, _, cx| this.reset(cx)),
                    )),
            )
            .child(div().flex().justify_center().child(note(
                t,
                if running {
                    "Running — refreshed every 10 ms, measured from a monotonic clock.".into()
                } else {
                    "Stopped — no timer is running.".into()
                },
                Note::Info,
            )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn format_boundaries() {
        assert_eq!(format(Duration::ZERO), "00:00.00");
        assert_eq!(format(Duration::from_millis(9)), "00:00.00");
        assert_eq!(format(Duration::from_millis(10)), "00:00.01");
        assert_eq!(format(Duration::from_millis(999)), "00:00.99");
        assert_eq!(format(Duration::from_secs(1)), "00:01.00");
        assert_eq!(format(Duration::from_secs(60)), "01:00.00");
        assert_eq!(format(Duration::from_secs(3599)), "59:59.00");
        assert_eq!(format(Duration::from_secs(3600)), "01:00:00.00");
        assert_eq!(format(Duration::from_millis(3_661_230)), "01:01:01.23");
        assert_eq!(format(Duration::from_secs(360_000)), "100:00:00.00");
    }

    #[test]
    fn idle_clock_stays_at_zero() {
        let base = Instant::now();
        let clock = Clock::default();
        assert!(!clock.running());
        assert_eq!(clock.elapsed_at(at(base, 5_000)), Duration::ZERO);
    }

    #[test]
    fn elapsed_is_derived_from_the_clock_not_the_ticks() {
        let base = Instant::now();
        let mut clock = Clock::default();
        clock.start_at(base);
        // No ticks happened at all; elapsed still tracks real time.
        assert_eq!(
            clock.elapsed_at(at(base, 1_234)),
            Duration::from_millis(1234)
        );
    }

    #[test]
    fn pause_resume_accumulates_only_running_time() {
        let base = Instant::now();
        let mut clock = Clock::default();
        clock.start_at(base);
        clock.pause_at(at(base, 1_000));
        // 4 seconds of being paused must not count.
        assert_eq!(clock.elapsed_at(at(base, 5_000)), Duration::from_secs(1));
        clock.start_at(at(base, 5_000));
        assert_eq!(
            clock.elapsed_at(at(base, 5_500)),
            Duration::from_millis(1500)
        );
        clock.pause_at(at(base, 6_000));
        assert_eq!(clock.elapsed_at(at(base, 9_999)), Duration::from_secs(2));
    }

    #[test]
    fn reset_zeroes_and_stops() {
        let base = Instant::now();
        let mut clock = Clock::default();
        clock.start_at(base);
        clock.reset();
        assert!(!clock.running());
        assert_eq!(clock.elapsed_at(at(base, 3_000)), Duration::ZERO);
        // And a fresh run after a reset starts from zero.
        clock.start_at(at(base, 3_000));
        assert_eq!(
            clock.elapsed_at(at(base, 3_250)),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn repeated_start_or_pause_is_idempotent() {
        let base = Instant::now();
        let mut clock = Clock::default();
        clock.start_at(base);
        clock.start_at(at(base, 500)); // must not rebase the start instant
        assert_eq!(clock.elapsed_at(at(base, 1_000)), Duration::from_secs(1));
        clock.pause_at(at(base, 1_000));
        clock.pause_at(at(base, 2_000)); // must not add paused time
        assert_eq!(clock.elapsed_at(at(base, 3_000)), Duration::from_secs(1));
    }
}
