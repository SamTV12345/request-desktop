//! Text, math and misc tools ported from it-tools. Owned by the text batch.
//!
//! ponytail: flat module — one `fn` per tool plus one `static` entry, no traits,
//! no macros. Anything that needs a webcam, a live timer, a key listener or a
//! rich-text surface is `Kind::Custom` with a `run` that errors, so the generic
//! form is skipped instead of rendered broken.

pub use rp_devtools_common::*;

mod ai_prompt_splitter;
mod ascii_text_drawer;
mod benchmark_builder;
mod camera_recorder;
mod chronometer;
mod color_converter;
mod common;
mod date_duration_calculator;
mod date_time_converter;
mod dice_roller;
mod emoji_picker;
mod eta_calculator;
mod folder_tree_diagram;
mod git_memo;
mod hdd_size_calculator;
mod html_wysiwyg_editor;
mod keycode_info;
mod lorem_ipsum_generator;
mod math_evaluator;
mod morse_converter;
mod numeronym_generator;
mod percentage_calculator;
mod qr_code_generator;
mod raid_calculator;
mod regex_memo;
mod regex_tester;
mod smart_text_replacer;
mod stacktrace_formatter;
mod svg_placeholder_generator;
mod temperature_converter;
mod testing;
mod text_diff;
mod text_statistics;
mod text_to_nato_alphabet;
mod ttl_calculator;
mod wifi_qr_code_generator;

// Every tool file opens with `use crate::*;`, so the crate root is this
// crate's prelude: the shared types plus any helper one tool lends another.
#[allow(unused_imports)]
pub(crate) use {
    ai_prompt_splitter::*, ascii_text_drawer::*, benchmark_builder::*, camera_recorder::*,
    chronometer::*, color_converter::*, common::*, date_duration_calculator::*,
    date_time_converter::*, dice_roller::*, emoji_picker::*, eta_calculator::*,
    folder_tree_diagram::*, git_memo::*, hdd_size_calculator::*, html_wysiwyg_editor::*,
    keycode_info::*, lorem_ipsum_generator::*, math_evaluator::*, morse_converter::*,
    numeronym_generator::*, percentage_calculator::*, qr_code_generator::*, raid_calculator::*,
    regex_memo::*, regex_tester::*, smart_text_replacer::*, stacktrace_formatter::*,
    svg_placeholder_generator::*, temperature_converter::*, text_diff::*, text_statistics::*,
    text_to_nato_alphabet::*, ttl_calculator::*, wifi_qr_code_generator::*,
};

/// Reached for by name from `crates/app`'s bespoke panes.
pub use common::{EMOJIS, emoji_search};

