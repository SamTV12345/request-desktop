//! Time tracking: start/stop a timer, browse what was booked by day and by
//! month, search it, copy a summary out.
//!
//! Two processes write the same table — this app and the MCP server an agent
//! talks to — so the view is a *reader* that refreshes: it polls
//! `Store::change_counter()` (SQLite's `PRAGMA data_version`) about once a
//! second and only re-queries when that number moved. Elapsed time for a
//! running entry is always derived from `started_at` and the current clock,
//! never accumulated per tick, so a late or dropped tick costs nothing but a
//! stale pixel (same reasoning as `custom/chronometer.rs`).
//!
//! ponytail: dates here are civil dates derived from the UTC clock, while the
//! store groups days with SQLite's `'localtime'`. The two disagree for the
//! length of the UTC offset around midnight. std has no local-time API and no
//! new dependency is allowed, so the upgrade path is a `Store` method that
//! answers `date('now','localtime')` (or exposes the offset) — one line in the
//! store, and `today()` calls it.
use crate::editor::Editor;
use crate::text_input::TextInput;
use crate::theme::{control, radius, space, text};
use crate::{Persist, theme};
use gpui::{
    App, Context, Div, Entity, IntoElement, ParentElement, Pixels, Rgba, SharedString, Stateful,
    Styled, Task, Window, div, prelude::*, px,
};
use rp_store::{DaySummary, Entry, Store};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How often the change counter is checked. Fast enough that an entry booked by
/// the agent feels immediate, slow enough to be free.
const POLL: Duration = Duration::from_secs(1);

/// Search results shown at once. A tracker is not a log viewer.
const SEARCH_LIMIT: usize = 100;

/// The one message for "there is no database". Named so the view can recognise
/// it and explain it calmly instead of shouting a query error at the user.
const NO_STORE: &str = "the database could not be opened — time tracking is unavailable";

// Layout dimensions: not padding or gaps, so not on the spacing scale, but kept
// on the same 4-px grid. They exist to align columns down the list.
const RAIL_W: Pixels = px(320.);
const COL_RANGE: Pixels = px(104.);
const COL_TOTAL: Pixels = px(72.);
const COL_DATE: Pixels = px(96.);
const SUMMARY_H: Pixels = px(180.);

// ---------------------------------------------------------------------------
// Pure logic. Everything below this line is testable without a window, which is
// where the bugs would otherwise live.
// ---------------------------------------------------------------------------

/// A civil date. No validation invariant is enforced by the type; construct it
/// through `parse_date` or the stepping helpers, which cannot produce an
/// invalid one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ymd {
    pub y: i32,
    pub m: u32,
    pub d: u32,
}

