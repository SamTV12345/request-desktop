//! Time tracking: start/stop a timer, browse what was booked by day, week and
//! month, search it, edit it, copy a summary out.
//!
//! Two processes write the same table — this app and the MCP server an agent
//! talks to — so the view is a *reader* that refreshes: it polls
//! `Store::change_counter()` (SQLite's `PRAGMA data_version`) about once a
//! second and only re-queries when that number moved. Elapsed time for a
//! running entry is always derived from `started_at` and the current clock,
//! never accumulated per tick, so a late or dropped tick costs nothing but a
//! stale pixel (same reasoning as `custom/chronometer.rs`).
//!
//! Local time is the store's, not ours: every day is grouped by SQLite's
//! `'localtime'`, so this view asks the store where a day begins
//! (`local_day_start`) and renders every clock face relative to that. std has
//! no local-time API and no date dependency is allowed, so probing the store is
//! the only way both sides can agree on what "today" and "12:36" mean.
use crate::editor::Editor;
use crate::text_input::TextInput;
use crate::theme::{Theme, control, radius, size, space, text};
use crate::{Persist, theme};
use gpui::{
    App, ClipboardItem, Context, Div, Entity, IntoElement, ParentElement, Pixels, Rgba,
    SharedString, Stateful, Styled, Task, Window, div, prelude::*, px, relative,
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

/// A day at or over this total reads as a heavy day in the month chart.
const HEAVY_DAY: i64 = 6 * 3_600;

/// The two sources an entry can carry. The store takes a free string; these are
/// the two this app and the MCP server actually write.
const SOURCES: [&str; 2] = ["app", "mcp"];

// Column widths. Not padding or gaps, so not on the spacing scale — they exist
// only to align the mono time columns down the list. The rail's width is a
// design token (`size::SIDEBAR_WIDE`), these are not.
//
// These are *starting* widths, handed to [`col`], which lets them shrink
// together when the pane is narrow. Every row shrinks by the same proportion —
// the flexible task column contributes no base width to the line — so the
// columns still line up under their headers at any width.
const COL_RANGE: Pixels = px(104.);
const COL_TOTAL: Pixels = px(72.);
const COL_DATE: Pixels = px(104.);
const COL_ID: Pixels = px(34.);
/// The tallest the chart-and-summary pair may get. It is a ceiling now, not a
/// height: on a short window the pair shrinks instead of pushing the entry list
/// out of the pane.
const SUMMARY_MAX_H: Pixels = px(180.);
/// The chart is 1.35 times the width of the summary next to it. gpui has no
/// grid, so the two are expressed as relative flex bases which sum to one; the
/// gap between them makes the line overflow slightly and both shrink in
/// proportion, which is what keeps the ratio.
const CHART_SHARE: f32 = 1.35 / 2.35;

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

/// Day of the week, 0 = Monday. 1970-01-01 was a Thursday, which is where the
/// `+ 3` comes from.
fn weekday(date: Ymd) -> u32 {
    (days_from_civil(date) + 3).rem_euclid(7) as u32
}

fn is_weekend(date: Ymd) -> bool {
    weekday(date) >= 5
}

/// The Monday of `date`'s week. ISO weeks, because this is a European
/// timesheet: a week that starts on Sunday would split every working week.
fn week_start(date: Ymd) -> Ymd {
    step_day(date, -(weekday(date) as i64))
}

const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn weekday_name(date: Ymd) -> &'static str {
    WEEKDAY_NAMES[weekday(date) as usize]
}

fn month_name(date: Ymd) -> &'static str {
    MONTH_NAMES[(date.m as usize - 1).min(11)]
}

/// `26 July 2026` — the heading form, spelled out.
fn fmt_long_date(date: Ymd) -> String {
    format!("{} {} {}", date.d, month_name(date), date.y)
}

/// `July 2026`.
fn fmt_long_month(date: Ymd) -> String {
    format!("{} {}", month_name(date), date.y)
}

