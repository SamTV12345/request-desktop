//! SQLite-backed persistence.
//!
//! Its own crate rather than a module in the app, because the dependency
//! direction has to stay `app -> store` and `core -> store`; a module inside
//! the GUI crate could never be reached from `rp-core`.
//!
//! Two things matter more than the current feature set:
//!
//! 1. **Migrations from the first release.** The schema is versioned via
//!    SQLite's own `user_version` and upgraded by [`MIGRATIONS`], so the next
//!    feature adds a step instead of breaking existing databases. A database
//!    written by a *newer* build is refused rather than silently mangled.
//! 2. **Persistence is never load-bearing.** Every call can fail, and the
//!    caller is expected to carry on without it. A corrupt database must cost
//!    the user their restored tabs, never their ability to start the app.
pub mod track;
pub use track::{DaySummary, Entry, Pushed, Task};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

/// One statement batch per schema version, applied in order. Append only —
/// never edit a shipped entry, or existing databases diverge from new ones.
const MIGRATIONS: &[&str] = &[
    // v1: window/tab session plus a general key-value bucket.
    r#"
    CREATE TABLE session_window (
        id          INTEGER PRIMARY KEY,
        position    INTEGER NOT NULL,
        active_tab  INTEGER
    );
    CREATE TABLE session_tab (
        window_id   INTEGER NOT NULL REFERENCES session_window(id) ON DELETE CASCADE,
        position    INTEGER NOT NULL,
        kind        TEXT    NOT NULL,
        key         TEXT    NOT NULL,
        detail      TEXT,
        PRIMARY KEY (window_id, position)
    );
    CREATE TABLE kv (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
    "#,
    // v2: local time tracking. Written by the desktop app AND by the MCP
    // server, i.e. two processes on one file, which is why the "only one timer
    // may run" rule is a database constraint rather than application code.
    r#"
    CREATE TABLE track_task (
        id         INTEGER PRIMARY KEY,
        name       TEXT    NOT NULL UNIQUE,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE track_entry (
        id         INTEGER PRIMARY KEY,
        task_id    INTEGER NOT NULL REFERENCES track_task(id) ON DELETE CASCADE,
        note       TEXT,
        -- Unix seconds, UTC. Local-day grouping happens in SQL via the
        -- 'localtime' modifier, so no timezone database is needed.
        started_at INTEGER NOT NULL,
        stopped_at INTEGER,
        -- 'app' or 'mcp', so a summary can say where the time came from.
        source     TEXT    NOT NULL,
        CHECK (stopped_at IS NULL OR stopped_at >= started_at)
    );
    CREATE INDEX track_entry_started ON track_entry (started_at);
    CREATE INDEX track_entry_task ON track_entry (task_id);
    -- At most one running entry, enforced in the schema. A plain UNIQUE on
    -- `stopped_at` would not do it: SQLite treats NULLs as distinct. Indexing
    -- the constant expression over just the running rows does.
    CREATE UNIQUE INDEX track_entry_single_running
        ON track_entry ((1)) WHERE stopped_at IS NULL;
    "#,
    // v3: a pinned tab. Separate from `detail`, which already carries a
    // request's index — overloading it would make "pinned" unrepresentable for
    // request tabs, which is exactly the case pinning is most useful for.
    r#"
    ALTER TABLE session_tab ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
    "#,
    // v4: which tab is active, per tab rather than per window.
    //
    // The tab strip used to be shared by every section, so one index per window
    // was enough. Requests and Tools now keep separate strips — switching
    // section switches the strip — so each has its own active tab and a single
    // `session_window.active_tab` can no longer express both.
    r#"
    ALTER TABLE session_tab ADD COLUMN active INTEGER NOT NULL DEFAULT 0;
    "#,
    // v5: the request files a window has open.
    //
    // Which files are open used to be inferred from the tabs, since a request tab
    // records its own path. That loses a file with no tab open in it — and adding
    // four files, opening one and restarting is exactly how someone uses this.
    r#"
    CREATE TABLE session_source (
        window_id INTEGER NOT NULL REFERENCES session_window(id) ON DELETE CASCADE,
        position  INTEGER NOT NULL,
        path      TEXT    NOT NULL,
        PRIMARY KEY (window_id, position)
    );
    "#,
    // v6: the Jira side of the tracker — which issue a block of work belongs to,
    // and what was already booked there.
    //
    // The issue key sits on both the task and the entry because both readings are
    // real: a task like "auth refactor" is always the same ticket, a task like
    // "meeting" is a different one every day. The entry wins where it is set; see
    // `SELECT_ENTRY`.
    //
    // `track_worklog` is the whole idempotency story. Jira has no client-supplied
    // key on `POST .../worklog`, so nothing at the API level stops the same block
    // of work from being booked twice — replaying a day *would* double-book. What
    // makes a replay safe is remembering the worklog id Jira handed back, per local
    // entry: a second run finds the row and updates that worklog instead of adding
    // another. `started_at`/`seconds` are the values as pushed, so a run can tell
    // "unchanged, skip" from "edited, update" without another round trip.
    r#"
    ALTER TABLE track_task  ADD COLUMN issue_key TEXT;
    ALTER TABLE track_entry ADD COLUMN issue_key TEXT;
    CREATE TABLE track_worklog (
        entry_id   INTEGER PRIMARY KEY REFERENCES track_entry(id) ON DELETE CASCADE,
        issue_key  TEXT    NOT NULL,
        -- Jira's id, as a string: it is an opaque identifier, not a number to do
        -- arithmetic on, and Data Center and Cloud do not agree on its width.
        worklog_id TEXT    NOT NULL,
        started_at INTEGER NOT NULL,
        seconds    INTEGER NOT NULL,
        pushed_at  INTEGER NOT NULL,
        -- One local entry per remote worklog and back. Without this, two entries
        -- could claim the same worklog and each would "update" the other's time.
        UNIQUE (issue_key, worklog_id)
    );
    -- A day the user has declared finished. Advisory, not a lock: reopening is
    -- just booking more time, and a replay then pushes the difference.
    CREATE TABLE track_day (
        date      TEXT    PRIMARY KEY,
        closed_at INTEGER NOT NULL
    );
    "#,
];

/// What a restored tab needs in order to be rebuilt.
///
/// Deliberately stringly-typed: the store must not depend on the UI's enums, or
/// every tab-model change would become a schema change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabRecord {
    /// `"tool"` or `"request"` today; the store does not interpret it.
    pub kind: String,
    /// Tool id, or the path of the file a request came from.
    pub key: String,
    /// Anything the caller needs to disambiguate, e.g. a request's index.
    pub detail: Option<String>,
    /// A pinned tab resists "close others" and sorts ahead of the rest.
    pub pinned: bool,
    /// The active tab *within its own kind*. Each section has its own strip, so
    /// a window has one active tab per kind rather than one overall — which is
    /// why this is here and not a single index on [`WindowSession`].
    pub active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowSession {
    /// Every request file the window has open, in order. Independent of `tabs`: a
    /// file with nothing open in it is still open.
    pub sources: Vec<String>,
    pub tabs: Vec<TabRecord>,
    /// Kept for databases written before v4, where one index per window was the
    /// whole story. New writers set [`TabRecord::active`] per kind instead; this
    /// is still read so an older session restores something sensible.
    pub active_tab: Option<usize>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open the database in the user's data directory, creating it if needed.
    pub fn open_default() -> Result<Self> {
        let path = default_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        Self::open_at(&path)
    }

    pub fn open_at(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("cannot open database {}", path.display()))?;
        Self::prepare(conn)
    }

    /// For tests: a throwaway database with the same schema.
    pub fn open_in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> Result<Self> {
        // Foreign keys are off by default in SQLite, so the cascade that keeps
        // orphaned tabs out would silently not happen.
        //
        // `busy_timeout` is the one that matters for more than one writer. WAL lets
        // readers and a writer run at once, but *two* writers still collide — and
        // without a timeout the loser gets `SQLITE_BUSY` immediately rather than
        // waiting the moment out. There are three writers here now: this app, the
        // MCP server an agent talks to, and the Jira push (its own connection, so it
        // does not block the window). Five seconds is far longer than any statement
        // in this schema takes and short enough not to look like a hang.
        conn.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;",
        )?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: u32 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);
        let target = MIGRATIONS.len() as u32;
        if version > target {
            bail!(
                "database schema is version {version}, but this build only knows {target}. \
                 It was written by a newer version of the app; refusing to touch it."
            );
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)
                .with_context(|| format!("migration to version {} failed", i + 1))?;
            // `user_version` does not accept a bound parameter.
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// The version this build migrates to. Callers that want to assert "the
    /// database is current" should compare against this rather than a literal,
    /// or every new migration breaks an unrelated test.
    pub const fn latest_schema_version() -> u32 {
        MIGRATIONS.len() as u32
    }

    pub fn schema_version(&self) -> Result<u32> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    /// Replace the stored session wholesale.
    ///
    /// One transaction, so a crash mid-save leaves the previous session intact
    /// rather than half of each.
    pub fn save_session(&mut self, windows: &[WindowSession]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM session_window", [])?;
        for (w, window) in windows.iter().enumerate() {
            tx.execute(
                "INSERT INTO session_window (position, active_tab) VALUES (?1, ?2)",
                params![w as i64, window.active_tab.map(|i| i as i64)],
            )?;
            let window_id = tx.last_insert_rowid();
            for (n, path) in window.sources.iter().enumerate() {
                tx.execute(
                    "INSERT INTO session_source (window_id, position, path) VALUES (?1, ?2, ?3)",
                    params![window_id, n as i64, path],
                )?;
            }
            for (t, tab) in window.tabs.iter().enumerate() {
                tx.execute(
                    "INSERT INTO session_tab \
                     (window_id, position, kind, key, detail, pinned, active) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        window_id,
                        t as i64,
                        tab.kind,
                        tab.key,
                        tab.detail,
                        tab.pinned as i64,
                        tab.active as i64
                    ],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_session(&self) -> Result<Vec<WindowSession>> {
        let mut windows = Vec::new();
        let mut stmt = self
            .conn
            .prepare("SELECT id, active_tab FROM session_window ORDER BY position")?;
        let rows: Vec<(i64, Option<i64>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;

        for (id, active) in rows {
            let mut source_stmt = self.conn.prepare(
                "SELECT path FROM session_source WHERE window_id = ?1 ORDER BY position",
            )?;
            let sources: Vec<String> = source_stmt
                .query_map([id], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            let mut tab_stmt = self.conn.prepare(
                "SELECT kind, key, detail, pinned, active FROM session_tab \
                 WHERE window_id = ?1 ORDER BY position",
            )?;
            let tabs: Vec<TabRecord> = tab_stmt
                .query_map([id], |r| {
                    Ok(TabRecord {
                        kind: r.get(0)?,
                        key: r.get(1)?,
                        detail: r.get(2)?,
                        pinned: r.get::<_, i64>(3)? != 0,
                        active: r.get::<_, i64>(4)? != 0,
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
            // A stored index can be stale if a tab failed to restore; clamp it
            // rather than handing the UI an out-of-bounds selection.
            let active_tab = active
                .map(|i| i as usize)
                .filter(|i| *i < tabs.len())
                .or(if tabs.is_empty() { None } else { Some(0) });
            windows.push(WindowSession {
                sources,
                tabs,
                active_tab,
            });
        }
        Ok(windows)
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

/// `%APPDATA%\request-pilot\state.db` and its equivalents.
pub fn default_path() -> Result<PathBuf> {
    let dir = dirs::data_dir().context("cannot determine the user's data directory")?;
    Ok(dir.join("request-pilot").join("state.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(kind: &str, key: &str) -> TabRecord {
        TabRecord {
            kind: kind.into(),
            key: key.into(),
            detail: None,
            pinned: false,
            active: false,
        }
    }

    #[test]
    fn a_fresh_database_is_migrated_to_the_latest_version() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as u32);
        assert!(store.load_session().unwrap().is_empty());
    }

    #[test]
    fn migrating_an_already_current_database_is_a_no_op() {
        let dir = std::env::temp_dir().join("rp-store-idempotent");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.db");

        {
            let mut store = Store::open_at(&path).unwrap();
            store
                .save_session(&[WindowSession {
                    sources: Vec::new(),
                    tabs: vec![tab("tool", "base64-string-converter")],
                    active_tab: Some(0),
                }])
                .unwrap();
        }
        // Reopening must not re-run migrations or lose data.
        let store = Store::open_at(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as u32);
        assert_eq!(store.load_session().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_session_round_trips_across_windows_and_order() {
        let mut store = Store::open_in_memory().unwrap();
        let windows = vec![
            WindowSession {
                sources: Vec::new(),
                tabs: vec![
                    tab("tool", "base64-string-converter"),
                    TabRecord {
                        kind: "request".into(),
                        key: "C:/x/api.http".into(),
                        detail: Some("2".into()),
                        pinned: true,
                        active: true,
                    },
                ],
                active_tab: Some(1),
            },
            WindowSession {
                sources: Vec::new(),
                tabs: vec![tab("tool", "chronometer")],
                active_tab: Some(0),
            },
        ];
        store.save_session(&windows).unwrap();
        assert_eq!(store.load_session().unwrap(), windows);
    }

    #[test]
    fn saving_replaces_the_previous_session_rather_than_appending() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .save_session(&[WindowSession {
                sources: Vec::new(),
                tabs: vec![tab("tool", "a")],
                active_tab: Some(0),
            }])
            .unwrap();
        store
            .save_session(&[WindowSession {
                sources: Vec::new(),
                tabs: vec![tab("tool", "b")],
                active_tab: Some(0),
            }])
            .unwrap();
        let loaded = store.load_session().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].tabs, [tab("tool", "b")]);

        // And an empty save clears it.
        store.save_session(&[]).unwrap();
        assert!(store.load_session().unwrap().is_empty());
    }

    #[test]
    fn closing_a_window_takes_its_tabs_with_it() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .save_session(&[WindowSession {
                sources: Vec::new(),
                tabs: vec![tab("tool", "a")],
                active_tab: Some(0),
            }])
            .unwrap();
        store
            .conn
            .execute("DELETE FROM session_window", [])
            .unwrap();
        // Relies on `PRAGMA foreign_keys = ON`, which SQLite does not default to.
        let orphans: i64 = store
            .conn
            .query_row("SELECT count(*) FROM session_tab", [], |r| r.get(0))
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn an_out_of_range_active_index_is_clamped_not_returned() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .save_session(&[WindowSession {
                sources: Vec::new(),
                tabs: vec![tab("tool", "a"), tab("tool", "b")],
                active_tab: Some(1),
            }])
            .unwrap();
        // Simulate a tab that failed to restore.
        store
            .conn
            .execute("DELETE FROM session_tab WHERE position = 1", [])
            .unwrap();
        let loaded = store.load_session().unwrap();
        assert_eq!(loaded[0].tabs.len(), 1);
        assert_eq!(loaded[0].active_tab, Some(0), "stale index must be clamped");
    }

    #[test]
    fn a_window_with_no_tabs_has_no_active_tab() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .save_session(&[WindowSession {
                sources: Vec::new(),
                tabs: vec![],
                active_tab: Some(3),
            }])
            .unwrap();
        assert_eq!(store.load_session().unwrap()[0].active_tab, None);
    }

    /// The pin flag is its own column, so it survives a save/load and does not
    /// collide with a request tab's index in `detail`.
    #[test]
    fn a_pin_survives_the_round_trip_alongside_a_detail() {
        let mut store = Store::open_in_memory().unwrap();
        let pinned = TabRecord {
            kind: "request".into(),
            key: "C:/x/api.http".into(),
            detail: Some("7".into()),
            pinned: true,
            active: true,
        };
        let plain = tab("tool", "chronometer");
        store
            .save_session(&[WindowSession {
                sources: Vec::new(),
                tabs: vec![pinned.clone(), plain.clone()],
                active_tab: Some(0),
            }])
            .unwrap();
        let back = store.load_session().unwrap();
        assert_eq!(back[0].tabs, [pinned, plain]);
        assert!(back[0].tabs[0].pinned);
        assert_eq!(back[0].tabs[0].detail.as_deref(), Some("7"));
        assert!(!back[0].tabs[1].pinned);
    }

    /// A database written before v3 has neither the `pinned` nor the `active`
    /// column; each migration's DEFAULT 0 must make every existing tab load
    /// rather than failing the read. `WindowSession::active_tab` is what still
    /// carries the active tab for such a session — see `tabs::plan`.
    #[test]
    fn a_pre_v3_database_migrates_and_reads_as_unpinned() {
        let dir = std::env::temp_dir().join("rp-store-v3");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.db");

        // Build a v2 database by hand: the first two migrations only.
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
            for (i, sql) in MIGRATIONS.iter().take(2).enumerate() {
                conn.execute_batch(sql).unwrap();
                conn.pragma_update(None, "user_version", (i + 1) as i64)
                    .unwrap();
            }
            conn.execute(
                "INSERT INTO session_window (position, active_tab) VALUES (0, 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO session_tab (window_id, position, kind, key, detail) VALUES (1, 0, 'tool', 'chronometer', NULL)",
                [],
            )
            .unwrap();
        }

        let store = Store::open_at(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len() as u32);
        let back = store.load_session().unwrap();
        assert_eq!(
            back[0].tabs.len(),
            1,
            "the pre-v3 tab must survive the upgrade"
        );
        assert!(!back[0].tabs[0].pinned);
        assert!(!back[0].tabs[0].active);
        assert_eq!(
            back[0].active_tab,
            Some(0),
            "the pre-v4 per-window index is all such a session has"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn key_value_bucket_reads_writes_and_overwrites() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(store.get("theme").unwrap(), None);
        store.set("theme", "dark").unwrap();
        assert_eq!(store.get("theme").unwrap().as_deref(), Some("dark"));
        store.set("theme", "light").unwrap();
        assert_eq!(store.get("theme").unwrap().as_deref(), Some("light"));
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused_not_mangled() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn
            .pragma_update(None, "user_version", (MIGRATIONS.len() + 5) as i64)
            .unwrap();
        let mut store = store;
        let err = store.migrate().unwrap_err().to_string();
        assert!(err.contains("newer version"), "{err}");
    }

    #[test]
    fn a_corrupt_file_errors_instead_of_panicking() {
        let dir = std::env::temp_dir().join("rp-store-corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.db");
        std::fs::write(&path, b"this is definitely not a sqlite database").unwrap();

        assert!(
            Store::open_at(&path).is_err(),
            "a corrupt file must be an Err, not a panic"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_path_lands_under_the_app_name() {
        let path = default_path().unwrap();
        assert!(
            path.ends_with("request-pilot/state.db") || path.ends_with("request-pilot\\state.db"),
            "{path:?}"
        );
    }
}