fn is_leap(y: i32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(y) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Days since 1970-01-01. Hinnant's `days_from_civil` — the standard
/// no-dependency way to do date arithmetic, and correct for negative years.
fn days_from_civil(date: Ymd) -> i64 {
    let (y, m, d) = (date.y as i64, date.m as i64, date.d as i64);
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Inverse of `days_from_civil`.
fn civil_from_days(z: i64) -> Ymd {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    Ymd {
        y: (if m <= 2 { y + 1 } else { y }) as i32,
        m: m as u32,
        d: d as u32,
    }
}

/// Move `delta` days, crossing month and year boundaries by construction.
fn step_day(date: Ymd, delta: i64) -> Ymd {
    civil_from_days(days_from_civil(date) + delta)
}

/// Move `delta` months, clamping the day into the target month: 31 January
/// minus a month is 31 December, plus a month is 28 or 29 February.
fn step_month(date: Ymd, delta: i64) -> Ymd {
    let total = date.y as i64 * 12 + (date.m as i64 - 1) + delta;
    let y = total.div_euclid(12) as i32;
    let m = total.rem_euclid(12) as u32 + 1;
    Ymd {
        y,
        m,
        d: date.d.min(days_in_month(y, m)),
    }
}

fn fmt_date(date: Ymd) -> String {
    format!("{:04}-{:02}-{:02}", date.y, date.m, date.d)
}

fn fmt_month(date: Ymd) -> String {
    format!("{:04}-{:02}", date.y, date.m)
}

/// `YYYY-MM-DD`, or `YYYY-MM` for the first of a month. Rejects anything the
/// calendar does not have, which is the whole point: 2023-02-29 is not a date.
fn parse_date(text: &str) -> Option<Ymd> {
    let text = text.trim();
    let mut parts = text.split('-');
    let y: i32 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = match parts.next() {
        Some(d) => d.parse().ok()?,
        None => 1,
    };
    if parts.next().is_some() || !(1..=9999).contains(&y) || !(1..=12).contains(&m) {
        return None;
    }
    (1..=days_in_month(y, m))
        .contains(&d)
        .then_some(Ymd { y, m, d })
}

/// Readable total: `Hh Mm`, minute resolution, no days unit — 30 hours is
/// `30h 00m`, because a timesheet is read in hours.
fn fmt_hm(seconds: i64) -> String {
    let mins = seconds.max(0) / 60;
    format!("{}h {:02}m", mins / 60, mins % 60)
}

/// Running elapsed, `HH:MM:SS`, because a live timer needs its seconds.
fn fmt_hms(seconds: i64) -> String {
    let s = seconds.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Wall-clock `HH:MM` of a Unix timestamp.
fn fmt_clock(ts: i64) -> String {
    let secs = ts.rem_euclid(86_400);
    format!("{:02}:{:02}", secs / 3600, (secs / 60) % 60)
}

/// Should the view re-query? `None` for `current` means the counter could not
/// be read at all — reloading would only fail the same way, so don't. Never
/// having seen a counter means the first successful poll reloads.
fn should_reload(seen: Option<i64>, current: Option<i64>) -> bool {
    match (seen, current) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(a), Some(b)) => a != b,
    }
}

/// The text the copy button hands over — the same shape a user would ask their
/// agent for. One day when `days` has one element, a month when it has many.
fn summary_text(title: &str, days: &[DaySummary]) -> String {
    let total: i64 = days.iter().map(|d| d.total_seconds).sum();
    let mut per_task: Vec<(String, i64)> = Vec::new();
    for day in days {
        for (task, secs) in &day.per_task {
            match per_task.iter_mut().find(|(n, _)| n == task) {
                Some(slot) => slot.1 += secs,
                None => per_task.push((task.clone(), *secs)),
            }
        }
    }
    per_task.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut out = format!("{title} — {}\n", fmt_hm(total));
    if days.len() > 1 {
        for day in days {
            out.push_str(&format!("  {}  {}\n", day.date, fmt_hm(day.total_seconds)));
        }
        out.push('\n');
    }
    for (task, secs) in &per_task {
        out.push_str(&format!("  {:>9}  {task}\n", fmt_hm(*secs)));
    }
    if per_task.is_empty() {
        out.push_str("  nothing booked\n");
    }
    out
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Today, as the *store* defines it.
///
/// Deriving this from the UTC clock was a real bug: every query groups days with
/// SQLite's `'localtime'`, so a UTC-derived "today" disagreed with its own
/// summaries for the width of the UTC offset either side of midnight — which in
/// a time tracker means looking at the wrong day's work. `Store::local_date` is
/// the single definition both sides share. The UTC fallback only applies when
/// there is no store at all, where nothing can be queried anyway.
fn today(store: Option<&Store>) -> Ymd {
    let now = now();
    store
        .and_then(|s| s.local_date(now).ok())
        .and_then(|d| parse_date(&d))
        .unwrap_or_else(|| civil_from_days(now.div_euclid(86_400)))
}

// ---------------------------------------------------------------------------
// The view.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Day,
    Month,
}

pub struct Track {
    task: Entity<TextInput>,
    note: Entity<TextInput>,
    search: Entity<TextInput>,
    jump: Entity<TextInput>,
    /// Read-only, so the summary can be selected and copied with Ctrl+C.
    summary: Entity<Editor>,
    date: Ymd,
    /// Whether `date` has been resolved through the store's local-time
    /// definition rather than the UTC fallback.
    date_from_store: bool,
    scope: Scope,
    running: Option<Entry>,
    entries: Vec<Entry>,
    month: Vec<DaySummary>,
    tasks: Vec<String>,
    hits: Vec<Entry>,
    /// Whatever went wrong last, shown in the panel. A failing store call must
    /// never be a panic: the query layer can be missing, the database can be
    /// gone.
    error: Option<SharedString>,
    /// Entry the pointer is over, so its delete affordance can appear. Real
    /// state rather than a hover style, because an invisible button that still
    /// takes clicks is an accidental-delete generator.
    hovered: Option<i64>,
    /// Last change counter we queried against.
    seen: Option<i64>,
    /// Held so dropping the view — or hiding the section — cancels the poll.
    poll: Option<Task<()>>,
}

impl Track {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let task = cx.new(|cx| TextInput::new(cx, "What are you working on?"));
        let note = cx.new(|cx| TextInput::new(cx, "note (optional)"));
        let search = cx.new(|cx| TextInput::new(cx, "Search tasks and notes…"));
        let jump = cx.new(|cx| TextInput::new(cx, "YYYY-MM-DD"));
        let summary = cx.new(|cx| Editor::new(cx, "no time booked").read_only());
        for input in [&task, &note, &jump] {
            cx.observe(input, |_, _, cx| cx.notify()).detach();
        }
        // Searching re-queries; it is a local SQLite LIKE over a small table.
        cx.observe(&search, |this, _, cx| this.reload(cx)).detach();
        Self {
            task,
            note,
            search,
            jump,
            summary,
            date: today(None),
            date_from_store: false,
            scope: Scope::Day,
            running: None,
            entries: Vec::new(),
            month: Vec::new(),
            tasks: Vec::new(),
            hits: Vec::new(),
            error: None,
            hovered: None,
            seen: None,
            poll: None,
        }
    }

    /// Start or stop polling. Called when the section is shown or hidden, so a
    /// hidden panel costs nothing.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if visible == self.poll.is_some() {
            return;
        }
        if !visible {
            self.poll = None;
            return;
        }
        self.reload(cx);
        self.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                // `Err` means the view is gone; stop instead of spinning.
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    /// Run `f` against the store, turning "no persistence" and any store error
    /// into a message in the panel. The query layer is allowed to be broken; the
    /// app is not allowed to crash because of it.
    fn with_store<T>(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Store) -> anyhow::Result<T>,
    ) -> Option<T> {
        let missing = SharedString::from(NO_STORE);
        let outcome = if cx.has_global::<Persist>() {
            match cx.global_mut::<Persist>().store.as_mut() {
                Some(store) => f(store).map_err(|e| SharedString::from(format!("{e:#}"))),
                None => Err(missing),
            }
        } else {
            Err(missing)
        };
        match outcome {
            Ok(value) => Some(value),
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                None
            }
        }
    }

    /// One poll: re-query only when another connection wrote, but still repaint
    /// while a timer runs so the elapsed value keeps climbing. Deliberately
    /// silent about errors — a broken store must not repaint a message a second.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let current = cx
            .try_global::<Persist>()
            .and_then(|p| p.store.as_ref())
            .and_then(|store| store.change_counter().ok());
        if should_reload(self.seen, current) {
            self.reload(cx);
        } else if self.running.is_some() {
            cx.notify();
        }
    }

    /// Re-read everything the panel shows. Cheap enough to do wholesale: it is
    /// a handful of indexed queries against a local file.
    fn reload(&mut self, cx: &mut Context<Self>) {
        let now = now();
        // The constructor has no store handle, so the initial date is the UTC
        // guess. Correct it once, before the first query, or the very first view
        // can show the wrong day near midnight.
        if !self.date_from_store {
            if let Some(local) = self.with_store(cx, |s| Ok(today(Some(s)))) {
                self.date = local;
            }
            self.date_from_store = true;
        }
        let date = fmt_date(self.date);
        let month = fmt_month(self.date);
        let needle = self.search.read(cx).text().trim().to_string();

        let loaded = self.with_store(cx, |store| {
            Ok((
                store.change_counter()?,
                store.track_running()?,
                store.track_entries_on(&date)?,
                store.track_summary_day(&date, now)?,
                store.track_summary_month(&month, now)?,
                store.track_tasks()?,
                if needle.is_empty() {
                    Vec::new()
                } else {
                    store.track_search(&needle, SEARCH_LIMIT)?
                },
            ))
        });
        let Some((counter, running, entries, day, month_days, tasks, hits)) = loaded else {
            return;
        };
        self.seen = Some(counter);
        self.running = running;
        self.entries = entries;
        self.tasks = tasks.into_iter().map(|t| t.name).collect();
        self.hits = hits;
        let text = match self.scope {
            Scope::Day => summary_text(&date, std::slice::from_ref(&day)),
            Scope::Month => summary_text(&month, &month_days),
        };
        self.month = month_days;
        self.error = None;
        self.summary.update(cx, |e, cx| e.set_text(text, cx));
        cx.notify();
    }

    /// Reload only when the write went through, so a failed call keeps its
    /// message instead of having it cleared by the refresh.
    fn start_or_stop(&mut self, cx: &mut Context<Self>) {
        let done = if self.running.is_some() {
            self.with_store(cx, |store| store.track_stop(now()))
                .is_some()
        } else {
            let task = self.task.read(cx).text().trim().to_string();
            if task.is_empty() {
                self.error = Some("type a task name first".into());
                cx.notify();
                return;
            }
            let note = self.note.read(cx).text().trim().to_string();
            let note = (!note.is_empty()).then_some(note);
            self.with_store(cx, |store| {
                store.track_start(&task, note.as_deref(), "app", now())
            })
            .is_some()
        };
        if done {
            self.reload(cx);
        }
    }

    fn delete(&mut self, id: i64, cx: &mut Context<Self>) {
        if self
            .with_store(cx, |store| store.track_delete_entry(id))
            .is_some()
        {
            self.reload(cx);
        }
    }

    fn go_to(&mut self, date: Ymd, scope: Scope, cx: &mut Context<Self>) {
        self.date = date;
        self.scope = scope;
        self.reload(cx);
    }

    fn jump(&mut self, cx: &mut Context<Self>) {
        let text = self.jump.read(cx).text().to_string();
        match parse_date(&text) {
            Some(date) => self.go_to(date, self.scope, cx),
            None => {
                self.error = Some(format!("{text:?} is not a date — use YYYY-MM-DD").into());
                cx.notify();
            }
        }
    }

    // -- rendering ----------------------------------------------------------

    /// The hero. A running timer has to be readable across the room; nothing
    /// running is an invitation, not an empty box.
    fn hero(&self, now: i64) -> impl IntoElement + use<> {
        let Some(entry) = self.running.clone() else {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .gap(space::XXS)
                .p(space::MD)
                .rounded(radius::SM)
                .bg(theme::SUNKEN)
                .border_1()
                .border_color(theme::BORDER)
                .child(theme::icon_sized("clock", text::H4).text_color(theme::TEXT_TERTIARY))
                .child(heading("No timer running"))
                .child(hint("Name a task below and press Start."))
                .into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap(space::XS)
            .p(space::MD)
            .rounded(radius::SM)
            .bg(theme::ACCENT_WASH)
            .border_1()
            .border_color(theme::ACCENT)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::XS)
                    // ponytail: the dot is the smallest step of the spacing
                    // scale used as a size; there is no dedicated dot token.
                    .child(div().size(space::XS).rounded(radius::PILL).bg(theme::SUCCESS))
                    .child(
                        div()
                            .text_size(text::SMALL)
                            .line_height(text::SMALL_LINE)
                            .text_color(theme::TEXT_SECONDARY)
                            .child("Running"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::SM)
                    .child(
                        // Mono, so the digits keep their column as they tick.
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(text::H3)
                            .line_height(text::H3_LINE)
                            .text_color(theme::TEXT)
                            .child(SharedString::from(fmt_hms(entry.seconds(now)))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(text::BODY)
                                    .line_height(text::BODY_LINE)
                                    .text_color(theme::TEXT)
                                    .truncate()
                                    .child(SharedString::from(entry.task.clone())),
                            )
                            .when_some(entry.note.clone(), |d, note| {
                                d.child(hint(SharedString::from(note)).truncate())
                            }),
                    ),
            )
            .into_any_element()
    }

    fn timer_block(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let now = now();
        let running = self.running.is_some();
        let mut chips = Vec::new();
        for (i, name) in self.tasks.iter().take(20).enumerate() {
            let label = SharedString::from(name.clone());
            let pick = label.clone();
            chips.push(quiet_button(
                SharedString::from(format!("task-{i}")),
                None,
                label,
                cx.listener(move |this, _, _, cx| {
                    let pick = pick.clone();
                    this.task.update(cx, |t, cx| t.set_text(pick, cx));
                }),
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap(space::SM)
            .p(space::MD)
            .border_b_1()
            .border_color(theme::BORDER)
            .child(self.hero(now))
            .child(self.task.clone())
            .child(self.note.clone())
            .child(primary_button(
                "track-toggle".into(),
                if running { "stop" } else { "play" },
                if running { "Stop" } else { "Start" }.into(),
                if running { theme::DANGER } else { theme::ACCENT },
                if running {
                    theme::DANGER_HOVER
                } else {
                    theme::ACCENT_HOVER
                },
                cx.listener(|this, _, _, cx| this.start_or_stop(cx)),
            ))
            .when(!chips.is_empty(), |d| {
                d.child(theme::section_label("Recent tasks").px_0().pt(space::XXS))
                    .child(div().flex().flex_wrap().gap(space::XXS).children(chips))
            })
    }

    fn nav_block(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let day = self.scope == Scope::Day;
        let shown = match self.scope {
            Scope::Day => fmt_date(self.date),
            Scope::Month => fmt_month(self.date),
        };
        div()
            .flex()
            .flex_col()
            .gap(space::XS)
            .p(space::MD)
            .border_b_1()
            .border_color(theme::BORDER)
            // The date is the answer to "what did I do on 1 February", so it
            // reads as a heading with the steppers either side of it.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::XS)
                    .child(icon_button(
                        "prev-day".into(),
                        "chevron_left",
                        cx.listener(|this, _, _, cx| {
                            let (d, s) = (step_day(this.date, -1), this.scope);
                            this.go_to(d, s, cx)
                        }),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .text_center()
                            .text_size(text::H5)
                            .line_height(text::H5_LINE)
                            .text_color(theme::TEXT)
                            .child(SharedString::from(shown)),
                    )
                    .child(icon_button(
                        "next-day".into(),
                        "chevron_right",
                        cx.listener(|this, _, _, cx| {
                            let (d, s) = (step_day(this.date, 1), this.scope);
                            this.go_to(d, s, cx)
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::XS)
                    .child(icon_button(
                        "prev-month".into(),
                        "chevron_left",
                        cx.listener(|this, _, _, cx| {
                            let (d, s) = (step_month(this.date, -1), this.scope);
                            this.go_to(d, s, cx)
                        }),
                    ))
                    .child(div().flex_1().text_center().child(hint("month")))
                    .child(icon_button(
                        "next-month".into(),
                        "chevron_right",
                        cx.listener(|this, _, _, cx| {
                            let (d, s) = (step_month(this.date, 1), this.scope);
                            this.go_to(d, s, cx)
                        }),
                    )),
            )
            .child(
                // One segmented control, not three loose chips.
                div()
                    .flex()
                    .rounded(radius::XS)
                    .border_1()
                    .border_color(theme::BORDER)
                    .overflow_hidden()
                    .child(segment(
                        "scope-day".into(),
                        "Day".into(),
                        day,
                        true,
                        cx.listener(|this, _, _, cx| {
                            let date = this.date;
                            this.go_to(date, Scope::Day, cx)
                        }),
                    ))
                    .child(segment(
                        "scope-month".into(),
                        "Month".into(),
                        !day,
                        false,
                        cx.listener(|this, _, _, cx| {
                            let date = this.date;
                            this.go_to(date, Scope::Month, cx)
                        }),
                    ))
                    .child(segment(
                        "today".into(),
                        "Today".into(),
                        false,
                        false,
                        cx.listener(|this, _, _, cx| {
                            let scope = this.scope;
                            let today = this
                                .with_store(cx, |s| Ok(today(Some(s))))
                                .unwrap_or(this.date);
                            this.go_to(today, scope, cx)
                        }),
                    )),
            )
            .child(
                field("calendar", self.jump.clone().into_any_element()).child(quiet_button(
                    "jump".into(),
                    None,
                    "Go".into(),
                    cx.listener(|this, _, _, cx| this.jump(cx)),
                )),
            )
    }

    /// The left column: timer, navigation, search results.
    fn list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let needle = self.search.read(cx).text().trim().to_string();
        let mut hits = Vec::with_capacity(self.hits.len());
        for entry in &self.hits {
            let date = fmt_date(civil_from_days(entry.started_at.div_euclid(86_400)));
            let jump = parse_date(&date);
            hits.push(
                div()
                    .id(SharedString::from(format!("hit-{}", entry.id)))
                    .flex()
                    .flex_col()
                    .px(space::MD)
                    .py(space::XS)
                    .border_b_1()
                    .border_color(theme::DIVIDER)
                    .hover(|s| s.bg(theme::FILL))
                    .child(
                        div()
                            .text_size(text::BODY)
                            .line_height(text::BODY_LINE)
                            .text_color(theme::TEXT)
                            .truncate()
                            .child(SharedString::from(entry.task.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(space::XS)
                            .text_size(text::SMALL)
                            .line_height(text::SMALL_LINE)
                            .text_color(theme::TEXT_TERTIARY)
                            .child(
                                div()
                                    .font_family(theme::FONT_MONO)
                                    .child(SharedString::from(date)),
                            )
                            .child(SharedString::from(fmt_hm(entry.seconds(now())))),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(date) = jump {
                            this.go_to(date, Scope::Day, cx);
                        }
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .w(RAIL_W)
            .h_full()
            .bg(theme::SURFACE)
            .border_r_1()
            .border_color(theme::BORDER)
            .child(self.timer_block(cx))
            .child(self.nav_block(cx))
            .child(
                div()
                    .px(space::MD)
                    .pt(space::MD)
                    .child(field("search", self.search.clone().into_any_element())),
            )
            .child(
                div()
                    .px(space::MD)
                    .py(space::XS)
                    .child(hint(SharedString::from(if needle.is_empty() {
                        "Type to search every day".to_string()
                    } else {
                        format!("{} matching", self.hits.len())
                    }))),
            )
            .child(
                div()
                    .id("track-hits")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .children(hits),
            )
    }

    /// The detail pane: the day's entries, or the month's per-day totals.
    fn detail(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let now = now();
        let mut rows = Vec::new();
        let total: i64 = match self.scope {
            Scope::Day => self.entries.iter().map(|e| e.seconds(now)).sum(),
            Scope::Month => self.month.iter().map(|d| d.total_seconds).sum(),
        };
        match self.scope {
            Scope::Day => {
                for entry in &self.entries {
                    rows.push(self.entry_row(entry, now, cx).into_any_element());
                }
            }
            Scope::Month => {
                for day in &self.month {
                    let jump = parse_date(&day.date);
                    let tasks = day
                        .per_task
                        .iter()
                        .map(|(t, _)| t.as_str())
                        .collect::<Vec<_>>()
                        .join(", ");
                    rows.push(
                        div()
                            .id(SharedString::from(format!("day-{}", day.date)))
                            .flex()
                            .items_center()
                            .gap(space::MD)
                            .px(space::MD)
                            .py(space::XS)
                            .border_b_1()
                            .border_color(theme::DIVIDER)
                            .text_size(text::BODY)
                            .line_height(text::BODY_LINE)
                            .hover(|s| s.bg(theme::FILL))
                            .child(
                                div()
                                    .w(COL_DATE)
                                    .flex_none()
                                    .font_family(theme::FONT_MONO)
                                    .text_color(theme::TEXT)
                                    .child(SharedString::from(day.date.clone())),
                            )
                            // Totals in their own right-aligned column, so a
                            // heavy day is found by scanning down.
                            .child(
                                div()
                                    .w(COL_TOTAL)
                                    .flex_none()
                                    .text_right()
                                    .font_family(theme::FONT_MONO)
                                    .text_color(theme::TEXT)
                                    .child(SharedString::from(fmt_hm(day.total_seconds))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .truncate()
                                    .text_size(text::SMALL)
                                    .line_height(text::SMALL_LINE)
                                    .text_color(theme::TEXT_SECONDARY)
                                    .child(SharedString::from(tasks)),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(date) = jump {
                                    this.go_to(date, Scope::Day, cx);
                                }
                            }))
                            .into_any_element(),
                    );
                }
            }
        }
        let title = match self.scope {
            Scope::Day => fmt_date(self.date),
            Scope::Month => fmt_month(self.date),
        };
        let empty = rows.is_empty();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .bg(theme::LAYOUT)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::XS)
                    .p(space::MD)
                    .border_b_1()
                    .border_color(theme::BORDER)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::SM)
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(text::H4)
                                    .line_height(text::H4_LINE)
                                    .text_color(theme::TEXT)
                                    .child(SharedString::from(title)),
                            )
                            .child(hint("total"))
                            .child(
                                div()
                                    .font_family(theme::FONT_MONO)
                                    .text_size(text::H5)
                                    .line_height(text::H5_LINE)
                                    .text_color(theme::TEXT)
                                    .child(SharedString::from(fmt_hm(total))),
                            ),
                    )
                    .when_some(self.error.clone(), |d, e| d.child(error_banner(e))),
            )
            .child(
                div()
                    .id("track-rows")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_y_scroll()
                    .children(rows)
                    .when(empty, |d| {
                        d.child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .items_center()
                                .justify_center()
                                .gap(space::XS)
                                .p(space::XXL)
                                .child(
                                    theme::icon_sized("clock", text::H3)
                                        .text_color(theme::TEXT_DISABLED),
                                )
                                .child(heading("Nothing booked here"))
                                .child(hint(
                                    "Start a timer, or step to another day with ‹ and ›.",
                                )),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .h(SUMMARY_H)
                    .gap(space::XS)
                    .p(space::MD)
                    .border_t_1()
                    .border_color(theme::BORDER)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::XS)
                            .text_color(theme::TEXT_TERTIARY)
                            .child(theme::icon_sized("copy", text::BODY))
                            .child(hint("Summary — select and Ctrl+C to copy")),
                    )
                    .child(div().flex_1().overflow_hidden().child(self.summary.clone())),
            )
    }

    fn entry_row(
        &self,
        entry: &Entry,
        now: i64,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = entry.id;
        let running = entry.is_running();
        let span = match entry.stopped_at {
            Some(end) => format!("{}–{}", fmt_clock(entry.started_at), fmt_clock(end)),
            None => format!("{}–…", fmt_clock(entry.started_at)),
        };
        // An entry the agent booked over MCP must be distinguishable from one
        // the user started by hand — that is the point of the integration.
        let mcp = entry.source == "mcp";
        div()
            .id(SharedString::from(format!("entry-{id}")))
            .flex()
            .items_center()
            .gap(space::MD)
            .px(space::MD)
            .py(space::XS)
            .border_b_1()
            .border_color(theme::DIVIDER)
            .text_size(text::BODY)
            .line_height(text::BODY_LINE)
            .hover(|s| s.bg(theme::FILL))
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                let next = if *over { Some(id) } else { None };
                if *over || this.hovered == Some(id) {
                    this.hovered = next;
                    cx.notify();
                }
            }))
            // Ranges in mono so they align into a column.
            .child(
                div()
                    .w(COL_RANGE)
                    .flex_none()
                    .font_family(theme::FONT_MONO)
                    .text_size(text::SMALL)
                    .line_height(text::SMALL_LINE)
                    .text_color(theme::TEXT_SECONDARY)
                    .child(SharedString::from(span)),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_color(theme::TEXT)
                            .truncate()
                            .child(SharedString::from(entry.task.clone())),
                    )
                    .when_some(entry.note.clone(), |d, note| {
                        d.child(hint(SharedString::from(note)).truncate())
                    }),
            )
            .child(badge(
                SharedString::from(entry.source.clone()),
                if mcp { theme::ACCENT } else { theme::TEXT_TERTIARY },
                mcp,
            ))
            // The id is what the MCP server reports back, so it has to be
            // readable here to be matched against.
            .child(
                div()
                    .flex_none()
                    .font_family(theme::FONT_MONO)
                    .text_size(text::SMALL)
                    .line_height(text::SMALL_LINE)
                    .text_color(theme::TEXT_TERTIARY)
                    .child(SharedString::from(format!("#{id}"))),
            )
            .child(
                div()
                    .w(COL_TOTAL)
                    .flex_none()
                    .text_right()
                    .font_family(theme::FONT_MONO)
                    .text_color(if running { theme::SUCCESS } else { theme::TEXT })
                    .child(SharedString::from(fmt_hm(entry.seconds(now)))),
            )
            // Deleting is destructive, so the affordance is only there — and
            // only clickable — while the pointer is on its row.
            .child(
                div()
                    .w(control::SM)
                    .h(control::SM)
                    .flex_none()
                    .when(self.hovered == Some(id), |d| {
                        d.child(icon_button(
                            SharedString::from(format!("del-{id}")),
                            "trash",
                            cx.listener(move |this, _, _, cx| this.delete(id, cx)),
                        ))
                    }),
            )
    }
}

