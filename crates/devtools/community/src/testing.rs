//! Scaffolding the per-tool test modules share.
#![cfg(test)]

use crate::*;

#[allow(unused_imports)]
use {
    anyhow::{Context, Result, bail},
    serde_json::Value,
    std::fmt::Write as _,
};

pub(crate) fn run(tool: &str, args: &[(&str, &str)]) -> String {
    let t = TOOLS.iter().find(|t| t.id == tool).expect("tool exists");
    let args: Args = args
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    (t.run)(&args).unwrap_or_else(|e| panic!("{tool}: {e:#}"))
}
