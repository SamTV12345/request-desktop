//! Web & network tools. Owned by the web batch.
//!
//! Ported from it-tools (`src/tools/<name>/`). Every tool is `fields in ->
//! text out`; output is a plain `Label: value` block because the UI renders it
//! verbatim.

pub use rp_devtools_common::*;

mod chmod_calculator;
mod cidr_to_ip_range;
mod common;
mod crontab_generator;
mod curl_to_http;
mod device_information;
mod docker_run_to_docker_compose_converter;
mod email_normalizer;
mod html_entities;
mod http_status_codes;
mod iban_validator_and_parser;
mod integers_to_ip;
mod ip_in_cidr;
mod ip_range_to_cidr;
mod ipv4_address_converter;
mod ipv4_range_expander;
mod ipv4_subnet_calculator;
mod ipv6_address_converter;
mod ipv6_subnet_calculator;
mod ipv6_ula_generator;
mod mac_address_converter;
mod mac_address_generator;
mod mac_address_lookup;
mod meta_tag_generator;
mod mime_encoder_decoder;
mod mime_types;
mod phone_parser_and_formatter;
mod port_info_search;
mod random_port_generator;
mod safelink_decoder;
mod sharepoint_url_decoder;
mod slugify_string;
mod subnet_exclude_calculator;
mod testing;
mod url_cleaner;
mod url_defanger;
mod url_encoder;
mod url_parser;
mod url_text_fragment_generator;
mod user_agent_parser;
mod utm_url_generator;

// Every tool file opens with `use crate::*;`, so the crate root is this
// crate's prelude: the shared types plus any helper one tool lends another.
#[allow(unused_imports)]
pub(crate) use {
    chmod_calculator::*, cidr_to_ip_range::*, common::*, crontab_generator::*, curl_to_http::*,
    device_information::*, docker_run_to_docker_compose_converter::*, email_normalizer::*,
    html_entities::*, http_status_codes::*, iban_validator_and_parser::*, integers_to_ip::*,
    ip_in_cidr::*, ip_range_to_cidr::*, ipv4_address_converter::*, ipv4_range_expander::*,
    ipv4_subnet_calculator::*, ipv6_address_converter::*, ipv6_subnet_calculator::*,
    ipv6_ula_generator::*, mac_address_converter::*, mac_address_generator::*,
    mac_address_lookup::*, meta_tag_generator::*, mime_encoder_decoder::*, mime_types::*,
    phone_parser_and_formatter::*, port_info_search::*, random_port_generator::*,
    safelink_decoder::*, sharepoint_url_decoder::*, slugify_string::*,
    subnet_exclude_calculator::*, url_cleaner::*, url_defanger::*, url_encoder::*, url_parser::*,
    url_text_fragment_generator::*, user_agent_parser::*, utm_url_generator::*,
};

/// Reached for by name from `crates/app`'s bespoke panes.
pub use chmod_calculator::{CHMOD_GROUPS, CHMOD_SCOPES, chmod_forms};
pub use mime_types::{extensions_for_mime, mime_for_extension};