impl Render for Track {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .h_full()
            .bg(theme::LAYOUT)
            .text_size(text::BODY)
            .line_height(text::BODY_LINE)
            .font_family(theme::FONT_UI)
            .text_color(theme::TEXT)
            .child(self.list(cx))
            .child(self.detail(cx))
    }
}

// ---------------------------------------------------------------------------
// Small building blocks, all of them token-only.
// ---------------------------------------------------------------------------

fn heading(label: impl Into<SharedString>) -> Div {
    div()
        .text_size(text::H5)
        .line_height(text::H5_LINE)
        .text_color(theme::TEXT)
        .child(label.into())
}

fn hint(label: impl Into<SharedString>) -> Div {
    div()
        .text_size(text::SMALL)
        .line_height(text::SMALL_LINE)
        .text_color(theme::TEXT_TERTIARY)
        .child(label.into())
}

/// An input with a leading icon. Further children sit after the field.
fn field(icon_name: &'static str, input: gpui::AnyElement) -> Div {
    div()
        .flex()
        .items_center()
        .gap(space::XS)
        .child(theme::icon(icon_name).text_color(theme::TEXT_TERTIARY))
        .child(div().flex_1().child(input))
}

/// The one primary action, which changes identity rather than multiplying.
fn primary_button(
    id: SharedString,
    icon_name: &'static str,
    label: SharedString,
    tint: Rgba,
    tint_hover: Rgba,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(space::XS)
        .h(control::MD)
        .px(space::MD)
        .rounded(radius::XS)
        .bg(tint)
        .text_color(theme::TEXT_ON_ACCENT)
        .hover(|s| s.bg(tint_hover))
        .child(theme::icon(icon_name))
        .child(label)
        .on_click(on_click)
}

