//! The tools an agent sees, and the text it gets back.
//!
//! The descriptions are the user interface: an agent reads them once and then
//! books time unsupervised, so they say what each tool does to the *state*
//! (start_timer stops the previous timer, log_time does not) rather than
//! restating the name.
//!
//! Every failure below is an `anyhow::Error` that becomes a tool result with
//! `isError: true` -- an agent can read that and retry with better arguments,
//! whereas a JSON-RPC error is a dead end for it.
use crate::proto::{INVALID_PARAMS, Server};
use crate::time::{self, fmt_hm};
use anyhow::{Context, Result, bail};
use rp_store::{DaySummary, Entry};
use serde_json::{Value, json};

/// The `source` column, so the desktop app can show where time came from.
const SOURCE: &str = "mcp";
const DEFAULT_LIMIT: usize = 200;
const MAX_LIMIT: usize = 1000;

/// A whole day is the longest single block worth believing; more is a unit or
/// timezone mistake, and refusing beats a timesheet with a 3000-hour entry.
const MAX_BLOCK_SECONDS: i64 = 24 * 3600;

pub fn definitions() -> Value {
    json!([
        {
            "name": "start_timer",
            "title": "Start a timer",
            "description": "Start tracking time on a task, from right now. \
    Any timer that is already running is stopped first, so you never need to call stop_timer \
    beforehand -- switching tasks is a single start_timer call. The task is created on first \
    use; reuse the exact same task name for related work so it totals up in the summary. Call \
    this when you begin a piece of work for the user, and stop_timer when you finish.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "Short, reusable name for what is being worked on, e.g. \"mcp server\" or \"invoice bug\". Not a sentence."
                    },
                    "note": {
                        "type": "string",
                        "description": "Optional detail about this particular session, e.g. \"pairing on the stdio loop\". Shows up in summaries and search."
                    }
                },
                "required": ["task"],
                "additionalProperties": false
            }
        },
        {
            "name": "stop_timer",
            "title": "Stop the running timer",
            "description": "Stop whatever timer is currently running and report the task \
    and how long it ran. Safe to call when nothing is running: it says so instead of failing. \
    Call it when work on a task ends -- an unstopped timer keeps accumulating and will \
    overstate the user's day.",
            "inputSchema": { "type": "object", "additionalProperties": false }
        },
        {
            "name": "log_time",
            "title": "Book a finished block of time",
            "description": "Record work that is already finished, with explicit start and \
    end times. Use this for time that was not tracked live -- work from earlier today, or from \
    a previous session. Does not start or stop any running timer. Refuses timestamps it cannot \
    read, times in the future, and blocks longer than 24 hours, rather than booking a wrong day.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "Task name; created if new." },
                    "started_at": { "type": "string", "description": "When the work began. Accepts ISO 8601 or Unix seconds (as a number or a string). Without a timezone it is read as the user's local time." },
                    "stopped_at": { "type": "string", "description": "When the work ended. Same formats as started_at. Must be at or after started_at and not in the future." },
                    "note": { "type": "string", "description": "Optional detail about what was done." }
                },
                "required": ["task", "started_at", "stopped_at"],
                "additionalProperties": false
            }
        },
        {
            "name": "current_timer",
            "title": "What is running",
            "description": "Report which task is being tracked right now, since when, and \
    for how long so far. Returns \"no timer is running\" when nothing is. Cheap -- check this \
    before starting a timer if you are unsure whether the user is already tracking something.",
            "inputSchema": { "type": "object", "additionalProperties": false }
        },
        {
            "name": "list_entries",
            "title": "List time entries",
            "description": "List individual booked entries with their ids, times, durations \
    and notes. Give exactly one selector: `date` for a single day, `from`/`to` for a range, or \
    `search` for free text across task names and notes. With no selector it lists today. Use \
    this to find the id of an entry you need to delete, or to check what was already booked \
    before booking more.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "date": { "type": "string", "description": "One day: \"today\", \"yesterday\", or YYYY-MM-DD." },
                    "from": { "type": "string", "description": "Start of a range, ISO 8601 or Unix seconds. Defaults the end to now if `to` is omitted." },
                    "to": { "type": "string", "description": "End of a range, ISO 8601 or Unix seconds." },
                    "search": { "type": "string", "description": "Free text matched against task names and notes, newest first." },
                    "limit": { "type": "integer", "description": "Maximum entries to return; default 200, capped at 1000." }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "summary",
            "title": "Summarise one day",
            "description": "The end-of-day report: total time for a day plus a breakdown \
    per task, formatted for a human to read. This is what the user asks for in the evening when \
    they want to know what they worked on. Defaults to today.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "date": { "type": "string", "description": "\"today\" (default), \"yesterday\", or YYYY-MM-DD." }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "summary_month",
            "title": "Summarise one month",
            "description": "Totals for a whole month: one line per day that has time \
    booked, then the month's per-task totals and grand total. Use it for a monthly review or to \
    answer \"how much did I spend on X this month\". Defaults to the current month.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "month": { "type": "string", "description": "\"this month\" (default), \"last month\", or YYYY-MM." }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "list_tasks",
            "title": "List known tasks",
            "description": "List every task name that has ever been used, oldest first. \
    Check here before inventing a new task name, so related work lands on one task instead of \
    three near-duplicates.",
            "inputSchema": { "type": "object", "additionalProperties": false }
        },
        {
            "name": "delete_entry",
            "title": "Delete a time entry",
            "description": "Permanently delete one booked entry by its id, for fixing a \
    mistaken booking. Get the id from list_entries first. Deletes exactly one entry and cannot \
    be undone; to correct a wrong time, delete and then log_time again.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Entry id as shown by list_entries (the number after #)." }
                },
                "required": ["id"],
                "additionalProperties": false
            }
        }
    ])
}

