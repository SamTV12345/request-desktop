//! Data-format converters, ported from it-tools `src/tools/*`.
//!
//! ponytail: every entry is `fn(&Args) -> Result<String>` plus one `static`.
//! No traits, no macros.
//!
//! `serde_json/preserve_order` is on workspace-wide, so object keys keep their
//! input order and the `sortKeys` switches (json-viewer, yaml-viewer) go through
//! [`sorted`] when the user asks for them.
//!
//! Where it-tools renders several panels at once — base64, text-to-binary,
//! text-to-unicode, case-converter, integer-base, roman numerals — so do we, one
//! labelled block each via `crate::section`, and no `mode` select anywhere.

pub use rp_devtools_common::*;

mod base64_file_converter;
mod base64_string_converter;
mod case_converter;
mod common;
mod hex_base64_converter;
mod integer_base_converter;
mod json_diff;
mod json_escape_unescape;
mod json_minify;
mod json_to_csv;
mod json_to_toml;
mod json_to_types;
mod json_to_xml;
mod json_to_yaml_converter;
mod json_viewer;
mod list_converter;
mod markdown_to_html;
mod markdown_toc_generator;
mod roman_numeral_converter;
mod sql_prettify;
mod testing;
mod text_to_binary;
mod text_to_unicode;
mod toml_to_json;
mod toml_to_yaml;
mod xml_formatter;
mod xml_to_json;
mod yaml_to_json_converter;
mod yaml_to_toml;
mod yaml_viewer;

// Every tool file opens with `use crate::*;`, so the crate root is this
// crate's prelude: the shared types plus any helper one tool lends another.
#[allow(unused_imports)]
pub(crate) use {
    base64_file_converter::*, base64_string_converter::*, case_converter::*, common::*,
    hex_base64_converter::*, integer_base_converter::*, json_diff::*, json_escape_unescape::*,
    json_minify::*, json_to_csv::*, json_to_toml::*, json_to_types::*, json_to_xml::*,
    json_to_yaml_converter::*, json_viewer::*, list_converter::*, markdown_to_html::*,
    markdown_toc_generator::*, roman_numeral_converter::*, sql_prettify::*, text_to_binary::*,
    text_to_unicode::*, toml_to_json::*, toml_to_yaml::*, xml_formatter::*, xml_to_json::*,
    yaml_to_json_converter::*, yaml_to_toml::*, yaml_viewer::*,
};

