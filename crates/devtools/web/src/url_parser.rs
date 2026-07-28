//! `url-parser`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;

pub(crate) struct Url {
    pub(crate) protocol: String,
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) hostname: String,
    pub(crate) port: String,
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) fragment: String,
}

/// ponytail: hand-rolled instead of the `url` crate — not in the dep list and
/// the scheme://user:pass@host:port/path?q#frag split is 30 lines. Upgrade to
/// `url` if anyone needs IDNA or full WHATWG normalisation.
pub(crate) fn parse_url(raw: &str) -> Result<Url> {
    let s = raw.trim();
    if s.is_empty() {
        bail!("empty url");
    }
    let (scheme, rest) = s.split_once(':').context("missing scheme, e.g. https://")?;
    if scheme.is_empty()
        || !scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        bail!("invalid scheme {scheme:?}");
    }
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let (rest, fragment) = match rest.split_once('#') {
        Some((a, b)) => (a, b.to_string()),
        None => (rest, String::new()),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((a, b)) => (a, format!("?{b}")),
        None => (rest, String::new()),
    };
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (userinfo, host) = match authority.rsplit_once('@') {
        Some((u, h)) => (u, h),
        None => ("", authority),
    };
    let (username, password) = match userinfo.split_once(':') {
        Some((u, p)) => (u.to_string(), p.to_string()),
        None => (userinfo.to_string(), String::new()),
    };
    // IPv6 literals keep their brackets and hide the colons.
    let (hostname, port) = if let Some(end) = host.find(']') {
        (
            host[..=end].to_string(),
            host[end + 1..].trim_start_matches(':').to_string(),
        )
    } else {
        match host.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.to_string()),
            None => (host.to_string(), String::new()),
        }
    };
    if hostname.is_empty() {
        bail!("missing host");
    }
    if !port.is_empty() && port.parse::<u16>().is_err() {
        bail!("invalid port {port:?}");
    }
    Ok(Url {
        protocol: format!("{scheme}:"),
        username,
        password,
        hostname,
        port,
        path,
        query,
        fragment,
    })
}

pub(crate) fn decode_form_component(s: &str) -> String {
    let plus_free = s.replace('+', " ");
    percent_encoding::percent_decode_str(&plus_free)
        .decode_utf8_lossy()
        .into_owned()
}

pub(crate) fn url_parser(args: &Args) -> Result<String> {
    let u = parse_url(arg(args, tool("url-parser"), "url"))?;
    let mut out = String::new();
    let _ = writeln!(out, "Protocol: {}", u.protocol);
    let _ = writeln!(out, "Username: {}", u.username);
    let _ = writeln!(out, "Password: {}", u.password);
    let _ = writeln!(out, "Hostname: {}", u.hostname);
    let _ = writeln!(out, "Port: {}", u.port);
    let _ = writeln!(out, "Path: {}", u.path);
    let _ = writeln!(out, "Params: {}", u.query);
    let _ = writeln!(out, "Hash: {}", u.fragment);
    let pairs = u.query.trim_start_matches('?');
    if !pairs.is_empty() {
        let _ = writeln!(out, "\nQuery parameters:");
        // it-tools PR #873: `?a=1&a=2` is two values, not one — a map keyed by
        // name loses the second. So every occurrence gets its own line, in input
        // order, and repeated names carry an `[i/n]` marker so the repetition is
        // visible rather than looking like a duplicated line.
        //
        // Counting is two passes over a map rather than a scan per pair: a pasted
        // 1 MB query string is ~500k pairs, and the quadratic version takes
        // minutes.
        let raw: Vec<&str> = pairs.split('&').filter(|p| !p.is_empty()).collect();
        fn name(pair: &str) -> &str {
            pair.split_once('=').map_or(pair, |(k, _)| k)
        }
        let mut totals: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for pair in &raw {
            *totals.entry(name(pair)).or_default() += 1;
        }
        let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for pair in &raw {
            let k = name(pair);
            let n = totals.get(k).copied().unwrap_or(1);
            let marker = if n > 1 {
                let i = seen.entry(k).or_default();
                *i += 1;
                format!(" [{i}/{n}]")
            } else {
                String::new()
            };
            let key = decode_form_component(k);
            // `?flag` has no value at all; `?flag=` has an empty one. The two mean
            // different things to most servers, so they must not print alike.
            match pair.split_once('=') {
                Some((_, v)) => {
                    let _ = writeln!(out, "  {key}{marker} = {}", decode_form_component(v));
                }
                None => {
                    let _ = writeln!(out, "  {key}{marker} (no value)");
                }
            }
        }
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "url-parser",
    name: "URL parser",
    description: "Parse a url into its separate components (protocol, origin, params, port, username, password...).",
    category: Category::Web,
    keywords: &[
        "url", "parser", "protocol", "origin", "params", "port", "username", "password", "href",
    ],
    fields: &[Field {
        key: "url",
        label: "Your url to parse",
        kind: crate::FieldKind::Text,
        default: "https://me:pwd@it-tools.tech:3000/url-parser?key1=value&key2=value2#the-hash",
        options: &[],
    }],
    kind: Kind::Pure,
    run: url_parser,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn repeated_and_valueless_query_parameters() {
        let out = ok(
            "url-parser",
            &[("url", "https://x.tld/p?a=1&a=2&flag&empty=&a=3")],
        );
        assert!(out.contains("  a [1/3] = 1"), "{out}");
        assert!(out.contains("  a [2/3] = 2"), "{out}");
        assert!(out.contains("  a [3/3] = 3"), "{out}");
        // `?flag` and `?empty=` must not print alike.
        assert!(out.contains("  flag (no value)"), "{out}");
        assert!(out.contains("  empty = \n"), "{out}");
        // A name that appears once keeps the old, unmarked line.
        let out = ok("url-parser", &[("url", "https://x.tld/p?a=1&b=2")]);
        assert!(out.contains("  a = 1\n  b = 2"), "{out}");
    }
}
