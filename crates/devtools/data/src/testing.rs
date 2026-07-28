//! Scaffolding the per-tool test modules share.
#![cfg(test)]

use crate::*;

#[allow(unused_imports)]
use {
    crate::{Args, Category, Field, Kind, Tool, arg},
    anyhow::{Context, Result, anyhow, bail},
    base64::Engine as _,
    base64::engine::general_purpose as b64,
    num_bigint::BigUint,
    std::fmt::Write as _,
};

pub(crate) fn args(pairs: &[(&str, &str)]) -> Args {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

pub(crate) fn run(id: &str, pairs: &[(&str, &str)]) -> Result<String> {
    let t = TOOLS.iter().find(|t| t.id == id).expect("tool exists");
    (t.run)(&args(pairs))
}

pub(crate) fn ok(id: &str, pairs: &[(&str, &str)]) -> String {
    run(id, pairs).unwrap_or_else(|e| panic!("{id} failed: {e:#}"))
}

// ── registry contract ───────────────────────────────────────────────

pub(crate) const J: &str = r#"{"hello":["world"],"n":1}"#;
pub(crate) const UNI: &str = "&#108;&#105;&#110;&#107;&#101;&#32;&#116;&#104;&#101;&#32;&#115;&#116;&#114;&#105;&#110;&#103;&#32;&#99;&#111;&#110;&#118;&#101;&#114;&#116;&#32;&#116;&#111;&#32;&#117;&#110;&#105;&#99;&#111;&#100;&#101;";
