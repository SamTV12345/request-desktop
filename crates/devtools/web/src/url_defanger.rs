//! `url-defanger` — it-tools PR #932.
//!
//! Pasting a live phishing URL into a ticket, a chat or a mail client is how
//! someone else clicks it. Defanging breaks the autolinker: the scheme stops
//! being a scheme and the dots stop being dots. The reverse direction matters
//! just as much, because the indicators arrive defanged from every threat feed
//! and have to go back into a scanner intact.

use crate::*;
use anyhow::Result;

/// Case-insensitive `str::replace`, ASCII needles only.
///
/// Defanged text arrives in whatever case the report was written in — `hxxp`,
/// `hXXp` and `HXXP` are all in the wild.
fn replace_ci(hay: &str, needle: &str, with: &str) -> String {
    // `to_ascii_lowercase` keeps every byte in place, so indices found in the
    // lowered copy address the same chars in the original.
    let lower_hay = hay.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let mut out = String::with_capacity(hay.len());
    let mut i = 0;
    while let Some(j) = lower_hay[i..].find(&lower_needle) {
        out.push_str(&hay[i..i + j]);
        out.push_str(with);
        i += j + needle.len();
    }
    out.push_str(&hay[i..]);
    out
}

/// Every dot goes, not just the ones in the host: a bare `1.2.3.4` has no host
/// to speak of, and a filename left as `payload.exe` still gets linkified by
/// half the chat clients out there.
pub(crate) fn defang(input: &str) -> String {
    let out = replace_ci(input, "http", "hxxp");
    out.replace('.', "[.]").replace('@', "[at]")
}

pub(crate) fn refang(input: &str) -> String {
    // `[://]` before `[:]` would matter if the shorter one could eat into it;
    // it cannot, but the order also reads as most-specific-first.
    let mut out = input.to_string();
    for (from, to) in [
        ("[://]", "://"),
        ("[.]", "."),
        ("(.)", "."),
        ("[dot]", "."),
        ("[:]", ":"),
        ("[at]", "@"),
        ("[@]", "@"),
        // Longest first: `hxxp` alone would leave the `s` of `HXXPS` stranded
        // in whatever case the feed used.
        ("hxxps", "https"),
        ("hxxp", "http"),
    ] {
        out = replace_ci(&out, from, to);
    }
    out
}

fn url_defanger(a: &Args) -> Result<String> {
    Ok([
        section("Defanged", Ok(defang(s(a, "live", LIVE)))),
        section("Refanged", Ok(refang(s(a, "defanged", DEFANGED)))),
    ]
    .join("\n\n"))
}

const LIVE: &str = "https://evil.example.com/payload.exe?id=1\n1.2.3.4\nphish@evil.example.com";
const DEFANGED: &str =
    "hxxps://evil[.]example[.]com/payload[.]exe?id=1\n1[.]2[.]3[.]4\nphish[at]evil[.]example[.]com";

pub static TOOL: Tool = Tool {
    id: "url-defanger",
    name: "URL defanger",
    description: "Make a URL, IP or email safe to paste into a ticket, and turn a defanged indicator back into a live one.",
    category: Category::Web,
    keywords: &[
        "defang",
        "refang",
        "url",
        "phishing",
        "ioc",
        "indicator",
        "security",
        "hxxp",
        "malware",
    ],
    fields: &[
        Field::area("live", "Live indicators").with_default(LIVE),
        Field::area("defanged", "Defanged indicators").with_default(DEFANGED),
    ],
    kind: Kind::Pure,
    run: url_defanger,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn both_panels_at_once() {
        let out = ok("url-defanger", &[]);
        assert!(
            out.contains("hxxps://evil[.]example[.]com/payload[.]exe?id=1"),
            "{out}"
        );
        assert!(out.contains("1[.]2[.]3[.]4"), "{out}");
        assert!(out.contains("phish[at]evil[.]example[.]com"), "{out}");
        // The refang panel gets the defanged default back to something live.
        assert!(
            out.contains("https://evil.example.com/payload.exe"),
            "{out}"
        );
        assert!(out.contains("phish@evil.example.com"), "{out}");
    }

    #[test]
    fn round_trips_and_takes_feed_spellings() {
        let live = "https://a.b.co/x?q=1#frag";
        assert_eq!(defang(live), "hxxps://a[.]b[.]co/x?q=1#frag");
        assert_eq!(refang(&defang(live)), live);
        // Feeds write the scheme in any case and the dot three different ways.
        assert_eq!(refang("hXXp://a(.)b[dot]co"), "http://a.b.co");
        assert_eq!(refang("HXXPS[://]a[.]b"), "https://a.b");
        // Nothing to do is not an error, just a passthrough.
        assert_eq!(defang("plain text"), "plain text");
    }
}
