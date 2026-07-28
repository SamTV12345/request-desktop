//! `url-cleaner` — it-tools PR #959.
//!
//! A link copied out of a newsletter carries the campaign that sent it, the ad
//! that paid for it and the session that opened it. None of that survives being
//! pasted into a chat usefully, and all of it identifies the person who pasted
//! it. Strip it, and say what was stripped — a silent cleaner is a cleaner you
//! cannot trust with a link that stopped working.

use crate::*;
use anyhow::{Result, bail};
use std::fmt::Write as _;

/// `base`, `query` (no `?`), `fragment` (no `#`). Shared with the two other
/// URL-editing tools so they agree on where a query ends.
pub(crate) fn split_url_parts(url: &str) -> (&str, &str, &str) {
    let (head, fragment) = url.split_once('#').unwrap_or((url, ""));
    let (base, query) = head.split_once('?').unwrap_or((head, ""));
    (base, query, fragment)
}

/// Exact names. Analytics vendors add new ones constantly, so this list is a
/// floor, not a promise.
#[rustfmt::skip]
const TRACKERS: &[&str] = &[
    "fbclid", "gclid", "dclid", "msclkid", "mc_eid", "mc_cid", "igshid", "ref", "ref_src",
    "si", "spm", "yclid", "_ga", "_gl", "vero_id", "oly_enc_id", "hsCtaTracking",
    "__hstc", "__hssc", "__hsfp", "wickedid", "twclid", "ttclid",
];

/// `utm_` is matched as a prefix, not as a fixed list: Google keeps minting new
/// ones (`utm_id`, `utm_source_platform`, `utm_creative_format`) and every one
/// of them is a campaign tag by construction. Nothing else in this tool matches
/// on a prefix.
fn is_tracking(key: &str) -> bool {
    // `get` rather than a slice: a key can be arbitrary bytes and `[..4]` would
    // panic in the middle of a multi-byte char.
    key.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("utm_"))
        || TRACKERS.iter().any(|t| t.eq_ignore_ascii_case(key))
}

fn url_cleaner(a: &Args) -> Result<String> {
    let url = s(a, "url", DEFAULT).trim();
    if url.is_empty() {
        bail!("paste a URL with tracking parameters, for example a link out of a newsletter");
    }
    let (base, query, fragment) = split_url_parts(url);
    let (mut kept, mut removed) = (Vec::new(), Vec::new());
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, _) = pair.split_once('=').unwrap_or((pair, ""));
        if is_tracking(key) {
            removed.push(pair);
        } else {
            kept.push(pair);
        }
    }
    let mut clean = base.to_string();
    if !kept.is_empty() {
        let _ = write!(clean, "?{}", kept.join("&"));
    }
    if !fragment.is_empty() {
        let _ = write!(clean, "#{fragment}");
    }

    let list = |what: &[&str], empty: &str| match what.is_empty() {
        true => empty.to_string(),
        false => what
            .iter()
            .map(|p| format!("  {p}"))
            .collect::<Vec<_>>()
            .join("\n"),
    };
    Ok([
        section("Clean URL", Ok(clean)),
        section(
            "Removed",
            Ok(list(&removed, "  nothing — that URL was already clean")),
        ),
        section("Kept", Ok(list(&kept, "  no query parameters"))),
    ]
    .join("\n\n"))
}

const DEFAULT: &str = "https://example.com/article?utm_source=newsletter&utm_medium=email&utm_campaign=spring&id=42&fbclid=IwAR0abc&page=3#comments";

pub static TOOL: Tool = Tool {
    id: "url-cleaner",
    name: "URL cleaner",
    description: "Drop utm_*, fbclid, gclid and the rest of the tracking parameters from a link, and show what went.",
    category: Category::Web,
    keywords: &[
        "url", "clean", "tracking", "utm", "fbclid", "gclid", "privacy", "share", "link",
    ],
    fields: &[Field::text("url", "URL to clean").with_default(DEFAULT)],
    kind: Kind::Pure,
    run: url_cleaner,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn strips_trackers_and_keeps_the_rest() {
        let out = ok("url-cleaner", &[]);
        assert!(
            out.contains("https://example.com/article?id=42&page=3#comments"),
            "{out}"
        );
        assert!(out.contains("  utm_source=newsletter"), "{out}");
        assert!(out.contains("  fbclid=IwAR0abc"), "{out}");
        assert!(out.contains("  id=42"), "{out}");
    }

    #[test]
    fn the_utm_prefix_catches_names_not_in_any_list() {
        let out = ok(
            "url-cleaner",
            &[(
                "url",
                "https://x.test/p?utm_source_platform=g&UTM_ID=9&utmm=keep&q=1",
            )],
        );
        assert!(
            out.contains("Clean URL:\nhttps://x.test/p?utmm=keep&q=1"),
            "{out}"
        );
        // Case does not save a tracker, and `utmm` is not a `utm_` parameter.
        assert!(out.contains("  UTM_ID=9"), "{out}");
        assert!(out.contains("  utmm=keep"), "{out}");
    }

    #[test]
    fn a_clean_url_and_a_url_without_a_query_both_survive() {
        let out = ok("url-cleaner", &[("url", "https://x.test/p")]);
        assert!(out.contains("Clean URL:\nhttps://x.test/p"), "{out}");
        assert!(out.contains("already clean"), "{out}");
        assert!(out.contains("no query parameters"), "{out}");
        // Everything tracked means no `?` is left dangling.
        let all = ok("url-cleaner", &[("url", "https://x.test/p?gclid=1&si=2")]);
        assert!(all.contains("Clean URL:\nhttps://x.test/p\n"), "{all}");
        assert!(run("url-cleaner", &[("url", "  ")]).is_err());
    }
}
