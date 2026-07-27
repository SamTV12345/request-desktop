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
use crate::Persist;
use crate::filters::{self, Filters};
use crate::year_strip;
use gpui::{
    App, ClipboardItem, Context, Div, Entity, IntoElement, ParentElement, Pixels, Rgba,
    SharedString, Stateful, Styled, Task, Window, div, prelude::*, px, relative,
};
use rp_store::{DaySummary, Entry, Store};
use rp_ui::button::{Button, IconButton, Size, Tone};
use rp_ui::data::{col, table_head, table_row};
use rp_ui::feedback::{ErrorBanner, hint};
use rp_ui::icon::icon_tinted;
use rp_ui::input::Editor;
use rp_ui::input::TextInput;
use rp_ui::layout::{card, divider};
use rp_ui::theme::{self, FONT_BODY, FONT_HEADING, FONT_MONO, Theme, alpha};
use rp_ui::tokens::{control, radius, size, space, text};

/// The week's hour grid and its inspector. A child module so it can read this
/// view's state and reuse its buttons rather than growing a second set.
mod week_grid;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How often the change counter is checked. Fast enough that an entry booked by
/// the agent feels immediate, slow enough to be free.
const POLL: Duration = Duration::from_secs(1);

/// Search results shown at once. A tracker is not a log viewer.
const SEARCH_LIMIT: usize = 100;

/// The one message for "there is no database". Named so the view can recognise
/// it and explain it calmly instead of shouting a query error at the user.
const NO_STORE: &str = "the database could not be opened — time tracking is unavailable";

/// What that means for the reader. Kept out of [`NO_STORE`] because that string is
/// also the sentinel `with_store` compares against.
const NO_STORE_HINT: &str = "Everything else keeps working; nothing here can be saved.";

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

