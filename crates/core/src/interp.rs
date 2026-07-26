//! `{{var}}` interpolation, shared by both file formats.
use std::collections::BTreeMap;

pub type Vars = BTreeMap<String, String>;

/// Replace `{{name}}` with its value. Unknown names are left verbatim so the
/// user sees what failed to resolve instead of a silent empty string.
pub fn interpolate(input: &str, vars: &Vars) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let name = after[..end].trim();
                match vars.get(name).cloned().or_else(|| dynamic(name)) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push_str("{{");
                        out.push_str(&after[..end]);
                        out.push_str("}}");
                    }
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Names referenced as `{{name}}` in `input` that `vars` does not define.
///
/// `interpolate` leaves those verbatim so the user can see them, which means an
/// unresolved variable reaches the HTTP client as literal `{{…}}` text and comes
/// back as an unhelpful parse failure. Callers use this to say which variable is
/// actually missing.
pub fn missing(input: &str, vars: &Vars) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        let name = after[..end].trim();
        // A dynamic variable is never "missing" — `interpolate` generates it.
        if !name.is_empty()
            && !vars.contains_key(name)
            && dynamic(name).is_none()
            && !out.iter().any(|n| n == name)
        {
            out.push(name.to_string());
        }
        rest = &after[end + 2..];
    }
    out
}

// ------------------------------------------------------- dynamic variables
//
// JetBrains' generated `{{$name}}` variables. Implemented:
//   $uuid / $random.uuid          random UUID v4
//   $timestamp                    Unix epoch seconds
//   $isoTimestamp                 ISO-8601 UTC, second precision
//   $randomInt                    0..=1000, as JetBrains documents
//   $random.integer(from, to)     inclusive range
//   $random.float(from, to)
//   $random.alphabetic(n) / .alphanumeric(n) / .hexadecimal(n)
//   $random.email
//
// Deliberately skipped, because they need context this crate does not have:
//   $exampleServer, $historyFolder, $projectRoot (IDE paths), $env.NAME and
//   $auth.token / $auth.idToken (auth provider state), and the whole
//   $random.* Faker-style catalogue beyond the above ($random.name, .city, …).
// They resolve to `None` and are therefore reported as undefined, which is
// honest: we cannot produce a value for them.

/// A value for `{{$name}}`, or `None` if this is not a dynamic variable we know.
fn dynamic(name: &str) -> Option<String> {
    let name = name.strip_prefix('$')?;
    let (head, args) = match name.split_once('(') {
        Some((h, a)) => (h, a.trim_end_matches(')')),
        None => (name, ""),
    };
    let nums: Vec<f64> = args
        .split(',')
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .collect();
    Some(match head.trim() {
        "uuid" | "random.uuid" => uuid_v4(),
        "timestamp" => secs_since_epoch().to_string(),
        "isoTimestamp" => iso_timestamp(secs_since_epoch()),
        "randomInt" => (rand_u64() % 1001).to_string(),
        "random.integer" => {
            let (lo, hi) = range(&nums, 0.0, 1000.0);
            (lo as i64 + (rand_u64() % (hi as i64 - lo as i64 + 1).max(1) as u64) as i64)
                .to_string()
        }
        "random.float" => {
            let (lo, hi) = range(&nums, 0.0, 1.0);
            format!(
                "{:.6}",
                lo + (rand_u64() as f64 / u64::MAX as f64) * (hi - lo)
            )
        }
        "random.alphabetic" => pick(
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz",
            &nums,
        ),
        "random.alphanumeric" => pick(
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
            &nums,
        ),
        "random.hexadecimal" => pick(b"0123456789abcdef", &nums),
        "random.email" => format!(
            "{}@example.com",
            pick(b"abcdefghijklmnopqrstuvwxyz", &[8.0])
        ),
        _ => return None,
    })
}

fn range(nums: &[f64], lo: f64, hi: f64) -> (f64, f64) {
    match nums {
        [a, b, ..] if b >= a => (*a, *b),
        _ => (lo, hi),
    }
}

