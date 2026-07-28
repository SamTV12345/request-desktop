//! `ipv4-subnet-calculator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;
use std::net::Ipv4Addr;

pub(crate) struct Subnet {
    pub(crate) net: Ipv4Net,
    pub(crate) size: u64,
    pub(crate) first: Ipv4Addr,
    pub(crate) last: Ipv4Addr,
    pub(crate) broadcast: Option<Ipv4Addr>,
}

use ipnet::Ipv4Net;

/// Accepts `1.2.3.4`, `1.2.3.4/24` and `1.2.3.4/255.255.255.0`, like
/// netmask.js does. No mask means /32.
pub(crate) fn parse_subnet(raw: &str) -> Result<Subnet> {
    let s = raw.trim();
    let (ip_part, mask_part) = match s.split_once('/') {
        Some((a, b)) => (a, b.trim()),
        None => (s, "32"),
    };
    let ip = parse_ipv4(ip_part)?;
    let prefix = if mask_part.contains('.') {
        let mask = u32::from(parse_ipv4(mask_part)?);
        if mask.leading_ones() + mask.trailing_zeros() != 32 {
            bail!("{mask_part} is not a contiguous netmask");
        }
        mask.leading_ones() as u8
    } else {
        mask_part
            .parse::<u8>()
            .with_context(|| format!("invalid prefix length {mask_part:?}"))?
    };
    let net = Ipv4Net::new(ip, prefix).context("prefix length must be 0..=32")?;
    let base = u32::from(net.network());
    let size = 1u64 << (32 - prefix);
    // netmask.js: /31 and /32 have no broadcast and no host reservation.
    // The top of the block comes from the host mask rather than from `size`,
    // which is 2^32 for a /0 and truncates to 0 as a u32 — that overflowed.
    let top = base | u32::from(net.hostmask());
    let (first, last, broadcast) = if prefix <= 30 {
        (base + 1, top - 1, Some(Ipv4Addr::from(top)))
    } else {
        (base, top, None)
    };
    Ok(Subnet {
        net,
        size,
        first: Ipv4Addr::from(first),
        last: Ipv4Addr::from(last),
        broadcast,
    })
}

pub(crate) fn ip_class(ip: Ipv4Addr) -> &'static str {
    match ip.octets()[0] {
        0..=127 => "A",
        128..=191 => "B",
        192..=223 => "C",
        224..=239 => "D",
        _ => "E",
    }
}

/// What the address is reserved for, if anything (it-tools PR #1465).
///
/// The ranges come from `std`'s own predicates where they exist, so this cannot
/// drift from the ones `Ipv4Addr` already knows; only the two `std` still marks
/// unstable (shared address space and the reserved class E) are spelled out.
pub(crate) fn address_scope(ip: Ipv4Addr) -> &'static str {
    let o = ip.octets();
    if ip.is_unspecified() {
        "This network (0.0.0.0/8, RFC 1122)"
    } else if ip.is_loopback() {
        "Loopback (127.0.0.0/8, RFC 1122)"
    } else if ip.is_private() {
        "Private (RFC 1918)"
    } else if ip.is_link_local() {
        "Link-local (169.254.0.0/16, RFC 3927)"
    } else if o[0] == 100 && (64..128).contains(&o[1]) {
        "Shared address space / CGNAT (100.64.0.0/10, RFC 6598)"
    } else if ip.is_broadcast() {
        "Limited broadcast (255.255.255.255)"
    } else if ip.is_multicast() {
        "Multicast (224.0.0.0/4, RFC 5771)"
    } else if ip.is_documentation() {
        "Documentation / examples (RFC 5737)"
    } else if o[0] >= 240 {
        "Reserved, class E (240.0.0.0/4, RFC 1112)"
    } else {
        "Public"
    }
}

/// The `in-addr.arpa` name of the network (it-tools PR #969).
///
/// ponytail: only the octet-aligned prefixes get a real zone, because only they
/// have one — a /26 is delegated with the RFC 2317 `0-63.` trick, which is a
/// naming convention between two operators and not something derivable from the
/// prefix. Unaligned prefixes therefore get the enclosing zone, said so.
pub(crate) fn reverse_dns(net: Ipv4Net) -> String {
    let o = net.network().octets();
    let full = |n: usize| {
        let mut parts: Vec<String> = o[..n].iter().map(u8::to_string).collect();
        parts.reverse();
        parts.push("in-addr.arpa".into());
        parts.join(".")
    };
    let bits = net.prefix_len();
    let zone = full((bits / 8) as usize);
    if bits.is_multiple_of(8) {
        zone
    } else {
        format!("{zone} (enclosing zone; /{bits} is not octet-aligned, see RFC 2317)")
    }
}