/// Handle a `tools/call`. Only a missing or unknown tool name is a protocol
/// error; anything else is a readable `isError` result the agent can act on.
pub fn call(srv: &mut Server, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            (
                INVALID_PARAMS,
                "tools/call needs a string \"name\"".to_string(),
            )
        })?
        .to_string();

    // An omitted `arguments` is normal for the no-argument tools.
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !KNOWN.contains(&name.as_str()) {
        return Err((INVALID_PARAMS, format!("Unknown tool: {name}")));
    }
    Ok(match run(srv, &name, &args) {
        Ok(text) => text_result(&text, false),
        // `{:#}` prints the whole anyhow chain, which is where the "here is the
        // format I wanted" detail lives.
        Err(e) => text_result(&format!("{e:#}"), true),
    })
}

const KNOWN: &[&str] = &[
    "start_timer",
    "stop_timer",
    "log_time",
    "current_timer",
    "list_entries",
    "summary",
    "summary_month",
    "list_tasks",
    "delete_entry",
];

fn text_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn run(srv: &mut Server, name: &str, args: &Value) -> Result<String> {
    if !args.is_object() && !args.is_null() {
        bail!("\"arguments\" must be a JSON object of named arguments");
    }
    match name {
        "start_timer" => start_timer(srv, args),
        "stop_timer" => stop_timer(srv),
        "log_time" => log_time(srv, args),
        "current_timer" => current_timer(srv),
        "list_entries" => list_entries(srv, args),
        "summary" => summary(srv, args),
        "summary_month" => summary_month(srv, args),
        "list_tasks" => list_tasks(srv),
        "delete_entry" => delete_entry(srv, args),
        // `call` filtered these out already.
        other => bail!("Unknown tool: {other}"),
    }
}

// -- argument access ---------------------------------------------------------

fn opt_str<'a>(args: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => {
            let s = s.trim();
            Ok(if s.is_empty() { None } else { Some(s) })
        }
        Some(other) => bail!("{key} must be a string, not {}", type_name(other)),
    }
}

fn req_str<'a>(args: &'a Value, key: &str) -> Result<&'a str> {
    opt_str(args, key)?.with_context(|| format!("{key} is required and must be a non-empty string"))
}

