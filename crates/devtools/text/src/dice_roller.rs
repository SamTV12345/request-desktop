//! `dice-roller` — it-tools PR #1532.
//!
//! Standard dice notation: `2d6+3`, `1d20`, and the `4d6kh3` that rolls a
//! character sheet. Alongside the roll it prints the range and the exact
//! expected value, because the interesting question about `4d6kh3` is not what
//! it rolled but what it should have rolled.
//!
//! Everything is bounded — line length, term count, dice count, sides — so the
//! adversarial battery cannot turn a pasted megabyte into a busy loop.

use crate::*;
use anyhow::{Result, bail};
use std::fmt::Write as _;

const MAX_DICE: u32 = 1_000;
const MAX_SIDES: u32 = 1_000;
const MAX_LINE: usize = 120;
const MAX_LINES: usize = 20;
const MAX_TERMS: usize = 16;

const DEFAULT: &str = "2d6+3\n4d6kh3\n1d20";

enum Term {
    Constant(i64),
    /// `keep` is the number of dice that count; it equals `dice` unless a
    /// `kh`/`kl` suffix trims the pool.
    Dice {
        dice: u32,
        sides: u32,
        keep: u32,
        keep_high: bool,
    },
}

/// Split on `+`/`-` keeping the sign that introduced each term. A leading sign
/// is fine, an empty piece is dropped, so `-3+1d6` and `1d6 + 2` both work.
fn split_terms(line: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    let (mut sign, mut cur) = (1i64, String::new());
    for c in line.chars() {
        if c == '+' || c == '-' {
            if !cur.trim().is_empty() {
                out.push((sign, std::mem::take(&mut cur)));
            }
            cur.clear();
            sign = if c == '-' { -1 } else { 1 };
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        out.push((sign, cur));
    }
    out
}

fn parse_term(raw: &str, budget: &mut u32) -> Result<Term> {
    let text = raw.trim().to_lowercase();
    if let Ok(c) = text.parse::<i64>() {
        return Ok(Term::Constant(c));
    }
    let caps = re(r"^(\d*)d(\d+)(?:(kh|kl)(\d+))?$")
        .captures(&text)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`{}` is not dice notation; try `2d6`, `1d20` or `4d6kh3`",
                raw.trim()
            )
        })?;
    // A number too long for u32 is out of range by definition, so the parse
    // failure and the bound check share one message.
    let dice: u32 = if caps[1].is_empty() {
        1
    } else {
        caps[1].parse().unwrap_or(u32::MAX)
    };
    let sides: u32 = caps[2].parse().unwrap_or(u32::MAX);
    if dice == 0 {
        bail!("`{}` rolls no dice at all", raw.trim());
    }
    if !(2..=MAX_SIDES).contains(&sides) {
        bail!(
            "a die needs between 2 and {MAX_SIDES} sides; `{}` asks for {sides}",
            raw.trim()
        );
    }
    if dice > *budget {
        bail!("at most {MAX_DICE} dice per run, and this expression asks for more");
    }
    *budget -= dice;

    let (keep, keep_high) = match caps.get(3) {
        Some(k) => {
            let want: u32 = caps[4].parse().unwrap_or(u32::MAX);
            if want == 0 {
                bail!("`{}` keeps no dice", raw.trim());
            }
            (want.min(dice), k.as_str() == "kh")
        }
        None => (dice, true),
    };
    Ok(Term::Dice {
        dice,
        sides,
        keep,
        keep_high,
    })
}

/// Binomial(n, p), anchored at the mode and then normalised.
///
/// The textbook `pmf[0] = (1-p)^n` recurrence underflows to zero long before
/// n = 1000, which would silently report an expected value of zero. Starting at
/// the mode keeps the largest term at 1.0, so only the genuinely negligible
/// tails underflow. `p` is strictly between 0 and 1 at every call site.
fn binomial(n: usize, p: f64, w: &mut [f64]) {
    let q = 1.0 - p;
    let mode = ((((n + 1) as f64) * p) as usize).min(n);
    w.fill(0.0);
    w[mode] = 1.0;
    for j in mode + 1..=n {
        w[j] = w[j - 1] * ((n - j + 1) as f64 / j as f64) * (p / q);
    }
    for j in (0..mode).rev() {
        w[j] = w[j + 1] * ((j + 1) as f64 / (n - j) as f64) * (q / p);
    }
    let total: f64 = w.iter().sum();
    if total > 0.0 {
        w.iter_mut().for_each(|x| *x /= total);
    }
}