pub(crate) fn days_in_month(y: i32, m: u32) -> u32 {
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
pub(crate) fn days_from_civil(date: Ymd) -> i64 {
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
pub(crate) fn civil_from_days(z: i64) -> Ymd {
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
pub(crate) fn step_day(date: Ymd, delta: i64) -> Ymd {
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
pub(crate) fn weekday(date: Ymd) -> u32 {
    (days_from_civil(date) + 3).rem_euclid(7) as u32
}

fn is_weekend(date: Ymd) -> bool {
    weekday(date) >= 5
}

/// The Monday of `date`'s week. ISO weeks, because this is a European
/// timesheet: a week that starts on Sunday would split every working week.
pub(crate) fn week_start(date: Ymd) -> Ymd {
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
/// `29 Jun – 5 Jul 2026 · week 27`.
///
/// The week number is what the design's title carries, and it is the ISO one:
/// early January belongs to the previous year's last week, and a title reading
/// "week 1" over dates in December makes the whole screen look wrong.
fn fmt_week(date: Ymd) -> String {
    let (a, b) = (week_start(date), step_day(week_start(date), 6));
    let week = crate::calendar::iso_week(date);
    if a.m == b.m {
        format!("{} – {} {} {} · week {week}", a.d, b.d, month_name(b), b.y)
    } else if a.y == b.y {
        format!(
            "{} {} – {} {} {} · week {week}",
            a.d,
            month_name(a),
            b.d,
            month_name(b),
            b.y
        )
    } else {
        // A week that crosses a new year, where both dates need their own year.
        format!("{} – {} · week {week}", fmt_long_date(a), fmt_long_date(b))
    }
}

pub(crate) fn fmt_date(date: Ymd) -> String {
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

/// The local-midnight shift in force at `ts`: local midnight minus UTC midnight,
/// so it is negative east of UTC (Berlin's midnight is 23:00 the day before).
///
/// `midnights` is the store's local midnight for *each* day of the shown span,
/// day 0 being `span_start` — one number per day rather than one for the span,
/// because the offset is not constant: on the two days a year DST moves, the days
/// either side of it sit an hour apart, and one shared offset draws one of them an
/// hour off.
///
/// An instant outside the span borrows the nearest day's shift, which is the
/// closest thing to right that is known without another query — a search hit from
/// last August is still a local clock face and not a UTC one. Empty `midnights`
/// means the store could not be asked at all, which is UTC, the old behaviour.
fn offset_at(span_start: Ymd, midnights: &[i64], ts: i64) -> i64 {
    if midnights.is_empty() {
        return 0;
    }
    let i = midnights.iter().rposition(|m| *m <= ts).unwrap_or(0);
    midnights[i] - (days_from_civil(span_start) + i as i64) * 86_400
}

/// Local midnight of `date`: the store's own answer where the span covers that
/// day, and the nearest day's shift where it does not.
fn local_midnight_of(span_start: Ymd, midnights: &[i64], date: Ymd) -> i64 {
    let day = days_from_civil(date);
    usize::try_from(day - days_from_civil(span_start))
        .ok()
        .and_then(|i| midnights.get(i).copied())
        .unwrap_or_else(|| day * 86_400 + offset_at(span_start, midnights, day * 86_400))
}

/// The local day an instant falls in. `offset` is that instant's local-midnight
/// shift — [`offset_at`], not one offset for the whole span — so this is the
/// inverse of `Track::local_midnight`: local midnight is `day * 86_400 + offset`,
/// and the day is what is left once that shift comes back off the instant. Never
/// derive a day from a UTC timestamp instead — see [`today`].
fn entry_day_of(ts: i64, offset: i64) -> Ymd {
    civil_from_days((ts - offset).div_euclid(86_400))
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

pub(crate) fn now() -> i64 {
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

/// The inline entry form: one row that either adds a manual entry or corrects
/// an existing one.
///
/// An edit goes through `Store::track_update_entry`, so the entry keeps its id
/// and whatever is selected in the week grid stays selected across a save.
struct Draft {
    /// `None` adds a new entry; `Some(id)` updates that one.
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
    /// The store's local midnight for every day of the shown span, day 0 being
    /// `span_start`, so every clock face agrees with the store's own day grouping.
    ///
    /// Per day, not per span: a week containing a DST change has two different
    /// offsets in it, and one offset for all seven days drew the wrong side of the
    /// change an hour off. Empty when the store cannot say, which is exactly the
    /// old UTC behaviour.
    midnights: Vec<i64>,
    /// The first day `midnights` describes.
    span_start: Ymd,
    scope: Scope,
    /// Which of `SOURCES` a newly started timer is attributed to.
    source: usize,
    running: Option<Entry>,
    entries: Vec<Entry>,
    /// Entry the hour grid's inspector shows, by id — an index or a clone would
    /// go stale on the next poll.
    ///
    /// The grid itself reads `span`, which already holds every entry of the shown
    /// span: for a week that *is* the week, so a second query for the same rows
    /// would only be a second thing to keep in step.
    selected: Option<i64>,
    week: Vec<DaySummary>,
    month: Vec<DaySummary>,
    /// Every entry overlapping the shown span, whatever the scope. The rail's
    /// facets are counted off this and a filtered week or month is listed from
    /// it — a per-day summary carries no source and cannot be narrowed.
    span: Vec<Entry>,
    /// The shown year's twelve monthly totals, for the year strip. Loaded in
    /// `reload` and only in Month scope: it is twelve queries, and nothing else
    /// draws it.
    year: [i64; 12],
    /// Which project and kind the rail is narrowing to. Both empty is
    /// everything.
    filters: Filters,
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
            midnights: Vec::new(),
            span_start: today(None),
            scope: Scope::Day,
            source: 0,
            running: None,
            entries: Vec::new(),
            selected: None,
            week: Vec::new(),
            month: Vec::new(),
            span: Vec::new(),
            year: [0; 12],
            filters: Filters::default(),
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
        // The span the rail's facets are counted over: whatever the scope shows.
        let (span_start, span_days) = match self.scope {
            Scope::Day => (self.date, 1),
            Scope::Week => (wk, 7),
            Scope::Month => (
                Ymd { d: 1, ..self.date },
                days_in_month(self.date.y, self.date.m) as i64,
            ),
        };
        let month_scope = self.scope == Scope::Month;
        let shown_year = self.date.y;

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
            // One local midnight per day of the span, plus the one that closes it:
            // the span is asked for in instants, and both ends — and every column
            // in between — have to be the store's own boundaries rather than
            // multiples of 86 400 from the first. A store that cannot say falls
            // back to UTC, which is what every other clock face here does.
            let midnights = store.local_midnights(&fmt_date(span_start), span_days + 1)?;
            let utc = days_from_civil(span_start) * 86_400;
            let base = midnights.first().copied().unwrap_or(utc);
            let span_end = midnights
                .last()
                .copied()
                .unwrap_or(utc + span_days * 86_400);
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
                midnights,
                store.track_entries_between(base, span_end)?,
                if month_scope {
                    year_strip::year_totals(shown_year, |m| store.track_summary_month(m, now))?
                } else {
                    [0; 12]
                },
            ))
        });
        let Some((
            counter,
            running,
            entries,
            day,
            month_days,
            tasks,
            hits,
            week,
            today,
            midnights,
            span,
            year,
        )) = loaded
        else {
            return;
        };
        self.seen = Some(counter);
        self.running = running;
        self.entries = entries;
        self.tasks = tasks.into_iter().map(|t| t.name).collect();
        self.hits = hits;
        self.today = today;
        self.midnights = midnights;
        self.span_start = span_start;
        let text = match self.scope {
            Scope::Day => summary_text(&date, std::slice::from_ref(&day)),
            Scope::Week => summary_text(&fmt_week(self.date), &week),
            Scope::Month => summary_text(&month, &month_days),
        };
        self.week = week;
        self.month = month_days;
        // Whatever the grid had selected keeps its place unless the row itself is
        // gone — another process may have deleted it between two polls.
        self.selected = week_grid::keep_selected(self.selected, &span);
        self.span = span;
        self.year = year;
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

    /// A clock face in the store's local time, using the offset in force on the
    /// day *that instant* falls in — see [`offset_at`]. Zero when the store could
    /// not be asked, which is the old UTC rendering.
    fn clock(&self, ts: i64) -> String {
        fmt_clock(ts - self.offset_at(ts))
    }

    /// The local-midnight shift at `ts`, per day rather than per span.
    fn offset_at(&self, ts: i64) -> i64 {
        offset_at(self.span_start, &self.midnights, ts)
    }

    /// The local day `ts` falls in, on the store's definition.
    fn day_of(&self, ts: i64) -> Ymd {
        entry_day_of(ts, self.offset_at(ts))
    }

    /// Local midnight of `date`, the basis every clock face and every day column
    /// is placed against.
    pub(crate) fn local_midnight(&self, date: Ymd) -> i64 {
        local_midnight_of(self.span_start, &self.midnights, date)
    }

    /// Local midnight of the shown day, for turning a typed `HH:MM` back into
    /// an instant.
    fn day_start(&self) -> i64 {
        self.local_midnight(self.date)
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

    /// Commit the inline form: a new entry is logged, an edited one is updated in
    /// place so it keeps its id and stays selected.
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
            .with_store(cx, |store| match id {
                Some(id) => store
                    .track_update_entry(id, &task, note.as_deref(), from, to, source)?
                    .ok_or_else(|| {
                        anyhow::anyhow!("entry #{id} is gone — it was deleted while you edited it")
                    }),
                None => store.track_log(&task, note.as_deref(), from, to, source),
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
                .child(icon_tinted("clock", text::H4, t.text_disabled()))
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
                    .font_family(FONT_HEADING)
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
                        Button::new("track-toggle", "Stop")
                            .icon("stop")
                            .tone(Tone::Danger)
                            .render(t, cx.listener(|this, _, _, cx| this.start_or_stop(cx)))
                            .flex_1(),
                    )
                    .child(
                        IconButton::new("drop-running", "trash")
                            .render(t, cx.listener(move |this, _, _, cx| this.delete(id, cx))),
                    ),
            )
            .into_any_element()
    }

    /// The timer card: what is running, or the fields to start something.
    ///
    /// It used to end with a "Recent tasks" list of up to twenty pills. That list
    /// is gone, for two reasons. It was **unbounded in a 308-px column**, so
    /// everything below it — the mini-month, the scope switcher, the filters —
    /// was pushed off the bottom of the rail and simply could not be found; and
    /// `track_tasks` returns tasks *oldest* first, so the heading was a lie. The
    /// design replaces it with the mini-month and the facet rows, which is what
    /// this rail now holds. Reusing a name is the search box's job.
    fn timer_block(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let now = now();
        let running = self.running.is_some();
        card(t)
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
                    .child(
                        Button::new("track-start", "Start new timer")
                            .icon("play")
                            .tone(Tone::Primary)
                            .render(t, cx.listener(|this, _, _, cx| this.start_or_stop(cx))),
                    )
            })
    }

    /// The mini-month from the design's left rail: a whole-week grid of bare day
    /// numbers, marked by whether anything is booked.
    ///
    /// It is what the design puts where the endless list of task pills used to be,
    /// and it does a job the pills could not: showing at a glance which days of the
    /// month have time on them. Clicking a day goes there.
    ///
    /// Deliberately not the big grid at a smaller size — no chips, no totals, no
    /// hover cards. It is a locator, and everything else in it would compete with
    /// the grid it locates you in.
    fn mini_month(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let grid = crate::calendar::month_grid(self.date);
        let month = self.date.m;
        let booked: std::collections::BTreeSet<&str> = self
            .month
            .iter()
            .filter(|d| d.total_seconds > 0)
            .map(|d| d.date.as_str())
            .collect();

        let mut head = div()
            .flex()
            .gap(space::S1 * 0.75)
            .font_family(FONT_MONO)
            .text_size(text::TINY)
            .text_color(t.text_meta());
        for name in WEEKDAY_NAMES {
            head = head.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_center()
                    // One letter: seven columns in a 308-px rail have no room for
                    // "Mon", and the position carries most of the meaning anyway.
                    .child(&name[..1]),
            );
        }

        let mut weeks = Vec::with_capacity(grid.len() / 7);
        for week in grid.chunks(7) {
            let mut cells = Vec::with_capacity(7);
            for date in week {
                let date = *date;
                let inside = date.m == month;
                let today = date == self.today;
                let shown = date == self.date;
                let has_time = booked.contains(fmt_date(date).as_str());
                let (fill, ink) = if today {
                    (t.accent, t.on_accent())
                } else if shown {
                    (t.active(), t.text)
                } else if has_time {
                    t.tag_accent_2()
                } else {
                    (alpha(t.divider, 0.), t.text)
                };
                cells.push(
                    div()
                        .id(SharedString::from(format!("mini-{}", fmt_date(date))))
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .justify_center()
                        .py(space::S1 * 0.5)
                        .rounded(radius::PILL)
                        .bg(fill)
                        .font_family(FONT_MONO)
                        .text_size(text::TINY)
                        .cursor_pointer()
                        .text_color(if inside { ink } else { t.text_disabled() })
                        .when(!today && !shown, |d| d.hover(|s| s.bg(t.hover())))
                        .child(SharedString::from(date.d.to_string()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            // Keep the scope: clicking a day in the mini-month
                            // while looking at a month is how you move the month,
                            // not how you leave it.
                            let scope = this.scope;
                            this.go_to(date, scope, cx)
                        })),
                );
            }
            weeks.push(div().flex().min_w_0().gap(space::S1 * 0.75).children(cells));
        }

        card(t)
            .gap(space::S2)
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
                            .font_family(FONT_HEADING)
                            .text_size(text::H5)
                            .child(SharedString::from(fmt_long_month(self.date))),
                    )
                    .child(mini_step(t, "chevron_left", -1, cx))
                    .child(mini_step(t, "chevron_right", 1, cx)),
            )
            .child(head)
            .children(weeks)
    }

    fn nav_block(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        card(t)
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
                    .child(
                        IconButton::new("prev", "chevron_left")
                            .render(t, cx.listener(|this, _, _, cx| this.step(-1, cx))),
                    )
                    .child(div().flex_1().min_w_0().child(self.jump.clone()))
                    .child(
                        IconButton::new("next", "chevron_right")
                            .render(t, cx.listener(|this, _, _, cx| this.step(1, cx))),
                    )
                    .child(
                        Button::new("today", "Today")
                            .tone(Tone::Quiet)
                            .size(Size::Sm)
                            .render(
                                t,
                                cx.listener(|this, _, _, cx| {
                                    let scope = this.scope;
                                    let today = this
                                        .with_store(cx, |s| Ok(today(Some(s))))
                                        .unwrap_or(this.date);
                                    this.go_to(today, scope, cx)
                                }),
                            ),
                    ),
            )
    }

    /// The rail's facets and the search box, under the navigation.
    ///
    /// The two narrow different things: a facet narrows the shown span, the
    /// search reaches past it into every day there is. Both end up replacing the
    /// detail pane's list, which is why they live together.
    fn filter_block(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let now = now();
        let mut projects = Vec::new();
        for (source, total) in filters::source_totals(&self.span, &SOURCES, now) {
            let active = self.filters.source.as_deref() == Some(source);
            projects.push(filters::facet_row(
                t,
                SharedString::from(format!("project-{source}")),
                filters::source_color(t, source),
                SharedString::from(source),
                SharedString::from(fmt_hm(total)),
                active,
                cx.listener(move |this, _, _, cx| {
                    Filters::toggle(&mut this.filters.source, source);
                    cx.notify();
                }),
            ));
        }
        let mut kinds = Vec::new();
        for (i, (kind, count)) in filters::kind_counts(&self.span).into_iter().enumerate() {
            let active = self.filters.kind.as_deref() == Some(kind.as_str());
            let pick = kind.clone();
            kinds.push(filters::facet_row(
                t,
                SharedString::from(format!("kind-{i}")),
                filters::kind_color(t, i),
                SharedString::from(kind),
                SharedString::from(count.to_string()),
                active,
                cx.listener(move |this, _, _, cx| {
                    Filters::toggle(&mut this.filters.kind, &pick);
                    cx.notify();
                }),
            ));
        }
        card(t)
            .flex_1()
            .min_h_0()
            .child(filters::section(t, "PROJECTS", projects))
            .child(divider(t))
            // The kinds are read out of task names, so there is no bound on how
            // many there are; they scroll and the search box below stays put.
            .child(
                div()
                    .id("track-kinds")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(filters::section(t, "KINDS", kinds)),
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

    /// The left column: timer, navigation, facets and search.
    fn list(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
            .flex_shrink_1()
            .h_full()
            .gap(space::S3)
            .p(space::S4)
            // Scrolls, and that is not decoration: an unbounded child in here once
            // pushed the mini-month, the scope switcher and the filter rows off the
            // bottom, where there was no way to reach them and no sign they
            // existed. Nothing in this column may be unreachable again.
            .id("track-rail")
            .overflow_y_scroll()
            .border_r_1()
            .border_color(t.divider)
            .child(self.timer_block(t, cx))
            // The mini-month sits above the scope switcher, as the design has it:
            // where in the month you are, then what span you are looking at.
            .child(self.mini_month(t, cx))
            .child(self.nav_block(t, cx))
            .child(self.filter_block(t, cx))
    }

    /// The detail pane: the day's entries, or the per-day totals of a week or
    /// month.
    fn detail(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let now = now();
        let day_scope = self.scope == Scope::Day;

        let searching = !self.search.read(cx).text().trim().is_empty();
        // Two things put entries from outside the shown day in this list: a
        // search, which is over all history, and a facet on a week or a month —
        // the store's per-day summaries carry no source, so a narrowed span can
        // only be shown as the entries themselves.
        let listing = searching || (!day_scope && self.filters.any());
        let history: Vec<Entry> = if searching {
            self.hits.clone()
        } else if listing {
            filters::retain(&self.span, &self.filters)
        } else {
            Vec::new()
        };
        let entries = filters::retain(&self.entries, &self.filters);
        let days: &[DaySummary] = match self.scope {
            _ if listing => &[],
            Scope::Day => &[],
            Scope::Week => &self.week,
            Scope::Month => &self.month,
        };
        // The total is the total of what is on screen. A filtered view whose
        // heading still showed the unfiltered month would be read as the answer
        // to the question the filter asked.
        let total: i64 = if listing {
            history.iter().map(|e| e.seconds(now)).sum()
        } else if day_scope {
            entries.iter().map(|e| e.seconds(now)).sum()
        } else {
            days.iter().map(|d| d.total_seconds).sum()
        };
        let editing = self.draft.as_ref().and_then(|d| d.id);
        let mut rows = Vec::new();
        if listing {
            for entry in &history {
                rows.push(self.history_row(t, entry, now, cx).into_any_element());
            }
        } else if day_scope {
            for entry in &entries {
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
                        .child(icon_tinted("plus", text::BODY, t.text_meta()))
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
                                .font_family(FONT_MONO)
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
                                .font_family(FONT_MONO)
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
            _ if listing => rows.is_empty(),
            Scope::Day => entries.is_empty() && self.draft.is_none(),
            _ => rows.is_empty(),
        };
        // An empty list after a search or a filter is not an empty day, and
        // saying "start a timer" there points at the wrong control.
        let empty_hint = if listing {
            "Nothing matches — clear the search box, or the filter row in the rail."
        } else {
            "Start a timer, or step to another day with ‹ and ›."
        };
        // The `.table` header: the same columns the rows use, so the mono
        // figures below each sit under their name.
        let head = if day_scope && !listing {
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
                                            .font_family(FONT_HEADING)
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
                                            .font_family(FONT_HEADING)
                                            .text_size(text::H2)
                                            .text_color(t.accent_ramp.s700)
                                            .child(SharedString::from(fmt_hm(total))),
                                    ),
                            ),
                    )
                    .when_some(self.error.clone(), |d, e| {
                        // "no database" is expected on a first run and is not an
                        // error; the crate draws, the caller knows which it has.
                        // The banner shows whatever message it is handed, so the
                        // consequence sentence is ours to add, not its to know.
                        let calm = e.as_ref() == NO_STORE;
                        let msg = if calm {
                            SharedString::from(format!("{NO_STORE}. {NO_STORE_HINT}"))
                        } else {
                            e
                        };
                        d.child(ErrorBanner::new(msg).calm(calm).render(t))
                    }),
            )
            // A month is a grid; a day and a week are a table. The design makes
            // that switch, and it is the right one: a table of a whole month's
            // days is a scroll, and the shape of the month is what you opened it
            // for.
            .map(|d| match self.scope {
                Scope::Month => d.child(self.month_grid(t, cx)),
                Scope::Week => d.child(self.week_view(t, cx)),
                _ => d.child(self.day_table(t, head, rows, empty, empty_hint, cx)),
            })
            .child(
                div()
                    .flex()
                    // Was a fixed 180 px slab. Now it is the last thing to get
                    // height: it shrinks on a short window and never grows past
                    // what it needs.
                    .flex_1()
                    .min_w_0()
                    .min_h(space::S8 * 3.)
                    .max_h(SUMMARY_MAX_H)
                    .gap(space::S3)
                    // A per-day chart under a table of the same month's days says
                    // it twice; zoomed out to the year it says something the
                    // table cannot.
                    .child(if self.scope == Scope::Month {
                        self.year_row(t, cx).into_any_element()
                    } else {
                        self.chart(t, cx).into_any_element()
                    })
                    .child(self.summary_card(t, cx)),
            )
    }

    /// The year strip and its row of affordances, in the place [`Track::chart`]
    /// holds for a day or a week.
    fn year_row(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut links = Vec::new();
        for back in 1..=2i64 {
            let year = self.date.y - back as i32;
            links.push(year_strip::year_link(
                t,
                year,
                cx.listener(move |this, _, _, cx| {
                    let scope = this.scope;
                    // Through `step_month`, so 29 February does not land on a
                    // date the target year does not have.
                    let date = step_month(this.date, -12 * back);
                    this.go_to(date, scope, cx);
                }),
            ));
        }
        let total: i64 = self.year.iter().sum();
        card(t)
            // The wide half of the 1.35:1 pair — see `CHART_SHARE`.
            .flex_basis(relative(CHART_SHARE))
            .flex_shrink_1()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(space::S2)
                    .child(
                        div()
                            .flex_none()
                            .font_family(FONT_HEADING)
                            .text_size(text::H5)
                            .text_color(t.text)
                            .child(SharedString::from(self.date.y.to_string())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(text::SMALL)
                            .text_color(t.text_meta())
                            .child("by month"),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_family(FONT_MONO)
                            .text_size(text::SMALL)
                            .text_color(t.accent_ramp.s700)
                            .child(SharedString::from(fmt_hm(total))),
                    ),
            )
            .child(year_strip::strip(
                t,
                &self.year,
                Some(self.date.m as usize - 1),
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S3)
                    .children(links)
                    .child(div().flex_1().min_w_0())
                    .child(
                        Button::new("export-month", "Export month")
                            .icon("copy")
                            .tone(Tone::Quiet)
                            .size(Size::Sm)
                            .render(t, cx.listener(|this, _, _, cx| this.copy(true, cx))),
                    ),
            )
    }

    /// A row for an entry from outside the shown day — a search hit, or one entry
    /// of a filtered week or month. It carries its date, because finding out
    /// *when* is the reason the row is in front of the user.
    fn history_row(
        &self,
        t: Theme,
        entry: &Entry,
        now: i64,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // Local midnight is `day * 86_400 + offset` (see `day_start`), so the day
        // an instant falls in takes that shift back *off* it. Adding it put every
        // entry within the offset of midnight on the wrong date — in Berlin, the
        // two hours after 22:00 UTC landed on tomorrow.
        let date = fmt_date(self.day_of(entry.started_at));
        let jump = parse_date(&date);
        let (tag_bg, tag_ink) = if entry.source == "mcp" {
            t.tag_accent_2()
        } else {
            t.tag_neutral()
        };
        table_row(t, SharedString::from(format!("hist-{}", entry.id)))
            .child(
                col(COL_DATE)
                    .font_family(FONT_MONO)
                    .text_size(text::SMALL)
                    .text_color(t.text_muted())
                    .child(SharedString::from(date)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(space::S2)
                    .flex_1()
                    .min_w(size::TABLE_COL_MIN_W)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(t.text)
                            .child(SharedString::from(entry.task.clone())),
                    )
                    .child(tag(
                        SharedString::from(entry.source.clone()),
                        tag_bg,
                        tag_ink,
                    )),
            )
            .child(
                col(COL_TOTAL)
                    .text_right()
                    .font_family(FONT_MONO)
                    .text_color(t.text)
                    .child(SharedString::from(fmt_hm(entry.seconds(now)))),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if let Some(date) = jump {
                    this.go_to(date, Scope::Day, cx);
                }
            }))
    }

    /// The entry table, for the Day and Week scopes.
    ///
    /// `empty_hint` rather than a fixed line: an empty list after a search or a
    /// filter is not an empty day, and "start a timer" there points at the wrong
    /// control.
    fn day_table(
        &self,
        t: Theme,
        head: Div,
        rows: Vec<gpui::AnyElement>,
        empty: bool,
        empty_hint: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let _ = cx;
        card(t)
            .flex_1()
            .min_h_0()
            .gap(space::S1)
            .child(head)
            .child(divider(t))
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
                                .child(icon_tinted("clock", text::H3, t.text_disabled()))
                                .child(heading(t, "Nothing booked here"))
                                .child(hint(t, empty_hint)),
                        )
                    })
                    .children(rows),
            )
    }

    /// The month as a calendar grid, from the `Time Calendar` design.
    ///
    /// Replaces the day table while the scope is Month: a table of one month's
    /// entries is a scroll, whereas the shape of a month — which days are empty,
    /// where the week boundaries fall — is the thing you open a month for.
    ///
    /// The measurements are the design's own and have no tokens in `theme.rs`,
    /// which is why they are named here rather than dropped inline: 28-px card,
    /// 18-px cells, 6-px gutters.
    fn month_grid(&self, t: Theme, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        const CARD: Pixels = px(28.);
        const CELL: Pixels = px(18.);
        const GUTTER: Pixels = px(6.);
        /// A cell below this is a day number and nothing else — the chips are the
        /// point of the grid, so it stops shrinking here and the grid scrolls
        /// instead. The design draws at 1080 px where six rows fit comfortably;
        /// on a shorter window something has to give, and clipped cells are worse
        /// than a scroll bar.
        const ROW_MIN_H: Pixels = px(84.);
        /// Chips per cell before the rest becomes "+N more". Four fits the cell
        /// height at 1080; a fifth is clipped, and a clipped chip reads as a
        /// rendering fault rather than as an overflow.
        const CHIPS: usize = 4;

        let grid = crate::calendar::month_grid(self.date);
        let month = self.date.m;
        // Day -> its summary, for the days this month actually has.
        let find = |date: Ymd| -> Option<&DaySummary> {
            let iso = fmt_date(date);
            self.month.iter().find(|d| d.date == iso)
        };

        let mut rows = Vec::with_capacity(grid.len() / 7);
        for week in grid.chunks(7) {
            let mut cells = Vec::with_capacity(7);
            for date in week {
                let date = *date;
                let inside = date.m == month;
                let today = date == self.today;
                let summary = find(date);
                let total = summary.map_or(0, |d| d.total_seconds);

                let mut cell = div()
                    .id(SharedString::from(fmt_date(date)))
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .gap(space::S1)
                    .px(space::S3)
                    .py(space::S2)
                    .rounded(CELL)
                    .overflow_hidden()
                    .cursor_pointer()
                    // Today is the accent wash; a day of this month sits on the
                    // window ground; a day belonging to a neighbouring month is
                    // barely there, so the month's own shape reads first.
                    .bg(if today {
                        t.accent_wash_strong()
                    } else if inside {
                        t.bg
                    } else {
                        t.hover()
                    })
                    .when(!today, |d| d.hover(|s| s.bg(t.active())))
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(space::S2)
                            .child(
                                div()
                                    .font_family(FONT_MONO)
                                    .text_size(text::CONTROL)
                                    .text_color(match (today, inside) {
                                        (true, _) => t.accent,
                                        (false, true) => t.text,
                                        (false, false) => t.text_disabled(),
                                    })
                                    .child(SharedString::from(date.d.to_string())),
                            )
                            .when(total > 0, |d| {
                                d.child(
                                    div()
                                        .font_family(FONT_MONO)
                                        .text_size(text::TINY)
                                        .text_color(t.accent)
                                        .child(SharedString::from(fmt_hm(total))),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        // Clicking a day is how you get from the shape of the
                        // month to the entries of one day.
                        this.go_to(date, Scope::Day, cx)
                    }));

                if let Some(summary) = summary {
                    for (task, seconds) in summary.per_task.iter().take(CHIPS) {
                        cell = cell.child(chip(t, task, *seconds));
                    }
                    let rest = summary.per_task.len().saturating_sub(CHIPS);
                    if rest > 0 {
                        cell = cell.child(
                            div()
                                .text_size(text::TINY)
                                .text_color(t.text_meta())
                                .child(SharedString::from(format!("+{rest} more"))),
                        );
                    }
                }
                cells.push(cell);
            }
            rows.push(
                div()
                    .flex()
                    .flex_1()
                    .min_h(ROW_MIN_H)
                    .gap(GUTTER)
                    .children(cells),
            );
        }

        // The weekday header, Monday first like the grid under it.
        let mut head = div()
            .flex()
            .flex_none()
            .min_w_0()
            .gap(GUTTER)
            .px(space::S2)
            .pb(space::S2)
            .text_size(text::LABEL)
            .text_color(t.text_meta());
        for name in WEEKDAY_NAMES {
            head = head.child(div().flex_1().min_w_0().truncate().child(name));
        }

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .p(space::S2)
            .rounded(CARD)
            .bg(t.surface)
            .overflow_hidden()
            .child(head)
            // The weeks scroll; the header does not. Six rows that will not go
            // below `ROW_MIN_H` cannot be squeezed into a short window, and
            // without this the card grew past the bottom of the screen instead.
            .child(
                div()
                    .id("month-weeks")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(GUTTER)
                    .overflow_y_scroll()
                    .children(rows),
            )
    }

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
                alpha(t.neutral.s300, 0.5)
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
                    .font_family(FONT_MONO)
                    .text_size(text::TINY)
                    .text_color(t.text_meta())
                    .child(SharedString::from(format!("{d:02}"))),
            );
        }
        card(t)
            // The wide half of the 1.35:1 pair — see `CHART_SHARE`.
            .flex_basis(relative(CHART_SHARE))
            .flex_shrink_1()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(space::S2)
                    .child(
                        div()
                            .flex_shrink_1()
                            .min_w_0()
                            .truncate()
                            .font_family(FONT_HEADING)
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
                            .font_family(FONT_MONO)
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
        card(t)
            // The narrow half of the 1.35:1 pair — see `CHART_SHARE`.
            .flex_basis(relative(1. - CHART_SHARE))
            .flex_shrink_1()
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
                            .font_family(FONT_HEADING)
                            .text_size(text::H5)
                            .text_color(t.text)
                            .child("Summary"),
                    )
                    .child(
                        Button::new("copy-summary", "Copy")
                            .tone(Tone::Quiet)
                            .size(Size::Sm)
                            .render(t, cx.listener(|this, _, _, cx| this.copy(false, cx))),
                    )
                    .child(
                        Button::new("copy-csv", "CSV")
                            .tone(Tone::Quiet)
                            .size(Size::Sm)
                            .render(t, cx.listener(|this, _, _, cx| this.copy(true, cx))),
                    ),
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
                    .font_family(FONT_MONO)
                    .text_color(t.text_muted())
                    .child(SharedString::from(span)),
            )
            .child(
                Button::new("draft-save", "Save")
                    .icon("save")
                    .tone(Tone::Primary)
                    .render(t, cx.listener(|this, _, _, cx| this.save_draft(cx))),
            )
            .child(IconButton::new("draft-cancel", "close").render(
                t,
                cx.listener(|this, _, _, cx| {
                    this.draft = None;
                    cx.notify();
                }),
            ))
            .when_some(id, |d, id| {
                d.child(
                    IconButton::new("draft-delete", "trash")
                        .render(t, cx.listener(move |this, _, _, cx| this.delete(id, cx))),
                )
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
                    .font_family(FONT_MONO)
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
                    .font_family(FONT_MONO)
                    .text_size(text::TINY)
                    .text_color(t.text_disabled())
                    .child(SharedString::from(format!("#{id}"))),
            )
            .child(
                col(COL_TOTAL)
                    .text_right()
                    .font_family(FONT_MONO)
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
                        d.child(
                            IconButton::new(SharedString::from(format!("del-{id}")), "trash")
                                .render(t, cx.listener(move |this, _, _, cx| this.delete(id, cx))),
                        )
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
            // The link that was missing: this row holds a fixed-width rail and a
            // flexible pane, and without being allowed to shrink it keeps the sum
            // of their content widths. Everything on the right — the total, the
            // Sunday column, the CSV button — was then pushed off the window by
            // exactly the overflow.
            .min_w_0()
            .h_full()
            .bg(t.bg)
            .text_size(text::CONTROL)
            .line_height(text::BODY_LINE)
            .font_family(FONT_BODY)
            .text_color(t.text)
            .child(self.list(t, cx))
            .child(self.detail(t, cx))
    }
}

