//! `subnet-exclude-calculator` — it-tools PR #1451.

use crate::*;
use anyhow::{Context, Result, bail};
use ipnet::IpNet;
use std::fmt::Write as _;

/// `network` minus every block in `holes`, as the fewest CIDR blocks.
///
/// The standard split: a block that overlaps a hole but is not swallowed by it
/// is halved and both halves reconsidered, so the result is already minimal and
/// aligned — no merge pass needed.
pub(crate) fn exclude(network: IpNet, holes: &[IpNet]) -> Vec<IpNet> {
    let mut out = Vec::new();
    let mut stack = vec![network];
    while let Some(net) = stack.pop() {
        if holes.iter().any(|h| h.contains(&net)) {
            continue; // fully excluded
        }
        if !holes.iter().any(|h| net.contains(h)) {
            out.push(net); // untouched
            continue;
        }
        // Overlaps without being contained: split and look again.
        match net.subnets(net.prefix_len() + 1) {
            Ok(halves) => stack.extend(halves),
            // Already a single address and still overlapping — it is a hole.
            Err(_) => continue,
        }
    }
    out.sort_by_key(|n| (n.network(), n.prefix_len()));
    out
}

fn subnet_exclude(a: &Args) -> Result<String> {
    let raw = s(a, "network", "10.0.0.0/16").trim();
    let network: IpNet = raw
        .parse()
        .with_context(|| format!("not a CIDR block: {raw:?}"))?;
    let mut holes = Vec::new();
    for line in lines(s(a, "exclude", "10.0.7.0/24")) {
        let hole: IpNet = line
            .parse()
            .with_context(|| format!("not a CIDR block: {line:?}"))?;
        if hole.addr().is_ipv4() != network.addr().is_ipv4() {
            bail!("{hole} is not the same address family as {network}");
        }
        holes.push(hole);
    }
    let left = exclude(network, &holes);
    let mut out = String::new();
    for net in &left {
        let _ = writeln!(out, "{net}");
    }
    if left.is_empty() {
        out.push_str("(nothing left — the exclusions cover the whole network)\n");
    }
    Ok(format!(
        "{} block(s) left of {network}\n\n{out}",
        left.len()
    ))
}

pub static TOOL: Tool = Tool {
    id: "subnet-exclude-calculator",
    name: "Subnet exclusion calculator",
    description: "Everything in a network except the blocks you carve out, as the fewest CIDRs.",
    category: Category::Network,
    keywords: &[
        "subnet", "exclude", "split", "cidr", "carve", "firewall", "route",
    ],
    fields: &[
        Field::text("network", "Network").with_default("10.0.0.0/16"),
        Field::area("exclude", "Blocks to exclude, one per line").with_default("10.0.7.0/24"),
    ],
    kind: Kind::Pure,
    run: subnet_exclude,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn carves_a_hole_and_stays_minimal() {
        let out = ok("subnet-exclude-calculator", &[]);
        // 10.0.0.0/16 minus 10.0.7.0/24 is eight aligned blocks.
        assert!(out.contains("8 block(s)"), "{out}");
        for expected in ["10.0.0.0/22", "10.0.4.0/23", "10.0.6.0/24", "10.0.8.0/21"] {
            assert!(out.contains(expected), "{expected} missing from {out}");
        }
        assert!(!out.contains("10.0.7.0/24"), "the hole must be gone: {out}");
        // Excluding the network itself leaves nothing.
        let none = ok(
            "subnet-exclude-calculator",
            &[("network", "10.0.0.0/24"), ("exclude", "10.0.0.0/24")],
        );
        assert!(
            none.contains("0 block(s)") && none.contains("nothing left"),
            "{none}"
        );
        // A hole outside the network changes nothing.
        let all = ok(
            "subnet-exclude-calculator",
            &[("network", "10.0.0.0/24"), ("exclude", "192.168.0.0/24")],
        );
        assert!(
            all.contains("1 block(s)") && all.contains("10.0.0.0/24"),
            "{all}"
        );
        assert!(
            run("subnet-exclude-calculator", &[("exclude", "2001:db8::/48")]).is_err(),
            "mixed families are a mistake, not an empty result"
        );
    }
}
