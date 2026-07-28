//! `phone-parser-and-formatter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;

/// ponytail: libphonenumber is not available, so this is a deliberate subset —
/// strip formatting, split the calling code off with the table below, validate
/// the E.164 shape. It cannot tell mobile from fixed line or validate national
/// numbering plans. Upgrade path: a libphonenumber port, when someone needs it.
pub(crate) static CALLING_CODES: &[(&str, &str)] = &[
    ("1", "United States / Canada (NANP)"),
    ("7", "Russia / Kazakhstan"),
    ("20", "Egypt"),
    ("27", "South Africa"),
    ("30", "Greece"),
    ("31", "Netherlands"),
    ("32", "Belgium"),
    ("33", "France"),
    ("34", "Spain"),
    ("36", "Hungary"),
    ("39", "Italy"),
    ("40", "Romania"),
    ("41", "Switzerland"),
    ("43", "Austria"),
    ("44", "United Kingdom"),
    ("45", "Denmark"),
    ("46", "Sweden"),
    ("47", "Norway"),
    ("48", "Poland"),
    ("49", "Germany"),
    ("51", "Peru"),
    ("52", "Mexico"),
    ("53", "Cuba"),
    ("54", "Argentina"),
    ("55", "Brazil"),
    ("56", "Chile"),
    ("57", "Colombia"),
    ("58", "Venezuela"),
    ("60", "Malaysia"),
    ("61", "Australia"),
    ("62", "Indonesia"),
    ("63", "Philippines"),
    ("64", "New Zealand"),
    ("65", "Singapore"),
    ("66", "Thailand"),
    ("81", "Japan"),
    ("82", "South Korea"),
    ("84", "Vietnam"),
    ("86", "China"),
    ("90", "Turkey"),
    ("91", "India"),
    ("92", "Pakistan"),
    ("93", "Afghanistan"),
    ("94", "Sri Lanka"),
    ("95", "Myanmar"),
    ("98", "Iran"),
    ("212", "Morocco"),
    ("213", "Algeria"),
    ("216", "Tunisia"),
    ("218", "Libya"),
    ("220", "Gambia"),
    ("221", "Senegal"),
    ("234", "Nigeria"),
    ("237", "Cameroon"),
    ("251", "Ethiopia"),
    ("254", "Kenya"),
    ("255", "Tanzania"),
    ("256", "Uganda"),
    ("263", "Zimbabwe"),
    ("350", "Gibraltar"),
    ("351", "Portugal"),
    ("352", "Luxembourg"),
    ("353", "Ireland"),
    ("354", "Iceland"),
    ("355", "Albania"),
    ("356", "Malta"),
    ("357", "Cyprus"),
    ("358", "Finland"),
    ("359", "Bulgaria"),
    ("370", "Lithuania"),
    ("371", "Latvia"),
    ("372", "Estonia"),
    ("373", "Moldova"),
    ("374", "Armenia"),
    ("375", "Belarus"),
    ("376", "Andorra"),
    ("377", "Monaco"),
    ("378", "San Marino"),
    ("380", "Ukraine"),
    ("381", "Serbia"),
    ("382", "Montenegro"),
    ("385", "Croatia"),
    ("386", "Slovenia"),
    ("387", "Bosnia and Herzegovina"),
    ("389", "North Macedonia"),
    ("420", "Czechia"),
    ("421", "Slovakia"),
    ("423", "Liechtenstein"),
    ("501", "Belize"),
    ("502", "Guatemala"),
    ("503", "El Salvador"),
    ("504", "Honduras"),
    ("505", "Nicaragua"),
    ("506", "Costa Rica"),
    ("507", "Panama"),
    ("591", "Bolivia"),
    ("593", "Ecuador"),
    ("595", "Paraguay"),
    ("598", "Uruguay"),
    ("852", "Hong Kong"),
    ("853", "Macau"),
    ("855", "Cambodia"),
    ("856", "Laos"),
    ("880", "Bangladesh"),
    ("886", "Taiwan"),
    ("960", "Maldives"),
    ("961", "Lebanon"),
    ("962", "Jordan"),
    ("963", "Syria"),
    ("964", "Iraq"),
    ("965", "Kuwait"),
    ("966", "Saudi Arabia"),
    ("967", "Yemen"),
    ("968", "Oman"),
    ("970", "Palestine"),
    ("971", "United Arab Emirates"),
    ("972", "Israel"),
    ("973", "Bahrain"),
    ("974", "Qatar"),
    ("975", "Bhutan"),
    ("976", "Mongolia"),
    ("977", "Nepal"),
    ("992", "Tajikistan"),
    ("993", "Turkmenistan"),
    ("994", "Azerbaijan"),
    ("995", "Georgia"),
    ("996", "Kyrgyzstan"),
    ("998", "Uzbekistan"),
];