/// Every tool in this crate, in registry order.
pub static TOOLS: &[&Tool] = &[
    &text_diff::TOOL,
    &text_statistics::TOOL,
    &text_to_nato_alphabet::TOOL,
    &lorem_ipsum_generator::TOOL,
    &numeronym_generator::TOOL,
    &regex_tester::TOOL,
    &regex_memo::TOOL,
    &git_memo::TOOL,
    &math_evaluator::TOOL,
    &percentage_calculator::TOOL,
    &eta_calculator::TOOL,
    &temperature_converter::TOOL,
    &date_time_converter::TOOL,
    &chronometer::TOOL,
    &benchmark_builder::TOOL,
    &color_converter::TOOL,
    &emoji_picker::TOOL,
    &ascii_text_drawer::TOOL,
    &keycode_info::TOOL,
    &svg_placeholder_generator::TOOL,
    &qr_code_generator::TOOL,
    &wifi_qr_code_generator::TOOL,
    &camera_recorder::TOOL,
    &html_wysiwyg_editor::TOOL,
    &stacktrace_formatter::TOOL,
    &smart_text_replacer::TOOL,
    &morse_converter::TOOL,
    &folder_tree_diagram::TOOL,
    &ai_prompt_splitter::TOOL,
    &ttl_calculator::TOOL,
    &raid_calculator::TOOL,
    &hdd_size_calculator::TOOL,
    &dice_roller::TOOL,
    &date_duration_calculator::TOOL,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;
    use chrono::{DateTime, Datelike};

    // ---- registry invariants

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate tool id");
    }

    #[test]
    fn all_expected_tools_registered() {
        for id in [
            "text-diff",
            "text-statistics",
            "text-to-nato-alphabet",
            "lorem-ipsum-generator",
            "numeronym-generator",
            "regex-tester",
            "regex-memo",
            "git-memo",
            "math-evaluator",
            "percentage-calculator",
            "eta-calculator",
            "temperature-converter",
            "date-time-converter",
            "chronometer",
            "benchmark-builder",
            "color-converter",
            "emoji-picker",
            "ascii-text-drawer",
            "keycode-info",
            "svg-placeholder-generator",
            "qr-code-generator",
            "wifi-qr-code-generator",
            "camera-recorder",
            "html-wysiwyg-editor",
            "ttl-calculator",
            "raid-calculator",
            "hdd-size-calculator",
            "dice-roller",
            "date-duration-calculator",
        ] {
            assert!(TOOLS.iter().any(|t| t.id == id), "missing tool {id}");
        }
        assert_eq!(TOOLS.len(), 34);
    }

    #[test]
    fn run_on_empty_args_never_panics() {
        let empty = Args::new();
        for t in TOOLS {
            let _ = (t.run)(&empty); // Err is fine, a panic is not
        }
    }

    #[test]
    fn custom_tools_error_out() {
        for id in [
            "chronometer",
            "emoji-picker",
            "keycode-info",
            "camera-recorder",
            "html-wysiwyg-editor",
        ] {
            let t = TOOLS.iter().find(|t| t.id == id).unwrap();
            assert_eq!(t.kind, Kind::Custom, "{id} should be Custom");
            assert!(
                (t.run)(&Args::new()).is_err(),
                "{id} should not produce output"
            );
        }
    }

    #[test]
    fn select_defaults_are_valid_options() {
        for t in TOOLS {
            for f in t.fields {
                if !f.options.is_empty() {
                    assert!(
                        f.options.contains(&f.default),
                        "{}: default `{}` not in options",
                        t.id,
                        f.default
                    );
                }
            }
        }
    }

    // ---- text-statistics (it-tools vectors)

    #[test]
    fn string_size_in_bytes_vectors() {
        assert_eq!(string_size_in_bytes(""), 0);
        assert_eq!(string_size_in_bytes("a"), 1);
        assert_eq!(string_size_in_bytes("aa"), 2);
        assert_eq!(string_size_in_bytes("😀"), 4);
        assert_eq!(string_size_in_bytes("aaaaaaaaaa"), 10);
    }

    #[test]
    fn text_stats_counts() {
        assert_eq!(text_stats(""), (0, 0, 0, 0));
        assert_eq!(text_stats("hello world"), (11, 2, 1, 11));
        assert_eq!(text_stats("a\nb\nc"), (5, 3, 3, 5));
        // it-tools counts `text.split(/\s+/)`, so a leading space is a word.
        assert_eq!(text_stats(" hi").1, 2);
        assert_eq!(text_stats("  ").1, 2);
        assert_eq!(text_stats("hi").1, 1);
        // Interface change: the four tiles exist for any input, empty included,
        // so this no longer errors out the way the old `nonempty` guard did.
        assert!(
            run("text-statistics", &[])
                .unwrap()
                .contains("Character count: 0")
        );
        assert!(
            run("text-statistics", &[("text", "hi")])
                .unwrap()
                .contains("Character count: 2")
        );
    }

    // ---- numeronym (it-tools vectors)

    #[test]
    fn numeronym_vectors() {
        assert_eq!(generate_numeronym("internationalization"), "i18n");
        assert_eq!(generate_numeronym("accessibility"), "a11y");
        assert_eq!(generate_numeronym("localization"), "l10n");
        assert_eq!(generate_numeronym("abc"), "abc");
        assert_eq!(generate_numeronym(""), "");
        assert_eq!(
            run("numeronym-generator", &[("word", "internationalization")]).unwrap(),
            "i18n"
        );
        assert!(run("numeronym-generator", &[]).is_err());
    }

    // ---- lorem ipsum

    #[test]
    fn lorem_shapes() {
        let latin = lorem_lang("latin");
        let out = generate_lorem_ipsum(2, 3, 5, true, false, latin);
        assert!(out.starts_with(LOREM_FIRST));
        assert_eq!(out.split("\n\n").count(), 2);

        let html = generate_lorem_ipsum(2, 1, 4, false, true, latin);
        assert!(html.starts_with("<p>") && html.ends_with("</p>"));

        assert!(run("lorem-ipsum-generator", &[("paragraphs", "0")]).is_err());
        assert!(run("lorem-ipsum-generator", &[("words", "abc")]).is_err());
        // Defaults must work with no args at all.
        assert!(
            run("lorem-ipsum-generator", &[])
                .unwrap()
                .starts_with(LOREM_FIRST)
        );
    }

    /// it-tools PR #1184: other scripts, same generator.
    #[test]
    fn lorem_languages() {
        // Every option is a real alphabet, and none of them is Latin's.
        for id in LOREM_LANGUAGES {
            let lang = lorem_lang(id);
            assert_eq!(lang.id, *id, "`{id}` has no vocabulary");
            assert!(lang.vocab.len() >= 20, "{id}: vocabulary too thin");
            let out = ok("lorem-ipsum-generator", &[("language", id), ("words", "4")]);
            assert!(out.starts_with(lang.opening), "{id}: {out}");
            assert!(out.trim_end().ends_with(lang.end), "{id}: {out}");
        }
        // A half-typed or unknown language keeps rendering, in Latin.
        assert_eq!(lorem_lang("klingon").id, "latin");
        assert_eq!(lorem_lang("").id, "latin");

        // Multi-byte scripts must survive the capitalisation of the first
        // character; slicing at byte 1 used to be enough only for ASCII.
        for id in ["cyrillic", "greek", "cjk"] {
            let body = generate_lorem_ipsum(1, 2, 6, false, false, lorem_lang(id));
            assert!(!body.is_empty(), "{id}");
        }
        // CJK sets no space between words.
        assert!(!generate_lorem_ipsum(1, 1, 6, false, false, lorem_lang("cjk")).contains(' '));
    }

    // ---- regex tester (it-tools vectors)

    #[test]
    fn regex_tester_vectors() {
        assert_eq!(regex_matches("", "", "").unwrap(), vec![]);
        assert_eq!(regex_matches(".*", "", "").unwrap(), vec![]);
        assert_eq!(regex_matches("", "aaa", "").unwrap(), vec![]);

        let m = regex_matches("a", "baaa", "").unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].index, 1);
        assert_eq!(m[0].value, "a");
        assert!(m[0].captures.is_empty() && m[0].groups.is_empty());

        let m = regex_matches("(.)(?<g>r)", "azertyr", "g").unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].index, 2);
        assert_eq!(m[0].value, "er");
        // `at` is `Some` for a group that took part; see `Capture` for why an
        // absent group is a row of its own rather than a missing one.
        assert_eq!(
            m[0].captures
                .iter()
                .map(|c| (c.name.as_str(), c.at.clone()))
                .collect::<Vec<_>>(),
            [
                ("1", Some(("e".to_string(), 2, 3))),
                ("2", Some(("r".to_string(), 3, 4))),
            ]
        );
        assert_eq!(
            m[0].groups,
            vec![Capture {
                name: "g".into(),
                at: Some(("r".to_string(), 3, 4)),
            }]
        );
        assert_eq!(m[1].index, 5);
        assert_eq!(m[1].value, "yr");
    }

    #[test]
    fn regex_tester_bad_input() {
        assert!(regex_matches("(unclosed", "x", "").is_err());
        // Nothing to test at all: still the one case that errors out.
        assert!(run("regex-tester", &[]).is_err());
        // Interface change: an invalid pattern reports itself inside the Matches
        // panel instead of taking the whole output down, as it-tools does.
        let out = run("regex-tester", &[("regex", "[")]).unwrap();
        assert!(out.starts_with("Matches:"), "{out}");
        assert!(out.contains("invalid regex"), "{out}");
        assert!(
            run("regex-tester", &[("regex", "z"), ("text", "abc")])
                .unwrap()
                .contains("No match.")
        );
        assert!(
            run(
                "regex-tester",
                &[("regex", "A"), ("text", "abc"), ("ignore_case", "true")]
            )
            .unwrap()
            .contains("1 match")
        );
        // `global` and `dot_all` default on, like the upstream checkboxes.
        assert!(
            run("regex-tester", &[("regex", "a"), ("text", "baaa")])
                .unwrap()
                .contains("3 match")
        );
        assert!(
            run(
                "regex-tester",
                &[("regex", "a"), ("text", "baaa"), ("global", "false")]
            )
            .unwrap()
            .contains("1 match")
        );
    }

    // ---- memos

    #[test]
    fn memos_ignore_input() {
        assert!(run("regex-memo", &[]).unwrap().contains("word boundary"));
        assert!(
            run("git-memo", &[("noise", "x")])
                .unwrap()
                .contains("git commit --amend")
        );
    }

    // ---- temperature converter (it-tools formulas)

    #[test]
    fn temperature_vectors() {
        assert_eq!(to_kelvin("celsius", 0.0).unwrap(), 273.15);
        assert_eq!(from_kelvin("celsius", 273.15).unwrap(), 0.0);
        assert_eq!(to_kelvin("rankine", 0.0).unwrap(), 0.0);
        assert_eq!(from_kelvin("rankine", 100.0).unwrap(), 180.0);
        assert_eq!(to_kelvin("delisle", 0.0).unwrap(), 373.15);
        assert_eq!(from_kelvin("delisle", 373.15).unwrap(), 0.0);
        assert_eq!(to_kelvin("reaumur", 0.0).unwrap(), 273.15);
        assert_eq!(from_kelvin("reaumur", 273.15).unwrap(), 0.0);
        assert_eq!(to_kelvin("romer", 7.5).unwrap(), 273.15);
        assert_eq!(from_kelvin("romer", 273.15).unwrap(), 7.5);
        assert!((to_kelvin("fahrenheit", 32.0).unwrap() - 273.15).abs() < 1e-9);
        assert!((from_kelvin("fahrenheit", 273.15).unwrap() - 32.0).abs() < 1e-9);
        assert!((to_kelvin("newton", 33.0).unwrap() - 373.15).abs() < 1e-9);
        assert!((from_kelvin("newton", 373.15).unwrap() - 33.0).abs() < 1e-9);
        assert!(to_kelvin("bogus", 0.0).is_err());

        let out = run(
            "temperature-converter",
            &[("value", "100"), ("from", "celsius")],
        )
        .unwrap();
        // Titles as in the `.vue`'s `units` record; the float noise of
        // 211.99999999999997 is rounded off rather than printed.
        assert!(out.contains("Kelvin: 373.15 K"), "{out}");
        assert!(out.contains("Fahrenheit: 212 °F"), "{out}");
        assert!(out.contains("Rømer: 60 °Rø"), "{out}");
        assert!(run("temperature-converter", &[("value", "hot")]).is_err());
    }

    /// it-tools PR #1491: the fixed two decimals threw away small differences.
    #[test]
    fn temperature_keeps_significant_digits() {
        let out = run(
            "temperature-converter",
            &[("value", "0.001"), ("from", "celsius")],
        )
        .unwrap();
        assert!(out.contains("Celsius: 0.001 °C"), "{out}");
        assert!(out.contains("Kelvin: 273.151 K"), "{out}");
        // …without the artefacts that dropping the rounding altogether creates.
        let out = run(
            "temperature-converter",
            &[("value", "37"), ("from", "celsius")],
        )
        .unwrap();
        assert!(out.contains("Fahrenheit: 98.6 °F"), "{out}");
        assert!(!out.contains("0000000"), "float noise leaked: {out}");
    }

    /// it-tools PR #1153: below 0 K the eight rows are fiction.
    #[test]
    fn temperature_below_absolute_zero_is_refused() {
        let e = run(
            "temperature-converter",
            &[("value", "-300"), ("from", "celsius")],
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("below absolute zero"), "{e}");
        assert!(e.contains("-273.15 °C"), "{e}");
        // Exactly 0 K is a temperature and still converts.
        assert!(
            run(
                "temperature-converter",
                &[("value", "-273.15"), ("from", "celsius")]
            )
            .unwrap()
            .contains("Kelvin: 0 K")
        );
        // Delisle runs backwards, so its limit is an upper one.
        assert!(
            run(
                "temperature-converter",
                &[("value", "600"), ("from", "delisle")]
            )
            .unwrap_err()
            .to_string()
            .contains("below absolute zero")
        );
    }

    // ---- date-time converter (it-tools vectors)

    #[test]
    fn iso8601_matcher() {
        for ok in [
            "2021-01-01T00:00:00.000Z",
            "2023-04-12T14:56:00+01:00",
            "20230412T145600+0100",
            "20230412T145600Z",
            "2016-02-01",
            "2016",
        ] {
            assert!(is_iso8601(ok), "{ok} should be ISO 8601");
        }
        for bad in ["", "qsdqsd", "2016-02-01-", "2021-01-01T00:00:00."] {
            assert!(!is_iso8601(bad), "{bad} should not be ISO 8601");
        }
    }

    #[test]
    fn iso9075_matcher() {
        for ok in [
            "2022-01-01 12:00:00Z",
            "2022-01-01 12:00:00.123456Z",
            "2022-01-01 12:00:00+01:00",
            "2022-01-01 12:00:00-05:00",
        ] {
            assert!(is_iso9075(ok), "{ok}");
        }
        for bad in [
            "2022/01/01T12:00:00Z",
            "2022-01-01 12:00:00.123456789Z",
            "2022-01-01 12:00:00+1:00",
            "2022-01-01 12:00:00-05:",
            "2022-01-01 12:00:00-05:00:00",
            "2022-01-01",
            "12:00:00Z",
            "2022-01-01T12:00:00Zfoo",
        ] {
            assert!(!is_iso9075(bad), "{bad}");
        }
    }

    #[test]
    fn rfc3339_matcher() {
        for ok in [
            "2022-01-01T12:00:00Z",
            "2022-01-01T12:00:00.123456789Z",
            "2022-01-01T12:00:00.123456789+01:00",
            "2022-01-01T12:00:00-05:00",
        ] {
            assert!(is_rfc3339(ok), "{ok}");
        }
        for bad in [
            "2022/01/01T12:00:00Z",
            "2022-01-01T12:00:00.123456789+1:00",
            "2022-01-01T12:00:00-05:",
            "2022-01-01T12:00:00-05:00:00",
            "2022-01-01",
            "12:00:00Z",
            "2022-01-01T12:00:00Zfoo",
        ] {
            assert!(!is_rfc3339(bad), "{bad}");
        }
    }

    #[test]
    fn rfc7231_and_utc_matchers() {
        for ok in [
            "Sun, 06 Nov 1994 08:49:37 GMT",
            "Tue, 22 Apr 2014 07:00:00 GMT",
        ] {
            assert!(is_rfc7231(ok), "{ok}");
            assert!(is_utc_date_string(ok), "{ok}");
        }
        for bad in [
            "06 Nov 1994 08:49:37 GMT",
            "Sun, 06 Nov 94 08:49:37 GMT",
            "Sun, 06 Nov 1994 8:49:37 GMT",
            "Sun, 06 Nov 1994 08:49:37 GMT-0500",
            "Sun, 06 November 1994 08:49:37 GMT",
            "Sunday, 06 Nov 1994 08:49:37 GMT",
            "06 Nov 1994",
        ] {
            assert!(!is_rfc7231(bad), "{bad}");
        }
        for bad in ["06 Nov 1994 08:49:37 GMT", "16497920261", "foo", ""] {
            assert!(!is_utc_date_string(bad), "{bad}");
        }
    }

    #[test]
    fn timestamp_and_id_matchers() {
        assert!(is_unix_timestamp("1649789394") && is_unix_timestamp("1234567890"));
        assert!(is_unix_timestamp("0"));
        assert!(!is_unix_timestamp("foo") && !is_unix_timestamp(""));

        assert!(
            is_timestamp("1649792026123") && is_timestamp("1234567890000") && is_timestamp("0")
        );
        assert!(!is_timestamp("foo") && !is_timestamp(""));

        assert!(is_mongo_object_id("507f1f77bcf86cd799439011"));
        assert!(is_mongo_object_id("507f1f77bcf86cd799439012"));
        for bad in [
            "507f1f77bcf86cd79943901",
            "507f1f77bcf86cd79943901z",
            "foo",
            "",
        ] {
            assert!(!is_mongo_object_id(bad), "{bad}");
        }

        for ok in ["0", "1", "1.1", "-1.1", "-1"] {
            assert!(is_excel_format(ok), "{ok}");
        }
        for bad in ["", "foo", "1.1.1"] {
            assert!(!is_excel_format(bad), "{bad}");
        }
    }

    fn utc_ms(iso: &str) -> i64 {
        DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .timestamp_millis()
    }

    #[test]
    fn excel_conversions() {
        assert_eq!(
            date_to_excel_format(utc_ms("2016-05-20T00:00:00.000Z")),
            "42510"
        );
        assert_eq!(
            date_to_excel_format(utc_ms("2016-05-20T12:00:00.000Z")),
            "42510.5"
        );
        assert_eq!(
            date_to_excel_format(utc_ms("2023-10-31T09:26:06.421Z")),
            "45230.39312987268"
        );
        assert_eq!(
            date_to_excel_format(utc_ms("1970-01-01T00:00:00.000Z")),
            "25569"
        );
        assert_eq!(
            date_to_excel_format(utc_ms("1800-01-01T00:00:00.000Z")),
            "-36522"
        );

        assert_eq!(excel_format_to_ms(0.0), utc_ms("1899-12-30T00:00:00.000Z"));
        assert_eq!(excel_format_to_ms(1.0), utc_ms("1899-12-31T00:00:00.000Z"));
        assert_eq!(excel_format_to_ms(2.0), utc_ms("1900-01-01T00:00:00.000Z"));
        assert_eq!(
            excel_format_to_ms(4242.4242),
            utc_ms("1911-08-12T10:10:50.880Z")
        );
        assert_eq!(
            excel_format_to_ms(42738.22626859954),
            utc_ms("2017-01-03T05:25:49.607Z")
        );
        assert_eq!(
            excel_format_to_ms(-1000.0),
            utc_ms("1897-04-04T00:00:00.000Z")
        );
    }

    #[test]
    fn date_format_detection() {
        assert_eq!(
            detect_date_format("2021-01-01T00:00:00.000Z"),
            Some("iso8601")
        );
        // it-tools checks ISO 8601 first and its regex also allows a space
        // separator, so ISO 9075 input is reported as ISO 8601 there too.
        assert_eq!(detect_date_format("2022-01-01 12:00:00Z"), Some("iso8601"));
        assert_eq!(
            detect_date_format("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some("rfc7231")
        );
        assert_eq!(detect_date_format("1649789394"), Some("unix"));
        assert_eq!(detect_date_format("1649792026123"), Some("timestamp"));
        assert_eq!(
            detect_date_format("507f1f77bcf86cd799439011"),
            Some("mongo")
        );
        assert_eq!(detect_date_format("-1.5"), Some("excel"));
        assert_eq!(detect_date_format("nonsense!"), None);
    }

    #[test]
    fn date_parsing_and_rendering() {
        let expected = utc_ms("2021-01-01T00:00:00.000Z");
        for (fmt, input) in [
            ("iso8601", "2021-01-01T00:00:00.000Z"),
            ("rfc3339", "2021-01-01T00:00:00Z"),
            ("iso9075", "2021-01-01 00:00:00"),
            ("unix", "1609459200"),
            ("timestamp", "1609459200000"),
            ("rfc7231", "Fri, 01 Jan 2021 00:00:00 GMT"),
            ("utc", "Fri, 01 Jan 2021 00:00:00 GMT"),
            ("auto", "1609459200000"),
        ] {
            assert_eq!(
                parse_date(fmt, input).unwrap().timestamp_millis(),
                expected,
                "{fmt} / {input}"
            );
        }
        // Mongo ObjectID keeps only the seconds prefix.
        assert_eq!(
            parse_date("mongo", "5fee6600cccccccccccccccc")
                .unwrap()
                .timestamp(),
            0x5fee6600
        );
        assert_eq!(parse_date("iso8601", "2016").unwrap().year(), 2016);
        assert_eq!(
            parse_date("iso8601", "20230412T145600Z")
                .unwrap()
                .timestamp(),
            1681311360
        );

        // A pre-epoch date has no ObjectID; that row alone says so.
        let pre = render_all_date_formats(parse_date("iso8601", "1960-01-01").unwrap());
        assert!(pre.contains("Mongo ObjectID: (no ObjectID"), "{pre}");
        assert!(pre.contains("ISO 8601: 1960-01-01T00:00:00+00:00"), "{pre}");

        let out = render_all_date_formats(parse_date("unix", "1609459200").unwrap());
        assert!(out.contains("ISO 8601: 2021-01-01T00:00:00+00:00"));
        assert!(out.contains("ISO 9075: 2021-01-01 00:00:00"));
        assert!(out.contains("RFC 3339: 2021-01-01T00:00:00Z"));
        assert!(out.contains("RFC 7231: Fri, 01 Jan 2021 00:00:00 GMT"));
        assert!(out.contains("Unix timestamp: 1609459200"));
        assert!(out.contains("Timestamp: 1609459200000"));
        assert!(
            out.contains("JS locale date string: Fri Jan 01 2021 00:00:00 GMT+0000"),
            "{out}"
        );
        assert!(out.contains("Mongo ObjectID: 5fee66000000000000000000"));
        assert!(out.contains("Excel date/time: 44197"));
    }

    #[test]
    fn date_bad_input() {
        // Interface change: blank input shows *now* in every format, as upstream
        // does, instead of erroring.
        assert!(
            run("date-time-converter", &[])
                .unwrap()
                .contains("ISO 8601: ")
        );
        assert!(run("date-time-converter", &[("date", "not a date")]).is_err());
        assert!(
            run(
                "date-time-converter",
                &[("date", "abc"), ("format", "unix")]
            )
            .is_err()
        );
        assert!(run("date-time-converter", &[("date", "1"), ("format", "mongo")]).is_err());
        assert!(
            run(
                "date-time-converter",
                &[("date", "1"), ("format", "martian")]
            )
            .is_err()
        );
        assert!(run("date-time-converter", &[("date", "1609459200")]).is_ok());
    }

    // ---- emoji data

    #[test]
    fn emoji_table_is_searchable() {
        assert!(EMOJIS.len() > 100);
        assert!(
            EMOJIS
                .iter()
                .all(|(e, n, _)| !e.is_empty() && !n.is_empty())
        );
        assert!(emoji_search("rocket").iter().any(|(e, _, _)| *e == "🚀"));
        assert!(emoji_search("laugh").len() >= 2);
        assert_eq!(emoji_search("").len(), EMOJIS.len());
        assert!(emoji_search("zzzzz-nothing").is_empty());
    }
}
