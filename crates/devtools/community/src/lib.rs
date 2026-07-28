//! Tools that never landed upstream: ported from open it-tools pull requests.
//!
//! it-tools has stopped merging, so ~200 pull requests sit there unreviewed.
//! This batch takes the ones that are *interesting* rather than the tenth
//! national-ID validator — id formats, test-data generation, HTTP-adjacent
//! sanitising, unit maths — and ports them to the same `fields in -> text out`
//! shape as the rest of the registry. The PR number is on each tool.
//!
//! `fake-data-generator` has no upstream PR at all; it is the one tool this
//! batch adds outright, because "a realistic person to paste into a request
//! body" is what an HTTP client actually needs and lorem ipsum is not it.
//!
//! The two rules from `lib.rs` hold here as everywhere: no direction toggle
//! (compute every panel), and one bad panel must not blank the others.

pub use rp_devtools_common::*;

mod common;
mod conventional_commits_cheatsheet;
mod crc_calculator;
mod data_storage_unit_converter;
mod fake_data_generator;
mod floating_point_converter;
mod geo_distance_computer;
mod gzip_string_converter;
mod har_sanitizer;
mod json_size_analyzer;
mod json_to_schema;
mod jwt_generator;
mod list_comparison;
mod luhn_checker;
mod markdown_table_generator;
mod nanoid_generator;
mod objectid_generator_and_parser;
mod passphrase_generator;
mod pin_code_generator;
mod punycode_converter;
mod sensitive_data_masker;
mod sla_computer;
mod snowflake_id_parser;
mod testing;
mod week_numbers_converter;

// Every tool file opens with `use crate::*;`, so the crate root is this
// crate's prelude: the shared types plus any helper one tool lends another.
#[allow(unused_imports)]
pub(crate) use {
    common::*, conventional_commits_cheatsheet::*, crc_calculator::*,
    data_storage_unit_converter::*, fake_data_generator::*, floating_point_converter::*,
    geo_distance_computer::*, gzip_string_converter::*, har_sanitizer::*, json_size_analyzer::*,
    json_to_schema::*, jwt_generator::*, list_comparison::*, luhn_checker::*,
    markdown_table_generator::*, nanoid_generator::*, objectid_generator_and_parser::*,
    passphrase_generator::*, pin_code_generator::*, punycode_converter::*,
    sensitive_data_masker::*, sla_computer::*, snowflake_id_parser::*, week_numbers_converter::*,
};

