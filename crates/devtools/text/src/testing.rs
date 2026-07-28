//! Scaffolding the per-tool test modules share.
#![cfg(test)]

use crate::*;

#[allow(unused_imports)]
use {
    crate::{Args, Category, Field, FieldKind, Kind, Tool},
    base64::Engine as _,
    chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc},
    regex::Regex,
};

pub(crate) fn args(pairs: &[(&str, &str)]) -> Args {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub(crate) fn run(id: &str, pairs: &[(&str, &str)]) -> anyhow::Result<String> {
    let t = TOOLS.iter().find(|t| t.id == id).expect("tool exists");
    (t.run)(&args(pairs))
}

/// `run`, for the cases where a failure is a test bug rather than the point.
pub(crate) fn ok(id: &str, pairs: &[(&str, &str)]) -> String {
    run(id, pairs).unwrap_or_else(|e| panic!("{id} failed: {e:#}"))
}
