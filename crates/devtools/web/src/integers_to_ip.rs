//! `integers-to-ip` — it-tools PR #1264.
//!
//! Both directions at once, because a database column full of `3232235777` and
//! a log line full of `192.168.1.1` are the same question asked twice.

use crate::*;
use anyhow::{Result, bail};
use std::net::{Ipv4Addr, Ipv6Addr};

fn integers_to_ip(a: &Args) -> Result<String> {
    let raw = s(a, "value", "3232235777").trim();
    if raw.is_empty() {
        bail!("enter an integer, or an IP address to turn into one");
    }
    let from_int = |n: u128| -> String {
        let mut out = String::new();
        if let Ok(v4) = u32::try_from(n) {
            out.push_str(&format!("IPv4: {}\n", Ipv4Addr::from(v4)));
        } else {
            out.push_str("IPv4: (out of range — an IPv4 address is at most 4294967295)\n");
        }
        out.push_str(&format!("IPv6: {}\n", Ipv6Addr::from(n)));
        out
    };
    // Accept 0x… and 0b… too: both turn up in packet dumps.
    let parsed = match raw.get(..2) {
        Some("0x" | "0X") => u128::from_str_radix(&raw[2..], 16).ok(),
        Some("0b" | "0B") => u128::from_str_radix(&raw[2..], 2).ok(),
        _ => raw.parse::<u128>().ok(),
    };
    if let Some(n) = parsed {
        return Ok(section("From the integer", Ok(from_int(n))));
    }
    let n = match raw.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => u128::from(u32::from(v4)),
        Ok(std::net::IpAddr::V6(v6)) => u128::from(v6),
        Err(_) => bail!("neither an integer nor an IP address: {raw:?}"),
    };
    Ok(section(
        "From the address",
        Ok(format!("integer: {n}\nhex: 0x{n:x}\n")),
    ))
}

pub static TOOL: Tool = Tool {
    id: "integers-to-ip",
    name: "Integer to IP, and back",
    description: "The 32- or 128-bit number behind an address — for database columns and packet dumps.",
    category: Category::Network,
    keywords: &[
        "integer", "ip", "ipv4", "ipv6", "decimal", "long", "convert",
    ],
    fields: &[Field::text("value", "Integer or IP address").with_default("3232235777")],
    kind: Kind::Pure,
    run: integers_to_ip,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn goes_both_ways() {
        let out = ok("integers-to-ip", &[]);
        assert!(out.contains("IPv4: 192.168.1.1"), "{out}");
        let back = ok("integers-to-ip", &[("value", "192.168.1.1")]);
        assert!(back.contains("integer: 3232235777"), "{back}");
        assert!(back.contains("hex: 0xc0a80101"), "{back}");
        // Hex in, address out.
        assert!(ok("integers-to-ip", &[("value", "0xc0a80101")]).contains("IPv4: 192.168.1.1"));
        // Too big for v4, still fine for v6.
        let big = ok("integers-to-ip", &[("value", "4294967296")]);
        assert!(
            big.contains("out of range") && big.contains("IPv6: ::1:0:0"),
            "{big}"
        );
        assert!(run("integers-to-ip", &[("value", "not an ip")]).is_err());
    }
}