/// Every tool in this crate, in registry order.
pub static TOOLS: &[&Tool] = &[
    &fake_data_generator::TOOL,
    &passphrase_generator::TOOL,
    &pin_code_generator::TOOL,
    &nanoid_generator::TOOL,
    &jwt_generator::TOOL,
    &crc_calculator::TOOL,
    &objectid_generator_and_parser::TOOL,
    &snowflake_id_parser::TOOL,
    &luhn_checker::TOOL,
    &floating_point_converter::TOOL,
    &json_to_schema::TOOL,
    &json_size_analyzer::TOOL,
    &har_sanitizer::TOOL,
    &list_comparison::TOOL,
    &sensitive_data_masker::TOOL,
    &markdown_table_generator::TOOL,
    &punycode_converter::TOOL,
    &gzip_string_converter::TOOL,
    &data_storage_unit_converter::TOOL,
    &geo_distance_computer::TOOL,
    &sla_computer::TOOL,
    &week_numbers_converter::TOOL,
    &conventional_commits_cheatsheet::TOOL,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use serde_json::Value;
    #[test]
    fn fake_data_fills_every_column_in_every_format() {
        let json = run("fake-data-generator", &[("count", "3"), ("format", "JSON")]);
        let rows: Vec<serde_json::Map<String, Value>> =
            serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert!(row["email"].as_str().unwrap().contains('@'));
            assert!(!row["first_name"].as_str().unwrap().is_empty());
        }
        // German names carry umlauts; the address keeps them, the email must not.
        let de = run("fake-data-generator", &[("count", "20"), ("locale", "de")]);
        assert!(de.is_ascii() || !de.contains("ü@"));
        for row in serde_json::from_str::<Vec<serde_json::Map<String, Value>>>(&de).unwrap() {
            assert!(row["email"].as_str().unwrap().is_ascii(), "{row:?}");
        }
        // CSV keeps its header, SQL its statement terminator.
        assert!(run("fake-data-generator", &[("format", "CSV")]).starts_with("first_name,"));
        assert!(run("fake-data-generator", &[("format", "SQL")]).contains("INSERT INTO people ("));
    }

    #[test]
    fn luhn_agrees_with_the_known_vectors() {
        let out = run(
            "luhn-checker",
            &[(
                "numbers",
                "4539 1488 0343 6467\n4539148803436468\n79927398713",
            )],
        );
        let mut lines = out.lines();
        assert!(lines.next().unwrap().contains("valid"));
        assert!(lines.next().unwrap().contains("INVALID"));
        assert!(lines.next().unwrap().contains("valid"));
        // The suggested check digit must actually complete the payload.
        let digits: Vec<u8> = "7992739871".bytes().map(|b| b - b'0').collect();
        assert_eq!(luhn(&digits).1, 3);
    }

    #[test]
    fn jwt_matches_the_rfc7519_example() {
        // RFC 7519 §3.1's token, whose secret and claims are fixed.
        let out = run(
            "jwt-generator",
            &[
                (
                    "payload",
                    r#"{"iss":"joe","exp":1300819380,"http://example.com/is_root":true}"#,
                ),
                ("secret", "secret"),
                ("algorithm", "HS256"),
            ],
        );
        let token = out.lines().nth(1).expect("the token line");
        assert_eq!(token.split('.').count(), 3);
        // Round-trips through the ported parser.
        let parsed = (rp_devtools_crypto::TOOLS
            .iter()
            .find(|t| t.id == "jwt-parser")
            .unwrap()
            .run)(
            &[("jwt".to_string(), token.to_string())]
                .into_iter()
                .collect(),
        )
        .expect("the generated token parses");
        assert!(parsed.contains("joe"), "{parsed}");
    }

    #[test]
    fn crc32_matches_the_check_vector() {
        // The catalogue's check value: CRC-32 of "123456789" is 0xCBF43926.
        let out = run("crc-calculator", &[("input", "123456789")]);
        assert!(out.contains("cbf43926"), "{out}");
        assert!(out.contains("e3069283"), "{out}"); // CRC-32C
        assert!(out.contains("bb3d"), "{out}"); // CRC-16/ARC
        assert!(out.contains("4b37"), "{out}"); // CRC-16/MODBUS
    }

    #[test]
    fn a_masked_log_keeps_its_shape_and_loses_its_secrets() {
        let out = run(
            "sensitive-data-masker",
            &[(
                "input",
                "GET /v1 HTTP/1.1\nAuthorization: Bearer sk-abcdef1234567890\n\
                 user=ada@example.com from 192.168.0.14\npassword=hunter2trombone",
            )],
        );
        assert!(out.contains("Authorization: Bearer *"), "{out}");
        assert!(!out.contains("sk-abcdef1234567890"), "{out}");
        assert!(!out.contains("ada@example.com"), "{out}");
        assert!(!out.contains("hunter2trombone"), "{out}");
        assert!(!out.contains("192.168.0.14"), "{out}");
        // Structure survives: the request line is untouched.
        assert!(out.contains("GET /v1 HTTP/1.1"), "{out}");
    }

    #[test]
    fn snowflake_and_objectid_read_their_timestamps_back() {
        let out = run("snowflake-id-parser", &[("id", "1541815603606036480")]);
        assert!(out.contains("2022-06-28"), "{out}");
        let out = run(
            "objectid-generator-and-parser",
            &[("objectId", "5f1d7f2a9b1e8a0017c3e4b5")],
        );
        assert!(out.contains("2020-07-26"), "{out}");
    }

    #[test]
    fn units_and_geometry_hold_their_reference_values() {
        let storage = run(
            "data-storage-unit-converter",
            &[("value", "1"), ("unit", "MiB")],
        );
        assert!(storage.contains("B: 1048576"), "{storage}");
        assert!(storage.contains("kB: 1048.576"), "{storage}");
        // Eiffel tower to Brandenburg gate, ~879 km great circle.
        let geo = run("geo-distance-computer", &[]);
        assert!(geo.contains("distance: 879.0"), "{geo}");
        assert!(geo.contains("NE"), "{geo}");
        // Three nines is famously ~8h45m a year.
        let sla = run("sla-computer", &[("uptime", "99.9")]);
        assert!(sla.contains("per year (365d): 8h 45m 36s"), "{sla}");
    }

    #[test]
    fn gzip_round_trips_and_punycode_goes_both_ways() {
        let packed = run("gzip-string-converter", &[("input", "hello hello hello")]);
        let b64 = packed.lines().nth(1).expect("the base64 line");
        let back = run("gzip-string-converter", &[("gzipped", b64)]);
        assert!(back.contains("hello hello hello"), "{back}");

        let out = run("punycode-converter", &[("domain", "münchen.example")]);
        assert!(out.contains("xn--mnchen-3ya.example"), "{out}");
        let out = run(
            "punycode-converter",
            &[("domain", "xn--mnchen-3ya.example")],
        );
        assert!(out.contains("münchen.example"), "{out}");
    }

    #[test]
    fn week_numbers_follow_iso_8601_at_the_year_boundary() {
        // 2021-01-01 is a Friday, and ISO puts it in week 53 of 2020.
        let out = run("week-numbers-converter", &[("date", "2021-01-01")]);
        assert!(out.contains("ISO week: 2020-W53"), "{out}");
        assert!(out.contains("week starts: 2020-12-28"), "{out}");
        assert!(out.contains("weeks in 2020: 53"), "{out}");
    }

    #[test]
    fn schema_and_size_describe_the_same_document() {
        let doc = r#"{"id":"6ba7b810-9dad-11d1-80b4-00c04fd430c8","n":3,"pi":3.5,"tags":["a"]}"#;
        let schema: Value =
            serde_json::from_str(&run("json-to-schema", &[("input", doc)])).unwrap();
        assert_eq!(schema["properties"]["id"]["format"], "uuid");
        assert_eq!(schema["properties"]["n"]["type"], "integer");
        assert_eq!(schema["properties"]["pi"]["type"], "number");
        assert_eq!(schema["properties"]["tags"]["items"]["type"], "string");
        assert_eq!(schema["required"][0], "id");

        let size = run("json-size-analyzer", &[("input", doc)]);
        assert!(size.contains("members: 4"), "{size}");
    }

    #[test]
    fn markdown_table_escapes_pipes_and_pads_short_rows() {
        let out = run(
            "markdown-table-generator",
            &[("input", "a,b\n1,2|3\n4"), ("align", "Center")],
        );
        assert_eq!(
            out, "| a | b |\n| :-: | :-: |\n| 1 | 2\\|3 |\n| 4 |  |\n",
            "{out}"
        );
    }

    #[test]
    fn floating_point_goes_both_ways() {
        let out = run("floating-point-converter", &[("value", "0.1")]);
        assert!(out.contains("hex: 0x3fb999999999999a"), "{out}");
        let back = run(
            "floating-point-converter",
            &[("value", "0x3fb999999999999a")],
        );
        assert!(back.contains("0.1"), "{back}");
    }

    #[test]
    fn generators_respect_their_shape() {
        let pins = run(
            "pin-code-generator",
            &[("length", "4"), ("count", "10"), ("distinct", "true")],
        );
        for pin in pins.lines() {
            assert_eq!(pin.len(), 4, "{pin}");
            assert!(pin.chars().all(|c| c.is_ascii_digit()));
            let mut seen: Vec<char> = pin.chars().collect();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), 4, "distinct digits requested: {pin}");
        }
        let phrase = run(
            "passphrase-generator",
            &[("words", "5"), ("count", "1"), ("separator", ".")],
        );
        assert_eq!(phrase.trim().split('.').count(), 5, "{phrase}");
        let ids = run("nanoid-generator", &[("size", "12"), ("count", "3")]);
        assert!(ids.lines().all(|l| l.chars().count() == 12), "{ids}");
    }
}