fn req_i64(args: &Value, key: &str) -> Result<i64> {
    match args.get(key) {
        Some(Value::Number(n)) => n
            .as_i64()
            .with_context(|| format!("{key} must be a whole number, not {n}")),
        // Agents stringify numbers often enough that refusing would be pedantry.
        Some(Value::String(s)) => s
            .trim()
            .parse()
            .with_context(|| format!("{key} must be a whole number, not {s:?}")),
        None | Some(Value::Null) => bail!("{key} is required"),
        Some(other) => bail!("{key} must be a whole number, not {}", type_name(other)),
    }
}

fn opt_usize(args: &Value, key: &str, default: usize, max: usize) -> Result<usize> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        _ => {
            let n = req_i64(args, key)?;
            if n <= 0 {
                bail!("{key} must be greater than zero");
            }
            Ok((n as usize).min(max))
        }
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

// -- tools -------------------------------------------------------------------

fn start_timer(srv: &mut Server, args: &Value) -> Result<String> {
    let task = req_str(args, "task")?;
    let note = opt_str(args, "note")?;
    let now = time::now();
    // Read the outgoing timer before starting, purely so the reply can say what
    // was interrupted -- track_start stops it either way.
    let previous = srv.store.track_running().unwrap_or(None);
    let entry = srv
        .store
        .track_start(task, note, SOURCE, now)
        .context("could not start the timer")?;

    let mut out = format!(
        "Started \"{}\" at {}.",
        entry.task,
        srv.clock.hm(entry.started_at)
    );
    if let Some(prev) = previous {
        out.push_str(&format!(
            "\nStopped \"{}\" first, after {}.",
            prev.task,
            fmt_hm(prev.seconds(now))
        ));
    }
    if let Some(note) = &entry.note {
        out.push_str(&format!("\nNote: {note}"));
    }
    Ok(out)
}

fn stop_timer(srv: &mut Server) -> Result<String> {
    let now = time::now();
    match srv
        .store
        .track_stop(now)
        .context("could not stop the timer")?
    {
        Some(e) => Ok(format!(
            "Stopped \"{}\" after {} ({}-{}).",
            e.task,
            fmt_hm(e.seconds(now)),
            srv.clock.hm(e.started_at),
            srv.clock.hm(e.stopped_at.unwrap_or(now)),
        )),
        None => Ok("No timer was running, so nothing was stopped.".to_string()),
    }
}

fn current_timer(srv: &mut Server) -> Result<String> {
    let now = time::now();
    match srv
        .store
        .track_running()
        .context("could not read the running timer")?
    {
        Some(e) => {
            let mut out = format!(
                "Running: \"{}\" since {} ({} so far).",
                e.task,
                srv.clock.fmt_local(e.started_at, "%Y-%m-%d %H:%M"),
                fmt_hm(e.seconds(now)),
            );
            if let Some(note) = &e.note {
                out.push_str(&format!("\nNote: {note}"));
            }
            Ok(out)
        }
        None => Ok("No timer is running.".to_string()),
    }
}

fn log_time(srv: &mut Server, args: &Value) -> Result<String> {
    let task = req_str(args, "task")?;
    let note = opt_str(args, "note")?;
    let started_at = srv.clock.parse_ts("started_at", args.get("started_at"))?;
    let stopped_at = srv.clock.parse_ts("stopped_at", args.get("stopped_at"))?;

    if stopped_at < started_at {
        bail!(
            "stopped_at ({}) is before started_at ({}). Swap them or fix the date.",
            srv.clock.fmt_local(stopped_at, "%Y-%m-%d %H:%M"),
            srv.clock.fmt_local(started_at, "%Y-%m-%d %H:%M"),
        );
    }
    let now = time::now();
    // A minute of slack absorbs clock skew between the agent and this machine.
    if stopped_at > now + 60 {
        bail!(
            "stopped_at ({}) is in the future; log_time is for work that is already \
             finished. Use start_timer for work in progress.",
            srv.clock.fmt_local(stopped_at, "%Y-%m-%d %H:%M"),
        );
    }
    if stopped_at - started_at > MAX_BLOCK_SECONDS {
        bail!(
            "that block is {} long, which is more than a day -- check the dates \
             ({} to {}). Book long stretches as one entry per day.",
            fmt_hm(stopped_at - started_at),
            srv.clock.fmt_local(started_at, "%Y-%m-%d %H:%M"),
            srv.clock.fmt_local(stopped_at, "%Y-%m-%d %H:%M"),
        );
    }

    let e = srv
        .store
        .track_log(task, note, started_at, stopped_at, SOURCE)
        .context("could not book that block")?;
    Ok(format!(
        "Booked {} on \"{}\" ({} {}-{}), entry #{}.",
        fmt_hm(e.seconds(now)),
        e.task,
        srv.clock.fmt_local(e.started_at, "%Y-%m-%d"),
        srv.clock.hm(e.started_at),
        srv.clock.hm(e.stopped_at.unwrap_or(now)),
        e.id,
    ))
}

