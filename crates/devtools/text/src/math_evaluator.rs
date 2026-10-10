//! `math-evaluator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

use regex::Regex;

/// mathjs accepts bare `sqrt(2)`; the evaluator below canonicalises those under
/// `math::` (the spelling the previous evalexpr backend required), so both
/// spellings keep working.
/// Longest names first so the alternation cannot bite off a prefix.
pub(crate) const MATH_FNS: &[&str] = &[
    "acosh", "asinh", "atanh", "atan2", "log10", "hypot", "sqrt", "cbrt", "cosh", "sinh", "tanh",
    "acos", "asin", "atan", "log2", "exp2", "log", "exp", "pow", "cos", "sin", "tan", "abs", "ln",
];

pub(crate) fn namespace_math_fns(expr: &str) -> String {
    // ponytail: swallowing an existing `math::` prefix in the same match keeps
    // the rewrite idempotent; a real expression parser is overkill here.
    let re =
        Regex::new(&format!(r"(math::)?\b({})\s*\(", MATH_FNS.join("|"))).expect("static pattern");
    re.replace_all(expr, "math::${2}(").into_owned()
}

/// Recursion budget for nested parens and unary chains. A megabyte of `(` has to
/// come back as an error, not as a stack overflow (which no `Result` can catch).
pub(crate) const MATH_MAX_DEPTH: u32 = 64;

/// ponytail: hand-rolled recursive-descent over f64 instead of a crate. Every
/// permissively-licensed evaluator on crates.io is missing part of the mathjs
/// function set below (fasteval and meval have no cbrt/hypot/exp2, evalexpr-jit
/// depends on AGPL evalexpr, rhai is a whole scripting language), and none of
/// them documents a depth bound we can rely on for the no-panic property test.
/// Upgrade path: swap in a crate if this ever needs variables or non-numerics.
pub(crate) struct MathParser<'a> {
    pub(crate) s: &'a [u8],
    pub(crate) i: usize,
    pub(crate) depth: u32,
}

impl MathParser<'_> {
    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn deeper(&mut self) -> anyhow::Result<()> {
        self.depth += 1;
        if self.depth > MATH_MAX_DEPTH {
            anyhow::bail!("expression is nested too deeply");
        }
        Ok(())
    }

    /// `expr := term (('+' | '-') term)*`
    fn expr(&mut self) -> anyhow::Result<f64> {
        self.deeper()?;
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.i += 1;
                    v += self.term()?;
                }
                Some(b'-') => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => break,
            }
        }
        self.depth -= 1;
        Ok(v)
    }

    /// `term := unary (('*' | '/' | '%') unary)*`
    fn term(&mut self) -> anyhow::Result<f64> {
        let mut v = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(c @ (b'*' | b'/' | b'%')) => c,
                _ => return Ok(v),
            };
            self.i += 1;
            let r = self.unary()?;
            v = match op {
                b'*' => v * r,
                b'/' => v / r,
                _ => v % r,
            };
        }
    }

    /// `unary := ('-' | '+') unary | power`
    fn unary(&mut self) -> anyhow::Result<f64> {
        self.deeper()?;
        let v = match self.peek() {
            Some(b'-') => {
                self.i += 1;
                -self.unary()?
            }
            Some(b'+') => {
                self.i += 1;
                self.unary()?
            }
            _ => self.power()?,
        };
        self.depth -= 1;
        Ok(v)
    }

    /// `power := atom ('^' unary)?` — right associative, as in mathjs.
    fn power(&mut self) -> anyhow::Result<f64> {
        let base = self.atom()?;
        if self.eat(b'^') {
            return Ok(base.powf(self.unary()?));
        }
        Ok(base)
    }

    fn atom(&mut self) -> anyhow::Result<f64> {
        match self.peek() {
            Some(b'(') => {
                self.i += 1;
                let v = self.expr()?;
                if !self.eat(b')') {
                    anyhow::bail!("expected `)`");
                }
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() || c == b'.' => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => self.call(),
            Some(c) => anyhow::bail!("unexpected `{}`", c as char),
            None => anyhow::bail!("unexpected end of expression"),
        }
    }

    fn number(&mut self) -> anyhow::Result<f64> {
        let start = self.i;
        while matches!(self.s.get(self.i), Some(c) if c.is_ascii_digit() || *c == b'.') {
            self.i += 1;
        }
        // Exponent, but only when it really is one: `2e3`, `2e-3`. A bare `2e`
        // is a syntax error and `2exp(1)` is not an implicit multiplication.
        if matches!(self.s.get(self.i), Some(b'e' | b'E')) {
            let mut j = self.i + 1;
            if matches!(self.s.get(j), Some(b'+' | b'-')) {
                j += 1;
            }
            if matches!(self.s.get(j), Some(c) if c.is_ascii_digit()) {
                self.i = j;
                while matches!(self.s.get(self.i), Some(c) if c.is_ascii_digit()) {
                    self.i += 1;
                }
            }
        }
        let raw = String::from_utf8_lossy(&self.s[start..self.i]);
        raw.parse::<f64>()
            .map_err(|_| anyhow::anyhow!("`{raw}` is not a number"))
    }

    fn call(&mut self) -> anyhow::Result<f64> {
        let start = self.i;
        while matches!(self.s.get(self.i), Some(c) if c.is_ascii_alphanumeric() || *c == b'_' || *c == b':')
        {
            self.i += 1;
        }
        let raw = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        let name = raw.strip_prefix("math::").unwrap_or(&raw);
        if !self.eat(b'(') {
            // A bare name is a constant, not a broken call. it-tools evaluates
            // with mathjs, so mathjs' constants are the reference — `e^2` has
            // to work.
            return math_constant(name)
                .ok_or_else(|| anyhow::anyhow!("`{name}` is not a known constant or function"));
        }
        let mut args = Vec::new();
        if !self.eat(b')') {
            args.push(self.expr()?);
            while self.eat(b',') {
                args.push(self.expr()?);
            }
            if !self.eat(b')') {
                anyhow::bail!("expected `)` after the arguments of `{name}`");
            }
        }
        apply_math_fn(name, &args)
    }
}