/// A bordered, quiet action: "Go", a recent-task pick.
fn quiet_button(
    id: SharedString,
    icon_name: Option<&'static str>,
    label: SharedString,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(space::XXS)
        .h(control::SM)
        .px(space::XS)
        .rounded(radius::XS)
        .border_1()
        .border_color(theme::BORDER)
        .text_size(text::SMALL)
        .line_height(text::SMALL_LINE)
        .text_color(theme::TEXT_SECONDARY)
        .hover(|s| s.bg(theme::FILL).text_color(theme::TEXT))
        .when_some(icon_name, |d, name| {
            d.child(theme::icon_sized(name, text::SMALL))
        })
        .child(label)
        .on_click(on_click)
}

/// A square icon-only button, for the steppers and the delete affordance.
fn icon_button(
    id: SharedString,
    icon_name: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(control::SM)
        .flex_none()
        .rounded(radius::XS)
        .text_color(theme::TEXT_TERTIARY)
        .hover(|s| s.bg(theme::FILL).text_color(theme::TEXT))
        .child(theme::icon_sized(icon_name, text::BODY))
        .on_click(on_click)
}

/// One member of a segmented control.
fn segment(
    id: SharedString,
    label: SharedString,
    selected: bool,
    first: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .h(control::SM)
        .text_size(text::SMALL)
        .line_height(text::SMALL_LINE)
        .when(!first, |d| d.border_l_1().border_color(theme::BORDER))
        .when(selected, |d| d.bg(theme::FILL_STRONG))
        .text_color(if selected {
            theme::TEXT
        } else {
            theme::TEXT_SECONDARY
        })
        .hover(|s| s.bg(theme::FILL))
        .child(label)
        .on_click(on_click)
}

