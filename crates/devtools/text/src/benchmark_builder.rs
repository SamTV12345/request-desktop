//! `benchmark-builder`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

pub(crate) fn parse_series(line: &str) -> (String, Vec<f64>) {
    let (label, rest) = match line.split_once(':') {
        Some((l, r)) if !l.trim().is_empty() && l.parse::<f64>().is_err() => (l.trim(), r),
        _ => ("", line),
    };
    let data: Vec<f64> = rest
        .split([',', ';', ' ', '\t'])
        .filter_map(|t| t.trim().parse::<f64>().ok())
        .collect();
    (label.to_string(), data)
}

pub(crate) fn mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        0.0
    } else {
        data.iter().sum::<f64>() / data.len() as f64
    }
}

pub(crate) fn variance(data: &[f64]) -> f64 {
    let m = mean(data);
    let squared: Vec<f64> = data.iter().map(|v| (v - m).powi(2)).collect();
    mean(&squared)
}

/// it-tools rounds every displayed statistic to three decimals.
pub(crate) fn round3(v: f64) -> String {
    fmt_f((v * 1000.0).round() / 1000.0)
}

/// The table has exactly the five columns of the `.vue`'s `header` map — no
/// median/min/max, which upstream does not compute.
///
/// A suite with no parseable values is kept with 0 samples and a mean of 0
/// (upstream `data.filter(_.isNumber)` does the same), rather than taking the
/// whole table down: one mistyped line must not blank the other suites.
pub(crate) fn benchmark_builder(args: &Args) -> anyhow::Result<String> {
    let id = "benchmark-builder";
    let text = nonempty("suites", a(args, id, "suites"))?;
    let unit = a(args, id, "unit").trim().to_string();

    let mut suites: Vec<(String, Vec<f64>)> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let (mut label, data) = parse_series(line);
        if label.is_empty() {
            label = format!("Suite {}", i + 1);
        }
        suites.push((label, data));
    }
    if suites.is_empty() {
        anyhow::bail!("no suites: put one per line, e.g. `Suite 1: 5, 10`");
    }
    suites.sort_by(|x, y| mean(&x.1).total_cmp(&mean(&y.1)));
    let best = mean(&suites[0].1);

    let mut out = String::from(
        "| Position | Suite | Samples | Mean | Variance |\n\
         | --- | --- | --- | --- | --- |\n",
    );
    for (i, (label, data)) in suites.iter().enumerate() {
        let m = mean(data);
        let cmp = if i != 0 && best != m {
            let ratio = if best == 0.0 {
                "∞".to_string()
            } else {
                round3(m / best)
            };
            format!(" (+{}{unit} ; x{ratio})", round3(m - best))
        } else {
            String::new()
        };
        out.push_str(&format!(
            "| {} | {} | {} | {}{unit}{} | {}{unit}{} |\n",
            i + 1,
            label,
            data.len(),
            round3(m),
            cmp,
            round3(variance(data)),
            if unit.is_empty() { "" } else { "²" },
        ));
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "benchmark-builder",
    name: "Benchmark builder",
    description: "Compare execution times of tasks: mean, median, min, max and variance.",
    category: Category::Math,
    keywords: &[
        "benchmark",
        "builder",
        "mean",
        "median",
        "variance",
        "compare",
        "statistics",
    ],
    fields: &[
        Field::area("suites", "One suite per line: `Suite name: 5, 10, 12`"),
        Field::text("unit", "Unit (e.g. ms)"),
    ],
    kind: Kind::Pure,
    run: benchmark_builder,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn benchmark_statistics() {
        assert_eq!(mean(&[]), 0.0);
        assert_eq!(mean(&[5.0, 10.0]), 7.5);
        assert_eq!(variance(&[5.0, 10.0]), 6.25);
        assert_eq!(variance(&[]), 0.0);
        let (label, data) = parse_series("Suite 1: 5, 10");
        assert_eq!(label, "Suite 1");
        assert_eq!(data, vec![5.0, 10.0]);
        let (label, data) = parse_series("1 2 3");
        assert_eq!(label, "");
        assert_eq!(data, vec![1.0, 2.0, 3.0]);

        let out = run(
            "benchmark-builder",
            &[("suites", "Suite 1: 5, 10\nSuite 2: 8, 12"), ("unit", "ms")],
        )
        .unwrap();
        // Exactly the five columns of the upstream table: no median/min/max.
        assert!(
            out.starts_with("| Position | Suite | Samples | Mean | Variance |\n"),
            "{out}"
        );
        assert!(
            out.contains("| 1 | Suite 1 | 2 | 7.5ms | 6.25ms² |"),
            "{out}"
        );
        assert!(out.contains("Suite 2"));
        assert!(out.contains("x1.333"), "{out}");

        assert!(run("benchmark-builder", &[]).is_err());
        // Interface change: a line with no numbers is a 0-sample suite, as
        // `data.filter(_.isNumber)` makes it upstream, not a hard error.
        assert!(
            run(
                "benchmark-builder",
                &[("suites", "nothing numeric\nSuite 2: 4")]
            )
            .unwrap()
            .contains("| 1 | Suite 1 | 0 | 0 |")
        );
    }
}
