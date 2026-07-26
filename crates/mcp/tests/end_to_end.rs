//! End-to-end tool behaviour against a real database.
//!
//! These call the tools the way an MCP client does -- a JSON-RPC line in, a
//! parsed result out -- and then assert on the *text* an agent would read,
//! because that text is the product.
//!
//! They exercise `rp-store`'s `track_*` methods, which are stubs that `bail!` at
//! the time of writing, so everything here fails until the store lands. That is
//! the point: these are the tests that will tell us it did.
use rp_mcp::{Server, tools};
use rp_store::Store;
use serde_json::{Value, json};

fn server() -> Server {
    Server::new(Store::open_in_memory().unwrap()).unwrap()
}

/// Call a tool through the protocol layer and return its text, asserting success.
fn ok(srv: &mut Server, name: &str, args: Value) -> String {
    let result = tools::call(srv, &json!({"name": name, "arguments": args}))
        .unwrap_or_else(|e| panic!("{name}: protocol error {e:?}"));
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert_eq!(result["isError"], false, "{name} failed: {text}");
    text
}

/// Same, but the tool is expected to refuse.
fn err(srv: &mut Server, name: &str, args: Value) -> String {
    let result = tools::call(srv, &json!({"name": name, "arguments": args})).unwrap();
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert_eq!(result["isError"], true, "{name} should have failed: {text}");
    text
}

#[test]
fn start_then_current_then_stop_then_summary() {
    let mut srv = server();

    assert!(ok(&mut srv, "current_timer", json!({})).contains("No timer is running"));

    let started = ok(
        &mut srv,
        "start_timer",
        json!({"task": "mcp server", "note": "stdio loop"}),
    );
    assert!(started.contains("mcp server"), "{started}");
    assert!(
        started.contains("stdio loop"),
        "the note should be echoed: {started}"
    );

    let current = ok(&mut srv, "current_timer", json!({}));
    assert!(current.contains("Running"), "{current}");
    assert!(current.contains("mcp server"), "{current}");

    // Starting a second task reports what it interrupted.
    let switched = ok(&mut srv, "start_timer", json!({"task": "code review"}));
    assert!(switched.contains("code review"), "{switched}");
    assert!(switched.contains("Stopped \"mcp server\""), "{switched}");

    let stopped = ok(&mut srv, "stop_timer", json!({}));
    assert!(stopped.contains("code review"), "{stopped}");

    // Stopping twice is not an error; an agent may do it.
    assert!(ok(&mut srv, "stop_timer", json!({})).contains("No timer was running"));

    // Both tasks exist and today's summary mentions them.
    let tasks = ok(&mut srv, "list_tasks", json!({}));
    assert!(
        tasks.contains("mcp server") && tasks.contains("code review"),
        "{tasks}"
    );

    let summary = ok(&mut srv, "summary", json!({}));
    assert!(summary.contains("(today)"), "{summary}");
    assert!(
        summary.contains("mcp server") && summary.contains("code review"),
        "{summary}"
    );
    assert!(summary.contains("total "), "{summary}");
}

#[test]
fn log_time_round_trips_through_list_entries_and_summary() {
    let mut srv = server();
    let today = srv.clock.today().unwrap();

    let booked = ok(
        &mut srv,
        "log_time",
        json!({
            "task": "invoice bug",
            "started_at": format!("{today}T09:00:00"),
            "stopped_at": format!("{today}T10:30:00"),
            "note": "traced the rounding",
        }),
    );
    assert!(booked.contains("1h 30m"), "{booked}");
    assert!(
        booked.contains("09:00-10:30"),
        "local wall clock must survive: {booked}"
    );

    // A second block, given as Unix seconds, on the same task.
    let start = srv
        .clock
        .parse_ts("started_at", Some(&json!(format!("{today}T13:00:00"))))
        .unwrap();
    ok(
        &mut srv,
        "log_time",
        json!({
            "task": "invoice bug",
            "started_at": start,
            "stopped_at": start + 1800,
        }),
    );

    let listed = ok(&mut srv, "list_entries", json!({"date": "today"}));
    assert!(listed.contains("2 entries"), "{listed}");
    assert!(
        ok(&mut srv, "list_entries", json!({"search": "rounding"})).contains("1 entry,"),
        "one match must not read as \"1 entries\""
    );
    assert!(listed.contains("2h 00m"), "the listing total: {listed}");
    assert!(
        listed.contains("traced the rounding"),
        "notes must be visible: {listed}"
    );

    let summary = ok(&mut srv, "summary", json!({"date": today.clone()}));
    assert!(
        summary.contains("2h 00m"),
        "two blocks on one task must total up: {summary}"
    );

    // Searchable by text, and by month.
    let found = ok(&mut srv, "list_entries", json!({"search": "invoice"}));
    assert!(found.contains("invoice bug"), "{found}");
    let month = ok(&mut srv, "summary_month", json!({"month": &today[..7]}));
    assert!(
        month.contains(&today),
        "the day must appear in the month view: {month}"
    );
    assert!(month.contains("2h 00m"), "{month}");

    // Delete the first entry by the id the listing showed.
    let id: i64 = listed
        .split('#')
        .nth(1)
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("no entry id in:\n{listed}"));
    assert!(ok(&mut srv, "delete_entry", json!({"id": id})).contains(&format!("#{id}")));
    assert!(err(&mut srv, "delete_entry", json!({"id": id})).contains("no entry"));
}

#[test]
fn yesterdays_work_lands_on_yesterday_not_today() {
    let mut srv = server();
    let yesterday = srv.clock.resolve_date(Some("yesterday")).unwrap();
    ok(
        &mut srv,
        "log_time",
        json!({
            "task": "planning",
            "started_at": format!("{yesterday}T16:00:00"),
            "stopped_at": format!("{yesterday}T17:00:00"),
        }),
    );

    assert!(ok(&mut srv, "summary", json!({"date": "today"})).contains("no time booked"));
    let y = ok(&mut srv, "summary", json!({"date": "yesterday"}));
    assert!(y.contains("1h 00m") && y.contains("planning"), "{y}");
    assert!(!y.contains("(today)"), "{y}");
}

#[test]
fn an_empty_day_and_an_empty_month_read_as_english_not_as_an_error() {
    let mut srv = server();
    assert!(ok(&mut srv, "summary", json!({"date": "2020-01-01"})).contains("no time booked"));
    assert!(ok(&mut srv, "summary_month", json!({"month": "2020-01"})).contains("no time booked"));
    assert!(ok(&mut srv, "list_entries", json!({"date": "2020-01-01"})).contains("none"));
    assert!(ok(&mut srv, "list_tasks", json!({})).contains("No tasks yet"));
}

#[test]
fn a_range_query_finds_a_block_inside_it() {
    let mut srv = server();
    let today = srv.clock.today().unwrap();
    ok(
        &mut srv,
        "log_time",
        json!({
            "task": "ranged",
            "started_at": format!("{today}T09:00:00"),
            "stopped_at": format!("{today}T09:30:00"),
        }),
    );
    let listed = ok(
        &mut srv,
        "list_entries",
        json!({
            "from": format!("{today}T08:00:00"),
            "to": format!("{today}T12:00:00"),
        }),
    );
    assert!(listed.contains("ranged"), "{listed}");
    // `from` alone means "until now".
    assert!(
        ok(
            &mut srv,
            "list_entries",
            json!({"from": format!("{today}T00:00:00")})
        )
        .contains("ranged")
    );
}
