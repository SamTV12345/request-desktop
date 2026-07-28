//! `ip-in-cidr` — it-tools PR #993.

use crate::*;
use anyhow::{Context, Result};
use ipnet::IpNet;
use std::fmt::Write as _;
use std::net::IpAddr;

const DEFAULT_NETS: &str = "10.0.0.0/8\n172.16.0.0/12\n192.168.0.0/16\n2001:db8::/32";

fn ip_in_cidr(a: &Args) -> Result<String> {
    let raw = s(a, "ip", "10.0.5.7").trim();
    let ip: IpAddr = raw
        .parse()
        .with_context(|| format!("not an IP address: {raw:?}"))?;
    let mut hits = 0;
    let mut out = String::new();
    for line in lines(s(a, "cidrs", DEFAULT_NETS)) {
        let net: IpNet = line
            .parse()
            .with_context(|| format!("not a CIDR block: {line:?}"))?;
        // A different family is a clean "no", not an error: pasting a mixed
        // list of networks is the normal case.
        let inside = net.contains(&ip);
        hits += usize::from(inside);
        let _ = writeln!(out, "{} {net}", if inside { "yes" } else { " no" });
    }
    Ok(format!("{ip} is in {hits} of the listed networks\n\n{out}"))
}

pub static TOOL: Tool = Tool {
    id: "ip-in-cidr",
    name: "Is this IP in that network?",
    description: "Check one address against a list of CIDR blocks — allow-lists, VPC ranges, firewall rules.",
    category: Category::Network,
    keywords: &[
        "cidr",
        "contains",
        "match",
        "subnet",
        "ip",
        "firewall",
        "allowlist",
    ],
    fields: &[
        Field::text("ip", "Address").with_default("10.0.5.7"),
        Field::area("cidrs", "Networks, one per line").with_default(DEFAULT_NETS),
    ],
    kind: Kind::Pure,
    run: ip_in_cidr,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn answers_per_network_and_counts() {
        let out = ok("ip-in-cidr", &[]);
        assert!(out.contains("is in 1 of the listed networks"), "{out}");
        assert!(out.contains("yes 10.0.0.0/8"), "{out}");
        assert!(out.contains(" no 192.168.0.0/16"), "{out}");
        // A v6 address against a mixed list: the v4 rows are a plain "no".
        let v6 = ok("ip-in-cidr", &[("ip", "2001:db8::1")]);
        assert!(
            v6.contains("yes 2001:db8::/32") && v6.contains(" no 10.0.0.0/8"),
            "{v6}"
        );
        assert!(run("ip-in-cidr", &[("ip", "999.1.1.1")]).is_err());
        assert!(run("ip-in-cidr", &[("cidrs", "10.0.0.0/33")]).is_err());
    }
}