/// Every tool in this crate, in registry order.
pub static TOOLS: &[&Tool] = &[
    &url_encoder::TOOL,
    &url_parser::TOOL,
    &html_entities::TOOL,
    &safelink_decoder::TOOL,
    &user_agent_parser::TOOL,
    &mime_types::TOOL,
    &http_status_codes::TOOL,
    &meta_tag_generator::TOOL,
    &ipv4_address_converter::TOOL,
    &ipv4_range_expander::TOOL,
    &ipv4_subnet_calculator::TOOL,
    &ipv6_ula_generator::TOOL,
    &mac_address_generator::TOOL,
    &mac_address_lookup::TOOL,
    &random_port_generator::TOOL,
    &iban_validator_and_parser::TOOL,
    &phone_parser_and_formatter::TOOL,
    &email_normalizer::TOOL,
    &docker_run_to_docker_compose_converter::TOOL,
    &crontab_generator::TOOL,
    &chmod_calculator::TOOL,
    &slugify_string::TOOL,
    &device_information::TOOL,
    &cidr_to_ip_range::TOOL,
    &ip_range_to_cidr::TOOL,
    &ip_in_cidr::TOOL,
    &subnet_exclude_calculator::TOOL,
    &ipv6_subnet_calculator::TOOL,
    &ipv6_address_converter::TOOL,
    &integers_to_ip::TOOL,
    &mac_address_converter::TOOL,
    &port_info_search::TOOL,
    &curl_to_http::TOOL,
    &mime_encoder_decoder::TOOL,
    &url_defanger::TOOL,
    &url_cleaner::TOOL,
    &utm_url_generator::TOOL,
    &url_text_fragment_generator::TOOL,
    &sharepoint_url_decoder::TOOL,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    // ---- url-encoder / url-parser

    #[test]
    fn url_encoding_shows_both_directions() {
        // it-tools defaults: 'Hello world :)' <-> 'Hello%20world%20%3A)'
        let out = run(
            "url-encoder",
            &[
                ("encode_input", "Hello world :)"),
                ("decode_input", "Hello%20world%20%3A)"),
            ],
        )
        .unwrap();
        assert!(out.contains("Encoded:\nHello%20world%20%3A)"), "{out}");
        assert!(out.contains("Decoded:\nHello world :)"), "{out}");
        // A bad decode input must not take the encode panel down with it.
        let out = run(
            "url-encoder",
            &[("encode_input", "a b"), ("decode_input", "%zz")],
        )
        .unwrap();
        assert!(out.contains("Encoded:\na%20b"), "{out}");
        assert!(out.contains("Impossible to parse this string"), "{out}");
    }

    #[test]
    fn url_parser_splits_all_components() {
        let out = run(
            "url-parser",
            &[(
                "url",
                "https://me:pwd@it-tools.tech:3000/url-parser?key1=value&key2=value2#the-hash",
            )],
        )
        .unwrap();
        assert!(out.contains("Protocol: https:"), "{out}");
        assert!(out.contains("Username: me"));
        assert!(out.contains("Password: pwd"));
        assert!(out.contains("Hostname: it-tools.tech"));
        assert!(out.contains("Port: 3000"));
        assert!(out.contains("Path: /url-parser"));
        assert!(out.contains("Hash: the-hash"));
        assert!(out.contains("key1 = value"));
        assert!(out.contains("key2 = value2"));
        assert!(run("url-parser", &[("url", "not a url")]).is_err());
        assert!(run("url-parser", &[("url", "https://")]).is_err());
    }

    // ---- safelink-decoder (vectors from safelink-decoder.service.test.ts)

    #[test]
    fn safelink_decoder_vectors() {
        let plain = "https://aus01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fwww.google.com%2Fsearch%3Fq%3Dsafelink%26rlz%3D1&data=05%7C02%7C&sdata=x&reserved=0";
        assert_eq!(
            run("safelink-decoder", &[("url", plain)]).unwrap(),
            "https://www.google.com/search?q=safelink&rlz=1"
        );
        let amped = "https://aus01.safelinks.protection.outlook.com/?url=https%3A%2F%2Fwww.google.com%2Fsearch%3Fq%3Dsafelink%26rlz%3D1&amp;data=05&amp;reserved=0";
        assert_eq!(
            run("safelink-decoder", &[("url", amped)]).unwrap(),
            "https://www.google.com/search?q=safelink&rlz=1"
        );
        assert!(run("safelink-decoder", &[("url", "https://google.com")]).is_err());
    }

    // ---- ipv4-address-converter (vectors from ipv4-address-converter.service.test.ts)

    #[test]
    fn ipv4_to_int_vectors() {
        for (ip, int) in [
            ("192.168.0.1", 3232235521u32),
            ("10.0.0.1", 167772161),
            ("255.255.255.255", 4294967295),
        ] {
            let out = run("ipv4-address-converter", &[("ip", ip)]).unwrap();
            assert!(out.contains(&format!("Decimal: {int}")), "{out}");
        }
        let out = run("ipv4-address-converter", &[("ip", "192.168.1.1")]).unwrap();
        assert!(out.contains("Hexadecimal: C0A80101"), "{out}");
        assert!(
            out.contains("Ipv6: 0000:0000:0000:0000:0000:ffff:c0a8:0101"),
            "{out}"
        );
        assert!(out.contains("Ipv6 (short): ::ffff:c0a8:0101"), "{out}");
    }

    #[test]
    fn invalid_ipv4_is_rejected() {
        for bad in [
            "256.168.0.1",
            "192.168.0",
            "192.168.0.1.2",
            "192.168.0.1.",
            ".192.168.0.1",
            "192.168.0.a",
            "",
            " ",
            "foo",
            "-1",
            "0",
        ] {
            assert!(
                run("ipv4-address-converter", &[("ip", bad)]).is_err(),
                "{bad} should be invalid"
            );
        }
        for good in ["192.168.0.1", "10.0.0.1"] {
            assert!(
                run("ipv4-address-converter", &[("ip", good)]).is_ok(),
                "{good} should be valid"
            );
        }
    }

    // ---- ipv4-range-expander (vectors from ipv4-range-expander.service.test.ts)

    #[test]
    fn range_expander_vectors() {
        let r = calculate_cidr(
            "192.168.1.1".parse().unwrap(),
            "192.168.7.255".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(r.old_size, 1791);
        assert_eq!(r.new_size, 2048);
        assert_eq!(r.new_start.to_string(), "192.168.0.0");
        assert_eq!(r.new_end.to_string(), "192.168.7.255");
        assert_eq!(r.new_cidr, "192.168.0.0/21");

        let r = calculate_cidr("10.0.0.1".parse().unwrap(), "10.0.0.17".parse().unwrap()).unwrap();
        assert_eq!(r.old_size, 17);
        assert_eq!(r.new_size, 32);
        assert_eq!(r.new_start.to_string(), "10.0.0.0");
        assert_eq!(r.new_end.to_string(), "10.0.0.31");
        assert_eq!(r.new_cidr, "10.0.0.0/27");

        // The rendered table is old value -> new value, per it-tools.
        let out = run(
            "ipv4-range-expander",
            &[("start_ip", "192.168.1.1"), ("end_ip", "192.168.6.255")],
        )
        .unwrap();
        assert!(out.contains("old value  ->  new value"), "{out}");
        assert!(out.contains("192.168.1.1  ->  192.168.0.0"), "{out}");
        assert!(out.contains("Addresses in range"), "{out}");
        assert!(out.contains("192.168.0.0/21"), "{out}");

        // start > end has no result
        assert!(
            run(
                "ipv4-range-expander",
                &[("start_ip", "192.168.7.1"), ("end_ip", "192.168.6.255")]
            )
            .is_err()
        );
        assert!(
            run(
                "ipv4-range-expander",
                &[("start_ip", "nope"), ("end_ip", "10.0.0.1")]
            )
            .is_err()
        );
        // identical addresses collapse to /32
        let r = calculate_cidr("10.0.0.5".parse().unwrap(), "10.0.0.5".parse().unwrap()).unwrap();
        assert_eq!(r.new_cidr, "10.0.0.5/32");
        assert_eq!(r.new_size, 1);
        // full space
        let r = calculate_cidr(
            "0.0.0.0".parse().unwrap(),
            "255.255.255.255".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(r.new_cidr, "0.0.0.0/0");
        assert_eq!(r.new_size, 1 << 32);
    }

    // ---- mac-address-generator (vectors from mac-adress-generator.models.test.ts)

    #[test]
    fn split_prefix_vectors() {
        assert!(split_prefix("").is_empty());
        assert_eq!(split_prefix("01"), ["01"]);
        assert_eq!(split_prefix("01:"), ["01"]);
        assert_eq!(split_prefix("01:23"), ["01", "23"]);
        assert_eq!(split_prefix("01-23"), ["01", "23"]);
        assert_eq!(split_prefix("0123"), ["01", "23"]);
        assert_eq!(split_prefix("012345"), ["01", "23", "45"]);
        assert_eq!(split_prefix("0123456"), ["01", "23", "45", "06"]);
    }

    #[test]
    fn mac_generator_respects_prefix_and_separator() {
        let out = run(
            "mac-address-generator",
            &[
                ("prefix", "ff:ee:aa"),
                ("separator", "-"),
                ("quantity", "3"),
                ("uppercase", "false"),
            ],
        )
        .unwrap();
        assert_eq!(out.lines().count(), 3);
        for line in out.lines() {
            assert!(line.starts_with("ff-ee-aa-"), "{line}");
            assert_eq!(line.split('-').count(), 6);
        }
        let out = run(
            "mac-address-generator",
            &[
                ("prefix", "ff:ee:a"),
                ("separator", ":"),
                ("uppercase", "true"),
                ("quantity", "1"),
            ],
        )
        .unwrap();
        assert!(out.starts_with("FF:EE:0A:"), "{out}");
        assert!(
            run(
                "mac-address-generator",
                &[
                    ("prefix", "0011223344556677"),
                    ("quantity", "1"),
                    ("separator", ":")
                ]
            )
            .is_err()
        );
        assert!(
            run(
                "mac-address-generator",
                &[("quantity", "0"), ("separator", ":"), ("prefix", "")]
            )
            .is_err()
        );
    }

    // ---- chmod-calculator (vectors from chmod-calculator.service.test.ts)

    /// Tick the checkboxes the way the grid would for this octal triple.
    fn chmod_grid(octal: &str) -> Args {
        let mut a = Args::new();
        for (digit, group) in octal.chars().zip(CHMOD_GROUPS) {
            let d = digit.to_digit(8).unwrap() as u8;
            for (scope, bit) in CHMOD_SCOPES {
                a.insert(format!("{group}_{scope}"), (d & bit != 0).to_string());
            }
        }
        a
    }

    #[test]
    fn chmod_octal_symbolic_round_trip() {
        let t = tool("chmod-calculator");
        for (octal, symbolic) in [
            ("777", "rwxrwxrwx"),
            ("000", "---------"),
            ("235", "-w--wxr-x"),
            ("421", "r---w---x"),
            ("124", "--x-w-r--"),
            ("222", "-w--w--w-"),
            ("755", "rwxr-xr-x"),
        ] {
            let out = (t.run)(&chmod_grid(octal)).unwrap();
            assert!(out.contains(&format!("Octal: {octal}")), "{out}");
            assert!(out.contains(&format!("Symbolic: {symbolic}")), "{out}");
            assert!(
                out.contains(&format!("Command: chmod {octal} path")),
                "{out}"
            );
        }
        // An untouched grid is 000, never an error — there is nothing to parse.
        let empty = (t.run)(&Args::new()).unwrap();
        assert!(empty.contains("Octal: 000"), "{empty}");
        assert!(empty.contains("Symbolic: ---------"), "{empty}");
    }
}
