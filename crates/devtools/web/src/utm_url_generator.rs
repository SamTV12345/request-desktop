//! `utm-url-generator` — it-tools PR #962.
//!
//! The other end of `url-cleaner`: building the tagged link rather than
//! stripping it. Hand-assembling these is where campaigns go wrong — a space in
//! the campaign name, a `?` where an `&` belonged, a tag appended after the
//! fragment so the analytics never sees it.

use crate::*;
use anyhow::{Result, bail};
use percent_encoding::utf8_percent_encode;
use std::fmt::Write as _;

/// Google's five, in the order the campaign URL builder writes them, each with
/// the field default it has to agree with when the args arrive empty.
const PARAMS: [(&str, &str, &str); 5] = [
    ("utm_source", "source", DEFAULT_SOURCE),
    ("utm_medium", "medium", DEFAULT_MEDIUM),
    ("utm_campaign", "campaign", DEFAULT_CAMPAIGN),
    ("utm_term", "term", ""),
    ("utm_content", "content", ""),
];

fn utm_url_generator(a: &Args) -> Result<String> {
    let url = s(a, "url", DEFAULT_URL).trim();
    if url.is_empty() {
        bail!(
            "enter the page the campaign should land on, for example https://example.com/pricing"
        );
    }
    let (base, query, fragment) = split_url_parts(url);
    let mut pairs: Vec<String> = query
        .split('&')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    let existing = pairs.len();
    let mut table = String::new();
    for (name, key, default) in PARAMS {
        let value = s(a, key, default).trim();
        if value.is_empty() {
            continue;
        }
        // `URI_COMPONENT` is the `encodeURIComponent` set: a space becomes
        // `%20` and `&` becomes `%26`, so a campaign name cannot smuggle in a
        // sixth parameter.
        let encoded = utf8_percent_encode(value, URI_COMPONENT).to_string();
        let _ = writeln!(table, "  {name} = {value}");
        pairs.push(format!("{name}={encoded}"));
    }
    if pairs.len() == existing {
        bail!("fill in at least one of source, medium or campaign");
    }
    // The fragment goes back on last; a tag written after `#` never reaches the
    // server and is the classic reason a campaign reports zero visits.
    let mut out = format!("{base}?{}", pairs.join("&"));
    if !fragment.is_empty() {
        let _ = write!(out, "#{fragment}");
    }
    Ok([section("Campaign URL", Ok(out)), section("Tags", Ok(table))].join("\n\n"))
}

const DEFAULT_URL: &str = "https://example.com/pricing?plan=team";
const DEFAULT_SOURCE: &str = "newsletter";
const DEFAULT_MEDIUM: &str = "email";
const DEFAULT_CAMPAIGN: &str = "spring sale";

pub static TOOL: Tool = Tool {
    id: "utm-url-generator",
    name: "UTM URL generator",
    description: "Build a campaign URL from source, medium, campaign, term and content, keeping the query string that was already there.",
    category: Category::Web,
    keywords: &[
        "utm",
        "campaign",
        "url",
        "marketing",
        "analytics",
        "tracking",
        "builder",
        "google",
    ],
    fields: &[
        Field::text("url", "Landing page URL").with_default(DEFAULT_URL),
        Field::text("source", "Campaign source (utm_source)").with_default(DEFAULT_SOURCE),
        Field::text("medium", "Campaign medium (utm_medium)").with_default(DEFAULT_MEDIUM),
        Field::text("campaign", "Campaign name (utm_campaign)").with_default(DEFAULT_CAMPAIGN),
        Field::text("term", "Paid keyword (utm_term)"),
        Field::text("content", "Ad or link variant (utm_content)"),
    ],
    kind: Kind::Pure,
    run: utm_url_generator,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn keeps_the_existing_query_and_encodes_the_tags() {
        let out = ok("utm-url-generator", &[]);
        assert!(
            out.contains(
                "https://example.com/pricing?plan=team&utm_source=newsletter&utm_medium=email&utm_campaign=spring%20sale"
            ),
            "{out}"
        );
        // The table shows the value as typed, not as encoded.
        assert!(out.contains("utm_campaign = spring sale"), "{out}");
    }

    #[test]
    fn empty_tags_are_left_out_and_the_fragment_stays_last() {
        let out = ok(
            "utm-url-generator",
            &[
                ("url", "https://x.test/p#pricing"),
                ("source", "twitter"),
                ("medium", ""),
                ("campaign", ""),
                ("content", "a&b"),
            ],
        );
        assert!(
            out.contains("https://x.test/p?utm_source=twitter&utm_content=a%26b#pricing"),
            "{out}"
        );
        assert!(!out.contains("utm_medium"), "{out}");
    }

    #[test]
    fn nothing_to_tag_says_so() {
        let all_blank = [
            ("url", "https://x.test/"),
            ("source", ""),
            ("medium", ""),
            ("campaign", ""),
            ("term", ""),
            ("content", ""),
        ];
        assert!(run("utm-url-generator", &all_blank).is_err());
        assert!(run("utm-url-generator", &[("url", " ")]).is_err());
    }
}