/// mathjs' constants. it-tools evaluates with mathjs, so this is the reference
/// rather than the `math::`-only surface of the AGPL crate this replaced.
pub(crate) fn math_constant(name: &str) -> Option<f64> {
    Some(match name {
        "e" | "E" => std::f64::consts::E,
        "pi" | "PI" => std::f64::consts::PI,
        "tau" => std::f64::consts::TAU,
        "phi" => std::f64::consts::GOLDEN_RATIO,
        "LN2" => std::f64::consts::LN_2,
        "LN10" => std::f64::consts::LN_10,
        "LOG2E" => std::f64::consts::LOG2_E,
        "LOG10E" => std::f64::consts::LOG10_E,
        "SQRT1_2" => std::f64::consts::FRAC_1_SQRT_2,
        "SQRT2" => std::f64::consts::SQRT_2,
        "Infinity" => f64::INFINITY,
        "NaN" => f64::NAN,
        _ => return None,
    })
}

/// `n!`, for the small non-negative integers where it is exact-ish.
pub(crate) fn factorial(x: f64) -> anyhow::Result<f64> {
    if x < 0.0 || x.fract() != 0.0 {
        anyhow::bail!("factorial needs a non-negative whole number");
    }
    if x > 170.0 {
        anyhow::bail!("factorial of {x} overflows a float");
    }
    Ok((1..=(x as u64)).fold(1.0f64, |acc, n| acc * n as f64))
}

