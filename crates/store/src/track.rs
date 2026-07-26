//! Local time tracking.
//!
//! Two processes write here: the desktop app and the MCP server that an agent
//! talks to. That shapes three decisions:
//!
//! * **"Only one timer runs" is a schema constraint**, not application logic —
//!   see the `track_entry_single_running` index in `MIGRATIONS`. Two processes
//!   cannot coordinate a check-then-insert, so the database decides.
//! * **`now` is always a parameter.** Nothing here reads the clock, so every
//!   duration and boundary case is testable without sleeping.
//! * **Times are Unix seconds UTC**; local-day grouping is done in SQL with
//!   SQLite's `'localtime'` modifier, so no timezone database is needed and the
//!   app and the MCP server agree by construction.
//!
//! # Windows and overlap
//!
//! A day (or month) is the half-open Unix-second range `[from, to)` that SQLite
//! reports for that local calendar span, so a 23- or 25-hour DST day is exactly
//! as long as the operating system says it is.
//!
//! An entry belongs to a window when it *overlaps* it, not when it starts in it.
//! The two views then differ deliberately:
//!
//! * **Lists** ([`Store::track_entries_on`], [`Store::track_entries_between`])
//!   return the *whole, unclipped* entry. A timer from 23:50 to 00:30 is one
//!   block of work and is listed on both days, with its real start and stop, so
//!   the UI can show what actually happened.
//! * **Summaries** ([`Store::track_summary_day`], [`Store::track_summary_month`])
//!   count only the seconds that fall *inside* the window. That same entry
//!   contributes 10 minutes to the first day and 30 to the second, so days sum
//!   to the month and nothing is double-counted.
use crate::Store;
use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: i64,
    pub task: String,
    pub note: Option<String>,
    pub started_at: i64,
    /// `None` while the timer is running.
    pub stopped_at: Option<i64>,
    /// `"app"` or `"mcp"`.
    pub source: String,
}

impl Entry {
    /// Seconds elapsed, using `now` for a still-running entry.
    pub fn seconds(&self, now: i64) -> i64 {
        (self.stopped_at.unwrap_or(now) - self.started_at).max(0)
    }

    pub fn is_running(&self) -> bool {
        self.stopped_at.is_none()
    }

    /// Does this entry overlap `[from, to)`?
    ///
    /// A running entry is open-ended, so it overlaps anything starting before
    /// `to`. A zero-length entry has no interior, so it is matched by its start
    /// instant instead — otherwise "logged 0 minutes at 09:00" would appear on
    /// no day at all.
    fn overlaps(&self, from: i64, to: i64) -> bool {
        self.started_at < to
            && (self.stopped_at.is_none_or(|s| s > from) || self.started_at >= from)
    }

    /// Seconds of this entry that fall inside `[from, to)`, counting a running
    /// entry up to `now`.
    fn seconds_within(&self, from: i64, to: i64, now: i64) -> i64 {
        let end = self.stopped_at.unwrap_or(now).min(to);
        (end - self.started_at.max(from)).max(0)
    }
}

/// Totals for one local day, newest-first per task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaySummary {
    /// `YYYY-MM-DD`, local time.
    pub date: String,
    pub total_seconds: i64,
    /// `(task, seconds)`, descending by seconds.
    pub per_task: Vec<(String, i64)>,
}

/// The one place entries are read, so every query agrees on shape and joins.
const SELECT_ENTRY: &str = "SELECT e.id, t.name, e.note, e.started_at, e.stopped_at, e.source \
     FROM track_entry e JOIN track_task t ON t.id = e.task_id";

fn entry_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        task: r.get(1)?,
        note: r.get(2)?,
        started_at: r.get(3)?,
        stopped_at: r.get(4)?,
        source: r.get(5)?,
    })
}

/// The overlap test from [`Entry::overlaps`], in SQL. `?1` is `from`, `?2` `to`.
const OVERLAPS_WINDOW: &str =
    "e.started_at < ?2 AND (e.stopped_at IS NULL OR e.stopped_at > ?1 OR e.started_at >= ?1)";

fn clean_task(task: &str) -> Result<&str> {
    let task = task.trim();
    if task.is_empty() {
        bail!("a task name cannot be empty");
    }
    Ok(task)
}

