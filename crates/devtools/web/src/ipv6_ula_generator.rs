//! `ipv6-ula-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result};
use std::fmt::Write as _;

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

pub(crate) fn parse_mac(raw: &str) -> Result<macaddr::MacAddr6> {
    raw.trim()
        .parse::<macaddr::MacAddr6>()
        .with_context(|| format!("invalid MAC address {:?}", raw.trim()))
}

pub(crate) fn ipv6_ula_generator(args: &Args) -> Result<String> {
    let mac = parse_mac(arg(args, tool("ipv6-ula-generator"), "mac"))?;
    // RFC 4193 method 1: sha1(timestamp + EUI-64/MAC), keep the low 40 bits.
    use sha1::Digest as _;
    let seed = format!("{}{}", chrono::Utc::now().timestamp_millis(), mac);
    let digest = hex(&sha1::Sha1::digest(seed.as_bytes()));
    let h = &digest[30..];
    let ula = format!("fd{}:{}:{}", &h[0..2], &h[2..6], &h[6..10]);
    Ok(format!(
        "IPv6 ULA: {ula}::/48\nFirst routable block: {ula}:0::/64\nLast routable block: {ula}:ffff::/64\n"
    ))
}

pub static TOOL: Tool = Tool {
    id: "ipv6-ula-generator",
    name: "IPv6 ULA generator",
    description: "Generate a random RFC4193 IPv6 unique local address from a MAC address.",
    category: Category::Network,
    keywords: &["ipv6", "ula", "generator", "rfc4193", "network", "private"],
    fields: &[Field {
        key: "mac",
        label: "MAC address",
        kind: crate::FieldKind::Text,
        default: "20:37:06:12:34:56",
        options: &[],
    }],
    kind: Kind::Pure,
    run: ipv6_ula_generator,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn ula_generator_shape() {
        let out = run("ipv6-ula-generator", &[("mac", "20:37:06:12:34:56")]).unwrap();
        assert!(out.starts_with("IPv6 ULA: fd"), "{out}");
        assert!(out.contains("::/48"), "{out}");
        assert!(out.contains(":0::/64"), "{out}");
        assert!(out.contains(":ffff::/64"), "{out}");
        assert!(run("ipv6-ula-generator", &[("mac", "nope")]).is_err());
    }
}
