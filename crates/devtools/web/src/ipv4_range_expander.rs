//! `ipv4-range-expander`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};
use std::fmt::Write as _;
use std::net::Ipv4Addr;

pub(crate) struct RangeExpansion {
    pub(crate) old_size: u64,
    pub(crate) new_start: Ipv4Addr,
    pub(crate) new_end: Ipv4Addr,
    pub(crate) new_cidr: String,
    pub(crate) new_size: u64,
}

/// Port of `calculateCidr` — grow the range to the smallest enclosing CIDR by
/// finding the first differing bit of start and end.
pub(crate) fn calculate_cidr(start_ip: Ipv4Addr, end_ip: Ipv4Addr) -> Result<RangeExpansion> {
    let start = u32::from(start_ip);
    let end = u32::from(end_ip);
    if end < start {
        bail!("end address must not be lower than the start address");
    }
    let mask = (0..32u32)
        .find(|i| (start >> (31 - i)) & 1 != (end >> (31 - i)) & 1)
        .unwrap_or(32);
    let host_bits = 32 - mask;
    let (new_start, new_end) = if host_bits == 32 {
        (0u32, u32::MAX)
    } else {
        let lo = start & (!0u32 << host_bits);
        (lo, lo | ((1u32 << host_bits) - 1))
    };
    Ok(RangeExpansion {
        old_size: u64::from(end - start) + 1,
        new_start: Ipv4Addr::from(new_start),
        new_end: Ipv4Addr::from(new_end),
        new_cidr: format!("{}/{mask}", Ipv4Addr::from(new_start)),
        new_size: u64::from(new_end - new_start) + 1,
    })
}

pub(crate) fn ipv4_range_expander(args: &Args) -> Result<String> {
    let t = tool("ipv4-range-expander");
    let (start, end) = (
        parse_ipv4(arg(args, t, "start_ip"))?,
        parse_ipv4(arg(args, t, "end_ip"))?,
    );
    let r = calculate_cidr(start, end)?;
    // it-tools renders an old value / new value table with exactly these rows.
    let mut out = String::from("                     old value  ->  new value\n");
    for (label, old, new) in [
        ("Start address", start.to_string(), r.new_start.to_string()),
        ("End address", end.to_string(), r.new_end.to_string()),
        (
            "Addresses in range",
            r.old_size.to_string(),
            r.new_size.to_string(),
        ),
        ("CIDR", String::new(), r.new_cidr.clone()),
    ] {
        let _ = writeln!(out, "{label:<18}  {old:>11}  ->  {new}");
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "ipv4-range-expander",
    name: "IPv4 range expander",
    description: "Given a start and an end IPv4 address, compute the smallest CIDR that contains the range.",
    category: Category::Network,
    keywords: &["ipv4", "range", "expander", "subnet", "creator", "cidr"],
    fields: &[
        Field {
            key: "start_ip",
            label: "Start address",
            kind: crate::FieldKind::Text,
            default: "192.168.1.1",
            options: &[],
        },
        Field {
            key: "end_ip",
            label: "End address",
            kind: crate::FieldKind::Text,
            default: "192.168.6.255",
            options: &[],
        },
    ],
    kind: Kind::Pure,
    run: ipv4_range_expander,
};