/// Blank notes are stored as NULL, so `""` and "not set" cannot drift apart.
fn clean_note(note: Option<&str>) -> Option<&str> {
    note.map(str::trim).filter(|n| !n.is_empty())
}

impl Store {
    /// Start a timer, stopping whatever was running first.
    ///
    /// Creates the task on first use — an agent booking time should not have to
    /// declare a task beforehand.
    pub fn track_start(
        &mut self,
        task: &str,
        note: Option<&str>,
        source: &str,
        now: i64,
    ) -> Result<Entry> {
        let task = clean_task(task)?;
        // One transaction: a crash can never leave two timers running, nor stop
        // the old one without starting the new.
        let tx = self.conn.transaction()?;
        // MAX guards against a clock that went backwards, which the CHECK
        // constraint would otherwise reject.
        tx.execute(
            "UPDATE track_entry SET stopped_at = MAX(started_at, ?1) WHERE stopped_at IS NULL",
            [now],
        )?;
        let task_id = task_id(&tx, task, now)?;
        let id = insert_entry(&tx, task_id, clean_note(note), now, None, source)?;
        tx.commit()?;
        self.entry(id)?
            .context("the entry just started has disappeared")
    }

    /// Stop the running timer. `Ok(None)` when nothing was running, which is not
    /// an error: an agent may stop twice.
    pub fn track_stop(&mut self, now: i64) -> Result<Option<Entry>> {
        let Some(running) = self.track_running()? else {
            return Ok(None);
        };
        let stopped_at = now.max(running.started_at);
        self.conn.execute(
            "UPDATE track_entry SET stopped_at = ?1 WHERE id = ?2 AND stopped_at IS NULL",
            params![stopped_at, running.id],
        )?;
        Ok(Some(Entry {
            stopped_at: Some(stopped_at),
            ..running
        }))
    }

    pub fn track_running(&self) -> Result<Option<Entry>> {
        Ok(self
            .conn
            .query_row(
                &format!("{SELECT_ENTRY} WHERE e.stopped_at IS NULL"),
                [],
                entry_from_row,
            )
            .optional()?)
    }

    /// Record an already-finished block of work, for an agent booking time after
    /// the fact.
    pub fn track_log(
        &mut self,
        task: &str,
        note: Option<&str>,
        started_at: i64,
        stopped_at: i64,
        source: &str,
    ) -> Result<Entry> {
        let task = clean_task(task)?;
        if stopped_at < started_at {
            // The CHECK constraint would also refuse this, but with a message
            // an agent cannot act on.
            bail!(
                "cannot log {task}: it ends before it starts \
                 (started_at {started_at}, stopped_at {stopped_at})"
            );
        }
        let tx = self.conn.transaction()?;
        let task_id = task_id(&tx, task, started_at)?;
        let id = insert_entry(
            &tx,
            task_id,
            clean_note(note),
            started_at,
            Some(stopped_at),
            source,
        )?;
        tx.commit()?;
        self.entry(id)?
            .context("the entry just logged has disappeared")
    }

    /// Entries overlapping the given local day (`YYYY-MM-DD`), oldest first.
    ///
    /// Whole entries, not clipped to the day: see the module docs.
    pub fn track_entries_on(&self, date: &str) -> Result<Vec<Entry>> {
        let (from, to) = self.day_bounds(date)?;
        self.track_entries_between(from, to)
    }

