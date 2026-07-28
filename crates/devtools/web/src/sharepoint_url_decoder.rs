//! `sharepoint-url-decoder` — it-tools PR #1524.
//!
//! A SharePoint or OneDrive link is a URL with another URL inside it, and the
//! inner one has usually been through the encoder twice — once by the web part
//! that built the link and once by whatever mail client forwarded it. So
//! `%2520` where a space belongs, and a server-relative path hidden in `id=`.
//! Decoding once is not enough; decoding forever is a hang, so it is capped.

use crate::*;
use anyhow::{Result, bail};
use std::fmt::Write as _;

/// Two rounds is the common case, four the worst seen. Eight leaves room and
/// still terminates on input engineered to keep re-encoding itself.
const MAX_ROUNDS: usize = 8;

/// Percent-decode until it stops changing. Lossy on purpose: half-decoded
/// SharePoint links contain byte sequences that were never valid UTF-8, and
/// showing the rest of the path beats refusing the whole link.
pub(crate) fn decode_until_stable(raw: &str) -> String {
    let mut current = raw.to_string();
    for _ in 0..MAX_ROUNDS {
        if !current.contains('%') {
            break;
        }
        let next = percent_encoding::percent_decode_str(&current)
            .decode_utf8_lossy()
            .into_owned();
        if next == current {
            break;
        }
        current = next;
    }
    current
}

/// The parameters that carry something worth reading, in the order SharePoint
/// tends to emit them. Everything else in the query is view state.
const INTERESTING: &[&str] = &[
    "id",
    "sourcedoc",
    "RootFolder",
    "FolderCTID",
    "parent",
    "file",
    "parentview",
    "web",
    "cid",
    "resid",
];

fn sharepoint_url_decoder(a: &Args) -> Result<String> {
    let raw = s(a, "url", DEFAULT).trim();
    if raw.is_empty() {
        bail!("paste a SharePoint or OneDrive link, the kind with `?id=` or `sourcedoc=` in it");
    }
    let (base, query, _) = split_url_parts(raw);

    let mut parts = String::new();
    let path = decode_until_stable(base);
    // `scheme://host/rest` — the host is worth its own line because the tenant
    // name is what tells you which SharePoint this even is.
    match path.split_once("://").and_then(|(_, r)| r.split_once('/')) {
        Some((host, rest)) => {
            let _ = writeln!(parts, "host: {host}");
            let _ = writeln!(parts, "path: /{rest}");
        }
        None => {
            let _ = writeln!(parts, "path: {path}");
        }
    }
    // `/sites/Foo` and `/personal/bar_contoso_com` are the two site roots; the
    // rest of every server-relative path in the query hangs off one of them.
    if let Some(site) = site_root(&path) {
        let _ = writeln!(parts, "site: {site}");
    }

    let mut file = None;
    let mut seen = Vec::new();
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_until_stable(key);
        if !INTERESTING.iter().any(|k| k.eq_ignore_ascii_case(&key)) {
            seen.push(key);
            continue;
        }
        let decoded = decode_until_stable(value);
        let _ = writeln!(parts, "{key}: {decoded}");
        // A path ending in an extension is the document the link points at.
        let last = decoded.rsplit('/').next().unwrap_or_default();
        if file.is_none() && last.contains('.') {
            file = Some(last.to_string());
        }
        if key.eq_ignore_ascii_case("file") {
            file = Some(decoded);
        }
    }
    if let Some(f) = file {
        let _ = writeln!(parts, "file: {f}");
    }
    if !seen.is_empty() {
        let _ = writeln!(parts, "other parameters: {}", seen.join(", "));
    }

    Ok([
        section("Readable link", Ok(decode_until_stable(raw))),
        section("Parts", Ok(parts)),
    ]
    .join("\n\n"))
}

fn site_root(path: &str) -> Option<String> {
    let rest = path.split_once("://")?.1;
    let mut segments = rest.split('/').skip(1);
    let kind = segments.next()?;
    if !matches!(kind, "sites" | "teams" | "personal") {
        return None;
    }
    Some(format!("/{kind}/{}", segments.next()?))
}

const DEFAULT: &str = "https://contoso.sharepoint.com/sites/Marketing/Shared%20Documents/Forms/AllItems.aspx?id=%2Fsites%2FMarketing%2FShared%2520Documents%2FReports%2FQ3%2520Plan%2Exlsx&parent=%2Fsites%2FMarketing%2FShared%2520Documents%2FReports&viewid=7a1b2c3d";

pub static TOOL: Tool = Tool {
    id: "sharepoint-url-decoder",
    name: "SharePoint URL decoder",
    description: "Unwrap a SharePoint or OneDrive link: the double-encoded id, sourcedoc and RootFolder paths, spelled out.",
    category: Category::Web,
    keywords: &[
        "sharepoint",
        "onedrive",
        "url",
        "decode",
        "microsoft",
        "office",
        "link",
        "percent",
    ],
    fields: &[Field::area("url", "SharePoint or OneDrive link").with_default(DEFAULT)],
    kind: Kind::Pure,
    run: sharepoint_url_decoder,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn unwraps_the_double_encoded_id() {
        let out = ok("sharepoint-url-decoder", &[]);
        assert!(out.contains("host: contoso.sharepoint.com"), "{out}");
        assert!(out.contains("site: /sites/Marketing"), "{out}");
        // %2520 is a space that was encoded twice; one pass would leave %20.
        assert!(
            out.contains("id: /sites/Marketing/Shared Documents/Reports/Q3 Plan.xlsx"),
            "{out}"
        );
        assert!(out.contains("file: Q3 Plan.xlsx"), "{out}");
        assert!(out.contains("other parameters: viewid"), "{out}");
    }

    #[test]
    fn the_doc_aspx_shape_and_a_personal_onedrive() {
        let out = ok(
            "sharepoint-url-decoder",
            &[(
                "url",
                "https://contoso.sharepoint.com/personal/ada_contoso_com/_layouts/15/Doc.aspx?sourcedoc=%7B12345678-90AB-CDEF-1234-567890ABCDEF%7D&file=Budget%2520FY25.xlsx&mobileredirect=true",
            )],
        );
        assert!(out.contains("site: /personal/ada_contoso_com"), "{out}");
        assert!(
            out.contains("sourcedoc: {12345678-90AB-CDEF-1234-567890ABCDEF}"),
            "{out}"
        );
        assert!(out.contains("file: Budget FY25.xlsx"), "{out}");
    }

    #[test]
    fn decoding_terminates_and_bad_input_is_told_what_to_paste() {
        // `%2525…25` re-encodes a `%` at every round; the cap is what stops it.
        let bomb = "%25".repeat(200);
        let out = ok("sharepoint-url-decoder", &[("url", &bomb)]);
        assert!(out.contains("Readable link"), "{out}");
        assert!(decode_until_stable("%2520").contains(' '));
        assert!(run("sharepoint-url-decoder", &[("url", "   ")]).is_err());
    }
}
