//! `user-agent-parser`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg, section};
use anyhow::{Result, bail};

/// Version digits following `token` in the user agent, e.g. `WebKit/537.36`.
pub(crate) fn ua_version_after(ua: &str, token: &str) -> Option<String> {
    let i = ua.find(token)? + token.len();
    let v: String = ua[i..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    (!v.is_empty()).then_some(v)
}

/// ponytail: woothee reports no engine and no CPU, but it-tools' ua-parser-js
/// does and shows both as rows, so read the usual tokens straight off the
/// string. Upgrade path: a real ua-parser-js port, if the rows ever need to be
/// exact for exotic clients.
pub(crate) fn ua_engine(ua: &str) -> Option<(&'static str, Option<String>)> {
    if ua.contains("Chrome/") || ua.contains("Chromium/") || ua.contains("Edg/") {
        return Some(("Blink", ua_version_after(ua, "Chrome/")));
    }
    for (token, name) in [
        ("Trident/", "Trident"),
        ("Presto/", "Presto"),
        ("EdgeHTML/", "EdgeHTML"),
    ] {
        if ua.contains(token) {
            return Some((name, ua_version_after(ua, token)));
        }
    }
    if ua.contains("Gecko/") && ua.contains("rv:") {
        return Some(("Gecko", ua_version_after(ua, "rv:")));
    }
    if ua.contains("AppleWebKit/") {
        return Some(("WebKit", ua_version_after(ua, "AppleWebKit/")));
    }
    None
}

pub(crate) fn ua_cpu(ua: &str) -> Option<&'static str> {
    let u = ua.to_lowercase();
    [
        ("x86_64", "amd64"),
        ("win64", "amd64"),
        ("wow64", "amd64"),
        ("amd64", "amd64"),
        ("x64", "amd64"),
        ("aarch64", "arm64"),
        ("arm64", "arm64"),
        ("armv", "arm"),
        ("i686", "ia32"),
        ("i386", "ia32"),
        ("ppc64", "ppc64"),
    ]
    .into_iter()
    .find(|(needle, _)| u.contains(needle))
    .map(|(_, arch)| arch)
}

/// woothee spells "no idea" several ways; it-tools shows a per-row fallback.
pub(crate) fn ua_row(label: &str, value: Option<&str>, fallback: &str) -> String {
    let v = match value.map(str::trim) {
        Some(s) if !s.is_empty() && s != "UNKNOWN" => s,
        _ => fallback,
    };
    format!("  {label}: {v}\n")
}

pub(crate) fn user_agent_parser(args: &Args) -> Result<String> {
    let ua = arg(args, tool("user-agent-parser"), "user_agent").trim();
    if ua.is_empty() {
        bail!("empty user agent");
    }
    // An unparseable UA still yields engine and CPU rows, so no `?` here.
    let parsed = woothee::parser::Parser::new().parse(ua);
    let p = parsed.as_ref();
    let engine = ua_engine(ua);
    let mut out = String::new();
    out.push_str(&section(
        "Browser",
        Ok(format!(
            "{}{}",
            ua_row("Name", p.map(|r| r.name), "No browser name available"),
            ua_row(
                "Version",
                p.map(|r| r.version),
                "No browser version available"
            ),
        )),
    ));
    out.push_str(&section(
        "Engine",
        Ok(format!(
            "{}{}",
            ua_row(
                "Name",
                engine.as_ref().map(|(n, _)| *n),
                "No engine name available"
            ),
            ua_row(
                "Version",
                engine.as_ref().and_then(|(_, v)| v.as_deref()),
                "No engine version available"
            ),
        )),
    ));
    out.push_str(&section(
        "OS",
        Ok(format!(
            "{}{}",
            ua_row("Name", p.map(|r| r.os), "No OS name available"),
            ua_row(
                "Version",
                p.map(|r| r.os_version.as_ref()),
                "No OS version available"
            ),
        )),
    ));
    out.push_str(&section(
        "Device",
        Ok(format!(
            "{}{}{}",
            // woothee has no device model; `category` is its device type.
            ua_row("Model", None, "No device model available"),
            ua_row("Type", p.map(|r| r.category), "No device type available"),
            ua_row("Vendor", p.map(|r| r.vendor), "No device vendor available"),
        )),
    ));
    out.push_str(&section(
        "CPU",
        Ok(ua_row(
            "Architecture",
            ua_cpu(ua),
            "No CPU architecture available",
        )),
    ));
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "user-agent-parser",
    name: "User-agent parser",
    description: "Detect browser, engine, OS and device from a user-agent string.",
    category: Category::Web,
    keywords: &[
        "user",
        "agent",
        "parser",
        "browser",
        "engine",
        "os",
        "cpu",
        "device",
        "user-agent",
        "client",
    ],
    fields: &[Field::area("user_agent", "User agent string")],
    kind: Kind::Pure,
    run: user_agent_parser,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn user_agent_parser_emits_every_it_tools_section() {
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";
        let out = run("user-agent-parser", &[("user_agent", ua)]).unwrap();
        for section in ["Browser:", "Engine:", "OS:", "Device:", "CPU:"] {
            assert!(out.contains(section), "missing {section} in {out}");
        }
        assert!(out.contains("  Name: Chrome"), "{out}");
        assert!(out.contains("  Version: 120.0.0.0"), "{out}");
        assert!(out.contains("  Name: Blink"), "{out}");
        assert!(out.contains("  Name: Windows 10"), "{out}");
        assert!(out.contains("  Architecture: amd64"), "{out}");
        assert!(out.contains("  Model: No device model available"), "{out}");
        // An unparseable UA still fills the engine/CPU rows it can.
        let out = run("user-agent-parser", &[("user_agent", "!!!!")]).unwrap();
        assert!(out.contains("No engine name available"), "{out}");
        assert!(run("user-agent-parser", &[("user_agent", "   ")]).is_err());
    }
}