/// ISO 3166-1 alpha-2 to calling code, for the countries [`CALLING_CODES`]
/// knows (it-tools PR #1107). Users think in `DE`, not in `49`; the field takes
/// either.
///
/// ponytail: one entry per country in the table above and no more. A code that
/// is not here is refused rather than guessed, which is the same ceiling the
/// calling-code table already has.
pub(crate) static ISO_COUNTRIES: &[(&str, &str)] = &[
    ("AD", "376"),
    ("AE", "971"),
    ("AF", "93"),
    ("AL", "355"),
    ("AM", "374"),
    ("AR", "54"),
    ("AT", "43"),
    ("AU", "61"),
    ("AZ", "994"),
    ("BA", "387"),
    ("BD", "880"),
    ("BE", "32"),
    ("BG", "359"),
    ("BH", "973"),
    ("BO", "591"),
    ("BR", "55"),
    ("BT", "975"),
    ("BY", "375"),
    ("BZ", "501"),
    ("CA", "1"),
    ("CH", "41"),
    ("CL", "56"),
    ("CM", "237"),
    ("CN", "86"),
    ("CO", "57"),
    ("CR", "506"),
    ("CU", "53"),
    ("CY", "357"),
    ("CZ", "420"),
    ("DE", "49"),
    ("DK", "45"),
    ("DZ", "213"),
    ("EC", "593"),
    ("EE", "372"),
    ("EG", "20"),
    ("ES", "34"),
    ("ET", "251"),
    ("FI", "358"),
    ("FR", "33"),
    ("GB", "44"),
    ("GE", "995"),
    ("GI", "350"),
    ("GM", "220"),
    ("GR", "30"),
    ("GT", "502"),
    ("HK", "852"),
    ("HN", "504"),
    ("HR", "385"),
    ("HU", "36"),
    ("ID", "62"),
    ("IE", "353"),
    ("IL", "972"),
    ("IN", "91"),
    ("IQ", "964"),
    ("IR", "98"),
    ("IS", "354"),
    ("IT", "39"),
    ("JO", "962"),
    ("JP", "81"),
    ("KE", "254"),
    ("KG", "996"),
    ("KH", "855"),
    ("KR", "82"),
    ("KW", "965"),
    ("KZ", "7"),
    ("LA", "856"),
    ("LB", "961"),
    ("LI", "423"),
    ("LK", "94"),
    ("LT", "370"),
    ("LU", "352"),
    ("LV", "371"),
    ("LY", "218"),
    ("MA", "212"),
    ("MC", "377"),
    ("MD", "373"),
    ("ME", "382"),
    ("MK", "389"),
    ("MM", "95"),
    ("MN", "976"),
    ("MO", "853"),
    ("MT", "356"),
    ("MV", "960"),
    ("MX", "52"),
    ("MY", "60"),
    ("NG", "234"),
    ("NI", "505"),
    ("NL", "31"),
    ("NO", "47"),
    ("NP", "977"),
    ("NZ", "64"),
    ("OM", "968"),
    ("PA", "507"),
    ("PE", "51"),
    ("PH", "63"),
    ("PK", "92"),
    ("PL", "48"),
    ("PS", "970"),
    ("PT", "351"),
    ("PY", "595"),
    ("QA", "974"),
    ("RO", "40"),
    ("RS", "381"),
    ("RU", "7"),
    ("SA", "966"),
    ("SE", "46"),
    ("SG", "65"),
    ("SI", "386"),
    ("SK", "421"),
    ("SM", "378"),
    ("SN", "221"),
    ("SV", "503"),
    ("SY", "963"),
    ("TH", "66"),
    ("TJ", "992"),
    ("TM", "993"),
    ("TN", "216"),
    ("TR", "90"),
    ("TW", "886"),
    ("TZ", "255"),
    ("UA", "380"),
    ("UG", "256"),
    ("US", "1"),
    ("UY", "598"),
    ("UZ", "998"),
    ("VE", "58"),
    ("VN", "84"),
    ("YE", "967"),
    ("ZA", "27"),
    ("ZW", "263"),
];