fn list_entries(srv: &mut Server, args: &Value) -> Result<String> {
    let limit = opt_usize(args, "limit", DEFAULT_LIMIT, MAX_LIMIT)?;
    let date = opt_str(args, "date")?;
    let search = opt_str(args, "search")?;
    let has_range = args.get("from").is_some_and(|v| !v.is_null())
        || args.get("to").is_some_and(|v| !v.is_null());

    // Guessing which selector was meant would mean answering a question the agent
    // did not ask.
    let selectors = [date.is_some(), search.is_some(), has_range]
        .iter()
        .filter(|b| **b)
        .count();
    if selectors > 1 {
        bail!("give only one of `date`, `from`/`to`, or `search` -- not several at once");
    }

    let (heading, entries) = if let Some(needle) = search {
        (
            format!("Entries matching {needle:?}"),
            srv.store
                .track_search(needle, limit)
                .context("search failed")?,
        )
    } else if has_range {
        let from = srv.clock.parse_ts("from", args.get("from"))?;
        // An open-ended range means "up to now", which is what an agent asking
        // "what have I done since 9am" wants.
        let to = match args.get("to") {
            Some(v) if !v.is_null() => srv.clock.parse_ts("to", Some(v))?,
            _ => time::now(),
        };
        if to < from {
            bail!("`to` is before `from`");
        }
        (
            format!(
                "Entries from {} to {}",
                srv.clock.fmt_local(from, "%Y-%m-%d %H:%M"),
                srv.clock.fmt_local(to, "%Y-%m-%d %H:%M")
            ),
            srv.store
                .track_entries_between(from, to)
                .context("range query failed")?,
        )
    } else {
        let day = srv.clock.resolve_date(date)?;
        (
            format!("Entries on {day}"),
            srv.store
                .track_entries_on(&day)
                .context("day query failed")?,
        )
    };

    if entries.is_empty() {
        return Ok(format!("{heading}: none."));
    }
    let now = time::now();
    let total: i64 = entries.iter().map(|e| e.seconds(now)).sum();
    let mut out = format!(
        "{heading} ({} {}, {}):\n",
        entries.len(),
        if entries.len() == 1 {
            "entry"
        } else {
            "entries"
        },
        fmt_hm(total)
    );
    for e in entries.iter().take(limit) {
        out.push('\n');
        out.push_str(&entry_line(srv, e, now));
    }
    if entries.len() > limit {
        out.push_str(&format!(
            "\n\n({} more not shown; raise `limit`.)",
            entries.len() - limit
        ));
    }
    Ok(out)
}

fn entry_line(srv: &Server, e: &Entry, now: i64) -> String {
    let end = match e.stopped_at {
        Some(t) => srv.clock.hm(t),
        None => "running".to_string(),
    };
    let mut line = format!(
        "#{} {} {}-{} {:>8}  {}",
        e.id,
        srv.clock.fmt_local(e.started_at, "%Y-%m-%d"),
        srv.clock.hm(e.started_at),
        end,
        fmt_hm(e.seconds(now)),
        e.task,
    );
    if e.source != SOURCE {
        line.push_str(&format!(" [{}]", e.source));
    }
    if let Some(note) = &e.note {
        line.push_str(&format!("\n    {note}"));
    }
    line
}

