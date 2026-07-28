//! `url-text-fragment-generator` — it-tools PR #918.
//!
//! A scroll-to-text fragment links to a sentence rather than to a page, without
//! the page needing an anchor for it. The syntax is fiddly in exactly two
//! places: `-`, `,` and `&` are structural and have to be percent-encoded even
//! though a normal fragment would leave them alone, and a long quote has to be
//! shortened to a `textStart,textEnd` range or the match silently fails.
//!
//! Reading one back is the direction people actually need more often — a
//! fragment in a bug report is unreadable until it is decoded.

use crate::*;
use anyhow::{Context, Result, bail};
use percent_encoding::utf8_percent_encode;
use std::fmt::Write as _;

/// `encodeURIComponent`'s set plus `-`: inside a text directive a hyphen marks
/// a prefix or suffix, so a quote containing one would change the meaning.
/// `,` and `&` are already in there, and they separate the parts and the
/// directives respectively.
const TEXT_DIRECTIVE: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

/// Beyond this many words the quote is sent as a `textStart,textEnd` range.
/// Chrome matches the range against the whole document text between the two
/// ends, so the exact figure only trades URL length against false matches.
const RANGE_ABOVE: usize = 10;
/// Words taken from each end when a quote goes over. Five is enough to be
/// unique in ordinary prose and short enough to stay readable in the URL.
const EDGE_WORDS: usize = 5;

fn build(url: &str, quote: &str) -> Result<String> {
    if url.trim().is_empty() {
        bail!("enter the page URL the quote is on");
    }
    // Whitespace in the source is not whitespace in the rendered text; the
    // matcher normalises, so the directive has to as well.
    let words: Vec<&str> = quote.split_whitespace().collect();
    if words.is_empty() {
        bail!("paste the sentence you want to link to");
    }
    let enc = |ws: &[&str]| utf8_percent_encode(&ws.join(" "), TEXT_DIRECTIVE).to_string();
    let directive = if words.len() > RANGE_ABOVE {
        format!(
            "{},{}",
            enc(&words[..EDGE_WORDS]),
            enc(&words[words.len() - EDGE_WORDS..])
        )
    } else {
        enc(&words)
    };
    // Any fragment already on the URL is replaced: two `#` are not a URL.
    let (base, query, _) = split_url_parts(url.trim());
    let mut out = base.to_string();
    if !query.is_empty() {
        let _ = write!(out, "?{query}");
    }
    let _ = write!(out, "#:~:text={directive}");
    Ok(out)
}

fn decode(part: &str) -> String {
    percent_encoding::percent_decode_str(part)
        .decode_utf8_lossy()
        .into_owned()
}

/// `[prefix-,]textStart[,textEnd][,-suffix]` — the optional parts are told
/// apart from the required ones by which side the `-` sits on, which is why an
/// unencoded hyphen in a quote breaks the whole directive.
fn explain(directive: &str, out: &mut String) {
    let mut parts: Vec<&str> = directive.split(',').collect();
    if parts.len() > 4 {
        let _ = writeln!(out, "  malformed: {} parts, at most 4 allowed", parts.len());
        return;
    }
    let mut prefix = None;
    let mut suffix = None;
    if parts.len() > 1 && parts[0].ends_with('-') {
        let first = parts.remove(0);
        prefix = Some(&first[..first.len() - 1]);
    }
    if parts.len() > 1 && parts.last().is_some_and(|p| p.starts_with('-')) {
        let last = parts.pop().unwrap_or_default();
        suffix = Some(&last[1..]);
    }
    if let Some(p) = prefix {
        let _ = writeln!(out, "  after:  {}", decode(p));
    }
    let _ = writeln!(out, "  starts: {}", decode(parts.first().unwrap_or(&"")));
    if let Some(end) = parts.get(1) {
        let _ = writeln!(out, "  ends:   {}", decode(end));
    }
    if let Some(sfx) = suffix {
        let _ = writeln!(out, "  before: {}", decode(sfx));
    }
}

