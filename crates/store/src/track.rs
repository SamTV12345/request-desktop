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
    /// The Jira issue this task books to by default, if any.
    pub issue_key: Option<String>,
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
    /// The Jira issue this block of work books to: the entry's own key where it
    /// has one, otherwise the task's. Read-only — set one or the other through
    /// [`Store::track_set_entry_issue`] / [`Store::track_set_task_issue`], which
    /// is why there is one field here and two columns in the schema.
    pub issue_key: Option<String>,
    /// Whether [`Entry::issue_key`] is this entry's own override rather than
    /// inherited from its task.
    ///
    /// Only the UI needs this, and it needs it badly: an editor that cannot tell
    /// the two apart either re-pins every entry it touches or silently retargets a
    /// whole task when the user meant one block of work.
    pub issue_own: bool,
}

/// A worklog this app has pushed to Jira, and the state it was pushed in.
///
/// Present means "we created worklog `worklog_id` for this entry"; the times are
/// what Jira was told, not what the entry says now, so a later run can see that
/// an edited entry needs updating rather than adding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pushed {
    pub entry_id: i64,
    pub issue_key: String,
    pub worklog_id: String,
    pub started_at: i64,
    pub seconds: i64,
    pub pushed_at: i64,
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
///
/// `COALESCE` is where "the entry overrides its task" is decided: one expression,
/// in the one query every reader goes through, rather than a rule each caller
/// re-implements.
const SELECT_ENTRY: &str = "SELECT e.id, t.name, e.note, e.started_at, e.stopped_at, e.source, \
            COALESCE(e.issue_key, t.issue_key), e.issue_key IS NOT NULL \
     FROM track_entry e JOIN track_task t ON t.id = e.task_id";

fn entry_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        task: r.get(1)?,
        note: r.get(2)?,
        started_at: r.get(3)?,
        stopped_at: r.get(4)?,
        source: r.get(5)?,
        issue_key: r.get(6)?,
        issue_own: r.get(7)?,
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