fn summary(srv: &mut Server, args: &Value) -> Result<String> {
    let day = srv.clock.resolve_date(opt_str(args, "date")?)?;
    let now = time::now();
    let s = srv
        .store
        .track_summary_day(&day, now)
        .context("could not build the summary")?;
    let today = srv.clock.today().unwrap_or_default();
    let label = if s.date == today {
        format!("{} (today)", s.date)
    } else {
        s.date.clone()
    };

    // Emptiness is "no entries", not "zero seconds": a task started and stopped
    // within the same minute still happened, and hiding it would look like a bug.
    if s.per_task.is_empty() {
        return Ok(format!("{label}: no time booked."));
    }
    let mut out = format!("{label} -- total {}\n", fmt_hm(s.total_seconds));
    out.push_str(&per_task_block(&s.per_task));
    Ok(out)
}

fn summary_month(srv: &mut Server, args: &Value) -> Result<String> {
    let month = srv.clock.resolve_month(opt_str(args, "month")?)?;
    let now = time::now();
    let days = srv
        .store
        .track_summary_month(&month, now)
        .context("could not build the summary")?;
    if days.is_empty() {
        return Ok(format!("{month}: no time booked."));
    }

    let total: i64 = days.iter().map(|d| d.total_seconds).sum();
    let mut out = format!(
        "{month} -- total {} over {} day{}\n\nBy day:\n",
        fmt_hm(total),
        days.len(),
        if days.len() == 1 { "" } else { "s" }
    );
    for d in &days {
        out.push_str(&format!(
            "  {}  {:>8}  {}\n",
            d.date,
            fmt_hm(d.total_seconds),
            top_tasks(d)
        ));
    }
    out.push_str("\nBy task:\n");
    out.push_str(&per_task_block(&roll_up(&days)));
    Ok(out)
}

/// Right-aligned durations, longest task first.
fn per_task_block(per_task: &[(String, i64)]) -> String {
    per_task
        .iter()
        .map(|(task, secs)| format!("  {:>8}  {task}\n", fmt_hm(*secs)))
        .collect()
}

/// The day's tasks on one line, for the month view.
fn top_tasks(d: &DaySummary) -> String {
    let mut names: Vec<&str> = d.per_task.iter().take(3).map(|(t, _)| t.as_str()).collect();
    if d.per_task.len() > 3 {
        names.push("...");
    }
    names.join(", ")
}

/// Month totals per task, descending. The store gives per-day breakdowns; summing
/// them here keeps the query layer from needing a second shape.
fn roll_up(days: &[DaySummary]) -> Vec<(String, i64)> {
    let mut totals: Vec<(String, i64)> = Vec::new();
    for day in days {
        for (task, secs) in &day.per_task {
            match totals.iter_mut().find(|(t, _)| t == task) {
                Some((_, acc)) => *acc += secs,
                // ponytail: linear scan. A month has a handful of task names; a
                // HashMap here would only cost the deterministic ordering below.
                None => totals.push((task.clone(), *secs)),
            }
        }
    }
    totals.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    totals
}

fn list_tasks(srv: &mut Server) -> Result<String> {
    let tasks = srv.store.track_tasks().context("could not list tasks")?;
    if tasks.is_empty() {
        return Ok("No tasks yet. start_timer or log_time creates one.".to_string());
    }
    let mut out = format!(
        "{} task{}:\n",
        tasks.len(),
        if tasks.len() == 1 { "" } else { "s" }
    );
    for t in &tasks {
        out.push_str(&format!(
            "  {} (first used {})\n",
            t.name,
            srv.clock.fmt_local(t.created_at, "%Y-%m-%d")
        ));
    }
    Ok(out)
}

