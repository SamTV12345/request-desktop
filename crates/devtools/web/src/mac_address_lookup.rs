//! `mac-address-lookup`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};

// ponytail: a curated subset of the IEEE OUI registry — the real list is ~35k
// entries / several MB and has no business being compiled into the binary.
// Upgrade path if anyone actually needs full coverage: ship the IEEE `oui.csv`
// as a data file next to the app and load it lazily instead of growing this.
pub(crate) static OUI: &[(&str, &str)] = &[
    ("000000", "Xerox Corporation"),
    ("00000C", "Cisco Systems, Inc"),
    ("00005E", "IANA"),
    ("000142", "Cisco Systems, Inc"),
    ("00040B", "Cisco Systems, Inc"),
    ("00044B", "NVIDIA Corporation"),
    ("000255", "IBM Corporation"),
    ("000393", "Apple, Inc."),
    ("0003FF", "Microsoft Corporation"),
    ("000502", "Apple, Inc."),
    ("000569", "VMware, Inc."),
    ("0005C9", "LG Electronics"),
    ("000902", "Redline Communications"),
    ("00090F", "Fortinet Inc."),
    ("0009BF", "Nintendo Co., Ltd."),
    ("000A27", "Apple, Inc."),
    ("000AF7", "Broadcom Corporation"),
    ("000C29", "VMware, Inc."),
    ("000C41", "Cisco-Linksys, LLC"),
    ("000C42", "Routerboard.com (MikroTik)"),
    ("000D3A", "Microsoft Corporation"),
    ("000E58", "Sonos, Inc."),
    ("000E7F", "Hewlett Packard"),
    ("000FB5", "NETGEAR Inc."),
    ("001018", "Broadcom Corporation"),
    ("001124", "Apple, Inc."),
    ("0012FB", "Samsung Electronics"),
    ("001349", "Zyxel Communications Corporation"),
    ("0013A9", "Sony Corporation"),
    ("0013CE", "Intel Corporate"),
    ("001451", "Apple, Inc."),
    ("001517", "Intel Corporate"),
    ("00155D", "Microsoft Corporation (Hyper-V)"),
    ("0015B9", "Samsung Electronics"),
    ("001560", "Hewlett Packard"),
    ("001656", "Nintendo Co., Ltd."),
    ("001617", "Micro-Star INTL CO., LTD."),
    ("0016CB", "Apple, Inc."),
    ("001788", "Philips Lighting BV"),
    ("001839", "Cisco-Linksys, LLC"),
    ("00188B", "Dell Inc."),
    ("0019FD", "Nintendo Co., Ltd."),
    ("001A11", "Google, Inc."),
    ("001A64", "IBM Corporation"),
    ("001A70", "Cisco-Linksys, LLC"),
    ("001A80", "Sony Corporation"),
    ("001B21", "Intel Corporate"),
    ("001B2F", "NETGEAR Inc."),
    ("001B63", "Apple, Inc."),
    ("001B77", "Intel Corporate"),
    ("001BFC", "ASUSTek Computer Inc."),
    ("001C14", "VMware, Inc."),
    ("001C23", "Dell Inc."),
    ("001C25", "Hon Hai Precision Ind. Co., Ltd."),
    ("001C42", "Parallels, Inc."),
    ("001C4A", "AVM GmbH"),
    ("001C62", "LG Electronics"),
    ("001C73", "Arista Networks"),
    ("001CC4", "Hewlett Packard"),
    ("001CDF", "Belkin International Inc."),
    ("001CF0", "D-Link Corporation"),
    ("001D0F", "TP-LINK Technologies Co., Ltd."),
    ("001DB5", "Juniper Networks"),
    ("001DD8", "Microsoft Corporation"),
    ("001E10", "Huawei Technologies Co., Ltd"),
    ("001EC2", "Apple, Inc."),
    ("001EC9", "Dell Inc."),
    ("001FD0", "GIGA-BYTE Technology Co., Ltd."),
    ("002037", "Seagate Technology"),
    ("002215", "ASUSTek Computer Inc."),
    ("0022A9", "LG Electronics"),
    ("002332", "Apple, Inc."),
    ("00219B", "Dell Inc."),
    ("0021BD", "Nintendo Co., Ltd."),
    ("0021D1", "Samsung Electronics"),
    ("002436", "Apple, Inc."),
    ("002490", "Samsung Electronics"),
    ("002556", "Hon Hai Precision Ind. Co., Ltd."),
    ("002590", "Super Micro Computer, Inc."),
    ("00259C", "Cisco-Linksys, LLC"),
    ("00259E", "Huawei Technologies Co., Ltd"),
    ("002608", "Apple, Inc."),
    ("002722", "Ubiquiti Networks Inc."),
    ("003048", "Super Micro Computer, Inc."),
    ("00306E", "Hewlett Packard"),
    ("0050BA", "D-Link Corporation"),
    ("005056", "VMware, Inc."),
    ("0050F2", "Microsoft Corporation"),
    ("0080C2", "IEEE 802.1 Working Group"),
    ("009EC8", "Xiaomi Communications Co Ltd"),
    ("00A0C9", "Intel Corporation"),
    ("00E04C", "Realtek Semiconductor Corp."),
    ("00095B", "NETGEAR Inc."),
    ("000B86", "Aruba Networks"),
    ("00163E", "Xensource, Inc."),
    ("00037F", "Atheros Communications, Inc."),
    ("000FB0", "Compal Electronics, Inc."),
    ("040CCE", "Apple, Inc."),
    ("0418D6", "Ubiquiti Networks Inc."),
    ("080027", "PCS Systemtechnik GmbH (VirtualBox)"),
    ("080046", "Sony Corporation"),
    ("085B0E", "Fortinet Inc."),
    ("08863B", "Belkin International Inc."),
    ("0819A6", "Huawei Technologies Co., Ltd"),
    ("0C47C9", "Amazon Technologies Inc."),
    ("10683F", "LG Electronics"),
    ("14CC20", "TP-LINK Technologies Co., Ltd."),
    ("18B430", "Nest Labs Inc."),
    ("1C6F65", "GIGA-BYTE Technology Co., Ltd."),
    ("240AC4", "Espressif Inc."),
    ("246F28", "Espressif Inc."),
    ("24A43C", "Ubiquiti Networks Inc."),
    ("283CE4", "Huawei Technologies Co., Ltd"),
    ("286C07", "Xiaomi Communications Co Ltd"),
    ("2C56DC", "ASUSTek Computer Inc."),
    ("2C6BF5", "Juniper Networks"),
    ("30AEA4", "Espressif Inc."),
    ("30F9ED", "Sony Corporation"),
    ("3085A9", "ASUSTek Computer Inc."),
    ("347E5C", "Sonos, Inc."),
    ("34BE00", "Samsung Electronics"),
    ("3C0754", "Apple, Inc."),
    ("3C4A92", "Hewlett Packard"),
    ("3C5AB4", "Google, Inc."),
    ("3C8AB0", "Juniper Networks"),
    ("3CA62F", "AVM GmbH"),
    ("3CBDD8", "LG Electronics"),
    ("40B4CD", "Amazon Technologies Inc."),
    ("40D28A", "Nintendo Co., Ltd."),
    ("44650D", "Amazon Technologies Inc."),
    ("44D9E7", "Ubiquiti Networks Inc."),
    ("48DB50", "Huawei Technologies Co., Ltd"),
    ("4C5E0C", "Routerboard.com (MikroTik)"),
    ("4C8D79", "Apple, Inc."),
    ("4CCC6A", "Micro-Star INTL CO., LTD."),
    ("4CFCAA", "Tesla, Inc."),
    ("50465D", "ASUSTek Computer Inc."),
    ("525400", "QEMU / KVM virtual NIC"),
    ("5C63BF", "TP-LINK Technologies Co., Ltd."),
    ("5CCF7F", "Espressif Inc."),
    ("5CF3FC", "IBM Corporation"),
    ("5CF6DC", "Samsung Electronics"),
    ("640980", "Xiaomi Communications Co Ltd"),
    ("641666", "Nest Labs Inc."),
    ("6837E9", "Amazon Technologies Inc."),
    ("6C3B6B", "Routerboard.com (MikroTik)"),
    ("6CC217", "Hewlett Packard Enterprise"),
    ("748114", "Routerboard.com (MikroTik)"),
    ("74C246", "Amazon Technologies Inc."),
    ("74D435", "GIGA-BYTE Technology Co., Ltd."),
    ("781DBA", "Huawei Technologies Co., Ltd"),
    ("788A20", "Ubiquiti Networks Inc."),
    ("8425DB", "Samsung Electronics"),
    ("8CAAB5", "Espressif Inc."),
    ("8CBEBE", "Xiaomi Communications Co Ltd"),
    ("8CDCD4", "Hewlett Packard Enterprise"),
    ("906CAC", "Fortinet Inc."),
    ("944452", "Belkin International Inc."),
    ("98ED5C", "Tesla, Inc."),
    ("A020A6", "Espressif Inc."),
    ("A0E4CB", "Zyxel Communications Corporation"),
    ("A45E60", "Apple, Inc."),
    ("AC1F6B", "Super Micro Computer, Inc."),
    ("AC220B", "ASUSTek Computer Inc."),
    ("B0A737", "Roku, Inc."),
    ("B827EB", "Raspberry Pi Foundation"),
    ("B8AC6F", "Dell Inc."),
    ("B8E937", "Sonos, Inc."),
    ("CC6DA0", "Roku, Inc."),
    ("DC3A5E", "Roku, Inc."),
    ("DCA632", "Raspberry Pi Trading Ltd"),
    ("E45F01", "Raspberry Pi Trading Ltd"),
    ("E48D8C", "Routerboard.com (MikroTik)"),
    ("E8508B", "Samsung Electronics"),
    ("ECFABC", "Espressif Inc."),
    ("F09FC2", "Ubiquiti Networks Inc."),
    ("F0B429", "Xiaomi Communications Co Ltd"),
    ("F0272D", "Amazon Technologies Inc."),
    ("F4F5E8", "Google, Inc."),
    ("20E52A", "NETGEAR Inc."),
];

