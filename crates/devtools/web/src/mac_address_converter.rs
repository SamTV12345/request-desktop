//! `mac-address-converter` — it-tools PR #867.

use crate::*;
use anyhow::{Result, bail};
use std::net::Ipv6Addr;

/// The six bytes of a MAC, whatever notation it arrived in.
pub(crate) fn mac_bytes(raw: &str) -> Result<[u8; 6]> {
    let hex: String = raw
        .chars()
        .filter(|c| !matches!(c, ':' | '-' | '.' | ' ' | '_'))
        .collect();
    if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("a MAC address is 12 hexadecimal digits, got {}", hex.len());
    }
    let mut out = [0u8; 6];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("checked above");
    }
    Ok(out)
}

/// The EUI-64 interface identifier: split in the middle, insert `fffe`, flip the
/// universal/local bit. This is how a NIC becomes an IPv6 address.
fn eui64(mac: [u8; 6]) -> [u8; 8] {
    [
        mac[0] ^ 0b0000_0010,
        mac[1],
        mac[2],
        0xff,
        0xfe,
        mac[3],
        mac[4],
        mac[5],
    ]
}

fn mac_convert(a: &Args) -> Result<String> {
    let mac = mac_bytes(s(a, "mac", "00:1A:2B:3C:4D:5E"))?;
    let hex: Vec<String> = mac.iter().map(|b| format!("{b:02x}")).collect();
    let plain = hex.concat();
    let dotted = format!("{}.{}.{}", &plain[..4], &plain[4..8], &plain[8..]);
    let id = eui64(mac);
    let link_local = Ipv6Addr::from([
        0xfe, 0x80, 0, 0, 0, 0, 0, 0, id[0], id[1], id[2], id[3], id[4], id[5], id[6], id[7],
    ]);

    Ok([
        section(
            "Notations",
            Ok(format!(
                "colon: {}\nhyphen: {}\nCisco dotted: {dotted}\nbare: {plain}\nupper case: {}\n",
                hex.join(":"),
                hex.join("-"),
                hex.join(":").to_uppercase(),
            )),
        ),
        section(
            "Derived",
            Ok(format!(
                "OUI (vendor prefix): {}\nEUI-64: {}\nIPv6 link-local: {link_local}\nlocally administered: {}\nmulticast: {}\n",
                hex[..3].join(":").to_uppercase(),
                id.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":"),
                if mac[0] & 0b10 != 0 { "yes" } else { "no" },
                if mac[0] & 1 != 0 { "yes" } else { "no" },
            )),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "mac-address-converter",
    name: "MAC address converter",
    description: "Every notation of a MAC — colon, hyphen, Cisco dotted — plus its EUI-64 and link-local address.",
    category: Category::Network,
    keywords: &[
        "mac",
        "eui64",
        "cisco",
        "notation",
        "convert",
        "link-local",
        "oui",
    ],
    fields: &[Field::text("mac", "MAC address").with_default("00:1A:2B:3C:4D:5E")],
    kind: Kind::Pure,
    run: mac_convert,
};

#[cfg(test)]
mod tests {
    use crate::testing::*;

    #[test]
    fn all_notations_of_one_address() {
        let out = ok("mac-address-converter", &[]);
        assert!(out.contains("colon: 00:1a:2b:3c:4d:5e"), "{out}");
        assert!(out.contains("hyphen: 00-1a-2b-3c-4d-5e"), "{out}");
        assert!(out.contains("Cisco dotted: 001a.2b3c.4d5e"), "{out}");
        assert!(out.contains("OUI (vendor prefix): 00:1A:2B"), "{out}");
        // The u/l bit flip is the whole point of EUI-64.
        assert!(out.contains("EUI-64: 02:1a:2b:ff:fe:3c:4d:5e"), "{out}");
        assert!(
            out.contains("IPv6 link-local: fe80::21a:2bff:fe3c:4d5e"),
            "{out}"
        );
        assert!(out.contains("multicast: no"), "{out}");
        // Any notation parses, including bare and Cisco.
        for form in ["001a2b3c4d5e", "001a.2b3c.4d5e", "00-1A-2B-3C-4D-5E"] {
            assert!(ok("mac-address-converter", &[("mac", form)]).contains("00:1a:2b:3c:4d:5e"));
        }
        assert!(run("mac-address-converter", &[("mac", "00:1a:2b")]).is_err());
    }
}