pub(crate) fn ipv4_subnet_calculator(args: &Args) -> Result<String> {
    let s = parse_subnet(arg(args, tool("ipv4-subnet-calculator"), "ip"))?;
    let bits = s.net.prefix_len() as usize;
    let binary: String = ("1".repeat(bits) + &"0".repeat(32 - bits))
        .as_bytes()
        .chunks(8)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(".");
    let mut out = String::new();
    let _ = writeln!(out, "Netmask: {}/{}", s.net.network(), bits);
    let _ = writeln!(out, "Network address: {}", s.net.network());
    let _ = writeln!(out, "Network mask: {}", s.net.netmask());
    let _ = writeln!(out, "Network mask in binary: {binary}");
    let _ = writeln!(out, "CIDR notation: /{bits}");
    let _ = writeln!(out, "Wildcard mask: {}", s.net.hostmask());
    let _ = writeln!(out, "Network size: {}", s.size);
    let _ = writeln!(out, "First address: {}", s.first);
    let _ = writeln!(out, "Last address: {}", s.last);
    let _ = writeln!(
        out,
        "Broadcast address: {}",
        s.broadcast
            .map(|b| b.to_string())
            .unwrap_or_else(|| "No broadcast address with this mask".into())
    );
    let _ = writeln!(out, "IP class: {}", ip_class(s.net.network()));
    // Total addresses minus network and broadcast, except where there are none
    // to subtract: a /31 is a point-to-point link (RFC 3021) and a /32 is a host.
    let usable = match bits {
        0..=30 => s.size - 2,
        _ => s.size,
    };
    let _ = writeln!(
        out,
        "Usable hosts: {usable} (of {} addresses in total)",
        s.size
    );
    let _ = writeln!(out, "Address type: {}", address_scope(s.net.network()));
    let _ = writeln!(out, "Reverse DNS: {}", reverse_dns(s.net));
    let _ = writeln!(
        out,
        "IPv6 note: this tool is IPv4 only; use the IPv6 subnet calculator for v6 prefixes."
    );
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "ipv4-subnet-calculator",
    name: "IPv4 subnet calculator",
    description: "Parse your IPv4 CIDR blocks and get all the information about the subnet.",
    category: Category::Network,
    keywords: &[
        "ipv4",
        "subnet",
        "calculator",
        "mask",
        "network",
        "cidr",
        "netmask",
        "bitmask",
        "broadcast",
    ],
    fields: &[Field {
        key: "ip",
        label: "An IPv4 address with or without mask",
        kind: crate::FieldKind::Text,
        default: "192.168.0.1/24",
        options: &[],
    }],
    kind: Kind::Pure,
    run: ipv4_subnet_calculator,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn subnet_calculator_slash_24() {
        let out = run("ipv4-subnet-calculator", &[("ip", "192.168.0.1/24")]).unwrap();
        assert!(out.contains("Netmask: 192.168.0.0/24"), "{out}");
        assert!(out.contains("Network address: 192.168.0.0"), "{out}");
        assert!(out.contains("Network mask: 255.255.255.0"), "{out}");
        assert!(
            out.contains("Network mask in binary: 11111111.11111111.11111111.00000000"),
            "{out}"
        );
        assert!(out.contains("Wildcard mask: 0.0.0.255"), "{out}");
        assert!(out.contains("Network size: 256"), "{out}");
        assert!(out.contains("First address: 192.168.0.1"), "{out}");
        assert!(out.contains("Last address: 192.168.0.254"), "{out}");
        assert!(out.contains("Broadcast address: 192.168.0.255"), "{out}");
        assert!(out.contains("IP class: C"), "{out}");
        assert!(
            out.contains("Usable hosts: 254 (of 256 addresses in total)"),
            "{out}"
        );
        assert!(out.contains("Address type: Private (RFC 1918)"), "{out}");
        assert!(
            out.contains("Reverse DNS: 0.168.192.in-addr.arpa\n"),
            "{out}"
        );
        assert!(out.contains("use the IPv6 subnet calculator"), "{out}");
    }

    #[test]
    fn scope_hosts_and_reverse_dns() {
        let usable = |cidr: &str| {
            let out = ok("ipv4-subnet-calculator", &[("ip", cidr)]);
            out.lines()
                .find_map(|l| l.strip_prefix("Usable hosts: "))
                .map(str::to_string)
                .unwrap_or_default()
        };
        // RFC 3021 point-to-point and a single host: nothing to reserve.
        assert_eq!(usable("10.0.0.0/31"), "2 (of 2 addresses in total)");
        assert_eq!(usable("10.0.0.0/32"), "1 (of 1 addresses in total)");
        assert_eq!(usable("10.0.0.0/30"), "2 (of 4 addresses in total)");

        for (ip, want) in [
            ("127.0.0.1/8", "Loopback"),
            ("172.16.5.0/24", "Private"),
            ("169.254.1.0/24", "Link-local"),
            ("224.0.0.1/32", "Multicast"),
            ("100.64.0.0/10", "Shared address space"),
            ("245.0.0.0/8", "Reserved, class E"),
            ("8.8.8.8/32", "Public"),
        ] {
            let out = ok("ipv4-subnet-calculator", &[("ip", ip)]);
            assert!(
                out.contains(&format!("Address type: {want}")),
                "{ip}: {out}"
            );
        }

        let out = ok("ipv4-subnet-calculator", &[("ip", "10.0.0.0/8")]);
        assert!(out.contains("Reverse DNS: 10.in-addr.arpa\n"), "{out}");
        let out = ok("ipv4-subnet-calculator", &[("ip", "10.0.0.0/26")]);
        assert!(
            out.contains("Reverse DNS: 0.0.10.in-addr.arpa (enclosing zone; /26 is not"),
            "{out}"
        );
        let out = ok("ipv4-subnet-calculator", &[("ip", "0.0.0.0/0")]);
        assert!(out.contains("Reverse DNS: in-addr.arpa\n"), "{out}");
    }

    #[test]
    fn subnet_calculator_edge_masks() {
        let s = parse_subnet("192.168.0.1/31").unwrap();
        assert_eq!(s.size, 2);
        assert_eq!(s.first.to_string(), "192.168.0.0");
        assert_eq!(s.last.to_string(), "192.168.0.1");
        assert!(s.broadcast.is_none());

        let s = parse_subnet("192.168.0.1/32").unwrap();
        assert_eq!(s.size, 1);
        assert_eq!(s.first.to_string(), "192.168.0.1");
        assert_eq!(s.last.to_string(), "192.168.0.1");
        assert!(s.broadcast.is_none());

        // No mask means /32, like netmask.js.
        assert_eq!(parse_subnet("10.0.0.1").unwrap().net.prefix_len(), 32);
        // Dotted netmasks are accepted too.
        assert_eq!(
            parse_subnet("10.0.0.1/255.255.255.0")
                .unwrap()
                .net
                .prefix_len(),
            24
        );
        // /30 keeps host reservation.
        let s = parse_subnet("10.0.0.0/30").unwrap();
        assert_eq!(
            (s.first.to_string(), s.last.to_string()),
            ("10.0.0.1".into(), "10.0.0.2".into())
        );
        assert_eq!(s.broadcast.unwrap().to_string(), "10.0.0.3");

        // /0 used to overflow: 2^32 does not fit the u32 the arithmetic used.
        let s = parse_subnet("10.0.0.1/0").unwrap();
        assert_eq!(s.size, 1 << 32);
        assert_eq!(
            (s.first.to_string(), s.last.to_string()),
            ("0.0.0.1".into(), "255.255.255.254".into())
        );
        assert_eq!(s.broadcast.unwrap().to_string(), "255.255.255.255");

        assert!(parse_subnet("10.0.0.1/33").is_err());
        assert!(parse_subnet("10.0.0.1/255.0.255.0").is_err());
        assert!(parse_subnet("garbage").is_err());
    }

    #[test]
    fn ip_classes() {
        assert_eq!(ip_class("10.0.0.0".parse().unwrap()), "A");
        assert_eq!(ip_class("172.16.0.0".parse().unwrap()), "B");
        assert_eq!(ip_class("192.168.0.0".parse().unwrap()), "C");
        assert_eq!(ip_class("224.0.0.1".parse().unwrap()), "D");
        assert_eq!(ip_class("250.0.0.1".parse().unwrap()), "E");
    }
}