pub(crate) fn apply_math_fn(name: &str, args: &[f64]) -> anyhow::Result<f64> {
    let one = |f: fn(f64) -> f64| match args {
        [x] => Ok(f(*x)),
        _ => Err(anyhow::anyhow!("`{name}` takes exactly 1 argument")),
    };
    let two = |f: fn(f64, f64) -> f64| match args {
        [x, y] => Ok(f(*x, *y)),
        _ => Err(anyhow::anyhow!("`{name}` takes exactly 2 arguments")),
    };
    match name {
        "abs" => one(f64::abs),
        "sqrt" => one(f64::sqrt),
        "cbrt" => one(f64::cbrt),
        "ln" => one(f64::ln),
        "log2" => one(f64::log2),
        "log10" => one(f64::log10),
        "exp" => one(f64::exp),
        "exp2" => one(f64::exp2),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "round" => one(f64::round),
        "sin" => one(f64::sin),
        "cos" => one(f64::cos),
        "tan" => one(f64::tan),
        "asin" => one(f64::asin),
        "acos" => one(f64::acos),
        "atan" => one(f64::atan),
        "sinh" => one(f64::sinh),
        "cosh" => one(f64::cosh),
        "tanh" => one(f64::tanh),
        "asinh" => one(f64::asinh),
        "acosh" => one(f64::acosh),
        "atanh" => one(f64::atanh),
        // mathjs functions the evalexpr set did not have.
        "sign" => one(f64::signum),
        "square" => one(|x| x * x),
        "cube" => one(|x| x * x * x),
        "trunc" | "fix" => one(f64::trunc),
        "expm1" => one(f64::exp_m1),
        "log1p" => one(f64::ln_1p),
        "sec" => one(|x| 1.0 / x.cos()),
        "csc" => one(|x| 1.0 / x.sin()),
        "cot" => one(|x| 1.0 / x.tan()),
        "sech" => one(|x| 1.0 / x.cosh()),
        "csch" => one(|x| 1.0 / x.sinh()),
        "coth" => one(|x| 1.0 / x.tanh()),
        "factorial" => match args {
            [x] => factorial(*x),
            _ => anyhow::bail!("`factorial` takes exactly 1 argument"),
        },
        "nthRoot" => two(|x, n| {
            // mathjs keeps odd roots of negatives real: nthRoot(-8, 3) == -2.
            if x < 0.0 && (n as i64) % 2 != 0 && n.fract() == 0.0 {
                -(-x).powf(1.0 / n)
            } else {
                x.powf(1.0 / n)
            }
        }),
        "mod" => two(|a, b| a.rem_euclid(b)),
        "gcd" => two(|a, b| {
            let (mut a, mut b) = (a.abs(), b.abs());
            while b > 0.0 {
                (a, b) = (b, a % b);
            }
            a
        }),
        "lcm" => two(|a, b| {
            if a == 0.0 || b == 0.0 {
                return 0.0;
            }
            let (mut x, mut y) = (a.abs(), b.abs());
            let (a0, b0) = (x, y);
            while y > 0.0 {
                (x, y) = (y, x % y);
            }
            a0 / x * b0
        }),
        // Two-argument forms, matching the evalexpr `math::` set they replace.
        "log" => two(f64::log),
        "pow" => two(f64::powf),
        "hypot" => two(f64::hypot),
        "atan2" => two(f64::atan2),
        "min" | "max" => {
            if args.is_empty() {
                anyhow::bail!("`{name}` takes at least 1 argument");
            }
            Ok(args.iter().copied().fold(f64::NAN, |acc, v| {
                if name == "min" {
                    v.min(acc)
                } else {
                    v.max(acc)
                }
            }))
        }
        other => anyhow::bail!("unknown function `{other}`"),
    }
}

pub(crate) fn eval_math(expr: &str) -> anyhow::Result<f64> {
    let mut p = MathParser {
        s: expr.as_bytes(),
        i: 0,
        depth: 0,
    };
    let v = p.expr()?;
    if p.peek().is_some() {
        anyhow::bail!("unexpected trailing input at byte {}", p.i);
    }
    if !v.is_finite() {
        anyhow::bail!("result is not a finite number (division by zero? overflow?)");
    }
    Ok(v)
}

pub(crate) fn math_evaluator(args: &Args) -> anyhow::Result<String> {
    let expr = nonempty("expression", a(args, "math-evaluator", "expression"))?;
    let value = eval_math(&namespace_math_fns(expr))
        .map_err(|e| anyhow::anyhow!("cannot evaluate: {e}"))?;
    Ok(value.to_string())
}

