//! `ipv6-address-converter` — it-tools PR #870.

use crate::*;
use anyhow::{Context, Result};
use std::fmt::Write as _;
use std::net::Ipv6Addr;

/// The full eight-group form, no `::` and no dropped leading zeros — what a log
/// grep or a regex needs, and what `Display` never gives you.
pub(crate) fn expand(ip: Ipv6Addr) -> String {
    ip.segments()
        .iter()
        .map(|g| format!("{g:04x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// The `ip6.arpa` name, nibble by nibble in reverse — the PTR you have to type
/// by hand often enough to want it generated.
pub(crate) fn arpa(ip: Ipv6Addr) -> String {
    let mut out = String::new();
    for nibble in format!("{:032x}", u128::from(ip)).chars().rev() {
        let _ = write!(out, "{nibble}.");
    }
    out.push_str("ip6.arpa");
    out
}

fn ipv6_convert(a: &Args) -> Result<String> {
    let raw = s(a, "address", "2001:db8::1").trim();
    let ip: Ipv6Addr = raw
        .parse()
        .with_context(|| format!("not an IPv6 address: {raw:?}"))?;
    let n = u128::from(ip);
    let mut forms = format!(
        "compressed: {ip}\nexpanded: {}\ninteger: {n}\nhex: 0x{n:032x}\n",
        expand(ip)
    );
    if let Some(v4) = ip.to_ipv4_mapped() {
        let _ = writeln!(forms, "IPv4-mapped: {v4}");
    }
    Ok([
        section("Forms", Ok(forms)),
        section("Reverse DNS", Ok(format!("{}\n", arpa(ip)))),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "ipv6-address-converter",
    name: "IPv6 address converter",
    description: "Compressed, expanded, integer, hex and the ip6.arpa reverse name.",
    category: Category::Network,
    keywords: &[
        "ipv6", "expand", "compress", "arpa", "reverse", "ptr", "hex",
    ],
    fields: &[Field::text("address", "IPv6 address").with_default("2001:db8::1")],
    kind: Kind::Pure,
    run: ipv6_convert,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn every_form_of_one_address() {
        let out = ok("ipv6-address-converter", &[]);
        assert!(out.contains("compressed: 2001:db8::1"), "{out}");
        assert!(
            out.contains("expanded: 2001:0db8:0000:0000:0000:0000:0000:0001"),
            "{out}"
        );
        assert!(
            out.contains("integer: 42540766411282592856903984951653826561"),
            "{out}"
        );
        assert!(
            out.contains(
                "1.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.0.8.b.d.0.1.0.0.2.ip6.arpa"
            ),
            "{out}"
        );
        // A mapped address says so.
        let mapped = ok(
            "ipv6-address-converter",
            &[("address", "::ffff:192.0.2.128")],
        );
        assert!(mapped.contains("IPv4-mapped: 192.0.2.128"), "{mapped}");
        assert!(run("ipv6-address-converter", &[("address", "10.0.0.1")]).is_err());
    }
}
