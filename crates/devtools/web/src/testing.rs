//! Scaffolding the per-tool test modules share.
#![cfg(test)]

use crate::*;

#[allow(unused_imports)]
use {
    crate::{Args, Category, Field, Kind, Tool, arg, section},
    anyhow::{Context, Result, bail},
    std::fmt::Write as _,
    std::net::Ipv4Addr,
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

/// `run` fns look themselves up by id now, so every id they ask for must
/// exist. This is what used to be `registry_order_is_pinned`.
#[test]
pub(crate) fn every_tool_can_find_itself() {
    for t in TOOLS {
        assert_eq!(tool(t.id).id, t.id);
    }
}

#[test]
pub(crate) fn registry_is_sane() {
    let mut ids: Vec<_> = TOOLS.iter().map(|t| t.id).collect();
    let count = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), count, "duplicate tool ids");
    for t in TOOLS {
        // Must not panic on empty args, and Custom tools must always error.
        let r = (t.run)(&Args::new());
        if t.kind == Kind::Custom {
            assert!(r.is_err(), "{} is Custom but returned Ok", t.id);
        }
        // Fields declare unique keys.
        let mut keys: Vec<_> = t.fields.iter().map(|f| f.key).collect();
        let n = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), n, "{} has duplicate field keys", t.id);
    }
}

#[test]
pub(crate) fn tools_do_not_panic_on_garbage() {
    for t in TOOLS {
        for junk in ["", " ", "\0\u{1f600}", "-1", "!!!!", "\n\n"] {
            let a: Args = t
                .fields
                .iter()
                .map(|f| (f.key.to_string(), junk.to_string()))
                .collect();
            let _ = (t.run)(&a); // Ok or Err, never a panic.
        }
    }
}

/// `run`, for the cases where a failure is a test bug rather than the point.
pub(crate) fn ok(id: &str, pairs: &[(&str, &str)]) -> String {
    run(id, pairs).unwrap_or_else(|e| panic!("{id} failed: {e:#}"))
}