fn pick(alphabet: &[u8], nums: &[f64]) -> String {
    let n = nums.first().copied().unwrap_or(16.0).clamp(0.0, 4096.0) as usize;
    (0..n)
        .map(|_| alphabet[(rand_u64() % alphabet.len() as u64) as usize] as char)
        .collect()
}

fn secs_since_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Days-to-civil-date, so no date dependency is needed for one format string.
fn iso_timestamp(secs: u64) -> String {
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = era * 400 + yoe + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Randomness from the standard library: every `RandomState` is freshly seeded
/// by the OS, so hashing a constant with one yields a fresh random u64.
// ponytail: no `rand` dependency for this; swap in a real CSPRNG only if these
// values ever need to be unpredictable to an attacker.
fn rand_u64() -> u64 {
    use std::hash::{BuildHasher, RandomState};
    RandomState::new().hash_one(0xA5u8)
}

fn uuid_v4() -> String {
    let (a, b) = (rand_u64(), rand_u64());
    let hex = format!("{a:016x}{b:016x}");
    format!(
        "{}-{}-4{}-{}{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[13..16],
        // variant: one of 8, 9, a, b
        ['8', '9', 'a', 'b'][(b % 4) as usize],
        &hex[17..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_variables_resolve_and_are_never_missing() {
        let empty = Vars::new();
        for name in [
            "$uuid",
            "$random.uuid",
            "$timestamp",
            "$isoTimestamp",
            "$randomInt",
            "$random.integer(5, 5)",
            "$random.hexadecimal(8)",
            "$random.email",
        ] {
            let src = format!("{{{{{name}}}}}");
            assert!(missing(&src, &empty).is_empty(), "{name} reported missing");
            let got = interpolate(&src, &empty);
            assert!(!got.contains("{{"), "{name} not generated: {got}");
        }
        assert_eq!(interpolate("{{$random.integer(5, 5)}}", &empty), "5");
        let u = interpolate("{{$uuid}}", &empty);
        assert_eq!(u.len(), 36, "{u}");
        assert_eq!(u.chars().filter(|c| *c == '-').count(), 4, "{u}");
        assert_eq!(&u[14..15], "4", "{u}");
        assert_eq!(iso_timestamp(1_700_000_000), "2023-11-14T22:13:20Z");
        // An unimplemented dynamic variable stays honestly unresolved.
        assert_eq!(missing("{{$random.city}}", &empty), ["$random.city"]);
        // An explicit variable wins over the generator.
        let fixed = Vars::from([("$uuid".to_string(), "fixed".to_string())]);
        assert_eq!(interpolate("{{$uuid}}", &fixed), "fixed");
    }

    #[test]
    fn reports_missing_names_once_each() {
        let vars = Vars::from([("host".into(), "http://x".into())]);
        assert!(missing("{{host}}/api", &vars).is_empty());
        assert_eq!(missing("{{a}}/{{host}}/{{b}}/{{a}}", &vars), ["a", "b"]);
        assert_eq!(missing("{{ a }}", &vars), ["a"]);
        assert!(missing("no placeholders", &vars).is_empty());
        assert!(missing("unterminated {{a", &vars).is_empty());
        assert!(missing("{{}}", &vars).is_empty());
    }

    #[test]
    fn interpolates_known_and_keeps_unknown() {
        let vars = Vars::from([("host".into(), "http://x".into())]);
        assert_eq!(interpolate("{{host}}/api", &vars), "http://x/api");
        assert_eq!(interpolate("{{ host }}/api", &vars), "http://x/api");
        assert_eq!(interpolate("{{nope}}/api", &vars), "{{nope}}/api");
        assert_eq!(interpolate("no braces", &vars), "no braces");
        assert_eq!(
            interpolate("unterminated {{host", &vars),
            "unterminated {{host"
        );
        assert_eq!(interpolate("{{host}}{{host}}", &vars), "http://xhttp://x");
    }
}
