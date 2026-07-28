//! `iban-validator-and-parser`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};
use std::fmt::Write as _;

/// `friendlyFormatIBAN`: groups of four, space separated.
pub(crate) fn friendly_iban(iban: &str) -> String {
    iban.as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// `isQRIBAN`: a Swiss or Liechtenstein IBAN whose institution id is a QR-IID.
pub(crate) fn is_qr_iban(iban: &str) -> bool {
    (iban.starts_with("CH") || iban.starts_with("LI"))
        && iban
            .get(4..9)
            .and_then(|iid| iid.parse::<u32>().ok())
            .is_some_and(|iid| (30000..=31999).contains(&iid))
}

/// it-tools shows every row for an invalid IBAN too — validity is just one of
/// them — so this never fails on a badly formed input, only on an empty one.
pub(crate) fn iban_validator(args: &Args) -> Result<String> {
    use iban::IbanLike as _;
    let raw: String = arg(args, tool("iban-validator-and-parser"), "iban")
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .flat_map(char::to_uppercase)
        .collect();
    if raw.is_empty() {
        bail!("no IBAN provided");
    }
    let parsed = raw.parse::<iban::Iban>();
    let mut out = String::new();
    let _ = writeln!(out, "Is IBAN valid ?: {}", parsed.is_ok());
    if let Err(e) = &parsed {
        let _ = writeln!(out, "IBAN errors: {e}");
    }
    let _ = writeln!(out, "Is IBAN a QR-IBAN ?: {}", is_qr_iban(&raw));
    let _ = writeln!(
        out,
        "Country code: {}",
        parsed
            .as_ref()
            .map(|i| i.country_code().to_string())
            .unwrap_or_else(|_| raw.chars().take(2).collect())
    );
    let _ = writeln!(
        out,
        "BBAN: {}",
        parsed
            .as_ref()
            .map(|i| i.bban().to_string())
            .unwrap_or_else(|_| raw.chars().skip(4).collect())
    );
    let _ = writeln!(out, "IBAN friendly format: {}", friendly_iban(&raw));
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "iban-validator-and-parser",
    name: "IBAN validator and parser",
    description: "Validate and parse IBAN numbers; check if the IBAN is valid and get its parts.",
    category: Category::Data,
    keywords: &["iban", "validator", "parser", "bic", "bank"],
    fields: &[Field::text("iban", "IBAN to validate and parse")],
    kind: Kind::Pure,
    run: iban_validator,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn iban_valid_and_invalid() {
        let out = run(
            "iban-validator-and-parser",
            &[("iban", "DE44 5001 0517 5407 3249 31")],
        )
        .unwrap();
        assert!(out.contains("Is IBAN valid ?: true"), "{out}");
        assert!(
            out.contains("IBAN friendly format: DE44 5001 0517 5407 3249 31"),
            "{out}"
        );
        assert!(out.contains("Is IBAN a QR-IBAN ?: false"), "{out}");
        assert!(out.contains("Country code: DE"), "{out}");
        assert!(out.contains("BBAN: 500105175407324931"), "{out}");

        let out = run(
            "iban-validator-and-parser",
            &[("iban", "FR7630006000011234567890189")],
        )
        .unwrap();
        assert!(out.contains("Country code: FR"), "{out}");

        // Broken checksum, unknown country and junk all still fill the rows —
        // it-tools reports validity, it does not blank the panel.
        for bad in [
            "DE44500105175407324932",
            "MR0000020001010000123456754",
            "not-an-iban",
        ] {
            let out = run("iban-validator-and-parser", &[("iban", bad)]).unwrap();
            assert!(out.contains("Is IBAN valid ?: false"), "{bad}: {out}");
            assert!(out.contains("IBAN errors: "), "{bad}: {out}");
            assert!(out.contains("IBAN friendly format: "), "{bad}: {out}");
        }
        assert!(run("iban-validator-and-parser", &[("iban", "")]).is_err());
        // QR-IBAN: a CH IBAN with a QR-IID institution number.
        assert!(
            run(
                "iban-validator-and-parser",
                &[("iban", "CH4431999123000889012")]
            )
            .unwrap()
            .contains("Is IBAN a QR-IBAN ?: true")
        );
    }
}