/// Normalise an issue key, rejecting anything that is not one.
///
/// This is validation at a trust boundary, not tidiness: the key is pasted by hand
/// and ends up in a REST path (`/rest/api/3/issue/{key}/worklog`). A value with a
/// slash or a `..` in it would address a different endpoint than the caller meant,
/// so the shape Jira itself uses — `LETTERS-DIGITS` — is the shape stored. Blank
/// clears the link.
///
/// Uppercased because Jira keys are, and because two spellings of one ticket in the
/// `per_task` list would look like two tickets.
pub(crate) fn clean_issue(key: Option<&str>) -> Result<Option<String>> {
    let Some(key) = key.map(str::trim).filter(|k| !k.is_empty()) else {
        return Ok(None);
    };
    let (project, number) = key
        .split_once('-')
        .with_context(|| format!("{key:?} is not an issue key; expected something like ABC-123"))?;
    let ok = !project.is_empty()
        && project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
        && project
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && !number.is_empty()
        && number.chars().all(|c| c.is_ascii_digit());
    if !ok {
        bail!("{key:?} is not an issue key; expected something like ABC-123");
    }
    Ok(Some(key.to_ascii_uppercase()))
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

    /// Correct an existing entry in place, keeping its id.
    ///
    /// The id is the whole point: it is what the UI's selection, the inspector and
    /// anything else holding a reference are keyed on, so an edit must not be a
    /// delete plus a fresh insert. `Ok(None)` means there is no such entry —
    /// another process may have deleted it while it was being edited — and nothing
    /// was written, not even the task.
    ///
    /// Only finished entries: a running entry has no end to correct, and clearing
    /// `stopped_at` here would let two timers run at once.
    pub fn track_update_entry(
        &mut self,
        id: i64,
        task: &str,
        note: Option<&str>,
        started_at: i64,
        stopped_at: i64,
        source: &str,
    ) -> Result<Option<Entry>> {
        let task = clean_task(task)?;
        if stopped_at < started_at {
            bail!(
                "cannot save {task}: it ends before it starts \
                 (started_at {started_at}, stopped_at {stopped_at})"
            );
        }
        let tx = self.conn.transaction()?;
        // Checked before `task_id`, so editing an entry that has just been deleted
        // elsewhere does not leave a task behind that nothing uses.
        if tx
            .query_row("SELECT 1 FROM track_entry WHERE id = ?1", [id], |_| Ok(()))
            .optional()?
            .is_none()
        {
            return Ok(None);
        }
        let task_id = task_id(&tx, task, started_at)?;
        tx.execute(
            "UPDATE track_entry \
                SET task_id = ?2, note = ?3, started_at = ?4, stopped_at = ?5, source = ?6 \
              WHERE id = ?1",
            params![
                id,
                task_id,
                clean_note(note),
                started_at,
                stopped_at,
                source
            ],
        )?;
        tx.commit()?;
        self.entry(id)
    }

    /// One entry by id. `Ok(None)` when there is no such entry.
    pub fn track_entry(&self, id: i64) -> Result<Option<Entry>> {
        self.entry(id)
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
            .prepare("SELECT id, name, created_at, issue_key FROM track_task ORDER BY name")?;
        Ok(stmt
            .query_map([], |r| {
                Ok(Task {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    issue_key: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Link a task to a Jira issue, or unlink it with `None`.
    ///
    /// Every entry of that task that has no key of its own follows, by
    /// construction — the link is read through `COALESCE`, never copied onto the
    /// entries, so linking a task retroactively covers work already booked. Which
    /// is the point: the ticket is usually known after the work, not before.
    ///
    /// `Ok(false)` means there is no such task and nothing was written.
    pub fn track_set_task_issue(&mut self, task: &str, issue: Option<&str>) -> Result<bool> {
        let task = clean_task(task)?;
        let issue = clean_issue(issue)?;
        Ok(self.conn.execute(
            "UPDATE track_task SET issue_key = ?2 WHERE name = ?1",
            params![task, issue],
        )? > 0)
    }

    /// Point one entry at a specific issue, overriding its task's link. `None`
    /// removes the override, so the entry falls back to the task's key.
    pub fn track_set_entry_issue(&mut self, id: i64, issue: Option<&str>) -> Result<bool> {
        let issue = clean_issue(issue)?;
        Ok(self.conn.execute(
            "UPDATE track_entry SET issue_key = ?2 WHERE id = ?1",
            params![id, issue],
        )? > 0)
    }

    /// What has already been pushed for the entries overlapping `[from, to)`.
    ///
    /// The window matches [`Store::track_entries_between`], so a caller loads the
    /// day's entries and the day's push records with the same bounds and joins them
    /// by `entry_id` — which is what makes a replay able to tell new work from work
    /// Jira already has.
    pub fn track_pushed_between(&self, from: i64, to: i64) -> Result<Vec<Pushed>> {
        let mut stmt = self.conn.prepare(
            "SELECT w.entry_id, w.issue_key, w.worklog_id, w.started_at, w.seconds, w.pushed_at \
               FROM track_worklog w JOIN track_entry e ON e.id = w.entry_id \
              WHERE e.started_at < ?2 AND (e.stopped_at IS NULL OR e.stopped_at > ?1 \
                    OR e.started_at >= ?1) \
              ORDER BY w.entry_id",
        )?;
        Ok(stmt
            .query_map(params![from, to], |r| {
                Ok(Pushed {
                    entry_id: r.get(0)?,
                    issue_key: r.get(1)?,
                    worklog_id: r.get(2)?,
                    started_at: r.get(3)?,
                    seconds: r.get(4)?,
                    pushed_at: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Remember that `entry_id` is booked as `worklog_id` on `issue_key`, in the
    /// shape it was pushed in.
    ///
    /// Upsert on the entry, so re-pushing an edited entry replaces the record
    /// instead of failing. Call this *immediately* after Jira answers: the worst
    /// outcome here is a worklog Jira has and this database does not know about,
    /// which is exactly the double-book the record exists to prevent.
    pub fn track_record_push(
        &mut self,
        entry_id: i64,
        issue_key: &str,
        worklog_id: &str,
        started_at: i64,
        seconds: i64,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO track_worklog \
                 (entry_id, issue_key, worklog_id, started_at, seconds, pushed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT (entry_id) DO UPDATE SET \
                 issue_key = ?2, worklog_id = ?3, started_at = ?4, seconds = ?5, pushed_at = ?6",
            params![entry_id, issue_key, worklog_id, started_at, seconds, now],
        )?;
        Ok(())
    }

    /// Forget a push record, for a worklog that is gone from Jira.
    ///
    /// Without this a worklog deleted in Jira's own UI could never be re-pushed:
    /// the record would keep claiming an id that no longer exists.
    pub fn track_forget_push(&mut self, entry_id: i64) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM track_worklog WHERE entry_id = ?1", [entry_id])?
            > 0)
    }

    /// Mark a local day (`YYYY-MM-DD`) as finished, or reopen it with `None`.
    ///
    /// Advisory: nothing here refuses to write to a closed day. It records that the
    /// user was satisfied with that day, so the UI can stop nagging about it and
    /// show at a glance which days are still open — including days closed weeks
    /// later.
    pub fn track_close_day(&mut self, date: &str, closed_at: Option<i64>) -> Result<()> {
        // For the validation, not the bounds: this is the one place that already
        // rejects a well-shaped impossible date like 2026-02-31.
        self.day_bounds(date)?;
        match closed_at {
            Some(at) => self.conn.execute(
                "INSERT INTO track_day (date, closed_at) VALUES (?1, ?2) \
                 ON CONFLICT (date) DO UPDATE SET closed_at = ?2",
                params![date, at],
            )?,
            None => self
                .conn
                .execute("DELETE FROM track_day WHERE date = ?1", [date])?,
        };
        Ok(())
    }

    /// When `date` was closed, if it was.
    pub fn track_day_closed(&self, date: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT closed_at FROM track_day WHERE date = ?1",
                [date],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The UTC offset in seconds that the operating system applies at `now`.
    ///
    /// Jira's `started` field carries an offset (`2026-07-29T09:00:00.000+0200`) and
    /// interprets the timestamp in it, so a worklog pushed with the wrong offset
    /// lands on the wrong hour — and either side of midnight, the wrong day. The
    /// offset is asked of SQLite for the same reason every day boundary is: it is
    /// the one clock the app, the MCP server and the summaries already agree on, and
    /// it is not constant across the year.
    pub fn local_offset(&self, now: i64) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT CAST(strftime('%s', ?1, 'unixepoch', 'localtime') AS INTEGER) - ?1",
            [now],
            |r| r.get(0),
        )?)
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

    /// Local midnight, in Unix seconds, for `days` consecutive local days starting
    /// at `date` (`YYYY-MM-DD`), oldest first.
    ///
    /// One per day, and one query for all of them, because the UTC offset is *not*
    /// constant across a span: on the two days a year DST moves, consecutive
    /// midnights are 23 or 25 hours apart. A caller that takes one offset and adds
    /// 86 400 for the following days draws those days an hour off. SQLite applies
    /// the operating system's own rules here, which is what keeps this agreeing
    /// with the day grouping every summary uses — and why no timezone database is
    /// needed.
    pub fn local_midnights(&self, date: &str, days: i64) -> Result<Vec<i64>> {
        check_shape(date, "YYYY-MM-DD", &[4, 2, 2])?;
        if days <= 0 {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            // The `date(?1) = ?1` guard rejects a normalising date like
            // "2024-02-31" instead of quietly answering about 2 March.
            "WITH RECURSIVE d(day, n) AS ( \
                 SELECT date(?1), 0 WHERE date(?1) = ?1 \
                 UNION ALL \
                 SELECT date(day, '+1 day'), n + 1 FROM d WHERE n + 1 < ?2 \
             ) \
             SELECT CAST(strftime('%s', day || ' 00:00:00', 'utc') AS INTEGER) FROM d",
        )?;
        let midnights: Vec<i64> = stmt
            .query_map(params![date, days], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        if midnights.is_empty() {
            bail!("{date:?} is not a valid date; expected YYYY-MM-DD");
        }
        Ok(midnights)
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

    /// The id is what the UI's selection is keyed on, so an edit that changes it
    /// is the bug this method exists to prevent.
    #[test]
    fn updating_an_entry_keeps_its_id_and_replaces_every_field() {
        let mut store = Store::open_in_memory().unwrap();
        let start = at(&store, "2024-05-14 09:00:00");
        let entry = store
            .track_log("coding", Some("first"), start, start + 600, "app")
            .unwrap();

        let saved = store
            .track_update_entry(
                entry.id,
                " review ",
                Some("  second  "),
                start + 60,
                start + 900,
                "mcp",
            )
            .unwrap()
            .expect("the entry is there");
        assert_eq!(saved.id, entry.id, "the id must survive an edit");
        assert_eq!(saved.task, "review");
        assert_eq!(saved.note.as_deref(), Some("second"));
        assert_eq!(
            (saved.started_at, saved.stopped_at),
            (start + 60, Some(start + 900))
        );
        assert_eq!(saved.source, "mcp");
        // Persisted, and still exactly one row.
        assert_eq!(store.entry(entry.id).unwrap(), Some(saved));
        assert_eq!(store.track_entries_on("2024-05-14").unwrap().len(), 1);
        // A blank note clears it rather than storing "".
        let cleared = store
            .track_update_entry(entry.id, "review", Some("  "), start, start, "app")
            .unwrap()
            .unwrap();
        assert_eq!(cleared.note, None);
    }

    #[test]
    fn updating_refuses_a_reversed_range_and_reports_a_missing_entry() {
        let mut store = Store::open_in_memory().unwrap();
        let entry = store.track_log("coding", None, 100, 200, "app").unwrap();
        let err = store
            .track_update_entry(entry.id, "coding", None, 200, 100, "app")
            .unwrap_err()
            .to_string();
        assert!(err.contains("ends before it starts"), "{err}");
        assert!(
            err.contains("coding"),
            "the message must name the task: {err}"
        );
        assert!(
            store
                .track_update_entry(entry.id, " ", None, 100, 200, "app")
                .is_err(),
            "an empty task name"
        );
        // The failed attempts changed nothing.
        assert_eq!(store.entry(entry.id).unwrap().unwrap(), entry);

        assert_eq!(
            store
                .track_update_entry(4_242, "ghost", None, 100, 200, "app")
                .unwrap(),
            None,
            "no such entry"
        );
        assert_eq!(
            store.track_tasks().unwrap().len(),
            1,
            "editing a missing entry must not create its task"
        );
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

    /// The UI places a whole week or month off these, so each one has to be the
    /// same instant that day's own summary starts at — including the two days a
    /// year that are 23 or 25 hours long, wherever this test happens to run.
    #[test]
    fn local_midnights_are_one_per_day_and_agree_with_each_day_bounds() {
        let store = Store::open_in_memory().unwrap();
        // A spring-forward week, an autumn-back week, and an ordinary one.
        for (first, days) in [("2024-03-25", 7), ("2024-10-21", 7), ("2026-07-27", 7)] {
            let midnights = store.local_midnights(first, days).unwrap();
            assert_eq!(midnights.len(), days as usize);
            let mut date = first.to_string();
            for (i, midnight) in midnights.iter().enumerate() {
                assert_eq!(
                    *midnight,
                    store.day_bounds(&date).unwrap().0,
                    "{date} (day {i} of {first})"
                );
                date = next_day(&store, &date);
            }
            // Strictly increasing, and a day is never more than an hour off 24.
            for pair in midnights.windows(2) {
                let length = pair[1] - pair[0];
                assert!(
                    (82_800..=90_000).contains(&length),
                    "{first}: a day of {length} seconds"
                );
            }
        }
        assert_eq!(store.local_midnights("2026-07-27", 1).unwrap().len(), 1);
        assert!(store.local_midnights("2026-07-27", 0).unwrap().is_empty());
        assert!(
            store.local_midnights("2024-02-31", 3).is_err(),
            "no such day"
        );
        assert!(store.local_midnights("nope", 3).is_err());
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

    #[test]
    fn an_issue_key_must_look_like_one_and_is_uppercased() {
        assert_eq!(clean_issue(None).unwrap(), None);
        assert_eq!(clean_issue(Some("  ")).unwrap(), None);
        assert_eq!(
            clean_issue(Some(" abc-12 ")).unwrap(),
            Some("ABC-12".into())
        );
        assert_eq!(clean_issue(Some("A1_B-7")).unwrap(), Some("A1_B-7".into()));
        for bad in [
            "ABC",
            "-12",
            "ABC-",
            "1BC-12",
            "ABC-1x",
            // The reason this is validated at all: none of these may reach a URL
            // path.
            "ABC-12/../../myself",
            "ABC 12",
            "ABC-12?x=1",
        ] {
            assert!(clean_issue(Some(bad)).is_err(), "{bad:?} must be refused");
        }
    }

    #[test]
    fn linking_a_task_covers_its_past_entries_and_an_entry_may_override_it() {
        let mut store = Store::open_in_memory().unwrap();
        let a = store
            .track_log("review", None, 1_000, 2_000, "app")
            .unwrap();
        let b = store
            .track_log("review", None, 3_000, 4_000, "app")
            .unwrap();
        assert_eq!(a.issue_key, None);

        // Retroactive by design: the ticket is known after the work.
        assert!(
            store
                .track_set_task_issue("review", Some("proj-7"))
                .unwrap()
        );
        let listed = store.track_entries_between(0, 9_000).unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|e| e.issue_key.as_deref())
                .collect::<Vec<_>>(),
            [Some("PROJ-7"), Some("PROJ-7")],
            "both earlier entries follow the task's link"
        );
        assert_eq!(
            store.track_tasks().unwrap()[0].issue_key.as_deref(),
            Some("PROJ-7")
        );

        // One entry of that task belongs elsewhere.
        assert!(store.track_set_entry_issue(b.id, Some("OTHER-1")).unwrap());
        let listed = store.track_entries_between(0, 9_000).unwrap();
        assert_eq!(listed[0].issue_key.as_deref(), Some("PROJ-7"));
        assert_eq!(listed[1].issue_key.as_deref(), Some("OTHER-1"));

        // Clearing the override falls back to the task, it does not clear the task.
        assert!(store.track_set_entry_issue(b.id, None).unwrap());
        assert_eq!(
            store.track_entries_between(0, 9_000).unwrap()[1]
                .issue_key
                .as_deref(),
            Some("PROJ-7")
        );

        // Unlinking the task leaves the entries unlinked too.
        assert!(store.track_set_task_issue("review", None).unwrap());
        assert!(
            store
                .track_entries_between(0, 9_000)
                .unwrap()
                .iter()
                .all(|e| e.issue_key.is_none())
        );
        assert!(!store.track_set_task_issue("nope", Some("X-1")).unwrap());
        assert!(!store.track_set_entry_issue(9_999, Some("X-1")).unwrap());
    }

    #[test]
    fn a_push_record_is_one_per_entry_and_survives_a_replay() {
        let mut store = Store::open_in_memory().unwrap();
        let e = store
            .track_log("coding", None, 1_000, 4_600, "app")
            .unwrap();
        assert!(store.track_pushed_between(0, 9_000).unwrap().is_empty());

        store
            .track_record_push(e.id, "PROJ-7", "10001", 1_000, 3_600, 5_000)
            .unwrap();
        let pushed = store.track_pushed_between(0, 9_000).unwrap();
        assert_eq!(pushed.len(), 1);
        assert_eq!(pushed[0].worklog_id, "10001");
        assert_eq!(pushed[0].seconds, 3_600);

        // The entry grew and was pushed again: the record is replaced, not added
        // to — two records for one entry is the double-book this table prevents.
        store
            .track_record_push(e.id, "PROJ-7", "10001", 1_000, 5_400, 6_000)
            .unwrap();
        let pushed = store.track_pushed_between(0, 9_000).unwrap();
        assert_eq!(pushed.len(), 1);
        assert_eq!(pushed[0].seconds, 5_400);
        assert_eq!(pushed[0].pushed_at, 6_000);

        // Two entries cannot claim one remote worklog.
        let other = store
            .track_log("coding", None, 8_000, 8_600, "app")
            .unwrap();
        assert!(
            store
                .track_record_push(other.id, "PROJ-7", "10001", 8_000, 600, 9_000)
                .is_err()
        );

        assert!(store.track_forget_push(e.id).unwrap());
        assert!(store.track_pushed_between(0, 9_000).unwrap().is_empty());
        assert!(!store.track_forget_push(e.id).unwrap());

        // Deleting the entry takes its record with it.
        store
            .track_record_push(other.id, "PROJ-7", "10001", 8_000, 600, 9_000)
            .unwrap();
        store.track_delete_entry(other.id).unwrap();
        assert!(store.track_pushed_between(0, 9_000).unwrap().is_empty());
    }

    #[test]
    fn a_day_can_be_closed_reopened_and_closed_again_after_the_fact() {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(store.track_day_closed("2026-07-29").unwrap(), None);
        store.track_close_day("2026-07-29", Some(1_000)).unwrap();
        assert_eq!(store.track_day_closed("2026-07-29").unwrap(), Some(1_000));
        // Closing twice is not an error; the later stamp wins.
        store.track_close_day("2026-07-29", Some(2_000)).unwrap();
        assert_eq!(store.track_day_closed("2026-07-29").unwrap(), Some(2_000));
        store.track_close_day("2026-07-29", None).unwrap();
        assert_eq!(store.track_day_closed("2026-07-29").unwrap(), None);
        assert!(store.track_close_day("2026-13-01", Some(1)).is_err());
    }

    #[test]
    fn the_local_offset_matches_the_day_boundary_it_is_pushed_against() {
        let store = Store::open_in_memory().unwrap();
        // Whatever the test machine's zone, midnight plus the offset at that
        // instant must be the local wall clock SQLite reports — that is exactly the
        // arithmetic a Jira `started` timestamp does.
        for date in ["2026-01-15", "2026-07-15"] {
            let midnight = day_start(&store, date);
            let offset = store.local_offset(midnight).unwrap();
            let local: String = store
                .conn
                .query_row(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%S', ?1, 'unixepoch')",
                    [midnight + offset],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(local, format!("{date}T00:00:00"));
        }
    }
}
