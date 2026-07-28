//! `punycode-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, section};
use anyhow::{Result, bail};

// it-tools PR #1258.

pub(crate) fn punycode(a: &Args) -> Result<String> {
    let input = s(a, "domain", "münchen.example").trim();
    if input.is_empty() {
        bail!("enter a domain name");
    }
    let ascii = idna::domain_to_ascii(input).map_err(|e| anyhow::anyhow!("{e}"));
    let (unicode, res) = idna::domain_to_unicode(input);
    Ok([
        section("Punycode (ASCII, what DNS sees)", ascii),
        section(
            "Unicode (what the user sees)",
            res.map(|()| unicode).map_err(|e| anyhow::anyhow!("{e}")),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "punycode-converter",
    name: "Punycode converter",
    description: "Internationalised domain names to the ASCII DNS actually carries, and back.",
    category: Category::Web,
    keywords: &["punycode", "idn", "domain", "unicode", "dns", "homograph"],
    fields: &[Field::text("domain", "Domain").with_default("münchen.example")],
    kind: Kind::Pure,
    run: punycode,
};
