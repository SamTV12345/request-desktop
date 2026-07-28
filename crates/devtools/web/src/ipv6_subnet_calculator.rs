//! `ipv6-subnet-calculator` — it-tools PRs #963 / #944 / #1465.

use crate::*;
use anyhow::{Context, Result};
use ipnet::Ipv6Net;

/// `2^n`, written out while it fits and as a power once it does not. A /48 has
/// 2^80 addresses, which no integer type here can hold.
pub(crate) fn two_pow(n: u32) -> String {
    if n < 128 {
        (1u128 << n).to_string()
    } else {
        format!("2^{n}")
    }
}

fn ipv6_subnet(a: &Args) -> Result<String> {
    let raw = s(a, "cidr", "2001:db8:1234::/48").trim();
    let net: Ipv6Net = raw
        .parse()
        .with_context(|| format!("not an IPv6 CIDR block: {raw:?}"))?;
    let bits = 128 - u32::from(net.prefix_len());
    let sixty_fours = if net.prefix_len() <= 64 {
        two_pow(64 - u32::from(net.prefix_len()))
    } else {
        "0 (the prefix is longer than /64)".into()
    };
    Ok([
        section(
            "Network",
            Ok(format!(
                "cidr: {net}\nnetwork: {}\nprefix length: /{}\nnetmask: {}\n",
                net.network(),
                net.prefix_len(),
                net.netmask()
            )),
        ),
        section(
            "Range",
            Ok(format!(
                "first: {}\nlast:  {}\naddresses: {}\n",
                net.network(),
                net.broadcast(),
                two_pow(bits)
            )),
        ),
        section(
            "Subnets",
            Ok(format!(
                "/64 subnets inside: {sixty_fours}\nexpanded: {}\n",
                crate::ipv6_address_converter::expand(net.network())
            )),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "ipv6-subnet-calculator",
    name: "IPv6 subnet calculator",
    description: "Prefix, range, netmask and how many /64s an IPv6 block holds.",
    category: Category::Network,
    keywords: &["ipv6", "subnet", "prefix", "cidr", "netmask", "calculator"],
    fields: &[Field::text("cidr", "IPv6 CIDR").with_default("2001:db8:1234::/48")],
    kind: Kind::Pure,
    run: ipv6_subnet,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn describes_a_48_the_way_an_allocation_letter_does() {
        let out = ok("ipv6-subnet-calculator", &[]);
        assert!(out.contains("prefix length: /48"), "{out}");
        assert!(out.contains("first: 2001:db8:1234::"), "{out}");
        assert!(
            out.contains("last:  2001:db8:1234:ffff:ffff:ffff:ffff:ffff"),
            "{out}"
        );
        assert!(out.contains("/64 subnets inside: 65536"), "{out}");
        assert!(
            out.contains("addresses: 1208925819614629174706176"),
            "{out}"
        );
        // A /128 is one address and no /64s.
        let host = ok("ipv6-subnet-calculator", &[("cidr", "2001:db8::1/128")]);
        assert!(host.contains("addresses: 1"), "{host}");
        assert!(host.contains("/64 subnets inside: 0"), "{host}");
        // The whole space needs the power form.
        assert!(ok("ipv6-subnet-calculator", &[("cidr", "::/0")]).contains("2^128"));
        assert!(run("ipv6-subnet-calculator", &[("cidr", "10.0.0.0/8")]).is_err());
    }
}