/// Every tool in this crate, in registry order.
pub static TOOLS: &[&Tool] = &[
    &base64_string_converter::TOOL,
    &base64_file_converter::TOOL,
    &json_to_csv::TOOL,
    &json_to_toml::TOOL,
    &json_to_xml::TOOL,
    &json_to_yaml_converter::TOOL,
    &json_minify::TOOL,
    &json_diff::TOOL,
    &json_viewer::TOOL,
    &toml_to_json::TOOL,
    &toml_to_yaml::TOOL,
    &yaml_to_json_converter::TOOL,
    &yaml_to_toml::TOOL,
    &yaml_viewer::TOOL,
    &xml_to_json::TOOL,
    &xml_formatter::TOOL,
    &list_converter::TOOL,
    &markdown_to_html::TOOL,
    &sql_prettify::TOOL,
    &case_converter::TOOL,
    &text_to_binary::TOOL,
    &text_to_unicode::TOOL,
    &integer_base_converter::TOOL,
    &roman_numeral_converter::TOOL,
    &markdown_toc_generator::TOOL,
    &json_to_types::TOOL,
    &json_escape_unescape::TOOL,
    &hex_base64_converter::TOOL,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    #[test]
    fn ids_are_unique_and_sane() {
        let mut ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate tool id");
        assert_eq!(n, 28);
    }

    #[test]
    fn empty_args_never_panic() {
        let empty = Args::new();
        for t in TOOLS {
            let _ = (t.run)(&empty);
        }
    }

    #[test]
    fn only_the_file_converter_is_custom() {
        let custom: Vec<&str> = TOOLS
            .iter()
            .filter(|t| t.kind == Kind::Custom)
            .map(|t| t.id)
            .collect();
        assert_eq!(custom, vec!["base64-file-converter"]);
        assert!(run("base64-file-converter", &[]).is_err());
    }

    #[test]
    fn select_defaults_are_listed_options() {
        for t in TOOLS {
            for f in t.fields {
                if f.kind == crate::FieldKind::Select {
                    assert!(f.options.contains(&f.default), "{}/{}", t.id, f.key);
                }
            }
        }
    }

    // ── base64 ──────────────────────────────────────────────────────────

    /// The `mode` select is gone: it-tools renders both directions at once, so
    /// these assert on the two labelled panels instead.
    #[test]
    fn base64_round_trip() {
        assert_eq!(
            ok("base64-string-converter", &[("input", "Hello, World!")]),
            "Base64 of string:\nSGVsbG8sIFdvcmxkIQ=="
        );
        assert_eq!(
            ok(
                "base64-string-converter",
                &[("base64", "SGVsbG8sIFdvcmxkIQ==")]
            ),
            "Decoded string:\nHello, World!"
        );
        // url-safe encoding drops padding and uses -_
        let url = ok(
            "base64-string-converter",
            &[("input", "\u{f8}\u{ff}?"), ("encodeUrlSafe", "true")],
        );
        let url = url.strip_prefix("Base64 of string:\n").unwrap().to_string();
        assert!(
            !url.contains('=') && !url.contains('+') && !url.contains('/'),
            "{url}"
        );
        assert_eq!(
            ok(
                "base64-string-converter",
                &[("base64", &url), ("decodeUrlSafe", "true")]
            ),
            "Decoded string:\n\u{f8}\u{ff}?"
        );
        // `decodeUrlSafe` is wired, not decorative: `-_` are only legal with it on.
        assert!(
            run("base64-string-converter", &[("base64", "-_-_")])
                .unwrap()
                .contains("Invalid base64 string")
        );
        // and it-tools strips a data-URL prefix before decoding
        assert_eq!(
            ok(
                "base64-string-converter",
                &[("base64", "data:text/plain;base64,aGk=")]
            ),
            "Decoded string:\nhi"
        );
    }

    /// The original bug report: `TESTtest` is valid base64 over six *binary*
    /// bytes. it-tools shows the (lossy) decode; we must not error out.
    #[test]
    fn base64_shows_both_panels_for_binary_payload() {
        let out = ok(
            "base64-string-converter",
            &[("input", "TESTtest"), ("base64", "TESTtest")],
        );
        assert!(out.contains("Base64 of string:\nVEVTVHRlc3Q="), "{out}");
        assert!(out.contains("Decoded string:\n"), "{out}");
        // Six bytes came out, however unprintable they are.
        assert!(!out.ends_with("Decoded string:\n"), "{out}");
    }

    /// Garbage in one panel must never blank the other.
    #[test]
    fn base64_one_bad_panel_keeps_the_other() {
        let out = ok(
            "base64-string-converter",
            &[("input", "hello"), ("base64", "!!!!")],
        );
        assert!(out.contains("Base64 of string:\naGVsbG8="), "{out}");
        assert!(out.contains("Invalid base64 string"), "{out}");
    }

    #[test]
    fn base64_rejects_only_a_wholly_empty_form() {
        assert!(run("base64-string-converter", &[]).is_err());
        assert!(run("base64-string-converter", &[("input", "")]).is_err());
        // one filled panel is enough
        assert!(run("base64-string-converter", &[("base64", "aGk=")]).is_ok());
    }

    // ── json-to-csv (vectors from json-to-csv.service.test.ts) ──────────

    #[test]
    fn json_yaml_round_trip() {
        let y = ok("json-to-yaml-converter", &[("input", J)]);
        assert!(y.contains("hello:"), "{y}");
        let back = ok("yaml-to-json-converter", &[("input", &y)]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&back).unwrap(),
            serde_json::from_str::<serde_json::Value>(J).unwrap()
        );
    }

    #[test]
    fn malformed_documents_return_err() {
        for id in [
            "json-to-toml",
            "json-to-xml",
            "json-to-yaml-converter",
            "json-minify",
            "json-viewer",
        ] {
            assert!(
                run(id, &[("input", "{\"a\":")]).is_err(),
                "{id} accepted half-typed JSON"
            );
            assert!(
                run(id, &[("input", "")]).is_err(),
                "{id} accepted empty input"
            );
        }
        assert!(run("toml-to-json", &[("input", "a = ")]).is_err());
        assert!(run("toml-to-yaml", &[("input", "= 1")]).is_err());
        assert!(run("yaml-to-json-converter", &[("input", "{a: 1")]).is_err());
        assert!(run("yaml-to-toml", &[("input", "1")]).is_err()); // scalar root is not a TOML table
        assert!(run("yaml-viewer", &[("input", "")]).is_err());
        for id in [
            "toml-to-json",
            "toml-to-yaml",
            "yaml-to-json-converter",
            "yaml-to-toml",
            "yaml-viewer",
        ] {
            assert!(
                run(id, &[("input", "")]).is_err(),
                "{id} accepted empty input"
            );
        }
    }

    #[test]
    fn xml_json_round_trip() {
        // it-tools default sample for xml-to-json
        let j = ok("xml-to-json", &[("input", "<a x=\"1.234\" y=\"It's\"/>")]);
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert_eq!(v["a"]["_attributes"]["x"], "1.234");
        assert_eq!(v["a"]["_attributes"]["y"], "It's");
        // and its json-to-xml counterpart
        assert_eq!(
            ok(
                "json-to-xml",
                &[("input", r#"{"a":{"_attributes":{"x":"1.234","y":"It's"}}}"#)]
            ),
            "<a x=\"1.234\" y=\"It&apos;s\"/>"
        );
        assert_eq!(
            ok(
                "json-to-xml",
                &[("input", r#"{"hello":{"world":["foo","bar"]}}"#)]
            ),
            "<hello><world>foo</world><world>bar</world></hello>"
        );
        let back = ok(
            "xml-to-json",
            &[(
                "input",
                "<hello><world>foo</world><world>bar</world></hello>",
            )],
        );
        let v: serde_json::Value = serde_json::from_str(&back).unwrap();
        assert_eq!(v["hello"]["world"][0]["_text"], "foo");
        assert_eq!(v["hello"]["world"][1]["_text"], "bar");
    }

    #[test]
    fn xml_rejects_non_xml() {
        for id in ["xml-formatter", "xml-to-json"] {
            assert!(
                run(id, &[("input", "hello world")]).is_err(),
                "{id} accepted plain text"
            );
            assert!(
                run(id, &[("input", "<a><b></a>")]).is_err(),
                "{id} accepted mismatched tags"
            );
            assert!(
                run(id, &[("input", "<a>")]).is_err(),
                "{id} accepted unclosed tag"
            );
            assert!(
                run(id, &[("input", "")]).is_err(),
                "{id} accepted empty input"
            );
        }
    }

    // ── list-converter (vectors from list-converter.models.test.ts) ─────

    /// it-tools has no case picker — it lists every case at once, so this now
    /// asserts on the lines of one output rather than on a `case` field.
    #[test]
    fn every_case_is_covered() {
        let out = ok("case-converter", &[("input", "lorem ipsum dolor sit amet")]);
        let expect = [
            "Lowercase: lorem ipsum dolor sit amet",
            "Uppercase: LOREM IPSUM DOLOR SIT AMET",
            "Camelcase: loremIpsumDolorSitAmet",
            "Capitalcase: Lorem Ipsum Dolor Sit Amet",
            "Constantcase: LOREM_IPSUM_DOLOR_SIT_AMET",
            "Dotcase: lorem.ipsum.dolor.sit.amet",
            "Headercase: Lorem-Ipsum-Dolor-Sit-Amet",
            "Nocase: lorem ipsum dolor sit amet",
            "Paramcase: lorem-ipsum-dolor-sit-amet",
            "Pascalcase: LoremIpsumDolorSitAmet",
            "Pathcase: lorem/ipsum/dolor/sit/amet",
            "Sentencecase: Lorem ipsum dolor sit amet",
            "Snakecase: lorem_ipsum_dolor_sit_amet",
            "Mockingcase: LoReM IpSuM DoLoR SiT AmEt",
        ];
        assert_eq!(out.lines().collect::<Vec<_>>(), expect);
        assert_eq!(CASES.len(), expect.len());
    }

    #[test]
    fn roman_numeral_vectors() {
        let cases = [
            (1, "I"),
            (2, "II"),
            (3, "III"),
            (4, "IV"),
            (5, "V"),
            (6, "VI"),
            (7, "VII"),
            (8, "VIII"),
            (9, "IX"),
            (10, "X"),
            (14, "XIV"),
            (19, "XIX"),
            (24, "XXIV"),
            (29, "XXIX"),
            (40, "XL"),
            (50, "L"),
            (90, "XC"),
            (400, "CD"),
            (500, "D"),
            (900, "CM"),
            (999, "CMXCIX"),
            (1000, "M"),
            (2000, "MM"),
            (3999, "MMMCMXCIX"),
        ];
        // Two panels, one input each — no `mode` select. Both fields carry
        // it-tools' prefilled defaults, so the unused one is blanked here.
        for (n, want) in cases {
            assert_eq!(
                ok(
                    "roman-numeral-converter",
                    &[("arabic", &n.to_string()), ("roman", "")]
                ),
                format!("Arabic to roman:\n{want}"),
                "{n}"
            );
            assert_eq!(
                ok(
                    "roman-numeral-converter",
                    &[("arabic", ""), ("roman", want)]
                ),
                format!("Roman to arabic:\n{n}")
            );
        }
        // floating point truncates down
        for (input, want) in [("1.9", "I"), ("17.6", "XVII"), ("29.999", "XXIX")] {
            assert_eq!(
                ok(
                    "roman-numeral-converter",
                    &[("arabic", input), ("roman", "")]
                ),
                format!("Arabic to roman:\n{want}")
            );
        }
        // both at once, and one bad side does not hide the good one
        assert_eq!(
            ok(
                "roman-numeral-converter",
                &[("arabic", "42"), ("roman", "X")]
            ),
            "Arabic to roman:\nXLII\n\nRoman to arabic:\n10"
        );
        let out = ok(
            "roman-numeral-converter",
            &[("arabic", "42"), ("roman", "IIII")],
        );
        assert!(out.contains("Arabic to roman:\nXLII"), "{out}");
        assert!(out.contains("not a valid roman number"), "{out}");
    }

    #[test]
    fn roman_numeral_out_of_range_and_garbage() {
        for bad in ["0", "0.9", "-42", "4000", "10000", "3999.1", "abc"] {
            let out = ok("roman-numeral-converter", &[("arabic", bad), ("roman", "")]);
            assert!(out.contains('('), "{bad:?} accepted: {out}");
        }
        for bad in ["IIII", "ABC", "XM"] {
            let out = ok("roman-numeral-converter", &[("arabic", ""), ("roman", bad)]);
            assert!(
                out.contains("not a valid roman number"),
                "{bad:?} accepted: {out}"
            );
        }
        // nothing typed anywhere is the only hard error
        assert!(run("roman-numeral-converter", &[("arabic", ""), ("roman", "")]).is_err());
    }
}