pub(crate) fn phone_parser(args: &Args) -> Result<String> {
    let t = tool("phone-parser-and-formatter");
    let raw = arg(args, t, "phone").trim();
    if raw.is_empty() {
        bail!("no phone number provided");
    }
    // The field takes `DE` or `49`; anything else is refused, so a typo does not
    // silently produce a number in the wrong country.
    let typed = arg(args, t, "default_calling_code").trim();
    let default_code: String = if typed.chars().any(|c| c.is_ascii_alphabetic()) {
        let iso = typed.to_ascii_uppercase();
        ISO_COUNTRIES
            .iter()
            .find(|(c, _)| *c == iso)
            .map(|(_, code)| (*code).to_string())
            .with_context(|| format!("unknown country code {typed:?}: use e.g. DE or 49"))?
    } else {
        typed.chars().filter(char::is_ascii_digit).collect()
    };
    let mut digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    let international = raw.trim_start().starts_with('+') || digits.starts_with("00");
    if digits.starts_with("00") {
        digits = digits[2..].to_string();
    }
    if !international {
        if default_code.is_empty() {
            bail!("no calling code: prefix the number with + or set a default calling code");
        }
        digits = format!("{default_code}{}", digits.trim_start_matches('0'));
    }
    if digits.is_empty() {
        bail!("no digits in {raw:?}");
    }
    let (code, country) = (1..=3)
        .rev()
        .filter(|n| digits.len() > *n)
        .find_map(|n| CALLING_CODES.iter().find(|(c, _)| *c == &digits[..n]))
        .copied()
        .context("unrecognised country calling code")?;
    let national = &digits[code.len()..];
    let valid = (8..=15).contains(&digits.len()) && !national.is_empty();
    let yes_no = |b: bool| if b { "Yes" } else { "No" };
    // it-tools' row set and order. Rows libphonenumber supplies and this subset
    // cannot say "Unknown", which is also what it-tools shows for a nil value.
    let mut out = String::new();
    let _ = writeln!(out, "Country: {country}");
    let _ = writeln!(out, "Country calling code: {code}");
    let _ = writeln!(out, "Is valid?: {}", yes_no(valid));
    let _ = writeln!(out, "Is possible?: {}", yes_no(!national.is_empty()));
    let _ = writeln!(out, "Type: Unknown");
    let _ = writeln!(out, "International format: +{code} {national}");
    let _ = writeln!(out, "National number: {national}");
    let _ = writeln!(out, "E.164 format: +{digits}");
    let _ = writeln!(out, "RFC3966 format: tel:+{digits}");
    // it-tools PR #1107: the three links you would otherwise assemble by hand.
    // wa.me wants the E.164 digits with no `+`.
    let _ = writeln!(out, "\nLinks:");
    let _ = writeln!(out, "  tel:+{digits}");
    let _ = writeln!(out, "  sms:+{digits}");
    let _ = writeln!(out, "  https://wa.me/{digits}");
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "phone-parser-and-formatter",
    name: "Phone parser and formatter",
    description: "Parse, validate and format a phone number: country calling code, national number and E.164.",
    category: Category::Data,
    keywords: &[
        "phone",
        "number",
        "parser",
        "formatter",
        "e164",
        "country",
        "international",
    ],
    fields: &[
        Field::text("phone", "Phone number"),
        Field::text(
            "default_calling_code",
            "Default country for numbers without + (DE or 49)",
        )
        .with_default("DE"),
    ],
    kind: Kind::Pure,
    run: phone_parser,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn phone_parser_subset() {
        let out = run(
            "phone-parser-and-formatter",
            &[("phone", "+49 30 1234 5678"), ("default_calling_code", "")],
        )
        .unwrap();
        assert!(out.contains("E.164 format: +493012345678"), "{out}");
        assert!(out.contains("RFC3966 format: tel:+493012345678"), "{out}");
        assert!(out.contains("Country calling code: 49"), "{out}");
        assert!(out.contains("Country: Germany"), "{out}");
        assert!(out.contains("National number: 3012345678"), "{out}");
        assert!(out.contains("Is valid?: Yes"), "{out}");

        // 00 prefix works like +
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "0033123456789"), ("default_calling_code", "")]
            )
            .unwrap()
            .contains("Country: France")
        );
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "030 12345678"), ("default_calling_code", "49")]
            )
            .unwrap()
            .contains("E.164 format: +493012345678")
        );
        // National format needs the default code
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "030 12345678"), ("default_calling_code", "")]
            )
            .is_err()
        );
        // Longest-prefix match beats the shorter one.
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "+420123456789"), ("default_calling_code", "")]
            )
            .unwrap()
            .contains("Czechia")
        );
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "+99900011122"), ("default_calling_code", "")]
            )
            .is_err()
        );
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", ""), ("default_calling_code", "")]
            )
            .is_err()
        );
    }

    #[test]
    fn iso_default_country_and_links() {
        // The field's default: a national number parses without a `+`.
        let out = ok("phone-parser-and-formatter", &[("phone", "030 12345678")]);
        assert!(out.contains("E.164 format: +493012345678"), "{out}");
        assert!(out.contains("  tel:+493012345678"), "{out}");
        assert!(out.contains("  sms:+493012345678"), "{out}");
        assert!(out.contains("  https://wa.me/493012345678"), "{out}");
        // Lower case and the numeric spelling both work.
        for code in ["de", "DE", "49"] {
            let out = ok(
                "phone-parser-and-formatter",
                &[("phone", "030 12345678"), ("default_calling_code", code)],
            );
            assert!(out.contains("Country: Germany"), "{code}: {out}");
        }
        assert!(
            run(
                "phone-parser-and-formatter",
                &[("phone", "030 12345678"), ("default_calling_code", "XX")]
            )
            .is_err()
        );
    }
}