fn delete_entry(srv: &mut Server, args: &Value) -> Result<String> {
    let id = req_i64(args, "id")?;
    if srv
        .store
        .track_delete_entry(id)
        .context("could not delete that entry")?
    {
        Ok(format!("Deleted entry #{id}."))
    } else {
        // An error, not a bland "nothing happened": the agent had the wrong id and
        // should look it up rather than assume the fix landed.
        bail!("There is no entry #{id}. Use list_entries to find the right id.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_declared_tool_is_reachable_and_every_handler_is_declared() {
        let defs = definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, KNOWN, "definitions() and KNOWN must not drift apart");
    }

    #[test]
    fn string_arguments_are_trimmed_and_empty_means_absent() {
        let args = json!({"task": "  x  ", "note": "   ", "n": 4});
        assert_eq!(req_str(&args, "task").unwrap(), "x");
        assert_eq!(
            opt_str(&args, "note").unwrap(),
            None,
            "whitespace is not a note"
        );
        assert_eq!(opt_str(&args, "missing").unwrap(), None);
        assert!(
            req_str(&args, "note")
                .unwrap_err()
                .to_string()
                .contains("note is required")
        );
        assert!(
            opt_str(&args, "n")
                .unwrap_err()
                .to_string()
                .contains("must be a string")
        );
    }

    #[test]
    fn integers_are_accepted_as_numbers_or_strings_but_not_as_nonsense() {
        assert_eq!(req_i64(&json!({"id": 7}), "id").unwrap(), 7);
        assert_eq!(req_i64(&json!({"id": "7"}), "id").unwrap(), 7);
        assert!(req_i64(&json!({"id": 1.5}), "id").is_err());
        assert!(req_i64(&json!({"id": "seven"}), "id").is_err());
        assert!(
            req_i64(&json!({}), "id")
                .unwrap_err()
                .to_string()
                .contains("id is required")
        );
    }

    #[test]
    fn a_limit_is_clamped_and_a_silly_one_is_refused() {
        assert_eq!(opt_usize(&json!({}), "limit", 200, 1000).unwrap(), 200);
        assert_eq!(
            opt_usize(&json!({"limit": 5}), "limit", 200, 1000).unwrap(),
            5
        );
        assert_eq!(
            opt_usize(&json!({"limit": 99999}), "limit", 200, 1000).unwrap(),
            1000
        );
        assert!(opt_usize(&json!({"limit": 0}), "limit", 200, 1000).is_err());
        assert!(opt_usize(&json!({"limit": -3}), "limit", 200, 1000).is_err());
    }

    #[test]
    fn a_month_rolls_task_totals_up_across_its_days() {
        let days = vec![
            DaySummary {
                date: "2026-07-01".into(),
                total_seconds: 5400,
                per_task: vec![("a".into(), 3600), ("b".into(), 1800)],
            },
            DaySummary {
                date: "2026-07-02".into(),
                total_seconds: 7200,
                per_task: vec![("b".into(), 7200)],
            },
        ];
        assert_eq!(
            roll_up(&days),
            vec![("b".to_string(), 9000), ("a".to_string(), 3600)]
        );
    }

    #[test]
    fn the_per_task_block_right_aligns_durations() {
        let block = per_task_block(&[("long task".into(), 36_000), ("short".into(), 60)]);
        assert_eq!(block, "   10h 00m  long task\n    0h 01m  short\n");
    }

    #[test]
    fn a_busy_day_lists_only_the_first_few_tasks_in_the_month_view() {
        let d = DaySummary {
            date: "2026-07-01".into(),
            total_seconds: 4,
            per_task: vec![
                ("a".into(), 1),
                ("b".into(), 1),
                ("c".into(), 1),
                ("d".into(), 1),
            ],
        };
        assert_eq!(top_tasks(&d), "a, b, c, ...");
    }

    #[test]
    fn list_entries_refuses_two_selectors_at_once() {
        let mut srv = Server::new(rp_store::Store::open_in_memory().unwrap()).unwrap();
        let err = list_entries(&mut srv, &json!({"date": "today", "search": "x"}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("only one of"), "{err}");
        let err = list_entries(
            &mut srv,
            &json!({"from": 1_785_049_200i64, "date": "today"}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("only one of"), "{err}");
    }

    #[test]
    fn log_time_refuses_reversed_future_and_over_long_blocks() {
        let mut srv = Server::new(rp_store::Store::open_in_memory().unwrap()).unwrap();
        let now = time::now();

        let err = log_time(
            &mut srv,
            &json!({"task": "x", "started_at": now - 60, "stopped_at": now - 600}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("before started_at"), "{err}");

        let err = log_time(
            &mut srv,
            &json!({"task": "x", "started_at": now, "stopped_at": now + 7200}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("future"), "{err}");

        let err = log_time(
            &mut srv,
            &json!({"task": "x", "started_at": now - 5 * 86_400, "stopped_at": now}),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("more than a day"), "{err}");
    }
}
