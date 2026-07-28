//! `ipv4-address-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result};
use std::net::Ipv4Addr;

pub(crate) fn parse_ipv4(raw: &str) -> Result<Ipv4Addr> {
    raw.trim()
        .parse::<Ipv4Addr>()
        .with_context(|| format!("invalid ipv4 address {:?}", raw.trim()))
}

pub(crate) fn ipv4_to_ipv6(ip: Ipv4Addr, prefix: &str) -> String {
    let o = ip.octets();
    format!("{prefix}{:02x}{:02x}:{:02x}{:02x}", o[0], o[1], o[2], o[3])
}

pub(crate) fn ipv4_address_converter(args: &Args) -> Result<String> {
    let ip = parse_ipv4(arg(args, tool("ipv4-address-converter"), "ip"))?;
    let n = u32::from(ip);
    Ok(format!(
        "Decimal: {n}\nHexadecimal: {n:X}\nBinary: {n:b}\nIpv6: {}\nIpv6 (short): {}\n",
        ipv4_to_ipv6(ip, "0000:0000:0000:0000:0000:ffff:"),
        ipv4_to_ipv6(ip, "::ffff:"),
    ))
}

pub static TOOL: Tool = Tool {
    id: "ipv4-address-converter",
    name: "IPv4 address converter",
    description: "Convert an IP address into decimal, binary, hexadecimal or the IPv6 mapped form.",
    category: Category::Network,
    keywords: &[
        "ipv4",
        "address",
        "converter",
        "decimal",
        "hexadecimal",
        "binary",
        "ipv6",
    ],
    fields: &[Field {
        key: "ip",
        label: "The ipv4 address",
        kind: crate::FieldKind::Text,
        default: "192.168.1.1",
        options: &[],
    }],
    kind: Kind::Pure,
    run: ipv4_address_converter,
};