fn resolve(url: &str) -> Result<String> {
    let fragment = url
        .trim()
        .split_once("#:~:")
        .context("no `#:~:text=` fragment in that URL")?
        .1;
    let mut out = String::new();
    let mut found = 0;
    for directive in fragment.split('&') {
        let Some(body) = directive.strip_prefix("text=") else {
            continue;
        };
        found += 1;
        let _ = writeln!(out, "match {found}:");
        explain(body, &mut out);
    }
    if found == 0 {
        bail!("that fragment has no `text=` directive");
    }
    Ok(out)
}

fn url_text_fragment_generator(a: &Args) -> Result<String> {
    Ok([
        section(
            "Fragment URL",
            build(s(a, "url", DEFAULT_URL), s(a, "quote", DEFAULT_QUOTE)),
        ),
        section(
            "Fragment read back",
            resolve(s(a, "fragment_url", DEFAULT_FRAGMENT)),
        ),
    ]
    .join("\n\n"))
}

const DEFAULT_URL: &str = "https://example.com/handbook";
const DEFAULT_QUOTE: &str =
    "Every deploy is reversible, and the rollback is tested on the same day it is written.";
const DEFAULT_FRAGMENT: &str = "https://example.com/handbook#:~:text=on%20call-,Every%20deploy%20is%20reversible,tested%20the%20same%20day,-and%20nothing%20else";

pub static TOOL: Tool = Tool {
    id: "url-text-fragment-generator",
    name: "URL text fragment generator",
    description: "Link to a sentence instead of a page with #:~:text=, and read an existing text fragment back in plain words.",
    category: Category::Web,
    keywords: &[
        "url",
        "fragment",
        "text",
        "scroll-to-text",
        "anchor",
        "deep link",
        "highlight",
        "quote",
    ],
    fields: &[
        Field::text("url", "Page URL").with_default(DEFAULT_URL),
        Field::area("quote", "Text to link to").with_default(DEFAULT_QUOTE),
        Field::text("fragment_url", "URL with a text fragment to read")
            .with_default(DEFAULT_FRAGMENT),
    ],
    kind: Kind::Pure,
    run: url_text_fragment_generator,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn long_quotes_become_a_range_and_short_ones_do_not() {
        let out = ok("url-text-fragment-generator", &[]);
        // 16 words, so the two ends are sent and the middle is left to Chrome.
        assert!(
            out.contains(
                "#:~:text=Every%20deploy%20is%20reversible%2C%20and,same%20day%20it%20is%20written."
            ),
            "{out}"
        );
        let short = ok(
            "url-text-fragment-generator",
            &[("url", "https://x.test/p"), ("quote", "hello there world")],
        );
        assert!(
            short.contains("https://x.test/p#:~:text=hello%20there%20world"),
            "{short}"
        );
    }

    #[test]
    fn the_structural_characters_are_encoded_and_the_query_survives() {
        let out = ok(
            "url-text-fragment-generator",
            &[("url", "https://x.test/p?v=2#old"), ("quote", "a-b, c & d")],
        );
        assert!(
            out.contains("https://x.test/p?v=2#:~:text=a%2Db%2C%20c%20%26%20d"),
            "{out}"
        );
        assert!(!out.contains("#old"), "the old fragment is replaced: {out}");
    }

    #[test]
    fn reads_prefix_start_end_and_suffix_back() {
        let out = ok("url-text-fragment-generator", &[]);
        assert!(out.contains("after:  on call"), "{out}");
        assert!(out.contains("starts: Every deploy is reversible"), "{out}");
        assert!(out.contains("ends:   tested the same day"), "{out}");
        assert!(out.contains("before: and nothing else"), "{out}");
    }

    #[test]
    fn each_panel_fails_on_its_own() {
        // No fragment to read: the generator panel still renders.
        let out = ok(
            "url-text-fragment-generator",
            &[
                ("url", "https://x.test/p"),
                ("quote", "hello"),
                ("fragment_url", "https://x.test/p"),
            ],
        );
        assert!(out.contains("#:~:text=hello"), "{out}");
        assert!(out.contains("no `#:~:text=` fragment"), "{out}");
        // And the other way round: an empty quote does not blank the reader.
        let out = ok(
            "url-text-fragment-generator",
            &[("url", "https://x.test/p"), ("quote", "  ")],
        );
        assert!(out.contains("paste the sentence"), "{out}");
        assert!(out.contains("starts: Every deploy is reversible"), "{out}");
    }
}