/// A small tinted label. `washed` fills it, for the one value worth spotting.
fn badge(label: SharedString, tint: Rgba, washed: bool) -> Div {
    div()
        .flex_none()
        .px(space::XXS)
        .rounded(radius::XS)
        .border_1()
        .border_color(tint)
        .when(washed, |d| d.bg(theme::ACCENT_WASH))
        .text_size(text::SMALL)
        .line_height(text::SMALL_LINE)
        .text_color(tint)
        .child(label)
}

/// The failure line. A store error is an error; no store at all is not the
/// user's fault, so it explains itself instead of shouting.
fn error_banner(message: SharedString) -> Div {
    let calm = message.as_ref() == NO_STORE;
    div()
        .flex()
        .items_start()
        .gap(space::XS)
        .p(space::XS)
        .rounded(radius::XS)
        .bg(if calm { theme::FILL } else { theme::SUNKEN })
        .border_1()
        .border_color(if calm { theme::BORDER } else { theme::DANGER })
        .text_size(text::SMALL)
        .line_height(text::SMALL_LINE)
        .text_color(if calm {
            theme::TEXT_SECONDARY
        } else {
            theme::DANGER
        })
        .child(theme::icon_sized("alert", text::BODY))
        .child(if calm {
            div().flex_1().child(
                div()
                    .child("Time tracking is unavailable: the database could not be opened.")
                    .child(hint("Everything else keeps working; nothing here can be saved.")),
            )
        } else {
            div().flex_1().child(message)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> Ymd {
        Ymd { y, m, d }
    }

    #[test]
    fn durations_read_as_hours_and_minutes() {
        assert_eq!(fmt_hm(0), "0h 00m");
        assert_eq!(fmt_hm(-5), "0h 00m"); // clock skew must not underflow
        assert_eq!(fmt_hm(59), "0h 00m");
        assert_eq!(fmt_hm(60), "0h 01m");
        assert_eq!(fmt_hm(3599), "0h 59m");
        assert_eq!(fmt_hm(3600), "1h 00m");
        assert_eq!(fmt_hm(3661), "1h 01m");
        // Over a day stays in hours: a timesheet is read in hours.
        assert_eq!(fmt_hm(86_400), "24h 00m");
        assert_eq!(fmt_hm(97_200), "27h 00m");
    }

    #[test]
    fn running_elapsed_keeps_its_seconds() {
        assert_eq!(fmt_hms(0), "00:00:00");
        assert_eq!(fmt_hms(-1), "00:00:00");
        assert_eq!(fmt_hms(59), "00:00:59");
        assert_eq!(fmt_hms(3600), "01:00:00");
        assert_eq!(fmt_hms(3661), "01:01:01");
        // Started days ago and never stopped.
        assert_eq!(fmt_hms(200_000), "55:33:20");
    }

    #[test]
    fn clock_of_timestamp() {
        assert_eq!(fmt_clock(0), "00:00");
        assert_eq!(fmt_clock(3_600 * 8 + 900), "08:15");
        assert_eq!(fmt_clock(86_399), "23:59");
        assert_eq!(fmt_clock(86_400), "00:00");
        // Pre-epoch must wrap, not go negative.
        assert_eq!(fmt_clock(-1), "23:59");
    }

    #[test]
    fn civil_days_round_trip() {
        assert_eq!(days_from_civil(ymd(1970, 1, 1)), 0);
        assert_eq!(civil_from_days(0), ymd(1970, 1, 1));
        for date in [
            ymd(1969, 12, 31),
            ymd(2000, 2, 29),
            ymd(2024, 2, 29),
            ymd(2026, 7, 26),
            ymd(2100, 3, 1),
        ] {
            assert_eq!(civil_from_days(days_from_civil(date)), date, "{date:?}");
        }
    }

    #[test]
    fn day_steps_across_month_and_year_boundaries() {
        assert_eq!(step_day(ymd(2026, 1, 31), 1), ymd(2026, 2, 1));
        assert_eq!(step_day(ymd(2026, 2, 1), -1), ymd(2026, 1, 31));
        assert_eq!(step_day(ymd(2025, 12, 31), 1), ymd(2026, 1, 1));
        assert_eq!(step_day(ymd(2026, 1, 1), -1), ymd(2025, 12, 31));
        assert_eq!(step_day(ymd(2026, 3, 1), -1), ymd(2026, 2, 28));
        assert_eq!(step_day(ymd(2026, 7, 26), 0), ymd(2026, 7, 26));
    }

    #[test]
    fn the_twentyninth_of_february_exists_only_in_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        // Centuries: 1900 is not a leap year, 2000 is.
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(step_day(ymd(2024, 2, 28), 1), ymd(2024, 2, 29));
        assert_eq!(step_day(ymd(2024, 2, 29), 1), ymd(2024, 3, 1));
        assert_eq!(step_day(ymd(2023, 2, 28), 1), ymd(2023, 3, 1));
        assert_eq!(parse_date("2024-02-29"), Some(ymd(2024, 2, 29)));
        assert_eq!(parse_date("2023-02-29"), None);
        assert_eq!(parse_date("1900-02-29"), None);
        assert_eq!(parse_date("2000-02-29"), Some(ymd(2000, 2, 29)));
    }

    #[test]
    fn month_steps_across_december_and_clamp_the_day() {
        assert_eq!(step_month(ymd(2025, 12, 15), 1), ymd(2026, 1, 15));
        assert_eq!(step_month(ymd(2026, 1, 15), -1), ymd(2025, 12, 15));
        assert_eq!(step_month(ymd(2026, 1, 31), 1), ymd(2026, 2, 28));
        assert_eq!(step_month(ymd(2024, 1, 31), 1), ymd(2024, 2, 29));
        assert_eq!(step_month(ymd(2026, 3, 31), -1), ymd(2026, 2, 28));
        assert_eq!(step_month(ymd(2026, 6, 1), -12), ymd(2025, 6, 1));
    }

    #[test]
    fn typed_dates_are_validated() {
        assert_eq!(parse_date("2026-02-01"), Some(ymd(2026, 2, 1)));
        assert_eq!(parse_date("  2026-02-01 "), Some(ymd(2026, 2, 1)));
        // A month alone means its first day.
        assert_eq!(parse_date("2026-02"), Some(ymd(2026, 2, 1)));
        assert_eq!(parse_date("2026-13-01"), None);
        assert_eq!(parse_date("2026-00-01"), None);
        assert_eq!(parse_date("2026-04-31"), None);
        assert_eq!(parse_date("2026-04-00"), None);
        assert_eq!(parse_date("0000-01-01"), None);
        assert_eq!(parse_date(""), None);
        assert_eq!(parse_date("yesterday"), None);
        assert_eq!(parse_date("1 February"), None);
        assert_eq!(parse_date("2026-02-01-01"), None);
        assert_eq!(parse_date("-2026-02-01"), None);
    }

    #[test]
    fn reload_only_when_another_connection_wrote() {
        assert!(should_reload(None, Some(7)), "first successful poll");
        assert!(should_reload(Some(7), Some(8)), "someone wrote");
        assert!(!should_reload(Some(7), Some(7)), "nothing changed");
        assert!(!should_reload(None, None), "counter unreadable");
        assert!(!should_reload(Some(7), None), "counter unreadable");
    }

    #[test]
    fn summary_is_readable_and_totals_per_task() {
        let day = DaySummary {
            date: "2026-02-01".into(),
            total_seconds: 5_400,
            per_task: vec![("report".into(), 3_600), ("email".into(), 1_800)],
        };
        assert_eq!(
            summary_text("2026-02-01", std::slice::from_ref(&day)),
            "2026-02-01 — 1h 30m\n     1h 00m  report\n     0h 30m  email\n"
        );
        // A month lists its days and merges the per-task totals.
        let other = DaySummary {
            date: "2026-02-02".into(),
            total_seconds: 3_600,
            per_task: vec![("email".into(), 3_600)],
        };
        let month = summary_text("2026-02", &[day, other]);
        assert!(month.starts_with("2026-02 — 2h 30m\n"));
        assert!(month.contains("  2026-02-02  1h 00m\n"));
        assert!(month.contains("1h 30m  email"), "{month}");
        // An empty day says so rather than looking broken.
        assert_eq!(
            summary_text("2026-02-03", &[]),
            "2026-02-03 — 0h 00m\n  nothing booked\n"
        );
    }
}
