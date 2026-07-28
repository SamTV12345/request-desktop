//! The tool registry: five crates of tools, one lookup.
//!
//! The types, the panel helpers and everything more than one crate needs live
//! in [`rp_devtools_common`] and are re-exported here, so `rp_devtools::Tool`
//! keeps working. The tools themselves live one file per tool in a crate per
//! category:
//!
//! | crate | what is in it |
//! |:--|:--|
//! | `rp-devtools-crypto` | hashes, HMAC, tokens, keys, JWT |
//! | `rp-devtools-data` | JSON/YAML/TOML/XML/CSV, base64, bases |
//! | `rp-devtools-text` | diffs, maths, dates, colour, QR |
//! | `rp-devtools-web` | URLs, IPs, MACs, HTTP, cron, chmod |
//! | `rp-devtools-community` | ports of open it-tools pull requests |
//!
//! Adding a tool is one new file in one of those crates plus one line in its
//! `TOOLS` list — nothing here changes.
pub use rp_devtools_common::*;

/// The five tool crates under their old module names, because
/// `crates/app` reaches into a handful of tables directly
/// (`tools::web::mime_for_extension`, `tools::text::emoji_search`, …).
pub mod tools {
    pub use rp_devtools_community as community;
    pub use rp_devtools_crypto as crypto;
    pub use rp_devtools_data as data;
    pub use rp_devtools_text as text;
    pub use rp_devtools_web as web;
}

pub fn all() -> &'static [&'static Tool] {
    static ALL: std::sync::OnceLock<Vec<&'static Tool>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let mut v: Vec<&'static Tool> = [
            tools::crypto::TOOLS,
            tools::data::TOOLS,
            tools::web::TOOLS,
            tools::text::TOOLS,
            tools::community::TOOLS,
        ]
        .into_iter()
        .flatten()
        .copied()
        .collect();
        v.sort_by_key(|t| t.name);
        v
    })
}

pub fn find(id: &str) -> Option<&'static Tool> {
    all().iter().copied().find(|t| t.id == id)
}

/// Substring match over name and keywords. ponytail: no fuzzy index for ~110
/// entries — add one when the list outgrows a linear scan, which it will not.
pub fn search(query: &str) -> Vec<&'static Tool> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return all().to_vec();
    }
    all()
        .iter()
        .copied()
        .filter(|t| {
            t.name.to_lowercase().contains(&q)
                || t.id.contains(&q)
                || t.keywords.iter().any(|k| k.contains(q.as_str()))
        })
        .collect()
}
