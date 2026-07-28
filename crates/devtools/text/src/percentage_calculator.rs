//! `percentage-calculator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

// it-tools puts three independently-filled calculators on the page, each in its
// own card with its own pair of inputs and its own readonly result — there is no
// mode select, and a blank or non-finite result only blanks that one card.

pub(crate) fn percent_of(x: f64, y: f64) -> f64 {
    x / 100.0 * y
}

pub(crate) fn percent_ratio(x: f64, y: f64) -> f64 {
    100.0 * x / y
}

pub(crate) fn percent_change(from: f64, to: f64) -> f64 {
    (to - from) / from * 100.0
}

/// it-tools renders `''` for a NaN/Infinity result; a form that only speaks text
/// says why instead, and `section` keeps that inside the one card.
pub(crate) fn finite(v: f64) -> anyhow::Result<String> {
    if !v.is_finite() {
        anyhow::bail!("no result (division by zero?)");
    }
    Ok(fmt_f(v))
}

pub(crate) fn percentage_calculator(args: &Args) -> anyhow::Result<String> {
    let id = "percentage-calculator";
    let pair = |x_key: &str, y_key: &str, x_label: &str, y_label: &str, f: fn(f64, f64) -> f64| {
        let x = parse_num(x_label, a(args, id, x_key))?;
        let y = parse_num(y_label, a(args, id, y_key))?;
        finite(f(x, y))
    };
    Ok([
        crate::section(
            "What is X% of Y",
            pair("pct_x", "pct_y", "X", "Y", percent_of),
        ),
        crate::section(
            "X is what percent of Y",
            pair("ratio_x", "ratio_y", "X", "Y", percent_ratio),
        ),
        crate::section(
            "What is the percentage increase/decrease",
            pair("change_from", "change_to", "From", "To", percent_change),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "percentage-calculator",
    name: "Percentage calculator",
    description: "Calculate percentages of a value, ratios and percentage change.",
    category: Category::Math,
    keywords: &[
        "percentage",
        "percent",
        "calculator",
        "ratio",
        "increase",
        "decrease",
    ],
    // Three separate calculators on one page upstream, so three pairs of
    // inputs here — no mode select. Blank by default, like the `n-input-number`
    // placeholders, and each pair only fills in its own result row.
    fields: &[
        // The two "… of Y" rows must not share a label: the generic form
        // shows nothing but labels, so two identical ones are two inputs the
        // user cannot tell apart.
        Field::number("pct_x", "What is X% …", ""),
        Field::number("pct_y", "… of Y (percent of)", ""),
        Field::number("ratio_x", "X is what percent …", ""),
        Field::number("ratio_y", "… of Y (is what percent)", ""),
        Field::number("change_from", "Percentage change from", ""),
        Field::number("change_to", "… to", ""),
    ],
    kind: Kind::Pure,
    run: percentage_calculator,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    /// Same three formulas as before; the `mode` select they used to hang off is
    /// gone, because it-tools shows the three calculators side by side.
    #[test]
    fn percentage_vectors() {
        assert_eq!(percent_of(5.0, 100.0), 5.0);
        assert_eq!(percent_of(50.0, 40.0), 20.0);
        assert_eq!(percent_ratio(5.0, 100.0), 5.0);
        assert_eq!(percent_ratio(20.0, 40.0), 50.0);
        assert_eq!(percent_change(100.0, 150.0), 50.0);
        assert_eq!(percent_change(100.0, 50.0), -50.0);
        assert!(finite(percent_ratio(5.0, 0.0)).is_err());

        let out = run(
            "percentage-calculator",
            &[
                ("pct_x", "5"),
                ("pct_y", "100"),
                ("change_from", "100"),
                ("change_to", "150"),
            ],
        )
        .unwrap();
        assert!(out.contains("What is X% of Y:\n5"), "{out}");
        assert!(
            out.contains("What is the percentage increase/decrease:\n50"),
            "{out}"
        );
        // The untouched calculator says so in its own panel, and the two filled
        // ones still answer -- this is the whole point of the rewrite.
        assert!(
            out.contains("X is what percent of Y:\n  (X is required)"),
            "{out}"
        );
        // Nothing filled in at all is still three labelled, empty panels.
        let empty = run("percentage-calculator", &[]).unwrap();
        assert_eq!(empty.matches("is required").count(), 3, "{empty}");
        assert!(
            run(
                "percentage-calculator",
                &[("pct_x", "nope"), ("pct_y", "1")]
            )
            .unwrap()
            .contains("is not a number")
        );
    }
}