    /// Entries overlapping `[from, to)` in Unix seconds, oldest first.
    pub fn track_entries_between(&self, from: i64, to: i64) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_ENTRY} WHERE {OVERLAPS_WINDOW} ORDER BY e.started_at, e.id"
        ))?;
        Ok(stmt
            .query_map(params![from, to], entry_from_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Free-text search over task names and notes, newest first.
    pub fn track_search(&self, needle: &str, limit: usize) -> Result<Vec<Entry>> {
        // LIKE is case-insensitive for ASCII in SQLite; the wildcards in the
        // needle are escaped so searching for "50%" means the literal.
        let pattern = format!(
            "%{}%",
            needle
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let mut stmt = self.conn.prepare(&format!(
            "{SELECT_ENTRY} WHERE t.name LIKE ?1 ESCAPE '\\' OR e.note LIKE ?1 ESCAPE '\\' \
             ORDER BY e.started_at DESC, e.id DESC LIMIT ?2"
        ))?;
        Ok(stmt
            .query_map(
                params![pattern, i64::try_from(limit).unwrap_or(i64::MAX)],
                entry_from_row,
            )?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn track_summary_day(&self, date: &str, now: i64) -> Result<DaySummary> {
        let (from, to) = self.day_bounds(date)?;
        let entries = self.track_entries_between(from, to)?;
        Ok(summarize(date.to_string(), &entries, from, to, now))
    }

    /// One entry per day that has time booked, oldest first. `month` is
    /// `YYYY-MM`, local time.
    pub fn track_summary_month(&self, month: &str, now: i64) -> Result<Vec<DaySummary>> {
        let days = self.month_days(month)?;
        let (Some(first), Some(last)) = (days.first(), days.last()) else {
            return Ok(Vec::new());
        };
        // Fetch the month once, then split per day in Rust with the same
        // clipping rule the day summary uses.
        let entries = self.track_entries_between(first.1, last.2)?;
        Ok(days
            .into_iter()
            .map(|(date, from, to)| summarize(date, &entries, from, to, now))
            .filter(|d| !d.per_task.is_empty())
            .collect())
    }

    pub fn track_tasks(&self) -> Result<Vec<Task>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at FROM track_task ORDER BY name")?;
        Ok(stmt
            .query_map([], |r| {
                Ok(Task {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn track_delete_entry(&mut self, id: i64) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM track_entry WHERE id = ?1", [id])?
            > 0)
    }

    /// A counter that changes whenever *any* connection writes to the database.
    ///
    /// This is `PRAGMA data_version`, which is how the desktop app notices that
    /// the MCP server booked time, without polling the tables themselves.
    pub fn change_counter(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA data_version", [], |r| r.get(0))?)
    }

    /// The local calendar date (`YYYY-MM-DD`) that `now` falls on.
    ///
    /// Callers must not derive this themselves from a UTC clock: everything here
    /// groups days with SQLite's `'localtime'`, so a caller computing the date in
    /// UTC disagrees with its own summaries for the width of the UTC offset
    /// either side of midnight — which in a time tracker means booking work on
    /// the wrong day. Asking SQLite keeps both sides on one definition.
    pub fn local_date(&self, now: i64) -> Result<String> {
        Ok(self
            .conn
            .query_row("SELECT date(?1, 'unixepoch', 'localtime')", [now], |r| {
                r.get(0)
            })?)
    }

    /// The local month (`YYYY-MM`) that `now` falls on.
    pub fn local_month(&self, now: i64) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT strftime('%Y-%m', ?1, 'unixepoch', 'localtime')",
            [now],
            |r| r.get(0),
        )?)
    }

    fn entry(&self, id: i64) -> Result<Option<Entry>> {
        Ok(self
            .conn
            .query_row(
                &format!("{SELECT_ENTRY} WHERE e.id = ?1"),
                [id],
                entry_from_row,
            )
            .optional()?)
    }

    /// `[start, end)` in Unix seconds for a local calendar day.
    ///
    /// SQLite does the arithmetic, so the OS timezone rules apply and a DST day
    /// is 23 or 25 hours long without any date library here.
    fn day_bounds(&self, date: &str) -> Result<(i64, i64)> {
        check_shape(date, "YYYY-MM-DD", &[4, 2, 2])?;
        // date() normalises, so "2024-02-31" comes back as "2024-03-02" and is
        // rejected rather than silently answering about another day.
        let bounds: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT CAST(strftime('%s', ?1 || ' 00:00:00', 'utc') AS INTEGER), \
                        CAST(strftime('%s', date(?1, '+1 day') || ' 00:00:00', 'utc') AS INTEGER) \
                 WHERE date(?1) = ?1",
                [date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        bounds.with_context(|| format!("{date:?} is not a valid date; expected YYYY-MM-DD"))
    }

    /// Every local day of `month` with its `[start, end)` bounds, oldest first.
    fn month_days(&self, month: &str) -> Result<Vec<(String, i64, i64)>> {
        check_shape(month, "YYYY-MM", &[4, 2])?;
        let mut stmt = self.conn.prepare(
            "WITH RECURSIVE d(day) AS ( \
                 SELECT date(?1 || '-01') \
                 UNION ALL \
                 SELECT date(day, '+1 day') FROM d \
                  WHERE strftime('%Y-%m', date(day, '+1 day')) = ?1 \
             ) \
             SELECT day, \
                    CAST(strftime('%s', day || ' 00:00:00', 'utc') AS INTEGER), \
                    CAST(strftime('%s', date(day, '+1 day') || ' 00:00:00', 'utc') AS INTEGER) \
               FROM d WHERE day IS NOT NULL",
        )?;
        let days: Vec<(String, i64, i64)> = stmt
            .query_map([month], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        if days.is_empty() {
            bail!("{month:?} is not a valid month; expected YYYY-MM");
        }
        Ok(days)
    }
}

/// `INSERT OR IGNORE` then read: creating a task is idempotent, so two
/// processes booking the same task at once cannot fail on the UNIQUE name.
fn task_id(tx: &rusqlite::Transaction<'_>, task: &str, created_at: i64) -> Result<i64> {
    tx.execute(
        "INSERT OR IGNORE INTO track_task (name, created_at) VALUES (?1, ?2)",
        params![task, created_at],
    )?;
    Ok(
        tx.query_row("SELECT id FROM track_task WHERE name = ?1", [task], |r| {
            r.get(0)
        })?,
    )
}

fn insert_entry(
    tx: &rusqlite::Transaction<'_>,
    task_id: i64,
    note: Option<&str>,
    started_at: i64,
    stopped_at: Option<i64>,
    source: &str,
) -> Result<i64> {
    tx.execute(
        "INSERT INTO track_entry (task_id, note, started_at, stopped_at, source) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![task_id, note, started_at, stopped_at, source],
    )
    .map_err(|e| {
        // Do not paper over it: the index is the only thing that keeps two
        // processes from both thinking they own the clock.
        if e.to_string().contains("track_entry_single_running") {
            anyhow::anyhow!(
                "another timer is already running; the database allows only one at a time. \
                 Stop it before starting a new one. ({e})"
            )
        } else {
            e.into()
        }
    })?;
    Ok(tx.last_insert_rowid())
}

/// Totals for `[from, to)`, counting only the seconds inside the window.
///
/// Ties are broken by task name so repeated renders do not flicker.
fn summarize(date: String, entries: &[Entry], from: i64, to: i64, now: i64) -> DaySummary {
    let mut per_task: Vec<(String, i64)> = Vec::new();
    for e in entries.iter().filter(|e| e.overlaps(from, to)) {
        let seconds = e.seconds_within(from, to, now);
        match per_task.iter_mut().find(|(name, _)| *name == e.task) {
            Some((_, total)) => *total += seconds,
            None => per_task.push((e.task.clone(), seconds)),
        }
    }
    per_task.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    DaySummary {
        date,
        total_seconds: per_task.iter().map(|(_, s)| s).sum(),
        per_task,
    }
}

/// Cheap shape check before SQLite sees the string, so garbage gets a message
/// naming the expected format instead of an empty result.
fn check_shape(value: &str, expected: &str, groups: &[usize]) -> Result<()> {
    let mut parts = value.split('-');
    let ok = groups.iter().all(|len| {
        parts
            .next()
            .is_some_and(|p| p.len() == *len && p.bytes().all(|b| b.is_ascii_digit()))
    }) && parts.next().is_none();
    if !ok {
        bail!("{value:?} is not a valid {expected}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Unix seconds for a *local* wall-clock time, computed by SQLite, so these
    /// tests are correct in whatever timezone they run in.
    fn at(store: &Store, local: &str) -> i64 {
        store
            .conn
            .query_row(
                "SELECT CAST(strftime('%s', ?1, 'utc') AS INTEGER)",
                [local],
                |r| r.get(0),
            )
            .unwrap()
    }

    fn day_start(store: &Store, date: &str) -> i64 {
        at(store, &format!("{date} 00:00:00"))
    }

    /// A temp directory that cleans up whatever the test leaves behind.
    struct TempDb(PathBuf);

    impl TempDb {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("rp-store-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> PathBuf {
            self.0.join("state.db")
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn start_then_stop_round_trips_with_the_elapsed_time() {
        let mut store = Store::open_in_memory().unwrap();
        let started = store
            .track_start("writing", Some("the report"), "app", 1_000)
            .unwrap();
        assert!(started.is_running());
        assert_eq!(started.task, "writing");
        assert_eq!(started.note.as_deref(), Some("the report"));
        assert_eq!(store.track_running().unwrap(), Some(started.clone()));

        let stopped = store.track_stop(1_600).unwrap().unwrap();
        assert_eq!(stopped.id, started.id);
        assert_eq!(stopped.stopped_at, Some(1_600));
        assert_eq!(stopped.seconds(9_999), 600, "a stopped entry ignores `now`");
        assert_eq!(store.track_running().unwrap(), None);
        // And it is persisted, not just returned.
        assert_eq!(store.entry(started.id).unwrap(), Some(stopped));
    }

    #[test]
    fn starting_a_second_timer_ends_the_first_and_leaves_exactly_one_running() {
        let mut store = Store::open_in_memory().unwrap();
        let first = store.track_start("email", None, "app", 100).unwrap();
        let second = store.track_start("coding", None, "mcp", 400).unwrap();

        let first = store.entry(first.id).unwrap().unwrap();
        assert_eq!(
            first.stopped_at,
            Some(400),
            "the first timer must have been stopped"
        );
        assert_eq!(store.track_running().unwrap(), Some(second));

        let running: i64 = store
            .conn
            .query_row(
                "SELECT count(*) FROM track_entry WHERE stopped_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(running, 1);
    }

    #[test]
    fn stopping_nothing_and_stopping_twice_are_both_fine() {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(
            store.track_stop(500).unwrap(),
            None,
            "nothing running is not an error"
        );
        store.track_start("email", None, "mcp", 100).unwrap();
        assert!(store.track_stop(200).unwrap().is_some());
        assert_eq!(
            store.track_stop(300).unwrap(),
            None,
            "an agent may stop twice"
        );
    }

    #[test]
    fn the_database_refuses_a_second_running_entry_from_another_connection() {
        let db = TempDb::new("track-two-conns");
        let mut app = Store::open_at(&db.path()).unwrap();
        let mut mcp = Store::open_at(&db.path()).unwrap();

        let running = app.track_start("coding", None, "app", 1_000).unwrap();
        assert_eq!(mcp.track_running().unwrap(), Some(running.clone()));

        // A raw insert is what a second process would do if it trusted its own
        // check-then-insert. The unique partial index is the only thing that
        // stops it, so assert on the index by name.
        let task_id: i64 = mcp
            .conn
            .query_row("SELECT id FROM track_task WHERE name = 'coding'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let err = mcp
            .conn
            .execute(
                "INSERT INTO track_entry (task_id, note, started_at, stopped_at, source) \
                 VALUES (?1, NULL, 2000, NULL, 'mcp')",
                [task_id],
            )
            .unwrap_err()
            .to_string();
        assert!(err.contains("track_entry_single_running"), "{err}");

        // And the supported path hands over instead of racing: the other
        // process's timer is stopped, and exactly one runs afterwards.
        let handed_over = mcp.track_start("email", None, "mcp", 2_000).unwrap();
        assert_eq!(
            app.entry(running.id).unwrap().unwrap().stopped_at,
            Some(2_000)
        );
        assert_eq!(app.track_running().unwrap(), Some(handed_over));
    }

    #[test]
    fn a_zero_length_log_is_allowed_and_a_reversed_one_is_not() {
        let mut store = Store::open_in_memory().unwrap();
        let zero = store
            .track_log("standup", None, 5_000, 5_000, "mcp")
            .unwrap();
        assert_eq!(zero.seconds(9_999), 0);
        assert!(
            !zero.is_running(),
            "a zero-length entry is finished, not running"
        );

        let err = store
            .track_log("standup", None, 5_000, 4_000, "mcp")
            .unwrap_err()
            .to_string();
        assert!(err.contains("ends before it starts"), "{err}");
        assert!(
            err.contains("standup"),
            "the message must name the task: {err}"
        );
    }

    #[test]
    fn a_logged_entry_keeps_its_note_and_blank_notes_become_none() {
        let mut store = Store::open_in_memory().unwrap();
        let noted = store
            .track_log("review", Some(" PR #12 "), 10, 70, "mcp")
            .unwrap();
        assert_eq!(noted.note.as_deref(), Some("PR #12"), "notes are trimmed");
        let blank = store
            .track_log("review", Some("   "), 80, 90, "mcp")
            .unwrap();
        assert_eq!(blank.note, None);
        assert!(
            store.track_log("  ", None, 0, 1, "mcp").is_err(),
            "empty task name"
        );
    }

    #[test]
    fn an_entry_inside_a_day_is_listed_and_summed_for_that_day_only() {
        let mut store = Store::open_in_memory().unwrap();
        let start = at(&store, "2024-05-14 09:00:00");
        store
            .track_log("coding", None, start, start + 3_600, "app")
            .unwrap();

        let day = store.track_entries_on("2024-05-14").unwrap();
        assert_eq!(day.len(), 1);
        let summary = store.track_summary_day("2024-05-14", start).unwrap();
        assert_eq!(summary.total_seconds, 3_600);
        assert_eq!(summary.per_task, [("coding".to_string(), 3_600)]);

        assert!(store.track_entries_on("2024-05-13").unwrap().is_empty());
        assert_eq!(
            store
                .track_summary_day("2024-05-15", start)
                .unwrap()
                .total_seconds,
            0
        );
    }

    #[test]
    fn an_entry_crossing_midnight_is_listed_on_both_days_and_split_between_them() {
        let mut store = Store::open_in_memory().unwrap();
        let start = at(&store, "2024-05-14 23:50:00");
        let stop = at(&store, "2024-05-15 00:30:00");
        let midnight = day_start(&store, "2024-05-15");
        let logged = store.track_log("deploy", None, start, stop, "app").unwrap();

        // Lists return the whole, unclipped entry on both days.
        assert_eq!(
            store.track_entries_on("2024-05-14").unwrap(),
            std::slice::from_ref(&logged)
        );
        assert_eq!(store.track_entries_on("2024-05-15").unwrap(), [logged]);

        // Summaries split the seconds, and the halves add up.
        let first = store.track_summary_day("2024-05-14", stop).unwrap();
        let second = store.track_summary_day("2024-05-15", stop).unwrap();
        assert_eq!(first.total_seconds, midnight - start);
        assert_eq!(second.total_seconds, stop - midnight);
        assert_eq!(first.total_seconds + second.total_seconds, stop - start);

        // The day it merely touches at midnight gets nothing and is not listed.
        assert!(store.track_entries_on("2024-05-13").unwrap().is_empty());
    }

    #[test]
    fn a_full_local_day_totals_the_day_length_even_across_a_dst_shift() {
        let mut store = Store::open_in_memory().unwrap();
        // Day bounds come from SQLite, so this holds where the day is 23 or 25
        // hours long just as well as where it is 24.
        for date in ["2024-03-31", "2024-10-27", "2024-11-03", "2024-05-14"] {
            let from = day_start(&store, date);
            let to = day_start(&store, &next_day(&store, date));
            store.track_log("all day", None, from, to, "app").unwrap();
            let summary = store.track_summary_day(date, to).unwrap();
            assert_eq!(
                summary.total_seconds,
                to - from,
                "{date} is {} seconds",
                to - from
            );
            store.conn.execute("DELETE FROM track_entry", []).unwrap();
        }
    }

    fn next_day(store: &Store, date: &str) -> String {
        store
            .conn
            .query_row("SELECT date(?1, '+1 day')", [date], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn a_running_entry_counts_up_to_now_however_old_it_is() {
        let mut store = Store::open_in_memory().unwrap();
        let start = at(&store, "2024-05-14 08:00:00");
        store.track_start("coding", None, "app", start).unwrap();

        let now = start + 1_800;
        assert_eq!(
            store
                .track_summary_day("2024-05-14", now)
                .unwrap()
                .total_seconds,
            1_800
        );

        // Still running three days later: each day gets its own share, and the
        // future day gets nothing.
        let later = at(&store, "2024-05-17 10:00:00");
        let d15 = day_start(&store, "2024-05-15");
        let d16 = day_start(&store, "2024-05-16");
        assert_eq!(
            store
                .track_summary_day("2024-05-15", later)
                .unwrap()
                .total_seconds,
            d16 - d15,
            "a running entry fills the whole intervening day"
        );
        assert_eq!(
            store
                .track_summary_day("2024-05-17", later)
                .unwrap()
                .total_seconds,
            later - day_start(&store, "2024-05-17")
        );
        assert_eq!(
            store
                .track_summary_day("2024-05-18", later)
                .unwrap()
                .total_seconds,
            0,
            "nothing is booked in the future"
        );
        assert_eq!(store.track_entries_on("2024-05-16").unwrap().len(), 1);
    }

    #[test]
    fn per_task_is_descending_by_seconds_with_ties_broken_by_name() {
        let mut store = Store::open_in_memory().unwrap();
        let start = at(&store, "2024-05-14 09:00:00");
        store
            .track_log("zeta", None, start, start + 60, "app")
            .unwrap();
        store
            .track_log("alpha", None, start, start + 60, "app")
            .unwrap();
        store
            .track_log("middle", None, start, start + 600, "app")
            .unwrap();
        // Two blocks on one task accumulate rather than appearing twice.
        store
            .track_log("alpha", None, start + 600, start + 660, "app")
            .unwrap();

        let summary = store.track_summary_day("2024-05-14", start).unwrap();
        assert_eq!(
            summary.per_task,
            [
                ("middle".to_string(), 600),
                ("alpha".to_string(), 120),
                ("zeta".to_string(), 60),
            ]
        );
        assert_eq!(summary.total_seconds, 780);
    }

    #[test]
    fn a_month_summary_lists_only_days_with_time_oldest_first() {
        let mut store = Store::open_in_memory().unwrap();
        let d3 = at(&store, "2024-05-03 09:00:00");
        let d1 = at(&store, "2024-05-01 09:00:00");
        let d20 = at(&store, "2024-05-20 09:00:00");
        store
            .track_log("coding", None, d3, d3 + 300, "app")
            .unwrap();
        store.track_log("email", None, d1, d1 + 100, "app").unwrap();
        store
            .track_log("coding", None, d20, d20 + 900, "mcp")
            .unwrap();
        // A neighbouring month must not leak in.
        let june = at(&store, "2024-06-01 09:00:00");
        store
            .track_log("coding", None, june, june + 60, "app")
            .unwrap();

        let month = store.track_summary_month("2024-05", d20).unwrap();
        let dates: Vec<&str> = month.iter().map(|d| d.date.as_str()).collect();
        assert_eq!(dates, ["2024-05-01", "2024-05-03", "2024-05-20"]);
        assert_eq!(month[2].total_seconds, 900);
        assert_eq!(month.iter().map(|d| d.total_seconds).sum::<i64>(), 1_300);

        assert!(
            store
                .track_summary_month("2024-04", d20)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_month_boundary_entry_is_split_across_both_months() {
        let mut store = Store::open_in_memory().unwrap();
        let start = at(&store, "2024-05-31 23:00:00");
        let stop = at(&store, "2024-06-01 01:00:00");
        let boundary = day_start(&store, "2024-06-01");
        store
            .track_log("release", None, start, stop, "app")
            .unwrap();

        let may = store.track_summary_month("2024-05", stop).unwrap();
        let june = store.track_summary_month("2024-06", stop).unwrap();
        assert_eq!(may.last().unwrap().total_seconds, boundary - start);
        assert_eq!(june.first().unwrap().total_seconds, stop - boundary);
        assert_eq!(may.len(), 1);
        assert_eq!(june.len(), 1);
    }

    #[test]
    fn search_covers_names_and_notes_case_insensitively_newest_first() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .track_log("Coding", Some("refactor the store"), 100, 200, "app")
            .unwrap();
        store
            .track_log("email", Some("reply to CODING thread"), 300, 400, "mcp")
            .unwrap();
        store.track_log("standup", None, 500, 600, "app").unwrap();

        let hits = store.track_search("coding", 10).unwrap();
        assert_eq!(hits.len(), 2, "matches both the name and the note");
        assert_eq!(hits[0].started_at, 300, "newest first");

        assert_eq!(store.track_search("REFACTOR", 10).unwrap().len(), 1);
        assert_eq!(
            store.track_search("coding", 1).unwrap().len(),
            1,
            "limit honoured"
        );
        assert!(store.track_search("coding", 0).unwrap().is_empty());
        assert!(store.track_search("nothing here", 10).unwrap().is_empty());
        // Wildcards in the needle are literal, not a match-everything.
        assert!(store.track_search("%", 10).unwrap().is_empty());
    }

    #[test]
    fn garbage_dates_and_months_are_errors_not_empty_results() {
        let store = Store::open_in_memory().unwrap();
        for bad in [
            "",
            "today",
            "2024-5-4",
            "2024-05-14T09:00",
            "2024-02-31",
            "24-05-14",
        ] {
            let err = store.track_summary_day(bad, 0).unwrap_err().to_string();
            assert!(
                err.contains("valid"),
                "the message must say what was expected: {err}"
            );
            assert!(store.track_entries_on(bad).is_err(), "entries_on({bad:?})");
        }
        for bad in ["", "2024", "2024-13", "May", "2024-05-14"] {
            assert!(
                store.track_summary_month(bad, 0).is_err(),
                "summary_month({bad:?})"
            );
        }
    }

    #[test]
    fn deleting_returns_whether_a_row_went_away() {
        let mut store = Store::open_in_memory().unwrap();
        let entry = store.track_log("coding", None, 100, 200, "app").unwrap();
        assert!(store.track_delete_entry(entry.id).unwrap());
        assert!(!store.track_delete_entry(entry.id).unwrap(), "already gone");
        assert!(!store.track_delete_entry(4_242).unwrap(), "never existed");
        // The task survives its entries, so it stays offerable in the UI.
        assert_eq!(store.track_tasks().unwrap().len(), 1);
    }

    #[test]
    fn tasks_are_created_on_first_use_and_listed_by_name() {
        let mut store = Store::open_in_memory().unwrap();
        store.track_log("zeta", None, 100, 200, "mcp").unwrap();
        store.track_start("alpha", None, "app", 300).unwrap();
        store.track_log(" zeta ", None, 400, 500, "mcp").unwrap();

        let tasks = store.track_tasks().unwrap();
        assert_eq!(
            tasks.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
            ["alpha", "zeta"],
            "a repeated (or padded) name must not create a second task"
        );
        assert_eq!(
            tasks[1].created_at, 100,
            "created_at is when it was first used"
        );
    }

    /// The UI used to compute "today" from the UTC clock while every query here
    /// groups by local time, so the two disagreed around midnight. These are the
    /// one definition both sides must share.
    #[test]
    fn local_date_and_month_agree_with_the_grouping_used_by_summaries() {
        let store = Store::open_in_memory().unwrap();
        // Pick an instant, ask for its local day, and check that an entry at
        // that instant really lands on that day's summary.
        let now = at(&store, "2024-05-14 12:00:00");
        let date = store.local_date(now).unwrap();
        let month = store.local_month(now).unwrap();
        assert_eq!(month, date[..7], "month must be the date's prefix");

        let mut store = store;
        store
            .track_log("write docs", None, now, now + 600, "app")
            .unwrap();
        let summary = store.track_summary_day(&date, now + 600).unwrap();
        assert_eq!(
            summary.total_seconds, 600,
            "the day the store names must hold the entry"
        );

        // And an instant just before local midnight belongs to the earlier day.
        let late = at(&store, "2024-05-14 23:59:59");
        assert_eq!(store.local_date(late).unwrap(), "2024-05-14");
        let just_after = at(&store, "2024-05-15 00:00:00");
        assert_eq!(store.local_date(just_after).unwrap(), "2024-05-15");
    }

    #[test]
    fn the_change_counter_moves_when_another_connection_writes() {
        let db = TempDb::new("track-change-counter");
        let app = Store::open_at(&db.path()).unwrap();
        let mut mcp = Store::open_at(&db.path()).unwrap();

        let before = app.change_counter().unwrap();
        assert_eq!(
            app.change_counter().unwrap(),
            before,
            "reads must not move it"
        );

        mcp.track_start("coding", None, "mcp", 1_000).unwrap();
        let after = app.change_counter().unwrap();
        assert_ne!(
            after, before,
            "the app must be able to notice the MCP server's write"
        );

        // And the app can then see the entry itself.
        assert_eq!(
            app.track_running().unwrap().map(|e| e.task),
            Some("coding".to_string())
        );
        mcp.track_stop(1_100).unwrap();
        assert_ne!(app.change_counter().unwrap(), after);
    }
}