pub static TOOL: Tool = Tool {
    id: "math-evaluator",
    name: "Math evaluator",
    description: "Evaluate mathematical expressions with functions like sqrt, cos, sin, abs.",
    category: Category::Math,
    keywords: &[
        "math",
        "evaluator",
        "calculator",
        "expression",
        "abs",
        "sqrt",
        "cos",
        "sin",
        "tan",
        "log",
        "exp",
        "pow",
    ],
    fields: &[Field::text(
        "expression",
        "Math expression (e.g. 2*sqrt(6))",
    )],
    kind: Kind::Pure,
    run: math_evaluator,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    /// Constants and the wider mathjs function set. it-tools evaluates with
    /// mathjs, so `e`, `pi` and friends must resolve — a bare name used to be
    /// rejected as "not a function", which is what a user hit with `1*5*e^2`.
    #[test]
    fn math_evaluator_knows_mathjs_constants_and_functions() {
        let eval = |e: &str| run("math-evaluator", &[("expression", e)]).unwrap();
        let num = |e: &str| eval(e).parse::<f64>().unwrap();

        // The reported expression.
        assert!((num("1*5*e^2") - 5.0 * std::f64::consts::E.powi(2)).abs() < 1e-9);
        assert!((num("e") - std::f64::consts::E).abs() < 1e-12);
        assert!((num("pi") - std::f64::consts::PI).abs() < 1e-12);
        assert!((num("tau") - std::f64::consts::TAU).abs() < 1e-12);
        assert!((num("phi") - std::f64::consts::GOLDEN_RATIO).abs() < 1e-12);
        assert!((num("2*pi") - std::f64::consts::TAU).abs() < 1e-12);
        assert!((num("SQRT2") - std::f64::consts::SQRT_2).abs() < 1e-12);

        // A digit-exponent must still parse as a number, not as `e` times 3.
        assert_eq!(num("2e3"), 2000.0);
        assert_eq!(num("2e-3"), 0.002);

        assert_eq!(num("square(4)"), 16.0);
        assert_eq!(num("cube(3)"), 27.0);
        assert_eq!(num("sign(-2)"), -1.0);
        assert_eq!(num("factorial(5)"), 120.0);
        assert_eq!(num("nthRoot(27, 3)"), 3.0);
        assert_eq!(num("nthRoot(-8, 3)"), -2.0); // mathjs keeps odd roots real
        assert_eq!(num("gcd(12, 18)"), 6.0);
        assert_eq!(num("lcm(4, 6)"), 12.0);
        assert_eq!(num("mod(-1, 3)"), 2.0); // euclidean, as in mathjs
        assert_eq!(num("trunc(2.7)"), 2.0);
        assert!((num("sec(0)") - 1.0).abs() < 1e-12);

        // An unknown bare name still reports itself clearly.
        let err = run("math-evaluator", &[("expression", "nope")])
            .unwrap_err()
            .to_string();
        assert!(err.contains("nope"), "{err}");
        assert!(err.contains("constant"), "{err}");
        assert!(run("math-evaluator", &[("expression", "factorial(-1)")]).is_err());
        assert!(run("math-evaluator", &[("expression", "factorial(0.5)")]).is_err());
    }

    #[test]
    fn math_evaluator_vectors() {
        assert_eq!(
            run("math-evaluator", &[("expression", "1+1")]).unwrap(),
            "2"
        );
        assert_eq!(
            run("math-evaluator", &[("expression", "2*3-1")]).unwrap(),
            "5"
        );
        assert_eq!(
            run("math-evaluator", &[("expression", "sqrt(9)")]).unwrap(),
            "3"
        );
        assert_eq!(
            run("math-evaluator", &[("expression", "abs(-4)")]).unwrap(),
            "4"
        );
        assert_eq!(
            run("math-evaluator", &[("expression", "math::sqrt(16)")]).unwrap(),
            "4"
        );
        assert!(
            run("math-evaluator", &[("expression", "2*sqrt(6)")])
                .unwrap()
                .starts_with("4.89")
        );
        assert!(run("math-evaluator", &[]).is_err());
        assert!(run("math-evaluator", &[("expression", "1+")]).is_err());
        assert!(run("math-evaluator", &[("expression", "nope(")]).is_err());
    }

    /// The evaluator is hand-rolled, so hostile input must come back as `Err`
    /// rather than as a panic, a stack overflow or a hang. (The property suite
    /// asserts the same thing generically; these are the named reproducers.)
    #[test]
    fn math_evaluator_survives_hostile_input() {
        for bad in [
            "(((((1)))))))))",
            "1/0",
            "0/0",
            "1%0",
            "9^9^9",
            "1e400",
            "2e",
            "sqrt()",
            "sqrt(1,2)",
            "log(2)",
            "atan2(1)",
            "nope(1)",
            "1 2",
            ")",
            "",
            "π",
            "--",
            "math::math::sqrt(4)",
        ] {
            assert!(
                eval_math(bad).is_err(),
                "`{bad}` should not evaluate to a number"
            );
        }
        // A megabyte of nesting: bounded by MATH_MAX_DEPTH, not by the stack.
        assert!(eval_math(&"(".repeat(1_000_000)).is_err());
        assert!(eval_math(&"-".repeat(1_000_000)).is_err());
        // ... but a long *flat* expression is iterative and still evaluates.
        assert_eq!(
            eval_math(&format!("{}1", "1+".repeat(10_000))).unwrap(),
            10_001.0
        );
    }

    #[test]
    fn math_namespacing_is_idempotent() {
        assert_eq!(namespace_math_fns("sqrt(2)"), "math::sqrt(2)");
        assert_eq!(namespace_math_fns("math::sqrt(2)"), "math::sqrt(2)");
        assert_eq!(namespace_math_fns("atan2(1,2)"), "math::atan2(1,2)");
        assert_eq!(namespace_math_fns("log10(100)"), "math::log10(100)");
    }
}
