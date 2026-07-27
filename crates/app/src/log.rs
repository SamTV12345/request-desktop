//! A log file beside the database, because a release build has no stderr.
//!
//! Every diagnostic in this app is of one kind: *something failed and we carried
//! on*. A failed theme write, a session that could not be saved, an MCP server
//! that did not start. They went to `eprintln!`, which was visible to whoever ran
//! the app from a terminal — and since the release binary became a GUI subsystem
//! program, visible to nobody.
//!
//! So they go to a file as well, next to `state.db`, and a panic hook puts the one
//! failure that is *not* survivable in the same place. Without that, a release
//! panic closes the window and leaves nothing to report.
//!
//! # Deliberately not a logging framework
//!
//! No levels, no targets, no filtering, no dependency. Fourteen call sites all
//! saying the same kind of thing do not need `tracing`, and a log nobody
//! configures cannot be misconfigured. If this ever needs structure, the shape
//! here — one `note()` behind a macro — is what gets replaced.
use crate::track::{civil_from_days, fmt_date};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Above this the file is started over. A log that grows without bound is a
/// disk-space bug that takes a year to show up, and the interesting lines are the
/// recent ones.
const MAX_BYTES: u64 = 1 << 20;

/// Where the log goes: beside the database, whatever `rp_store` decides that is.
///
/// Derived from `default_path` rather than rebuilt from `dirs`, so the two cannot
/// end up in different directories.
pub fn path() -> Option<PathBuf> {
    rp_store::default_path()
        .ok()
        .map(|db| db.with_file_name("request-pilot.log"))
}

fn target() -> Option<&'static Path> {
    static PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
    PATH.get_or_init(path).as_deref()
}

/// `2026-07-27 21:50:03` in UTC.
///
/// UTC and said so, rather than local time: the offset would have to come from the
/// store, which is one of the things that can fail and be logged. A log line that
/// needs the database to render its own timestamp is a log line missing exactly
/// when it matters.
///
/// The date arithmetic is [`civil_from_days`], borrowed and not copied — a second
/// copy of it in this crate was once three days out, and that is recorded at the
/// top of `calendar.rs`.
pub fn stamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let mut out = fmt_date(civil_from_days(days));
    let _ = write!(
        out,
        " {:02}:{:02}:{:02}",
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    );
    out
}

/// Append one line. Never fails loudly: this *is* the failure path.
///
/// Called through the [`note!`] macro, which is why it is not called `note` —
/// a `macro_rules!` and a function of the same name cannot both be re-exported.
pub fn append(message: &str) {
    // Still stderr in a debug build, where a console exists and watching it is
    // faster than opening a file.
    #[cfg(debug_assertions)]
    eprintln!("request-pilot: {message}");

    let Some(path) = target() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let over = std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES);
    let mut open = std::fs::OpenOptions::new();
    open.create(true);
    if over {
        open.write(true).truncate(true);
    } else {
        open.append(true);
    }
    if let Ok(mut f) = open.open(path) {
        let _ = writeln!(f, "{} {message}", stamp(crate::track::now()));
    }
}

/// Route panics into the same file.
///
/// The default hook writes to stderr, which in a release build goes nowhere — so a
/// panic took the window and left no trace at all. The default hook still runs
/// afterwards, so a debug build keeps its familiar backtrace on the console.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // `PanicHookInfo::payload_as_str` is not in this toolchain's stable API, so
        // the two shapes a payload actually takes are unwrapped by hand.
        let what = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("(no message)");
        let where_ = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".into());
        append(&format!("PANIC at {where_}: {what}"));
        previous(info);
    }));
}

/// `note!("cannot store the theme: {e:#}")` — the one call shape.
macro_rules! note {
    ($($arg:tt)*) => { $crate::log::append(&format!($($arg)*)) };
}
pub(crate) use note;

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the path: beside the database, not merely in the same
    /// general area.
    #[test]
    fn the_log_sits_in_the_same_directory_as_the_database() {
        let (Some(log), Ok(db)) = (path(), rp_store::default_path()) else {
            return; // No data directory on this machine; nothing to assert.
        };
        assert_eq!(log.parent(), db.parent());
        assert_eq!(
            log.file_name().and_then(|n| n.to_str()),
            Some("request-pilot.log")
        );
    }

    #[test]
    fn a_stamp_reads_as_a_time_and_matches_the_epoch() {
        assert_eq!(stamp(0), "1970-01-01 00:00:00");
        // 2026-07-27T21:50:03Z, checked against the epoch second, not against
        // another rendering of the same arithmetic.
        assert_eq!(stamp(1_785_189_003), "2026-07-27 21:50:03");
    }

    /// Negative epoch seconds come from a clock that has not been set, and a log
    /// line is the *last* place that should panic.
    #[test]
    fn a_time_before_the_epoch_still_renders() {
        assert_eq!(stamp(-1), "1969-12-31 23:59:59");
        assert_eq!(stamp(-86_400), "1969-12-31 00:00:00");
    }
}
