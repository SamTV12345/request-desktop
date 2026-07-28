//! `temperature-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

pub(crate) const TEMP_SCALES: &[&str] = &[
    "kelvin",
    "celsius",
    "fahrenheit",
    "rankine",
    "delisle",
    "newton",
    "reaumur",
    "romer",
];

pub(crate) fn to_kelvin(scale: &str, t: f64) -> anyhow::Result<f64> {
    Ok(match scale {
        "kelvin" => t,
        "celsius" => t + 273.15,
        "fahrenheit" => (t + 459.67) * (5.0 / 9.0),
        "rankine" => t * (5.0 / 9.0),
        "delisle" => 373.15 - (2.0 / 3.0) * t,
        "newton" => t * (100.0 / 33.0) + 273.15,
        "reaumur" => t * (5.0 / 4.0) + 273.15,
        "romer" => (t - 7.5) * (40.0 / 21.0) + 273.15,
        other => anyhow::bail!("unknown temperature scale `{other}`"),
    })
}

pub(crate) fn from_kelvin(scale: &str, k: f64) -> anyhow::Result<f64> {
    Ok(match scale {
        "kelvin" => k,
        "celsius" => k - 273.15,
        "fahrenheit" => k * (9.0 / 5.0) - 459.67,
        "rankine" => k * (9.0 / 5.0),
        "delisle" => (3.0 / 2.0) * (373.15 - k),
        "newton" => (k - 273.15) * (33.0 / 100.0),
        "reaumur" => (k - 273.15) * (4.0 / 5.0),
        "romer" => (k - 273.15) * (21.0 / 40.0) + 7.5,
        other => anyhow::bail!("unknown temperature scale `{other}`"),
    })
}

/// (title, unit) as spelled in the `.vue`'s `units` record.
pub(crate) fn temp_title_unit(scale: &str) -> (&'static str, &'static str) {
    match scale {
        "kelvin" => ("Kelvin", "K"),
        "celsius" => ("Celsius", "°C"),
        "fahrenheit" => ("Fahrenheit", "°F"),
        "rankine" => ("Rankine", "°R"),
        "delisle" => ("Delisle", "°De"),
        "newton" => ("Newton", "°N"),
        "reaumur" => ("Réaumur", "°Ré"),
        _ => ("Rømer", "°Rø"),
    }
}

/// it-tools shows all eight scales as eight *editable* inputs: typing into any
/// one of them recomputes the other seven. A one-way form cannot bind both ways,
/// so the honest approximation is one chosen input scale plus all eight outputs —
/// the same eight rows in the same order, just with the driving scale named
/// explicitly instead of inferred from which box was last touched.
///
/// Upstream shows two decimals, floored (`Math.floor(v * 100) / 100`), which
/// turns 100 °C into "211.99 °F" and 0.001 °C into a flat "0". it-tools PR #1491
/// drops the fixed two decimals; [`fmt_f`] is the same idea — round the float
/// noise off at the sixth decimal and print the shortest form of what is left,
/// so 211.99999999999997 reads "212" and 0.001 survives.
///
/// PR #1153: nothing is colder than 0 K. Below it the eight rows are not merely
/// imprecise, they are a temperature that does not exist, so the tool says so
/// instead of printing them.
pub(crate) fn temperature_converter(args: &Args) -> anyhow::Result<String> {
    let id = "temperature-converter";
    let from = a(args, id, "from");
    let value = parse_num("temperature", a(args, id, "value"))?;
    let k = to_kelvin(from, value)?;
    if k < 0.0 {
        let (title, unit) = temp_title_unit(from);
        // Named in the scale the user typed in, which is the one they can act on.
        let zero = from_kelvin(from, 0.0)?;
        anyhow::bail!(
            "{} {unit} is below absolute zero — {title} cannot go under {} {unit} (0 K)",
            fmt_f(value),
            fmt_f(zero)
        );
    }
    Ok(TEMP_SCALES
        .iter()
        .map(|scale| {
            let (title, unit) = temp_title_unit(scale);
            row(
                title,
                from_kelvin(scale, k).map(|v| format!("{} {unit}", fmt_f(v))),
            )
        })
        .collect())
}

pub static TOOL: Tool = Tool {
    id: "temperature-converter",
    name: "Temperature converter",
    description: "Convert between Kelvin, Celsius, Fahrenheit, Rankine, Delisle, Newton, Réaumur and Rømer.",
    category: Category::Measurement,
    keywords: &[
        "temperature",
        "converter",
        "kelvin",
        "celsius",
        "fahrenheit",
        "rankine",
        "delisle",
        "newton",
        "reaumur",
        "romer",
    ],
    fields: &[
        Field::number("value", "Temperature", "0"),
        Field::select("from", "Scale", TEMP_SCALES, "celsius"),
    ],
    kind: Kind::Pure,
    run: temperature_converter,
};