pub(crate) fn mac_address_lookup(args: &Args) -> Result<String> {
    let raw = arg(args, tool("mac-address-lookup"), "mac").trim();
    let digits: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if digits.len() < 6 {
        bail!("need at least the 6 hex digits of the OUI prefix");
    }
    let prefix = digits[..6].to_uppercase();
    // it-tools prints "Unknown vendor for this address" in the card; a miss is
    // not an error.
    let vendor = OUI
        .iter()
        .find(|(p, _)| *p == prefix)
        .map_or("Unknown vendor for this address", |(_, v)| v);
    Ok(format!("OUI prefix: {prefix}\nVendor info: {vendor}\n"))
}

pub static TOOL: Tool = Tool {
    id: "mac-address-lookup",
    name: "MAC address lookup",
    description: "Find the vendor of a network device from its MAC address.",
    category: Category::Network,
    keywords: &[
        "mac",
        "address",
        "lookup",
        "vendor",
        "parser",
        "manufacturer",
    ],
    fields: &[Field {
        key: "mac",
        label: "MAC address",
        kind: crate::FieldKind::Text,
        default: "00:0C:29:12:34:56",
        options: &[],
    }],
    kind: Kind::Pure,
    run: mac_address_lookup,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn mac_lookup_known_and_unknown() {
        let out = run("mac-address-lookup", &[("mac", "00:0C:29:12:34:56")]).unwrap();
        assert!(out.contains("VMware"), "{out}");
        // Separator-insensitive and case-insensitive.
        assert!(
            run("mac-address-lookup", &[("mac", "b8-27-eb-11-22-33")])
                .unwrap()
                .contains("Raspberry Pi")
        );
        assert!(
            run("mac-address-lookup", &[("mac", "00000c112233")])
                .unwrap()
                .contains("Cisco")
        );
        // A miss is a message, not an error.
        assert!(
            run("mac-address-lookup", &[("mac", "FF:FF:FF:FF:FF:FF")])
                .unwrap()
                .contains("Unknown vendor for this address")
        );
        assert!(run("mac-address-lookup", &[("mac", "zz")]).is_err());
        // The table itself has no duplicate prefixes.
        let mut p: Vec<_> = OUI.iter().map(|(p, _)| *p).collect();
        let n = p.len();
        p.sort_unstable();
        p.dedup();
        assert_eq!(p.len(), n, "duplicate OUI prefixes");
    }
}
