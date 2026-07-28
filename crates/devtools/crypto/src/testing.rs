//! Scaffolding the per-tool test modules share.
#![cfg(test)]

#[allow(unused_imports)]
use {
    crate::{Args, Category, Field, FieldKind, Kind, Tool},
    anyhow::{Context, Result, bail},
    base64::Engine,
    base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    hmac::{KeyInit, Mac, SimpleHmac},
    sha2::Digest,
};

pub(crate) fn args(pairs: &[(&str, &str)]) -> Args {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
