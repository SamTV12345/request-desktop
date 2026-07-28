//! `cidr-to-ip-range` — it-tools PR #871.

use crate::*;
use anyhow::{Context, Result};
use ipnet::IpNet;
use std::fmt::Write as _;

/// One CIDR, spelled out. Shared with [`crate::ip_range_to_cidr`]'s output so
/// the two directions read the same.
pub(crate) fn describe(net: &IpNet) -> String {
    let hosts = match net {
        IpNet::V4(n) => u128::from(u32::from(n.broadcast()) - u32::from(n.network())) + 1,
        IpNet::V6(n) => {
            let (lo, hi) = (u128::from(n.network()), u128::from(n.broadcast()));
            // A /0 spans 2^128 addresses, one more than u128 can hold; say so
            // rather than wrapping to zero.
            if lo == 0 && hi == u128::MAX {
                return format!(
                    "{net}\n  first: {}\n  last:  {}\n  addresses: 2^128\n",
                    n.network(),
                    n.broadcast()
                );
            }
            hi - lo + 1
        }
    };
    format!(
        "{net}\n  first: {}\n  last:  {}\n  addresses: {hosts}\n",
        net.network(),
        net.broadcast()
    )
}

fn cidr_to_range(a: &Args) -> Result<String> {
    let input = s(a, "cidrs", DEFAULT);
    let mut out = String::new();
    for line in lines(input) {
        let net: IpNet = line
            .parse()
            .with_context(|| format!("not a CIDR block: {line:?}"))?;
        let _ = write!(out, "{}", describe(&net));
    }
    if out.is_empty() {
        anyhow::bail!("enter one CIDR block per line, for example 192.168.1.0/24");
    }
    Ok(out)
}

const DEFAULT: &str = "192.168.1.0/24\n2001:db8::/48";

pub static TOOL: Tool = Tool {
    id: "cidr-to-ip-range",
    name: "CIDR to IP range",
    description: "First and last address of a CIDR block, and how many it holds. IPv4 and IPv6.",
    category: Category::Network,
    keywords: &["cidr", "range", "subnet", "ip", "ipv4", "ipv6", "network"],
    fields: &[Field::area("cidrs", "CIDR blocks, one per line").with_default(DEFAULT)],
    kind: Kind::Pure,
    run: cidr_to_range,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn spells_out_both_families() {
        let out = ok(
            "cidr-to-ip-range",
            &[("cidrs", "10.0.0.0/30\n2001:db8::/126")],
        );
        assert!(out.contains("first: 10.0.0.0"), "{out}");
        assert!(out.contains("last:  10.0.0.3"), "{out}");
        assert!(out.contains("addresses: 4"), "{out}");
        assert!(out.contains("first: 2001:db8::"), "{out}");
        // A single host is a /32, and still one address.
        let one = ok("cidr-to-ip-range", &[("cidrs", "1.2.3.4/32")]);
        assert!(one.contains("addresses: 1"), "{one}");
        // The whole IPv6 space would overflow a u128 count.
        let all = ok("cidr-to-ip-range", &[("cidrs", "::/0")]);
        assert!(all.contains("2^128"), "{all}");
        assert!(run("cidr-to-ip-range", &[("cidrs", "nope")]).is_err());
    }
}