// ---------------------------------------------------------------------------
// Small building blocks, all of them token-only.
// ---------------------------------------------------------------------------

/// One entry chip in a month cell: the duration, then the task name.
///
/// Duration first and in mono, because a column of chips is scanned for "how long
/// did that take", and a proportional number in a stack of them does not line up.
/// The name truncates; the duration never does, or the chip says nothing.
fn chip(t: Theme, task: &str, seconds: i64) -> Div {
    let (fill, ink) = t.tag_accent_2();
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(space::S1)
        .px(space::S2)
        .rounded(radius::PILL)
        .bg(fill)
        .text_color(ink)
        .text_size(text::TINY)
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .font_family(FONT_MONO)
                .child(SharedString::from(fmt_hm(seconds))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(SharedString::from(task.to_string())),
        )
}

/// A month step for the mini-month's header. Steps whole months whatever the
/// scope is — the card shows a month, so its arrows move a month.
fn mini_step(t: Theme, glyph: &'static str, delta: i32, cx: &mut Context<Track>) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("mini-step{delta}")))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(space::S6)
        .rounded(radius::PILL)
        .cursor_pointer()
        .hover(|s| s.bg(t.hover()))
        .child(icon_tinted(glyph, text::SMALL, t.text_muted()))
        .on_click(cx.listener(move |this, _, _, cx| {
            let date = step_month(this.date, delta as i64);
            let scope = this.scope;
            this.go_to(date, scope, cx)
        }))
}

