//! `ip-range-to-cidr` — it-tools PR #869.

use crate::*;
use anyhow::{Context, Result, bail};
use ipnet::{IpNet, Ipv4Subnets, Ipv6Subnets};
use std::fmt::Write as _;
use std::net::IpAddr;

/// The smallest set of CIDR blocks that covers `start..=end` exactly.
///
/// `ipnet`'s `Subnets` iterators already do the aggregation; the work here is
/// only rejecting a mixed or reversed pair before they panic on it.
pub(crate) fn range_to_cidrs(start: IpAddr, end: IpAddr) -> Result<Vec<IpNet>> {
    if end < start {
        bail!("the end address is lower than the start address");
    }
    Ok(match (start, end) {
        (IpAddr::V4(a), IpAddr::V4(b)) => Ipv4Subnets::new(a, b, 0).map(IpNet::V4).collect(),
        (IpAddr::V6(a), IpAddr::V6(b)) => Ipv6Subnets::new(a, b, 0).map(IpNet::V6).collect(),
        _ => bail!("both addresses must be of the same family"),
    })
}

fn range_to_cidr(a: &Args) -> Result<String> {
    let parse = |key, default| -> Result<IpAddr> {
        let raw = s(a, key, default).trim();
        raw.parse()
            .with_context(|| format!("not an IP address: {raw:?}"))
    };
    let (start, end) = (
        parse("start", "192.168.1.5")?,
        parse("end", "192.168.1.20")?,
    );
    let nets = range_to_cidrs(start, end)?;
    let mut out = String::new();
    for net in &nets {
        let _ = writeln!(out, "{net}");
    }
    let _ = write!(out, "\n{} block(s) cover {start} … {end}\n", nets.len());
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "ip-range-to-cidr",
    name: "IP range to CIDR",
    description: "The smallest set of CIDR blocks that covers a first-to-last address range.",
    category: Category::Network,
    keywords: &["cidr", "range", "aggregate", "subnet", "ip", "ipv4", "ipv6"],
    fields: &[
        Field::text("start", "First address").with_default("192.168.1.5"),
        Field::text("end", "Last address").with_default("192.168.1.20"),
    ],
    kind: Kind::Pure,
    run: range_to_cidr,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn covers_the_range_exactly() {
        // The classic example: 5..20 is not one block.
        let out = ok("ip-range-to-cidr", &[]);
        for expected in [
            "192.168.1.5/32",
            "192.168.1.6/31",
            "192.168.1.8/29",
            "192.168.1.16/30",
            "192.168.1.20/32",
        ] {
            assert!(out.contains(expected), "{expected} missing from {out}");
        }
        assert!(out.contains("5 block(s)"), "{out}");
        // An aligned range collapses to one block.
        let one = ok(
            "ip-range-to-cidr",
            &[("start", "10.0.0.0"), ("end", "10.0.0.255")],
        );
        assert!(
            one.contains("10.0.0.0/24") && one.contains("1 block(s)"),
            "{one}"
        );
        // IPv6 works the same way; mixing families does not.
        let v6 = ok(
            "ip-range-to-cidr",
            &[("start", "2001:db8::"), ("end", "2001:db8::ff")],
        );
        assert!(v6.contains("2001:db8::/120"), "{v6}");
        assert!(run("ip-range-to-cidr", &[("start", "10.0.0.0"), ("end", "::1")]).is_err());
        assert!(
            run(
                "ip-range-to-cidr",
                &[("start", "10.0.0.9"), ("end", "10.0.0.1")]
            )
            .is_err()
        );
    }
}