/// Exact expected sum of the highest `k` of `n` dice with `sides` faces.
///
/// The sum of the kept dice equals the number of kept dice at or above each
/// threshold, summed over the thresholds; keeping the highest `k` means that
/// count is `min(k, dice at or above t)`. That turns the order statistics into
/// one binomial per face value, which costs `sides * n` and no combinatorics.
pub(crate) fn expected_top(n: u32, sides: u32, k: u32) -> f64 {
    let (n, k) = (n as usize, k.min(n) as usize);
    if k == 0 || n == 0 {
        return 0.0;
    }
    let mut w = vec![0.0f64; n + 1];
    let mut total = k as f64; // threshold 1: every kept die clears it
    for t in 2..=sides {
        binomial(n, (sides - t + 1) as f64 / sides as f64, &mut w);
        // sum over j of P(count >= j), for j = 1..=k
        let (mut acc, mut tail) = (0.0, 0.0);
        for (j, weight) in w.iter().enumerate().skip(1).rev() {
            acc += weight;
            if j <= k {
                tail += acc;
            }
        }
        total += tail;
    }
    total
}

/// (printable roll, value, minimum, maximum, expected value)
fn roll_term(term: &Term) -> (String, i64, i64, i64, f64) {
    match *term {
        Term::Constant(c) => (c.to_string(), c, c, c, c as f64),
        Term::Dice {
            dice,
            sides,
            keep,
            keep_high,
        } => {
            let values: Vec<u32> = (0..dice).map(|_| rand::random_range(1..=sides)).collect();
            let mut order: Vec<usize> = (0..values.len()).collect();
            // Ties break by index so the printed pool stays stable.
            order.sort_by_key(|&i| {
                if keep_high {
                    (std::cmp::Reverse(values[i]), i)
                } else {
                    (std::cmp::Reverse(u32::MAX - values[i]), i)
                }
            });
            let mut kept = vec![false; values.len()];
            for &i in order.iter().take(keep as usize) {
                kept[i] = true;
            }
            let sum: i64 = values
                .iter()
                .zip(&kept)
                .filter(|&(_, &k)| k)
                .map(|(v, _)| *v as i64)
                .sum();
            let shown: Vec<String> = values
                .iter()
                .zip(&kept)
                .map(|(v, &k)| if k { v.to_string() } else { format!("({v})") })
                .collect();

            let mean = dice as f64 * (sides as f64 + 1.0) / 2.0;
            let expected = if keep == dice {
                mean
            } else if keep_high {
                expected_top(dice, sides, keep)
            } else {
                // The lowest k are everything except the highest n-k.
                mean - expected_top(dice, sides, dice - keep)
            };
            (
                format!("[{}]", shown.join(", ")),
                sum,
                keep as i64,
                keep as i64 * sides as i64,
                expected,
            )
        }
    }
}

fn eval(line: &str, budget: &mut u32) -> Result<(String, i64, i64, i64, f64)> {
    if line.len() > MAX_LINE {
        bail!(
            "an expression should be shorter than {MAX_LINE} characters; this one is {}",
            line.len()
        );
    }
    let terms = split_terms(line);
    if terms.is_empty() {
        bail!("nothing to roll");
    }
    if terms.len() > MAX_TERMS {
        bail!(
            "at most {MAX_TERMS} terms per expression, and this one has {}",
            terms.len()
        );
    }
    let (mut detail, mut sum, mut min, mut max, mut expected) =
        (Vec::new(), 0i64, 0i64, 0i64, 0.0f64);
    for (sign, raw) in terms {
        let (text, value, lo, hi, e) = roll_term(&parse_term(&raw, budget)?);
        detail.push(match (sign < 0, detail.is_empty()) {
            (true, _) => format!("- {text}"),
            (false, true) => text,
            (false, false) => format!("+ {text}"),
        });
        sum = sum.saturating_add(sign.saturating_mul(value));
        // A subtracted term flips which end of its range is the worst case.
        let (lo, hi) = if sign < 0 { (-hi, -lo) } else { (lo, hi) };
        min = min.saturating_add(lo);
        max = max.saturating_add(hi);
        expected += sign as f64 * e;
    }
    Ok((detail.join(" "), sum, min, max, expected))
}

fn dice_roller(a: &Args) -> Result<String> {
    let rows = lines(s(a, "expressions", DEFAULT));
    if rows.is_empty() {
        bail!("enter a dice expression such as `2d6+3`, `1d20` or `4d6kh3`, one per line");
    }
    if rows.len() > MAX_LINES {
        bail!(
            "at most {MAX_LINES} expressions at a time, and you gave {}",
            rows.len()
        );
    }
    let mut budget = MAX_DICE;
    let (mut rolls, mut stats) = (String::new(), String::new());
    for line in rows {
        match eval(line, &mut budget) {
            Ok((detail, sum, min, max, expected)) => {
                let _ = writeln!(rolls, "  {line} -> {detail} = {sum}");
                let _ = writeln!(
                    stats,
                    "  {line} -> min {min}, max {max}, expected {}",
                    fmt_f((expected * 1e3).round() / 1e3)
                );
            }
            Err(e) => {
                // One bad line must not take the other nineteen down.
                let _ = writeln!(rolls, "  {line} -> ({e:#})");
                let _ = writeln!(stats, "  {line} -> ({e:#})");
            }
        }
    }
    Ok([
        section("Rolls", Ok(rolls)),
        section("Statistics", Ok(stats)),
    ]
    .join("\n"))
}