/// `20 – 26 July 2026`, collapsing whatever the two ends share.
fn fmt_week(date: Ymd) -> String {
    let (a, b) = (week_start(date), step_day(week_start(date), 6));
    if a.m == b.m {
        format!("{} – {} {} {}", a.d, b.d, month_name(b), b.y)
    } else if a.y == b.y {
        format!(
            "{} {} – {} {} {}",
            a.d,
            month_name(a),
            b.d,
            month_name(b),
            b.y
        )
    } else {
        format!("{} – {}", fmt_long_date(a), fmt_long_date(b))
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

/// `HH:MM` as seconds into the day. Rejects 24:00 and 12:60, because a range
/// that does not exist must not be bookable.
fn parse_hm(text: &str) -> Option<i64> {
    let (h, m) = text.trim().split_once(':')?;
    let h: i64 = h.trim().parse().ok()?;
    let m: i64 = m.trim().parse().ok()?;
    (0..24).contains(&h).then_some(())?;
    (0..60).contains(&m).then_some(())?;
    Some(h * 3_600 + m * 60)
}

/// `09:00–10:30` as seconds into the day. Accepts the en dash the view renders
/// as well as a plain hyphen, since one of them is what a user will type.
/// A range that ends before it starts is rejected here rather than by the
/// store's CHECK constraint, so the message can point at the field.
fn parse_range(text: &str) -> Option<(i64, i64)> {
    let text = text.trim();
    let (a, b) = text
        .split_once('–')
        .or_else(|| text.split_once('—'))
        .or_else(|| text.split_once('-'))?;
    let (a, b) = (parse_hm(a)?, parse_hm(b)?);
    (a <= b).then_some((a, b))
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

/// Wall-clock `HH:MM` of a Unix timestamp. Callers pass an instant already
/// shifted into local time — see `Track::clock`.
fn fmt_clock(ts: i64) -> String {
    let secs = ts.rem_euclid(86_400);
    format!("{:02}:{:02}", secs / 3600, (secs / 60) % 60)
}

/// The Unix timestamp at which `date` begins locally, found by asking the store
/// which local day a given instant falls in and bisecting for the boundary.
///
/// The store groups every day with SQLite's `'localtime'`, so it is the only
/// authority on the offset — including historic DST, which a fixed offset would
/// get wrong. `probe` is `Store::local_date`; `None` from it means the store
/// cannot answer and the caller should fall back to UTC.
fn local_day_start(date: Ymd, probe: impl Fn(i64) -> Option<String>) -> Option<i64> {
    let want = fmt_date(date);
    let utc = days_from_civil(date) * 86_400;
    // Every real offset is inside ±15h, so the boundary is inside this window.
    let (mut lo, mut hi) = (utc - 15 * 3_600, utc + 15 * 3_600);
    if probe(lo)? >= want || probe(hi)? < want {
        return None;
    }
    // Invariant: lo is before the day, hi is inside it.
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if probe(mid)? < want {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(hi)
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
/// agent for. One day when `days` has one element, a range when it has many.
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

/// The same numbers as `summary_text`, as a spreadsheet would want them:
/// `date,task,seconds,hours`. Fields are quoted and inner quotes doubled, so a
/// task named `He said "no"` does not shear the row.
fn csv_text(days: &[DaySummary]) -> String {
    let mut out = String::from("date,task,seconds,hours\n");
    for day in days {
        for (task, secs) in &day.per_task {
            out.push_str(&format!(
                "{},\"{}\",{},{}\n",
                day.date,
                task.replace('"', "\"\""),
                secs,
                fmt_hm(*secs)
            ));
        }
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
    Week,
    Month,
}

/// The inline entry form: one row that either adds a manual entry or replaces
/// an existing one.
///
/// ponytail: "replace" is literally delete + `track_log`, because `rp_store`
/// has no update path and is not ours to extend. The visible cost is that an
/// edited entry gets a new id — see the report; the upgrade path is a
/// `Store::track_update_entry`.
struct Draft {
    /// `None` adds a new entry; `Some(id)` replaces that one.
    id: Option<i64>,
    range: Entity<TextInput>,
    task: Entity<TextInput>,
    note: Entity<TextInput>,
    source: usize,
}

pub struct Track {
    task: Entity<TextInput>,
    note: Entity<TextInput>,
    search: Entity<TextInput>,
    jump: Entity<TextInput>,
    /// Read-only, so the summary can be selected and copied with Ctrl+C.
    summary: Entity<Editor>,
    date: Ymd,
    /// Today as the store defines it, for the chart's "future" and "today"
    /// marks. Kept fresh on every reload rather than read from the UTC clock.
    today: Ymd,
    /// Whether `date` has been resolved through the store's local-time
    /// definition rather than the UTC fallback.
    date_from_store: bool,
    /// Seconds the shown day's local midnight sits ahead of UTC midnight, so
    /// every clock face agrees with the store's own day grouping. Zero when the
    /// store cannot say, which is exactly the old UTC behaviour.
    offset: i64,
    scope: Scope,
    /// Which of `SOURCES` a newly started timer is attributed to.
    source: usize,
    running: Option<Entry>,
    entries: Vec<Entry>,
    week: Vec<DaySummary>,
    month: Vec<DaySummary>,
    tasks: Vec<String>,
    hits: Vec<Entry>,
    draft: Option<Draft>,
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
    /// Whether the section is on screen. Gates the *expensive* reload, not the
    /// tick: the title bar shows the running timer from every screen, so the
    /// elapsed value has to keep moving while this pane is hidden.
    visible: bool,
    /// Held so dropping the view cancels the poll.
    poll: Option<Task<()>>,
}

impl Track {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let task = cx.new(|cx| TextInput::new(cx, "What are you working on?"));
        let note = cx.new(|cx| TextInput::new(cx, "note (optional)"));
        let search = cx.new(|cx| TextInput::new(cx, "Search tasks and notes…").with_icon("search"));
        let jump = cx.new(|cx| TextInput::new(cx, "YYYY-MM-DD").with_icon("calendar"));
        let summary = cx.new(|cx| Editor::new(cx, "no time booked").read_only());
        for input in [&task, &note] {
            cx.observe(input, |_, _, cx| cx.notify()).detach();
        }
        // Searching re-queries; it is a local SQLite LIKE over a small table.
        cx.observe(&search, |this, _, cx| this.reload(cx)).detach();
        // The date field *is* the date: the design has no Go button, so a fully
        // typed date navigates as soon as it parses.
        cx.observe(&jump, |this, _, cx| this.jump(cx)).detach();
        let mut this = Self {
            task,
            note,
            search,
            jump,
            summary,
            date: today(None),
            today: today(None),
            date_from_store: false,
            offset: 0,
            scope: Scope::Day,
            source: 0,
            running: None,
            entries: Vec::new(),
            week: Vec::new(),
            month: Vec::new(),
            tasks: Vec::new(),
            hits: Vec::new(),
            draft: None,
            error: None,
            hovered: None,
            seen: None,
            visible: false,
            poll: None,
        };
        // The tick lives as long as the view, not as long as the section is on
        // screen: another pane draws the running timer too. It is one `PRAGMA`
        // and a notify unless something actually changed — see `tick`.
        this.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                // `Err` means the view is gone; stop instead of spinning.
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        }));
        this
    }

    /// The running entry, for whoever draws the timer outside this section.
    /// Borrowed rather than cloned: the caller renders it and drops it, and the
    /// elapsed value is `Entry::seconds(now)`, never a stored count.
    // Read by the title bar in `main.rs`; allowed so this file stays
    // warning-free until that call site lands.
    #[allow(dead_code)]
    pub fn running(&self) -> Option<&Entry> {
        self.running.as_ref()
    }

    /// Called when the section is shown or hidden. Only the wholesale reload is
    /// gated on this; a hidden panel costs a counter read a second.
    pub fn set_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.reload(cx);
        }
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
    ///
    /// Nothing here accumulates: a repaint re-derives elapsed from `started_at`
    /// and the clock, so a late or dropped tick costs a stale pixel and nothing
    /// else.
    fn tick(&mut self, cx: &mut Context<Self>) {
        let current = cx
            .try_global::<Persist>()
            .and_then(|p| p.store.as_ref())
            .and_then(|store| store.change_counter().ok());
        if should_reload(self.seen, current) {
            if self.visible {
                self.reload(cx);
            } else {
                self.refresh_running(cx);
            }
        } else if self.running.is_some() {
            // The common case while a timer runs: no query at all, just a
            // repaint so the digits move.
            cx.notify();
        }
    }

    /// The hidden path: learn *which* entry is running without re-reading the
    /// day, the month or the search.
    ///
    /// One indexed row, and only when the change counter moved — so a timer the
    /// agent starts while the user is on another screen still reaches whoever
    /// draws it, without this pane querying per frame. Silent like `tick`.
    fn refresh_running(&mut self, cx: &mut Context<Self>) {
        let read = cx
            .try_global::<Persist>()
            .and_then(|p| p.store.as_ref())
            .and_then(|store| Some((store.change_counter().ok()?, store.track_running().ok()?)));
        if let Some((counter, running)) = read {
            self.seen = Some(counter);
            self.running = running;
        }
        cx.notify();
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
        let shown = self.date;
        let needle = self.search.read(cx).text().trim().to_string();
        // A week can straddle two months, and the only per-day summary the store
        // offers is per month, so ask for both and keep the seven days.
        let week_scope = self.scope == Scope::Week;
        let wk = week_start(self.date);
        let week_dates: Vec<String> = (0..7).map(|i| fmt_date(step_day(wk, i))).collect();
        let mut week_months = vec![fmt_month(wk)];
        let last = fmt_month(step_day(wk, 6));
        if last != week_months[0] {
            week_months.push(last);
        }

        let loaded = self.with_store(cx, |store| {
            let mut week = Vec::new();
            if week_scope {
                for m in &week_months {
                    for day in store.track_summary_month(m, now)? {
                        if week_dates.contains(&day.date) {
                            week.push(day);
                        }
                    }
                }
                week.sort_by(|a, b| a.date.cmp(&b.date));
            }
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
                week,
                today(Some(store)),
                local_day_start(shown, |ts| store.local_date(ts).ok()),
            ))
        });
        let Some((counter, running, entries, day, month_days, tasks, hits, week, today, day_start)) =
            loaded
        else {
            return;
        };
        self.seen = Some(counter);
        self.running = running;
        self.entries = entries;
        self.tasks = tasks.into_iter().map(|t| t.name).collect();
        self.hits = hits;
        self.today = today;
        self.offset = day_start
            .map(|s| s - days_from_civil(self.date) * 86_400)
            .unwrap_or(0);
        let text = match self.scope {
            Scope::Day => summary_text(&date, std::slice::from_ref(&day)),
            Scope::Week => summary_text(&fmt_week(self.date), &week),
            Scope::Month => summary_text(&month, &month_days),
        };
        self.week = week;
        self.month = month_days;
        self.error = None;
        self.summary.update(cx, |e, cx| e.set_text(text, cx));
        // Stepping days must keep the date field showing the day it moved to.
        let field = fmt_date(self.date);
        if self.jump.read(cx).text() != field {
            self.jump.update(cx, |i, cx| i.set_text(field, cx));
        }
        cx.notify();
    }

    /// The days the current scope summarises, for the summary and CSV buttons.
    fn shown_days(&self, day: &DaySummary) -> Vec<DaySummary> {
        match self.scope {
            Scope::Day => vec![day.clone()],
            Scope::Week => self.week.clone(),
            Scope::Month => self.month.clone(),
        }
    }

    /// A clock face in the store's local time. `offset` is zero when the store
    /// could not be asked, which is the old UTC rendering.
    fn clock(&self, ts: i64) -> String {
        fmt_clock(ts - self.offset)
    }

    /// Local midnight of the shown day, for turning a typed `HH:MM` back into
    /// an instant.
    fn day_start(&self) -> i64 {
        days_from_civil(self.date) * 86_400 + self.offset
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
            let source = SOURCES[self.source];
            self.with_store(cx, |store| {
                store.track_start(&task, note.as_deref(), source, now())
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
            if self.draft.as_ref().and_then(|d| d.id) == Some(id) {
                self.draft = None;
            }
            self.reload(cx);
        }
    }

    /// Open the inline form, either on an existing entry or empty for a new one.
    fn edit(&mut self, entry: Option<&Entry>, cx: &mut Context<Self>) {
        let range_text = match entry {
            Some(e) => format!(
                "{}–{}",
                self.clock(e.started_at),
                self.clock(e.stopped_at.unwrap_or(e.started_at))
            ),
            None => String::new(),
        };
        let range = cx.new(|cx| {
            let mut i = TextInput::new(cx, "09:00–10:30");
            i.set_text(range_text, cx);
            i
        });
        let task_text = entry.map(|e| e.task.clone()).unwrap_or_default();
        let task = cx.new(|cx| {
            let mut i = TextInput::new(cx, "task");
            i.set_text(task_text, cx);
            i
        });
        let note_text = entry.and_then(|e| e.note.clone()).unwrap_or_default();
        let note = cx.new(|cx| {
            let mut i = TextInput::new(cx, "note (optional)");
            i.set_text(note_text, cx);
            i
        });
        for input in [&range, &task, &note] {
            cx.observe(input, |_, _, cx| cx.notify()).detach();
        }
        let source = entry
            .and_then(|e| SOURCES.iter().position(|s| *s == e.source))
            .unwrap_or(0);
        self.draft = Some(Draft {
            id: entry.map(|e| e.id),
            range,
            task,
            note,
            source,
        });
        cx.notify();
    }

    /// Commit the inline form. An edit is a delete followed by a fresh log, so
    /// the two halves are ordered to fail safe: nothing is deleted until the
    /// replacement has been validated.
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        let Some(draft) = self.draft.as_ref() else {
            return;
        };
        let (id, source) = (draft.id, SOURCES[draft.source]);
        let range = draft.range.read(cx).text().trim().to_string();
        let task = draft.task.read(cx).text().trim().to_string();
        let note = draft.note.read(cx).text().trim().to_string();
        let note = (!note.is_empty()).then_some(note);
        if task.is_empty() {
            self.error = Some("type a task name first".into());
            cx.notify();
            return;
        }
        let Some((from, to)) = parse_range(&range) else {
            self.error = Some(format!("{range:?} is not a time range — use HH:MM–HH:MM").into());
            cx.notify();
            return;
        };
        let base = self.day_start();
        // ponytail: the range is read against the day on screen, so an entry
        // that crosses midnight has to be split into two. The store keeps whole
        // entries; a day-crossing edit is the case this form does not serve.
        let (from, to) = (base + from, base + to);
        let done = self
            .with_store(cx, |store| {
                if let Some(id) = id {
                    store.track_delete_entry(id)?;
                }
                store.track_log(&task, note.as_deref(), from, to, source)
            })
            .is_some();
        if done {
            self.draft = None;
            self.reload(cx);
        }
    }

    fn go_to(&mut self, date: Ymd, scope: Scope, cx: &mut Context<Self>) {
        self.date = date;
        self.scope = scope;
        self.reload(cx);
    }

    /// The date field navigates as soon as what is typed is a whole date, so the
    /// design's button-less field still commits.
    ///
    /// ponytail: "whole" is judged by length, since `2026-02` is also a valid
    /// date (the first of that month) and would otherwise navigate halfway
    /// through typing `2026-02-15`. The cost is one extra reload for anyone who
    /// stops typing at the month.
    fn jump(&mut self, cx: &mut Context<Self>) {
        let text = self.jump.read(cx).text().trim().to_string();
        match parse_date(&text) {
            Some(date) if date != self.date => {
                let scope = self.scope;
                self.go_to(date, scope, cx)
            }
            Some(_) => {}
            None if text.len() >= "YYYY-MM-DD".len() => {
                self.error = Some(format!("{text:?} is not a date — use YYYY-MM-DD").into());
                cx.notify();
            }
            None => {}
        }
    }

    fn copy(&mut self, csv: bool, cx: &mut Context<Self>) {
        let now = now();
        let date = fmt_date(self.date);
        let Some(day) = self.with_store(cx, move |s| s.track_summary_day(&date, now)) else {
            return;
        };
        let days = self.shown_days(&day);
        let text = if csv {
            csv_text(&days)
        } else {
            summary_text(&self.title(), &days)
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    fn title(&self) -> String {
        match self.scope {
            Scope::Day => fmt_long_date(self.date),
            Scope::Week => fmt_week(self.date),
            Scope::Month => fmt_long_month(self.date),
        }
    }

    // -- rendering ----------------------------------------------------------

    /// The hero. A running timer has to be readable across the room; nothing
    /// running is an invitation, not an empty box.
    fn hero(&self, t: Theme, now: i64, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let Some(entry) = self.running.clone() else {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .gap(space::S1)
                .p(space::S4)
                .rounded(radius::CARD)
                .bg(t.hover())
                .child(theme::icon_tinted("clock", text::H4, t.text_disabled()))
                .child(heading(t, "No timer running"))
                .child(hint(t, "Name a task below and press Start."))
                .into_any_element();
        };
        let id = entry.id;
        let ink = t.accent_ramp.s900;
        let (tag_bg, tag_ink) = (t.accent_ramp.s100, t.accent_ramp.s800);
        div()
            .flex()
            .flex_col()
            .gap(space::S4)
            .p(space::S6)
            .rounded(radius::CARD)
            .bg(t.accent_ramp.s200)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    // ponytail: the dot is the smallest spacing step used as a
                    // size; the system has no dedicated dot token.
                    .child(div().size(space::S2).rounded(radius::PILL).bg(t.accent))
                    .child(
                        div()
                            .flex_1()
                            .text_size(text::LABEL)
                            .text_color(t.accent_ramp.s800)
                            .child("RUNNING"),
                    )
                    .child(
                        div()
                            .text_size(text::LABEL)
                            .text_color(t.accent_ramp.s800)
                            .child(SharedString::from(format!(
                                "started {}",
                                self.clock(entry.started_at)
                            ))),
                    ),
            )
            .child(
                // The heading face, not mono: the design sets the digits in
                // Caprasimo and lets its figures carry the size.
                div()
                    .font_family(theme::FONT_HEADING)
                    .text_size(text::H1)
                    .text_color(ink)
                    .child(SharedString::from(fmt_hms(entry.seconds(now)))),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(space::S2)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(text::H5)
                                    .text_color(ink)
                                    .truncate()
                                    .child(SharedString::from(entry.task.clone())),
                            )
                            .when_some(entry.note.clone(), |d, note| {
                                d.child(
                                    div()
                                        .text_size(text::TINY)
                                        .text_color(t.accent_ramp.s800)
                                        .truncate()
                                        .child(SharedString::from(note)),
                                )
                            }),
                    )
                    .child(tag(
                        SharedString::from(entry.source.clone()),
                        tag_bg,
                        tag_ink,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .child(
                        primary_button(
                            t,
                            "track-toggle".into(),
                            "stop",
                            "Stop".into(),
                            true,
                            cx.listener(|this, _, _, cx| this.start_or_stop(cx)),
                        )
                        .flex_1(),
                    )
                    .child(icon_button(
                        t,
                        "drop-running".into(),
                        "trash",
                        cx.listener(move |this, _, _, cx| this.delete(id, cx)),
                    )),
            )
            .into_any_element()
    }

    fn timer_block(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let now = now();
        let running = self.running.is_some();
        let mut chips = Vec::new();
        for (i, name) in self.tasks.iter().take(20).enumerate() {
            let label = SharedString::from(name.clone());
            let pick = label.clone();
            chips.push(pill_button(
                t,
                SharedString::from(format!("task-{i}")),
                label,
                cx.listener(move |this, _, _, cx| {
                    let pick = pick.clone();
                    this.task.update(cx, |t, cx| t.set_text(pick, cx));
                }),
            ));
        }
        theme::card(&t)
            .gap(space::S3)
            .child(self.hero(t, now, cx))
            .when(!running, |d| {
                d.child(self.task.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::S2)
                            .child(div().flex_1().min_w_0().child(self.note.clone()))
                            .child(source_picker(
                                t,
                                "new-source".into(),
                                self.source,
                                cx.listener(|this, _, _, cx| {
                                    this.source = (this.source + 1) % SOURCES.len();
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(primary_button(
                        t,
                        "track-start".into(),
                        "play",
                        "Start new timer".into(),
                        false,
                        cx.listener(|this, _, _, cx| this.start_or_stop(cx)),
                    ))
            })
            .when(!chips.is_empty(), |d| {
                d.child(
                    theme::section_label(&t, "Recent tasks")
                        .px_0()
                        .pt(space::S1)
                        .pb_0(),
                )
                .child(div().flex().flex_wrap().gap(space::S1).children(chips))
            })
    }

    fn nav_block(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        theme::card(&t)
            .gap(space::S3)
            .child(
                // `.seg`: one pill trough with a pill thumb, not loose chips.
                div()
                    .flex()
                    .gap(space::S1)
                    .p(space::S1)
                    .rounded(radius::PILL)
                    .bg(t.hover())
                    .child(segment(
                        t,
                        "scope-day".into(),
                        "Day".into(),
                        self.scope == Scope::Day,
                        cx.listener(|this, _, _, cx| {
                            let date = this.date;
                            this.go_to(date, Scope::Day, cx)
                        }),
                    ))
                    .child(segment(
                        t,
                        "scope-week".into(),
                        "Week".into(),
                        self.scope == Scope::Week,
                        cx.listener(|this, _, _, cx| {
                            let date = this.date;
                            this.go_to(date, Scope::Week, cx)
                        }),
                    ))
                    .child(segment(
                        t,
                        "scope-month".into(),
                        "Month".into(),
                        self.scope == Scope::Month,
                        cx.listener(|this, _, _, cx| {
                            let date = this.date;
                            this.go_to(date, Scope::Month, cx)
                        }),
                    )),
            )
            // The steppers move by whatever the scope is looking at, so ‹ means
            // "the previous one of these" rather than always a day.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .child(icon_button(
                        t,
                        "prev".into(),
                        "chevron_left",
                        cx.listener(|this, _, _, cx| this.step(-1, cx)),
                    ))
                    .child(div().flex_1().min_w_0().child(self.jump.clone()))
                    .child(icon_button(
                        t,
                        "next".into(),
                        "chevron_right",
                        cx.listener(|this, _, _, cx| this.step(1, cx)),
                    ))
                    .child(pill_button(
                        t,
                        "today".into(),
                        "Today".into(),
                        cx.listener(|this, _, _, cx| {
                            let scope = this.scope;
                            let today = this
                                .with_store(cx, |s| Ok(today(Some(s))))
                                .unwrap_or(this.date);
                            this.go_to(today, scope, cx)
                        }),
                    )),
            )
            .child(self.search.clone())
    }

    /// One step of whatever the scope shows.
    fn step(&mut self, delta: i64, cx: &mut Context<Self>) {
        let date = match self.scope {
            Scope::Day => step_day(self.date, delta),
            Scope::Week => step_day(self.date, delta * 7),
            Scope::Month => step_month(self.date, delta),
        };
        let scope = self.scope;
        self.go_to(date, scope, cx);
    }

    /// The left column: timer, navigation, search results.
    fn list(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let needle = self.search.read(cx).text().trim().to_string();
        let mut hits = Vec::with_capacity(self.hits.len());
        for entry in &self.hits {
            let date = fmt_date(civil_from_days((entry.started_at + self.offset) / 86_400));
            let jump = parse_date(&date);
            hits.push(
                div()
                    .id(SharedString::from(format!("hit-{}", entry.id)))
                    .flex()
                    .flex_col()
                    .px(space::S2)
                    .py(space::S2)
                    .rounded(radius::SM)
                    .hover(|s| s.bg(t.hover()))
                    .child(
                        div()
                            .text_color(t.text)
                            .truncate()
                            .child(SharedString::from(entry.task.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(space::S2)
                            .font_family(theme::FONT_MONO)
                            .text_size(text::SMALL)
                            .text_color(t.text_meta())
                            .child(SharedString::from(date))
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
            // `size::COLUMN` is where this column starts, not what it is: past
            // half the window it stops growing with it, and it never goes under
            // a sidebar's width, so the detail pane always has room.
            .flex_basis(size::COLUMN)
            .w(size::COLUMN)
            .min_w(size::SIDEBAR)
            .max_w_1_2()
            .flex_shrink()
            .h_full()
            .gap(space::S3)
            .p(space::S4)
            .overflow_hidden()
            .border_r_1()
            .border_color(t.divider)
            .child(self.timer_block(t, cx))
            .child(self.nav_block(t, cx))
            .child(
                theme::card(&t)
                    .flex_1()
                    .min_h_0()
                    .child(hint(
                        t,
                        SharedString::from(if needle.is_empty() {
                            "Type to search every day".to_string()
                        } else {
                            format!("{} matching", self.hits.len())
                        }),
                    ))
                    .child(
                        div()
                            .id("track-hits")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .children(hits),
                    ),
            )
    }

    /// The detail pane: the day's entries, or the per-day totals of a week or
    /// month.
    fn detail(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let now = now();
        let day_scope = self.scope == Scope::Day;
        let days: &[DaySummary] = match self.scope {
            Scope::Day => &[],
            Scope::Week => &self.week,
            Scope::Month => &self.month,
        };
        let total: i64 = match self.scope {
            Scope::Day => self.entries.iter().map(|e| e.seconds(now)).sum(),
            _ => days.iter().map(|d| d.total_seconds).sum(),
        };
        let editing = self.draft.as_ref().and_then(|d| d.id);
        let mut rows = Vec::new();
        if day_scope {
            for entry in &self.entries {
                if editing == Some(entry.id) {
                    rows.push(self.draft_row(t, cx).into_any_element());
                } else {
                    rows.push(self.entry_row(t, entry, now, cx).into_any_element());
                }
            }
            // A new entry's form takes the place of the invitation to add one.
            if self.draft.as_ref().is_some_and(|d| d.id.is_none()) {
                rows.push(self.draft_row(t, cx).into_any_element());
            } else {
                rows.push(
                    div()
                        .id("add-entry")
                        .flex()
                        .items_center()
                        .gap(space::S2)
                        .px(space::S4)
                        .py(space::S3)
                        .rounded(radius::LG)
                        .text_size(text::SMALL)
                        .text_color(t.text_meta())
                        .hover(|s| s.bg(t.hover()).text_color(t.text))
                        .child(theme::icon_tinted("plus", text::BODY, t.text_meta()))
                        .child("Add entry manually")
                        .on_click(cx.listener(|this, _, _, cx| this.edit(None, cx)))
                        .into_any_element(),
                );
            }
        } else {
            for day in days {
                let jump = parse_date(&day.date);
                let shown = parse_date(&day.date).unwrap_or(self.date);
                let tasks = day
                    .per_task
                    .iter()
                    .map(|(t, _)| t.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                rows.push(
                    table_row(t, SharedString::from(format!("day-{}", day.date)))
                        .child(
                            col(COL_DATE)
                                .font_family(theme::FONT_MONO)
                                .text_size(text::SMALL)
                                .text_color(if is_weekend(shown) {
                                    t.text_meta()
                                } else {
                                    t.text
                                })
                                .child(SharedString::from(format!(
                                    "{} {}",
                                    &WEEKDAY_NAMES[weekday(shown) as usize][..2],
                                    day.date
                                ))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(size::TABLE_COL_MIN_W)
                                .truncate()
                                .text_size(text::SMALL)
                                .text_color(t.text_muted())
                                .child(SharedString::from(tasks)),
                        )
                        // Totals in their own right-aligned column, so a heavy
                        // day is found by scanning down.
                        .child(
                            col(COL_TOTAL)
                                .text_right()
                                .font_family(theme::FONT_MONO)
                                .text_color(t.text)
                                .child(SharedString::from(fmt_hm(day.total_seconds))),
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
        let empty = match self.scope {
            Scope::Day => self.entries.is_empty() && self.draft.is_none(),
            _ => rows.is_empty(),
        };
        // The `.table` header: the same columns the rows use, so the mono
        // figures below each sit under their name.
        let head = if day_scope {
            table_head(t)
                .child(col(COL_RANGE).child("WHEN"))
                .child(
                    div()
                        .flex_1()
                        .min_w(size::TABLE_COL_MIN_W)
                        .truncate()
                        .child("TASK"),
                )
                .child(col(COL_ID).text_right().child("ID"))
                .child(col(COL_TOTAL).text_right().child("TOTAL"))
                .child(div().size(control::ICON).flex_none())
        } else {
            table_head(t)
                .child(col(COL_DATE).child("DAY"))
                .child(
                    div()
                        .flex_1()
                        .min_w(size::TABLE_COL_MIN_W)
                        .truncate()
                        .child("TASKS"),
                )
                .child(col(COL_TOTAL).text_right().child("TOTAL"))
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap(space::S3)
            .p(space::S3)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(space::S2)
                    .px(space::S3)
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .gap(space::S3)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .text_size(text::LABEL)
                                            .text_color(t.text_meta())
                                            .child(SharedString::from(match self.scope {
                                                Scope::Day => {
                                                    weekday_name(self.date).to_uppercase()
                                                }
                                                Scope::Week => "WEEK".to_string(),
                                                Scope::Month => "MONTH".to_string(),
                                            })),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .font_family(theme::FONT_HEADING)
                                            .text_size(text::H3)
                                            .text_color(t.text)
                                            .child(SharedString::from(self.title())),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_none()
                                    .items_end()
                                    .child(
                                        div()
                                            .text_size(text::LABEL)
                                            .text_color(t.text_meta())
                                            .child("TOTAL"),
                                    )
                                    .child(
                                        div()
                                            .font_family(theme::FONT_HEADING)
                                            .text_size(text::H2)
                                            .text_color(t.accent_ramp.s700)
                                            .child(SharedString::from(fmt_hm(total))),
                                    ),
                            ),
                    )
                    .when_some(self.error.clone(), |d, e| d.child(error_banner(t, e))),
            )
            .child(
                theme::card(&t)
                    .flex_1()
                    .min_h_0()
                    .gap(space::S1)
                    .child(head)
                    .child(theme::divider(&t))
                    .child(
                        div()
                            .id("track-rows")
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .when(empty, |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .gap(space::S2)
                                        .p(space::S8)
                                        .child(theme::icon_tinted(
                                            "clock",
                                            text::H3,
                                            t.text_disabled(),
                                        ))
                                        .child(heading(t, "Nothing booked here"))
                                        .child(hint(
                                            t,
                                            "Start a timer, or step to another day with ‹ and ›.",
                                        )),
                                )
                            })
                            .children(rows),
                    ),
            )
            .child(
                div()
                    .flex()
                    // Was a fixed 180 px slab. Now it is the last thing to get
                    // height: it shrinks on a short window and never grows past
                    // what it needs.
                    .flex_1()
                    .min_h(space::S8 * 3.)
                    .max_h(SUMMARY_MAX_H)
                    .gap(space::S3)
                    .child(self.chart(t, cx))
                    .child(self.summary_card(t, cx)),
            )
    }

    /// The month at a glance: one bar a day, so a gap in the timesheet is seen
    /// rather than counted.
    fn chart(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let days = days_in_month(self.date.y, self.date.m);
        let max = self
            .month
            .iter()
            .map(|d| d.total_seconds)
            .max()
            .unwrap_or(0)
            .max(HEAVY_DAY);
        let tracked = self.month.iter().filter(|d| d.total_seconds > 0).count();
        let total: i64 = self.month.iter().map(|d| d.total_seconds).sum();
        // Derived from a type step rather than typed as a literal, so the chart
        // still scales if the type scale moves.
        let plot = text::H1 * 2.;
        let mut bars = Vec::with_capacity(days as usize);
        for d in 1..=days {
            let date = Ymd { d, ..self.date };
            let key = fmt_date(date);
            let secs = self
                .month
                .iter()
                .find(|s| s.date == key)
                .map(|s| s.total_seconds)
                .unwrap_or(0);
            let is_today = date == self.today;
            let future = days_from_civil(date) > days_from_civil(self.today);
            let fill = if is_today {
                t.accent
            } else if secs >= HEAVY_DAY {
                t.accent_2_ramp.s400
            } else if secs > 0 {
                t.accent_2_ramp.s300
            } else if future {
                theme::alpha(t.neutral.s300, 0.5)
            } else {
                t.neutral.s300
            };
            // A tracked day never rounds away to nothing: the floor is the
            // smallest step of the scale.
            let h = if secs > 0 {
                (plot * (secs as f32 / max as f32)).max(space::S1)
            } else {
                space::S1
            };
            bars.push(
                div()
                    .id(SharedString::from(format!("bar-{d}")))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .h_full()
                    .child(div().w_full().h(h).rounded(radius::SM).bg(fill))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let scope = this.scope;
                        this.go_to(date, scope, cx)
                    })),
            );
        }
        let mut axis = Vec::new();
        for d in [1, 8, 15, 22, days] {
            axis.push(
                div()
                    .font_family(theme::FONT_MONO)
                    .text_size(text::TINY)
                    .text_color(t.text_meta())
                    .child(SharedString::from(format!("{d:02}"))),
            );
        }
        theme::card(&t)
            // The wide half of the 1.35:1 pair — see `CHART_SHARE`.
            .flex_basis(relative(CHART_SHARE))
            .flex_shrink()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(space::S2)
                    .child(
                        div()
                            .flex_shrink()
                            .min_w_0()
                            .truncate()
                            .font_family(theme::FONT_HEADING)
                            .text_size(text::H5)
                            .text_color(t.text)
                            .child(SharedString::from(fmt_long_month(self.date))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(text::SMALL)
                            .text_color(t.text_meta())
                            .child(SharedString::from(format!("{tracked} tracked days"))),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme::FONT_MONO)
                            .text_size(text::SMALL)
                            .text_color(t.accent_ramp.s700)
                            .child(SharedString::from(fmt_hm(total))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_end()
                    .flex_1()
                    .min_h_0()
                    .gap(space::S1)
                    .children(bars),
            )
            .child(div().flex().justify_between().children(axis))
    }

    fn summary_card(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        theme::card(&t)
            // The narrow half of the 1.35:1 pair — see `CHART_SHARE`.
            .flex_basis(relative(1. - CHART_SHARE))
            .flex_shrink()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(theme::FONT_HEADING)
                            .text_size(text::H5)
                            .text_color(t.text)
                            .child("Summary"),
                    )
                    .child(pill_button(
                        t,
                        "copy-summary".into(),
                        "Copy".into(),
                        cx.listener(|this, _, _, cx| this.copy(false, cx)),
                    ))
                    .child(pill_button(
                        t,
                        "copy-csv".into(),
                        "CSV".into(),
                        cx.listener(|this, _, _, cx| this.copy(true, cx)),
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .rounded(radius::MD)
                    .bg(t.bg)
                    .p(space::S2)
                    .child(self.summary.clone()),
            )
    }

    /// The inline form, shown in the place of the row it edits.
    fn draft_row(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let Some(draft) = self.draft.as_ref() else {
            return div().into_any_element();
        };
        let id = draft.id;
        let span = parse_range(draft.range.read(cx).text())
            .map(|(a, b)| fmt_hm(b - a))
            .unwrap_or_else(|| "—".to_string());
        div()
            .flex()
            .items_center()
            .gap(space::S2)
            .p(space::S2)
            .rounded(radius::LG)
            .bg(t.bg)
            .border_2()
            .border_color(t.accent)
            .child(col(COL_RANGE).child(draft.range.clone()))
            .child(div().flex_1().min_w_0().child(draft.task.clone()))
            .child(div().flex_1().min_w_0().child(draft.note.clone()))
            .child(source_picker(
                t,
                "draft-source".into(),
                draft.source,
                cx.listener(|this, _, _, cx| {
                    if let Some(d) = this.draft.as_mut() {
                        d.source = (d.source + 1) % SOURCES.len();
                    }
                    cx.notify();
                }),
            ))
            .child(
                col(COL_TOTAL)
                    .text_right()
                    .font_family(theme::FONT_MONO)
                    .text_color(t.text_muted())
                    .child(SharedString::from(span)),
            )
            .child(primary_button(
                t,
                "draft-save".into(),
                "save",
                "Save".into(),
                false,
                cx.listener(|this, _, _, cx| this.save_draft(cx)),
            ))
            .child(icon_button(
                t,
                "draft-cancel".into(),
                "close",
                cx.listener(|this, _, _, cx| {
                    this.draft = None;
                    cx.notify();
                }),
            ))
            .when_some(id, |d, id| {
                d.child(icon_button(
                    t,
                    "draft-delete".into(),
                    "trash",
                    cx.listener(move |this, _, _, cx| this.delete(id, cx)),
                ))
            })
            .into_any_element()
    }

    fn entry_row(
        &self,
        t: Theme,
        entry: &Entry,
        now: i64,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = entry.id;
        let running = entry.is_running();
        let span = match entry.stopped_at {
            Some(end) => format!("{}–{}", self.clock(entry.started_at), self.clock(end)),
            None => format!("{}–now", self.clock(entry.started_at)),
        };
        // An entry the agent booked over MCP must be distinguishable from one
        // the user started by hand — that is the point of the integration.
        let mcp = entry.source == "mcp";
        let (tag_bg, tag_ink) = if mcp {
            t.tag_accent_2()
        } else {
            t.tag_neutral()
        };
        let clone = entry.clone();
        table_row(t, SharedString::from(format!("entry-{id}")))
            // A running row is provisional: dashed, quieter, and it says so.
            .when(running, |d| {
                d.border_dashed().border_color(t.divider).opacity(0.75)
            })
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                let next = if *over { Some(id) } else { None };
                if *over || this.hovered == Some(id) {
                    this.hovered = next;
                    cx.notify();
                }
            }))
            // Clicking a row opens it in the inline form; the design has no
            // pencil, the row itself is the affordance.
            .on_click(cx.listener(move |this, _, _, cx| {
                if !clone.is_running() {
                    this.edit(Some(&clone), cx);
                }
            }))
            // Ranges in mono so they align into a column.
            .child(
                col(COL_RANGE)
                    .font_family(theme::FONT_MONO)
                    .text_size(text::SMALL)
                    .text_color(t.text_muted())
                    .child(SharedString::from(span)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(size::TABLE_COL_MIN_W)
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::S2)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(t.text)
                                    .truncate()
                                    .child(SharedString::from(entry.task.clone())),
                            )
                            .when(running, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .text_size(text::TINY)
                                        .text_color(t.accent_ramp.s700)
                                        .child("· running"),
                                )
                            })
                            .child(tag(
                                SharedString::from(entry.source.clone()),
                                tag_bg,
                                tag_ink,
                            )),
                    )
                    .when_some(entry.note.clone(), |d, note| {
                        d.child(hint(t, SharedString::from(note)).truncate())
                    }),
            )
            // The id is what the MCP server reports back, so it has to be
            // readable here to be matched against.
            .child(
                col(COL_ID)
                    .text_right()
                    .font_family(theme::FONT_MONO)
                    .text_size(text::TINY)
                    .text_color(t.text_disabled())
                    .child(SharedString::from(format!("#{id}"))),
            )
            .child(
                col(COL_TOTAL)
                    .text_right()
                    .font_family(theme::FONT_MONO)
                    .text_color(if running { t.accent_ramp.s700 } else { t.text })
                    .child(SharedString::from(fmt_hm(entry.seconds(now)))),
            )
            // Deleting is destructive, so the affordance is only there — and
            // only clickable — while the pointer is on its row.
            .child(
                div()
                    .size(control::ICON)
                    .flex_none()
                    .when(self.hovered == Some(id), |d| {
                        d.child(icon_button(
                            t,
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
        let t = theme::theme(cx);
        div()
            .flex()
            .flex_1()
            .h_full()
            .bg(t.bg)
            .text_size(text::CONTROL)
            .line_height(text::BODY_LINE)
            .font_family(theme::FONT_BODY)
            .text_color(t.text)
            .child(self.list(t, cx))
            .child(self.detail(t, cx))
    }
}

// ---------------------------------------------------------------------------
// Small building blocks, all of them token-only.
// ---------------------------------------------------------------------------

fn heading(t: Theme, label: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::FONT_HEADING)
        .text_size(text::H5)
        .text_color(t.text)
        .child(label.into())
}

fn hint(t: Theme, label: impl Into<SharedString>) -> Div {
    div()
        .text_size(text::SMALL)
        .text_color(t.text_muted())
        .child(label.into())
}

/// One aligned column of the entry table, in the header and in every row.
///
/// The width is a starting size: the cell may shrink to half of it before the
/// flexible task column gives up any more, and its content ellipsizes rather
/// than spilling into the neighbouring column. Both the header and the rows go
/// through here, so a change cannot align one and not the other.
fn col(w: Pixels) -> Div {
    div()
        .flex_basis(w)
        .w(w)
        .min_w(w * 0.5)
        .flex_shrink()
        .truncate()
}

/// A `.table` header row: the column names, quiet and letter-spaced-small.
fn table_head(t: Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(space::S3)
        .px(space::S3)
        .pb(space::S1)
        // The rows carry a 1-px border so a running row can go dashed without
        // changing height; the header needs the same one or every column sits a
        // pixel off its heading.
        .border_1()
        .border_color(theme::alpha(t.divider, 0.))
        .text_size(text::LABEL)
        .text_color(t.text_meta())
}

/// A `.table` body row. Clickable, so it carries an id and a hover wash; the
/// corner is the large radius rather than a pill, because a row is not a
/// control.
fn table_row(t: Theme, id: SharedString) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        // The same gap and inset as [`table_head`], and not a wider one: the
        // columns shrink by the space their line is missing, so a row with more
        // chrome than its header shrinks further and stops lining up under it.
        // Measured at 760 px the head still had its full 104-px WHEN column
        // while the rows were down to 78.
        .gap(space::S3)
        .px(space::S3)
        .py(space::S3)
        .rounded(radius::LG)
        // A transparent border, so switching a running row to dashed cannot
        // change the row's height.
        .border_1()
        .border_color(theme::alpha(t.divider, 0.))
        .hover(|s| s.bg(t.hover()))
}

/// The one primary action, which changes identity rather than multiplying.
/// `destructive` is the running state: the button becomes Stop.
fn primary_button(
    t: Theme,
    id: SharedString,
    icon_name: &'static str,
    label: SharedString,
    destructive: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let tint = if destructive {
        t.danger()
    } else {
        t.accent_ramp.s800
    };
    let ink = if destructive {
        t.on_accent()
    } else {
        t.accent_ramp.s100
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(space::S2)
        .h(control::MD)
        .px(space::S4)
        .rounded(radius::PILL)
        .bg(tint)
        .text_size(text::CONTROL)
        .text_color(ink)
        .hover(|s| s.bg(theme::alpha(tint, 0.85)))
        .child(theme::icon_tinted(icon_name, text::BODY, ink))
        .child(label)
        .on_click(on_click)
}

/// A quiet pill action: "Today", "Copy", a recent-task pick.
fn pill_button(
    t: Theme,
    id: SharedString,
    label: SharedString,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .flex_none()
        .h(control::MD)
        .px(space::S3)
        .rounded(radius::PILL)
        .bg(t.hover())
        .text_size(text::SMALL)
        .text_color(t.text_label())
        .hover(|s| s.bg(t.active()).text_color(t.text))
        .child(label)
        .on_click(on_click)
}

/// The source a booking is attributed to. Two values, so the design's dropdown
/// is a pill that cycles.
///
/// ponytail: a real menu is only worth it at three values — which is also when
/// the store would need to hand out the list instead of this file naming it.
fn source_picker(
    t: Theme,
    id: SharedString,
    selected: usize,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .flex_none()
        .gap(space::S1)
        .h(control::MD)
        .px(space::S3)
        .rounded(radius::PILL)
        .bg(t.hover())
        .text_size(text::SMALL)
        .text_color(t.text_label())
        .hover(|s| s.bg(t.active()))
        .child(div().size(space::S2).rounded(radius::PILL).bg(t.accent_2))
        .child(SharedString::from(SOURCES[selected]))
        .child(theme::icon_tinted(
            "chevron_right",
            text::SMALL,
            t.text_meta(),
        ))
        .on_click(on_click)
}

/// A round icon-only button, for the steppers and the delete affordance.
fn icon_button(
    t: Theme,
    id: SharedString,
    icon_name: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .size(control::ICON)
        .flex_none()
        .rounded(radius::PILL)
        .hover(|s| s.bg(t.hover()))
        .child(theme::icon_tinted(icon_name, text::BODY, t.text_muted()))
        .on_click(on_click)
}

/// One option of a `.seg`. The selected thumb is a filled pill.
fn segment(
    t: Theme,
    id: SharedString,
    label: SharedString,
    selected: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .py(space::S2)
        .rounded(radius::PILL)
        .text_size(text::SMALL)
        .when(selected, |d| d.bg(t.accent))
        .text_color(if selected {
            t.on_accent()
        } else {
            t.text_muted()
        })
        .when(!selected, |d| d.hover(|s| s.bg(t.active())))
        .child(label)
        .on_click(on_click)
}

/// A `.tag`: a tint from one end of a ramp with the far end as its ink.
fn tag(label: SharedString, bg: Rgba, ink: Rgba) -> Div {
    div()
        .flex_none()
        .px(space::S3)
        .rounded(radius::PILL)
        .bg(bg)
        .text_size(text::TINY)
        .text_color(ink)
        .child(label)
}

/// The failure line. A store error is an error; no store at all is not the
/// user's fault, so it explains itself instead of shouting.
fn error_banner(t: Theme, message: SharedString) -> Div {
    let calm = message.as_ref() == NO_STORE;
    let ink = if calm { t.text_muted() } else { t.danger() };
    div()
        .flex()
        .items_start()
        .gap(space::S2)
        .p(space::S3)
        .rounded(radius::MD)
        .bg(if calm { t.hover() } else { t.danger_wash() })
        .text_size(text::SMALL)
        .text_color(ink)
        .child(theme::icon_tinted("alert", text::BODY, ink))
        .child(if calm {
            div().flex_1().child(
                div()
                    .child("Time tracking is unavailable: the database could not be opened.")
                    .child(hint(
                        t,
                        "Everything else keeps working; nothing here can be saved.",
                    )),
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

    #[test]
    fn weekdays_and_weeks_start_on_monday() {
        // 26 July 2026 is a Sunday, which is what the design's header says.
        assert_eq!(weekday_name(ymd(2026, 7, 26)), "Sunday");
        assert_eq!(weekday_name(ymd(1970, 1, 1)), "Thursday");
        assert_eq!(weekday_name(ymd(2000, 1, 1)), "Saturday");
        assert!(is_weekend(ymd(2026, 7, 25)) && is_weekend(ymd(2026, 7, 26)));
        assert!(!is_weekend(ymd(2026, 7, 24)));
        // A Sunday belongs to the week that began the Monday before it.
        assert_eq!(week_start(ymd(2026, 7, 26)), ymd(2026, 7, 20));
        assert_eq!(week_start(ymd(2026, 7, 20)), ymd(2026, 7, 20));
        // Across a month, and across a year.
        assert_eq!(week_start(ymd(2026, 3, 1)), ymd(2026, 2, 23));
        assert_eq!(week_start(ymd(2026, 1, 1)), ymd(2025, 12, 29));
    }

    #[test]
    fn headings_spell_the_date_out() {
        assert_eq!(fmt_long_date(ymd(2026, 7, 26)), "26 July 2026");
        assert_eq!(fmt_long_month(ymd(2026, 7, 26)), "July 2026");
        assert_eq!(fmt_week(ymd(2026, 7, 26)), "20 – 26 July 2026");
        // A week that straddles two months, and two years.
        assert_eq!(fmt_week(ymd(2026, 3, 1)), "23 February – 1 March 2026");
        assert_eq!(
            fmt_week(ymd(2026, 1, 1)),
            "29 December 2025 – 4 January 2026"
        );
    }

    #[test]
    fn typed_ranges_are_validated() {
        assert_eq!(parse_range("09:00–10:30"), Some((32_400, 37_800)));
        assert_eq!(parse_range(" 9:00 - 10:30 "), Some((32_400, 37_800)));
        assert_eq!(parse_range("09:00—10:30"), Some((32_400, 37_800)));
        // Zero length is a real booking of no time, not an error.
        assert_eq!(parse_range("09:00-09:00"), Some((32_400, 32_400)));
        // Backwards, and times the clock does not have.
        assert_eq!(parse_range("10:30-09:00"), None);
        assert_eq!(parse_range("24:00-24:30"), None);
        assert_eq!(parse_range("09:60-10:00"), None);
        assert_eq!(parse_range("09:00"), None);
        assert_eq!(parse_range(""), None);
        assert_eq!(parse_range("morning-noon"), None);
        assert_eq!(parse_hm("00:00"), Some(0));
        assert_eq!(parse_hm("23:59"), Some(86_340));
    }

    /// The store owns the offset, so the bisection has to land on the exact
    /// instant its own `date(…,'localtime')` flips — including a half-hour zone
    /// and a negative one.
    #[test]
    fn local_midnight_is_found_by_asking_the_store() {
        fn probe(shift: i64) -> impl Fn(i64) -> Option<String> {
            move |ts| Some(fmt_date(civil_from_days((ts + shift).div_euclid(86_400))))
        }
        let date = ymd(2026, 7, 26);
        let utc = days_from_civil(date) * 86_400;
        for shift in [0, 2 * 3_600, -5 * 3_600, 5 * 3_600 + 1_800, 14 * 3_600] {
            assert_eq!(
                local_day_start(date, probe(shift)),
                Some(utc - shift),
                "shift {shift}"
            );
        }
        // A store that cannot answer must not be guessed at.
        assert_eq!(local_day_start(date, |_| None), None);
    }

    #[test]
    fn csv_quotes_what_would_otherwise_shear_a_row() {
        let day = DaySummary {
            date: "2026-02-01".into(),
            total_seconds: 5_400,
            per_task: vec![
                ("report, final".into(), 3_600),
                ("he said \"no\"".into(), 1_800),
            ],
        };
        assert_eq!(
            csv_text(std::slice::from_ref(&day)),
            "date,task,seconds,hours\n\
             2026-02-01,\"report, final\",3600,1h 00m\n\
             2026-02-01,\"he said \"\"no\"\"\",1800,0h 30m\n"
        );
        assert_eq!(csv_text(&[]), "date,task,seconds,hours\n");
    }
}