fn heading(t: Theme, label: impl Into<SharedString>) -> Div {
    div()
        .font_family(FONT_HEADING)
        .text_size(text::H5)
        .text_color(t.text)
        .child(label.into())
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
        .child(icon_tinted("chevron_right", text::SMALL, t.text_meta()))
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
        // The week number rides along, as the calendar design's title has it.
        assert_eq!(fmt_week(ymd(2026, 7, 26)), "20 – 26 July 2026 · week 30");
        // A week that straddles two months, and two years. The second is the case
        // the ISO rule is for: these dates start in 2025 and are week 1 of 2026.
        assert_eq!(
            fmt_week(ymd(2026, 3, 1)),
            "23 February – 1 March 2026 · week 9"
        );
        assert_eq!(
            fmt_week(ymd(2026, 1, 1)),
            "29 December 2025 – 4 January 2026 · week 1"
        );
        // Every day of one week gets the same heading, so paging inside a week
        // cannot change the title under the user.
        for offset in 0..7 {
            assert_eq!(
                fmt_week(step_day(week_start(ymd(2026, 7, 26)), offset)),
                fmt_week(ymd(2026, 7, 26)),
                "day +{offset}"
            );
        }
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

    /// A week with a spring-forward Sunday in it, as the store reports it: the
    /// midnights are 23 hours apart across the change, and the offset is +1h before
    /// it and +2h after. One offset for the week draws one of the two an hour off,
    /// which is the bug these two functions exist to prevent.
    ///
    /// The numbers are the store's to produce; this only checks that the view reads
    /// whatever it is given per day. No DST rule is implemented here.
    fn dst_week() -> (Ymd, Vec<i64>) {
        let monday = ymd(2026, 3, 23); // the Sunday is 2026-03-29
        let utc = days_from_civil(monday) * 86_400;
        let midnights = (0..8)
            .map(|i| {
                // Days 0..=5 begin at 23:00 UTC the evening before (UTC+1), days 6
                // and 7 at 22:00 (UTC+2) — the change is on day 6. East of UTC the
                // local midnight is *earlier* than the UTC one, so the shift these
                // functions return is negative.
                let east = if i < 6 { 3_600 } else { 7_200 };
                utc + i * 86_400 - east
            })
            .collect();
        (monday, midnights)
    }

    #[test]
    fn the_offset_is_per_day_so_a_dst_week_keeps_every_column_on_its_own_hour() {
        let (monday, midnights) = dst_week();
        // Each day's own midnight comes back exactly, whichever side it is on.
        for (i, midnight) in midnights.iter().enumerate() {
            let date = step_day(monday, i as i64);
            assert_eq!(
                local_midnight_of(monday, &midnights, date),
                *midnight,
                "{i}"
            );
            // 09:00 local on that day reads as 09:00, not 08:00 or 10:00.
            let nine = midnight + 9 * 3_600;
            assert_eq!(
                fmt_clock(nine - offset_at(monday, &midnights, nine)),
                "09:00"
            );
            assert_eq!(
                entry_day_of(nine, offset_at(monday, &midnights, nine)),
                date
            );
        }
        // The change really is in there: 23 hours between Saturday and Sunday.
        assert_eq!(midnights[6] - midnights[5], 82_800);
        assert_eq!(offset_at(monday, &midnights, midnights[0]), -3_600);
        assert_eq!(offset_at(monday, &midnights, midnights[6]), -7_200);
        // Monday's offset used for the whole week draws Sunday an hour off, which
        // is what this is instead of.
        let sunday_nine = midnights[6] + 9 * 3_600;
        assert_eq!(fmt_clock(sunday_nine + 3_600), "08:00");
    }

    #[test]
    fn an_instant_outside_the_span_borrows_the_nearest_day_and_no_store_is_utc() {
        let (monday, midnights) = dst_week();
        // Before the span: the first day's offset. After it: the last one's.
        let earlier = midnights[0] - 30 * 86_400;
        assert_eq!(offset_at(monday, &midnights, earlier), -3_600);
        let later = midnights[7] + 30 * 86_400;
        assert_eq!(offset_at(monday, &midnights, later), -7_200);
        // A day outside the span is still placed, off the nearest offset.
        let far = step_day(monday, 40);
        assert_eq!(
            local_midnight_of(monday, &midnights, far),
            days_from_civil(far) * 86_400 - 7_200
        );
        // No store, no offset: UTC, exactly the old rendering.
        assert_eq!(offset_at(monday, &[], midnights[0]), 0);
        assert_eq!(
            local_midnight_of(monday, &[], monday),
            days_from_civil(monday) * 86_400
        );
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