pub static TOOL: Tool = Tool {
    id: "dice-roller",
    name: "Dice roller",
    description: "Roll standard dice notation — 2d6+3, 1d20, 4d6kh3 — with the range and expected value.",
    category: Category::Math,
    keywords: &[
        "dice", "roll", "d20", "rpg", "random", "notation", "tabletop",
    ],
    fields: &[Field::area("expressions", "Dice expressions, one per line").with_default(DEFAULT)],
    kind: Kind::Pure,
    run: dice_roller,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn statistics_are_exact() {
        // 4d6 keep highest 3 has expectation 15869/1296, the number every
        // character-generation table is built on.
        assert!((expected_top(4, 6, 3) - 15869.0 / 1296.0).abs() < 1e-9);
        // Keeping all of them is just the mean.
        assert!((expected_top(2, 6, 2) - 7.0).abs() < 1e-9);
        // The maximum of 4d6.
        assert!((expected_top(4, 6, 1) - 5.244598765432099).abs() < 1e-9);
        assert_eq!(expected_top(4, 6, 0), 0.0);
        // 1000 dice is where the naive (1-p)^n recurrence underflows to zero.
        assert!((expected_top(1000, 6, 1000) - 3500.0).abs() < 1e-6);
        assert!(expected_top(1000, 1000, 1) > 990.0);
    }

    #[test]
    fn the_default_sheet() {
        let out = ok("dice-roller", &[]);
        assert!(out.starts_with("Rolls:"), "{out}");
        assert!(out.contains("2d6+3 -> min 5, max 15, expected 10"), "{out}");
        assert!(
            out.contains("4d6kh3 -> min 3, max 18, expected 12.245"),
            "{out}"
        );
        assert!(
            out.contains("1d20 -> min 1, max 20, expected 10.5"),
            "{out}"
        );
        // The dropped die is shown in brackets rather than hidden.
        assert!(out.contains('('), "{out}");
    }

    #[test]
    fn rolls_stay_inside_their_range() {
        for _ in 0..50 {
            let out = ok("dice-roller", &[("expressions", "3d8+2\n4d6kl3\n10-1d4")]);
            for line in out.lines().filter(|l| l.contains(" = ")) {
                let sum: i64 = line.rsplit(" = ").next().unwrap_or("").parse().unwrap_or(0);
                // 3d8+2 is 5..26, 4d6kl3 is 3..18, 10-1d4 is 6..9.
                assert!((3..=26).contains(&sum), "{line}");
            }
        }
        // Keep-lowest is the complement of keep-highest: 4*3.5 - E[max of 4d6].
        let out = ok("dice-roller", &[("expressions", "4d6kl3")]);
        assert!(out.contains("expected 8.755"), "{out}");
        // A subtracted die flips the range: 10-1d4 runs 6..9.
        let out = ok("dice-roller", &[("expressions", "10-1d4")]);
        assert!(out.contains("min 6, max 9, expected 7.5"), "{out}");
    }

    #[test]
    fn bad_notation_reports_itself_per_line() {
        let out = ok("dice-roller", &[("expressions", "2d6\nchicken\n1d20")]);
        assert!(
            out.contains("chicken -> (`chicken` is not dice notation"),
            "{out}"
        );
        assert!(out.contains("1d20 ->"), "{out}");
        assert!(run("dice-roller", &[("expressions", "   ")]).is_err());
    }

    #[test]
    fn adversarial_inputs_are_bounded() {
        // A pasted megabyte, ten thousand brackets, and the integer extremes:
        // every one of these must come back promptly with a message.
        for v in [
            "x".repeat(1_000_000),
            "(".repeat(10_000),
            format!("{}d6", u64::MAX),
            format!("2d{}", u64::MAX),
            format!("{}d{}", i64::MIN, i64::MIN),
            "999d999".to_string(),
            "1d6+".repeat(400),
            "1d6\n".repeat(10_000),
            "\u{0}\u{1b}\u{7}".to_string(),
        ] {
            let _ = run("dice-roller", &[("expressions", v.as_str())]);
        }
        // The dice budget is per run, not per line: 20 lines of 999d6 cannot
        // roll 19,980 dice.
        let out = ok("dice-roller", &[("expressions", &"999d6\n".repeat(20))]);
        assert!(out.contains("at most 1000 dice per run"), "{out}");
    }
}
