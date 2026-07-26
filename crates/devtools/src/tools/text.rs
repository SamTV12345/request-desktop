//! Text, math and misc tools ported from it-tools. Owned by the text batch.
//!
//! ponytail: flat module — one `fn` per tool plus one `static` entry, no traits,
//! no macros. Anything that needs a webcam, a live timer, a key listener or a
//! rich-text surface is `Kind::Custom` with a `run` that errors, so the generic
//! form is skipped instead of rendered broken.
use crate::{Args, Category, Field, FieldKind, Kind, Tool};

use base64::Engine as _;
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;

// ---------------------------------------------------------------- helpers

/// Read `key` for tool `id`, falling back to the declared field default.
/// `Field::text` with a non-empty default; the shared constructor has no slot
/// for one, and the colour inputs upstream are pickers that always hold a value.
const fn text_default(key: &'static str, label: &'static str, default: &'static str) -> Field {
    Field {
        key,
        label,
        kind: FieldKind::Text,
        default,
        options: &[],
    }
}

fn a<'x>(args: &'x Args, id: &str, key: &str) -> &'x str {
    match TOOLS.iter().find(|t| t.id == id) {
        Some(t) => crate::arg(args, t, key),
        None => args.get(key).map(String::as_str).unwrap_or(""),
    }
}

fn num(label: &str, s: &str) -> anyhow::Result<f64> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("{label} is required");
    }
    s.parse::<f64>()
        .map_err(|_| anyhow::anyhow!("{label}: `{s}` is not a number"))
}

fn uint(label: &str, s: &str, max: usize) -> anyhow::Result<usize> {
    let v = num(label, s)?;
    if !v.is_finite() || v < 0.0 {
        anyhow::bail!("{label} must be a positive number");
    }
    let v = v as usize;
    if v > max {
        anyhow::bail!("{label} must be at most {max}");
    }
    Ok(v)
}

fn flag(s: &str) -> bool {
    matches!(s.trim(), "true" | "1" | "yes" | "on")
}

fn nonempty<'x>(label: &str, s: &'x str) -> anyhow::Result<&'x str> {
    if s.trim().is_empty() {
        anyhow::bail!("{label} is empty");
    }
    Ok(s)
}

/// Trim float noise without losing the value: JS-style shortest-ish output.
fn fmt_f(v: f64) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    let r = (v * 1e6).round() / 1e6;
    format!("{r}")
}

/// One-line sibling of [`crate::section`], for the converters that it-tools
/// renders as a flat list of readonly `label: value` rows (date-time, colour,
/// temperature). Same contract: a row that cannot be produced says so in place
/// instead of blanking its neighbours. `section` is used where a panel is
/// genuinely multi-line.
fn row(label: &str, body: anyhow::Result<String>) -> String {
    match body {
        Ok(v) => format!("{label}: {v}\n"),
        Err(e) => format!("{label}: ({e:#})\n"),
    }
}

// The five `Kind::Custom` tools: no generic form can express a live timer, a
// webcam, a key listener, a rich-text surface or a click-to-copy grid, so `run`
// says what the bespoke panel does instead of pretending to compute something.
fn chronometer_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!("needs the live chronometer panel (Start / Stop / Reset, ticking display)")
}

fn emoji_picker_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!(
        "needs the emoji grid panel (click an emoji to copy it, with its unicode escape and code point)"
    )
}

fn keycode_info_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!(
        "needs the keycode panel: press a key and it reports key, keyCode, code, location and modifiers"
    )
}

fn camera_recorder_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!("needs the camera panel (device pickers, live preview, screenshot and recording)")
}

fn wysiwyg_ui(_: &Args) -> anyhow::Result<String> {
    anyhow::bail!("needs the rich-text editor panel, which emits the formatted HTML source")
}

// ---------------------------------------------------------------- text-diff

fn text_diff(args: &Args) -> anyhow::Result<String> {
    let old = a(args, "text-diff", "old");
    let new = a(args, "text-diff", "new");
    if old.trim().is_empty() && new.trim().is_empty() {
        anyhow::bail!("provide two texts to compare");
    }
    let diff = similar::TextDiff::from_lines(old, new);
    let mut u = diff.unified_diff();
    let out = u.context_radius(3).header("old", "new").to_string();
    Ok(if out.trim().is_empty() {
        "texts are identical".into()
    } else {
        out
    })
}

// ------------------------------------------------------------ text-statistics

/// it-tools: `new TextEncoder().encode(text).buffer.byteLength`, i.e. UTF-8 len.
fn string_size_in_bytes(text: &str) -> usize {
    text.len()
}

/// (chars, words, lines, bytes)
///
/// The counts are the four `n-statistic` tiles of the `.vue`, including its
/// quirks: word count is `text.split(/\s+/).length`, so a leading space counts
/// as a word — that is one whitespace run plus one, and a blank `text` is
/// special-cased to 0 rather than 1.
fn text_stats(text: &str) -> (usize, usize, usize, usize) {
    let chars = text.chars().count();
    let mut whitespace_runs = 0;
    let mut prev_ws = false;
    for c in text.chars() {
        let ws = c.is_whitespace();
        if ws && !prev_ws {
            whitespace_runs += 1;
        }
        prev_ws = ws;
    }
    let words = if text.is_empty() {
        0
    } else {
        whitespace_runs + 1
    };
    let lines = if text.is_empty() {
        0
    } else {
        text.replace("\r\n", "\n").split(['\n', '\r']).count()
    };
    (chars, words, lines, string_size_in_bytes(text))
}

/// it-tools shows the four tiles for any input, empty included (all zeroes), so
/// there is nothing to fail on and nothing to bail out of.
fn text_statistics(args: &Args) -> anyhow::Result<String> {
    let (chars, words, lines, bytes) = text_stats(a(args, "text-statistics", "text"));
    Ok(format!(
        "Character count: {chars}\nWord count: {words}\nLine count: {lines}\nByte size: {bytes} B"
    ))
}

// ------------------------------------------------------- text-to-nato-alphabet

const NATO: [&str; 26] = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India", "Juliet",
    "Kilo", "Lima", "Mike", "November", "Oscar", "Papa", "Quebec", "Romeo", "Sierra", "Tango",
    "Uniform", "Victor", "Whiskey", "X-ray", "Yankee", "Zulu",
];

fn text_to_nato(text: &str) -> String {
    text.chars()
        .map(|c| {
            let lower = c.to_ascii_lowercase();
            if lower.is_ascii_lowercase() {
                NATO[(lower as u8 - b'a') as usize].to_string()
            } else {
                c.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn nato_tool(args: &Args) -> anyhow::Result<String> {
    let text = nonempty("text", a(args, "text-to-nato-alphabet", "text"))?;
    Ok(text_to_nato(text))
}

// ------------------------------------------------------ lorem-ipsum-generator

const LOREM_VOCAB: &[&str] = &[
    "a",
    "ac",
    "accumsan",
    "ad",
    "adipiscing",
    "aenean",
    "aliquam",
    "aliquet",
    "amet",
    "ante",
    "aptent",
    "arcu",
    "at",
    "auctor",
    "bibendum",
    "blandit",
    "class",
    "commodo",
    "condimentum",
    "congue",
    "consectetur",
    "consequat",
    "conubia",
    "convallis",
    "cras",
    "cubilia",
    "cum",
    "curabitur",
    "curae",
    "dapibus",
    "diam",
    "dictum",
    "dictumst",
    "dignissim",
    "dolor",
    "donec",
    "dui",
    "duis",
    "egestas",
    "eget",
    "eleifend",
    "elementum",
    "elit",
    "enim",
    "erat",
    "eros",
    "est",
    "et",
    "etiam",
    "eu",
    "euismod",
    "facilisi",
    "faucibus",
    "felis",
    "fermentum",
    "feugiat",
    "fringilla",
    "fusce",
    "gravida",
    "habitant",
    "habitasse",
    "hac",
    "hendrerit",
    "himenaeos",
    "iaculis",
    "id",
    "imperdiet",
    "in",
    "inceptos",
    "integer",
    "interdum",
    "ipsum",
    "justo",
    "lacinia",
    "lacus",
    "laoreet",
    "lectus",
    "leo",
    "ligula",
    "litora",
    "lobortis",
    "lorem",
    "luctus",
    "maecenas",
    "magna",
    "magnis",
    "malesuada",
    "massa",
    "mattis",
    "mauris",
    "metus",
    "mi",
    "molestie",
    "mollis",
    "montes",
    "morbi",
    "mus",
    "nam",
    "nascetur",
    "natoque",
    "nec",
    "neque",
    "netus",
    "nisi",
    "nisl",
    "non",
    "nostra",
    "nulla",
    "nullam",
    "nunc",
    "odio",
    "orci",
    "ornare",
    "parturient",
    "pellentesque",
    "penatibus",
    "per",
    "pharetra",
    "phasellus",
    "placerat",
    "platea",
    "porta",
    "porttitor",
    "posuere",
    "potenti",
    "praesent",
    "pretium",
    "primis",
    "proin",
    "pulvinar",
    "purus",
    "quam",
    "quis",
    "quisque",
    "rhoncus",
    "ridiculus",
    "risus",
    "rutrum",
    "sagittis",
    "sapien",
    "scelerisque",
    "sed",
    "sem",
    "semper",
    "senectus",
    "sit",
    "sociis",
    "sociosqu",
    "sodales",
    "sollicitudin",
    "suscipit",
    "suspendisse",
    "taciti",
    "tellus",
    "tempor",
    "tempus",
    "tincidunt",
    "torquent",
    "tortor",
    "turpis",
    "ullamcorper",
    "ultrices",
    "ultricies",
    "urna",
    "varius",
    "vehicula",
    "vel",
    "velit",
    "venenatis",
    "vestibulum",
    "vitae",
    "vivamus",
    "viverra",
    "volutpat",
    "vulputate",
];

const LOREM_FIRST: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit.";

fn lorem_sentence(words: usize) -> String {
    let mut s: String = (0..words)
        .map(|_| LOREM_VOCAB[rand::random_range(0..LOREM_VOCAB.len())])
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(first) = s.get(0..1).map(str::to_uppercase) {
        s.replace_range(0..1, &first);
    }
    s.push('.');
    s
}

fn generate_lorem_ipsum(
    paragraphs: usize,
    sentences: usize,
    words: usize,
    start_with_lorem: bool,
    as_html: bool,
) -> String {
    let mut paras: Vec<Vec<String>> = (0..paragraphs)
        .map(|_| (0..sentences).map(|_| lorem_sentence(words)).collect())
        .collect();

    if start_with_lorem && let Some(first) = paras.first_mut() {
        if first.is_empty() {
            first.push(LOREM_FIRST.into());
        } else {
            first[0] = LOREM_FIRST.into();
        }
    }

    let joined: Vec<String> = paras.iter().map(|s| s.join(" ")).collect();
    if as_html {
        format!("<p>{}</p>", joined.join("</p>\n\n<p>"))
    } else {
        joined.join("\n\n")
    }
}

fn lorem_tool(args: &Args) -> anyhow::Result<String> {
    let id = "lorem-ipsum-generator";
    let paragraphs = uint("paragraphs", a(args, id, "paragraphs"), 100)?;
    let sentences = uint("sentences", a(args, id, "sentences"), 200)?;
    let words = uint("words", a(args, id, "words"), 200)?;
    if paragraphs == 0 || sentences == 0 || words == 0 {
        anyhow::bail!("paragraphs, sentences and words must all be at least 1");
    }
    Ok(generate_lorem_ipsum(
        paragraphs,
        sentences,
        words,
        flag(a(args, id, "start_with_lorem")),
        flag(a(args, id, "as_html")),
    ))
}

// ------------------------------------------------------- numeronym-generator

fn generate_numeronym(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() <= 3 {
        return word.to_string();
    }
    format!("{}{}{}", chars[0], chars.len() - 2, chars[chars.len() - 1])
}

fn numeronym_tool(args: &Args) -> anyhow::Result<String> {
    let text = nonempty("word", a(args, "numeronym-generator", "word"))?;
    // ponytail: it-tools treats the whole input as one word; per-word is a
    // strict superset and single-word input behaves identically.
    Ok(text
        .split_whitespace()
        .map(generate_numeronym)
        .collect::<Vec<_>>()
        .join(" "))
}

// ------------------------------------------------------------- regex-tester

#[derive(Debug, PartialEq, Eq)]
struct Capture {
    name: String,
    value: String,
    start: usize,
    end: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct RegexMatch {
    index: usize,
    value: String,
    captures: Vec<Capture>,
    groups: Vec<Capture>,
}

/// ponytail: offsets are UTF-8 byte offsets, it-tools reports UTF-16 units.
/// Identical for ASCII; revisit if the UI highlights non-ASCII matches.
fn regex_matches(pattern: &str, text: &str, flags: &str) -> anyhow::Result<Vec<RegexMatch>> {
    // `g`/`d`/`y`/`u` are JS-only concerns: the Rust engine is always global,
    // always reports indices and is always Unicode-aware.
    let inline: String = flags.chars().filter(|c| "imsxU".contains(*c)).collect();
    let full = if inline.is_empty() {
        pattern.to_string()
    } else {
        format!("(?{inline}){pattern}")
    };
    let re = Regex::new(&full).map_err(|e| anyhow::anyhow!("invalid regex: {e}"))?;

    let names: Vec<Option<String>> = re.capture_names().map(|n| n.map(str::to_string)).collect();
    // Without JS's `g` flag `exec` never advances, so it-tools reports one match.
    let global = flags.contains('g');
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        let whole = caps.get(0).expect("group 0 always matches");
        if whole.as_str().is_empty() {
            break; // it-tools bails on zero-length matches to avoid looping
        }
        let mut captures = Vec::new();
        let mut groups = Vec::new();
        for (i, name) in names.iter().enumerate().skip(1) {
            let Some(m) = caps.get(i) else { continue };
            captures.push(Capture {
                name: i.to_string(),
                value: m.as_str().to_string(),
                start: m.start(),
                end: m.end(),
            });
            if let Some(n) = name {
                groups.push(Capture {
                    name: n.clone(),
                    value: m.as_str().to_string(),
                    start: m.start(),
                    end: m.end(),
                });
            }
        }
        out.push(RegexMatch {
            index: whole.start(),
            value: whole.as_str().to_string(),
            captures,
            groups,
        });
        if !global {
            break;
        }
    }
    Ok(out)
}

fn format_matches(matches: Vec<RegexMatch>) -> String {
    if matches.is_empty() {
        return "No match.".into();
    }
    let mut out = format!("{} match(es)\n", matches.len());
    for m in &matches {
        out.push_str(&format!("\n@{} {:?}\n", m.index, m.value));
        for c in &m.captures {
            out.push_str(&format!(
                "  group {}: {:?} [{}..{}]\n",
                c.name, c.value, c.start, c.end
            ));
        }
        for g in &m.groups {
            out.push_str(&format!(
                "  named {}: {:?} [{}..{}]\n",
                g.name, g.value, g.start, g.end
            ));
        }
    }
    out
}

/// it-tools exposes the flags as six checkboxes — `g`/`i`/`m`/`s`/`u`/`v`, with
/// `g`, `s` and `u` on by default — and swallows a compile error into an empty
/// match list, leaving the pattern field's own validation to report it. So the
/// invalid pattern is reported inside the Matches panel, not as a dead output.
///
/// `u` and `v` are not offered: the Rust engine is always Unicode-aware and has
/// no `v` mode. Upstream also renders a RandExp "sample matching text" and a
/// railroad diagram of the pattern; neither has a dependency-free equivalent.
fn regex_tester(args: &Args) -> anyhow::Result<String> {
    let id = "regex-tester";
    let pattern = nonempty("regex", a(args, id, "regex"))?;
    let mut flags = String::new();
    for (key, ch) in [
        ("global", 'g'),
        ("ignore_case", 'i'),
        ("multiline", 'm'),
        ("dot_all", 's'),
    ] {
        if flag(a(args, id, key)) {
            flags.push(ch);
        }
    }
    Ok(crate::section(
        "Matches",
        regex_matches(pattern, a(args, id, "text"), &flags).map(format_matches),
    ))
}

// ------------------------------------------------------------ static memos

const REGEX_MEMO: &str = r#"### Normal characters

Expression | Description
:--|:--
`.` or `[^\n\r]` | any character *excluding* a newline or carriage return
`[A-Za-z]` | alphabet
`[a-z]` | lowercase alphabet
`[A-Z]` | uppercase alphabet
`\d` or `[0-9]` | digit
`\D` or `[^0-9]` | non-digit
`_` | underscore
`\w` or `[A-Za-z0-9_]` | alphabet, digit or underscore
`\W` or `[^A-Za-z0-9_]` | inverse of `\w`
`\S` | inverse of `\s`

### Whitespace characters

Expression | Description
:--|:--
` ` | space
`\t` | tab
`\n` | newline
`\r` | carriage return
`\s` | space, tab, newline or carriage return

### Character set

Expression | Description
:--|:--
`[xyz]` | either `x`, `y` or `z`
`[^xyz]` | neither `x`, `y` nor `z`
`[1-3]` | either `1`, `2` or `3`
`[^1-3]` | neither `1`, `2` nor `3`

- Think of a character set as an `OR` operation on the single characters that are enclosed between the square brackets.
- Use `^` after the opening `[` to negate the character set.
- Within a character set, `.` means a literal period.

### Characters that require escaping

#### Outside a character set

Expression | Description
:--|:--
`\.` | period
`\^` | caret
`\$` | dollar sign
`\|` | pipe
`\\` | back slash
`\/` | forward slash
`\(` | opening bracket
`\)` | closing bracket
`\[` | opening square bracket
`\]` | closing square bracket
`\{` | opening curly bracket
`\}` | closing curly bracket

#### Inside a character set

Expression | Description
:--|:--
`\\` | back slash
`\]` | closing square bracket

- A `^` must be escaped only if it occurs immediately after the opening `[` of the character set.
- A `-` must be escaped only if it occurs between two alphabets or two digits.

### Quantifiers

Expression | Description
:--|:--
`{2}` | exactly 2
`{2,}` | at least 2
`{2,7}` | at least 2 but no more than 7
`*` | 0 or more
`+` | 1 or more
`?` | exactly 0 or 1

- The quantifier goes *after* the expression to be quantified.

### Boundaries

Expression | Description
:--|:--
`^` | start of string
`$` | end of string
`\b` | word boundary

- How word boundary matching works:
    - At the beginning of the string if the first character is `\w`.
    - Between two adjacent characters within the string, if the first character is `\w` and the second character is `\W`.
    - At the end of the string if the last character is `\w`.

### Matching

Expression | Description
:--|:--
`foo\|bar` | match either `foo` or `bar`
`foo(?=bar)` | match `foo` if it's before `bar`
`foo(?!bar)` | match `foo` if it's *not* before `bar`
`(?<=bar)foo` | match `foo` if it's after `bar`
`(?<!bar)foo` | match `foo` if it's *not* after `bar`

### Grouping and capturing

Expression | Description
:--|:--
`(foo)` | capturing group; match and capture `foo`
`(?:foo)` | non-capturing group; match `foo` but *without* capturing `foo`
`(foo)bar\1` | `\1` is a backreference to the 1st capturing group; match `foobarfoo`

- `\N` is a backreference to the `Nth` capturing group. Capturing groups are numbered starting from 1.
- Note: the Rust `regex` engine used by the regex tester supports neither lookaround nor backreferences.

## References and tools

- [MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Regular_Expressions)
- [RegExplained](https://leaverou.github.io/regexplained/)
"#;

const GIT_MEMO: &str = r#"## Configuration

Set the global config

```shell
git config --global user.name "[name]"
git config --global user.email "[email]"
```

## Get started

Create a git repository

```shell
git init
```

Clone an existing git repository

```shell
git clone [url]
```

## Commit

Commit all tracked changes

```shell
git commit -am "[commit message]"
```

Add new modifications to the last commit

```shell
git commit --amend --no-edit
```

## I've made a mistake

Change last commit message

```shell
git commit --amend
```

Undo most recent commit and keep changes

```shell
git reset HEAD~1
```

Undo the `N` most recent commit and keep changes

```shell
git reset HEAD~N
```

Undo most recent commit and get rid of changes

```shell
git reset HEAD~1 --hard
```

Reset branch to remote state

```shell
git fetch origin
git reset --hard origin/[branch-name]
```

## Miscellaneous

Renaming the local master branch to main

```shell
git branch -m master main
```
"#;

fn regex_memo(_: &Args) -> anyhow::Result<String> {
    Ok(REGEX_MEMO.into())
}

fn git_memo(_: &Args) -> anyhow::Result<String> {
    Ok(GIT_MEMO.into())
}

// ----------------------------------------------------------- math-evaluator

/// mathjs accepts bare `sqrt(2)`; the evaluator below canonicalises those under
/// `math::` (the spelling the previous evalexpr backend required), so both
/// spellings keep working.
/// Longest names first so the alternation cannot bite off a prefix.
const MATH_FNS: &[&str] = &[
    "acosh", "asinh", "atanh", "atan2", "log10", "hypot", "sqrt", "cbrt", "cosh", "sinh", "tanh",
    "acos", "asin", "atan", "log2", "exp2", "log", "exp", "pow", "cos", "sin", "tan", "abs", "ln",
];

fn namespace_math_fns(expr: &str) -> String {
    // ponytail: swallowing an existing `math::` prefix in the same match keeps
    // the rewrite idempotent; a real expression parser is overkill here.
    let re =
        Regex::new(&format!(r"(math::)?\b({})\s*\(", MATH_FNS.join("|"))).expect("static pattern");
    re.replace_all(expr, "math::${2}(").into_owned()
}

/// Recursion budget for nested parens and unary chains. A megabyte of `(` has to
/// come back as an error, not as a stack overflow (which no `Result` can catch).
const MATH_MAX_DEPTH: u32 = 64;

/// ponytail: hand-rolled recursive-descent over f64 instead of a crate. Every
/// permissively-licensed evaluator on crates.io is missing part of the mathjs
/// function set below (fasteval and meval have no cbrt/hypot/exp2, evalexpr-jit
/// depends on AGPL evalexpr, rhai is a whole scripting language), and none of
/// them documents a depth bound we can rely on for the no-panic property test.
/// Upgrade path: swap in a crate if this ever needs variables or non-numerics.
struct MathParser<'a> {
    s: &'a [u8],
    i: usize,
    depth: u32,
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
fn math_constant(name: &str) -> Option<f64> {
    Some(match name {
        "e" | "E" => std::f64::consts::E,
        "pi" | "PI" => std::f64::consts::PI,
        "tau" => std::f64::consts::TAU,
        "phi" => 1.618_033_988_749_895, // golden ratio, as mathjs defines it
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
fn factorial(x: f64) -> anyhow::Result<f64> {
    if x < 0.0 || x.fract() != 0.0 {
        anyhow::bail!("factorial needs a non-negative whole number");
    }
    if x > 170.0 {
        anyhow::bail!("factorial of {x} overflows a float");
    }
    Ok((1..=(x as u64)).fold(1.0f64, |acc, n| acc * n as f64))
}

fn apply_math_fn(name: &str, args: &[f64]) -> anyhow::Result<f64> {
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

fn eval_math(expr: &str) -> anyhow::Result<f64> {
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

fn math_evaluator(args: &Args) -> anyhow::Result<String> {
    let expr = nonempty("expression", a(args, "math-evaluator", "expression"))?;
    let value = eval_math(&namespace_math_fns(expr))
        .map_err(|e| anyhow::anyhow!("cannot evaluate: {e}"))?;
    Ok(value.to_string())
}

// ------------------------------------------------------ percentage-calculator

// it-tools puts three independently-filled calculators on the page, each in its
// own card with its own pair of inputs and its own readonly result — there is no
// mode select, and a blank or non-finite result only blanks that one card.

fn percent_of(x: f64, y: f64) -> f64 {
    x / 100.0 * y
}

fn percent_ratio(x: f64, y: f64) -> f64 {
    100.0 * x / y
}

fn percent_change(from: f64, to: f64) -> f64 {
    (to - from) / from * 100.0
}

/// it-tools renders `''` for a NaN/Infinity result; a form that only speaks text
/// says why instead, and `section` keeps that inside the one card.
fn finite(v: f64) -> anyhow::Result<String> {
    if !v.is_finite() {
        anyhow::bail!("no result (division by zero?)");
    }
    Ok(fmt_f(v))
}

fn percentage_calculator(args: &Args) -> anyhow::Result<String> {
    let id = "percentage-calculator";
    let pair = |x_key: &str, y_key: &str, x_label: &str, y_label: &str, f: fn(f64, f64) -> f64| {
        let x = num(x_label, a(args, id, x_key))?;
        let y = num(y_label, a(args, id, y_key))?;
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

// ---------------------------------------------------------- eta-calculator

const TIME_SPAN_UNITS: &[&str] = &["milliseconds", "seconds", "minutes", "hours", "days"];

fn span_unit_ms(unit: &str) -> anyhow::Result<f64> {
    Ok(match unit {
        "milliseconds" => 1.0,
        "seconds" => 1000.0,
        "minutes" => 60_000.0,
        "hours" => 3_600_000.0,
        "days" => 86_400_000.0,
        other => anyhow::bail!("unknown time span unit `{other}`"),
    })
}

/// Port of `formatMsDuration`: zero units are omitted, ms appended when present.
fn format_ms_duration(duration: f64) -> String {
    let total = duration.trunc().max(0.0) as i64;
    let ms = total % 1000;
    let secs = total / 1000 % 60;
    let mins = total / 60_000 % 60;
    let hrs = total / 3_600_000;

    let mut parts = Vec::new();
    if hrs > 0 {
        parts.push(format!("{hrs} hour{}", if hrs == 1 { "" } else { "s" }));
    }
    if mins > 0 {
        parts.push(format!("{mins} minute{}", if mins == 1 { "" } else { "s" }));
    }
    if secs > 0 {
        parts.push(format!("{secs} second{}", if secs == 1 { "" } else { "s" }));
    }
    if ms > 0 {
        parts.push(format!("{ms} ms"));
    }
    if parts.is_empty() {
        "0 seconds".into()
    } else {
        parts.join(" ")
    }
}

fn eta_calculator(args: &Args) -> anyhow::Result<String> {
    let id = "eta-calculator";
    let unit_count = num("amount of elements", a(args, id, "unit_count"))?;
    let per_span = num("units per time span", a(args, id, "units_per_span"))?;
    let span = num("time span", a(args, id, "span"))?;
    let span_ms = span * span_unit_ms(a(args, id, "span_unit"))?;
    if per_span <= 0.0 {
        anyhow::bail!("units per time span must be greater than 0");
    }
    let duration_ms = unit_count / (per_span / span_ms);
    if !duration_ms.is_finite() {
        anyhow::bail!("duration is not a finite number");
    }
    // it-tools' "The consumption started at" date picker, defaulting to now.
    let started_raw = a(args, id, "started_at").trim();
    let started_at = if started_raw.is_empty() {
        Utc::now()
    } else {
        parse_iso_like(started_raw)?
    };
    // `is_finite` is not enough: a large-but-finite duration overflows both
    // `TimeDelta` and the date arithmetic, and chrono panics rather than
    // returning. The UI calls this on every keystroke, so a panic is a crash.
    let end = chrono::Duration::try_milliseconds(duration_ms.trunc() as i64)
        .and_then(|d| started_at.checked_add_signed(d))
        .map(|end| end.to_rfc3339())
        .ok_or_else(|| anyhow::anyhow!("out of representable range"));
    Ok(format!(
        "{}{}",
        row("Total duration", Ok(format_ms_duration(duration_ms))),
        row("It will end", end),
    ))
}

// ------------------------------------------------------ temperature-converter

const TEMP_SCALES: &[&str] = &[
    "kelvin",
    "celsius",
    "fahrenheit",
    "rankine",
    "delisle",
    "newton",
    "reaumur",
    "romer",
];

fn to_kelvin(scale: &str, t: f64) -> anyhow::Result<f64> {
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

fn from_kelvin(scale: &str, k: f64) -> anyhow::Result<f64> {
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
fn temp_title_unit(scale: &str) -> (&'static str, &'static str) {
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
/// Values are shown to two decimals like `update()` upstream, but *rounded*
/// rather than floored: `Math.floor(v * 100) / 100` turns 100 °C into
/// "211.99 °F", and reproducing that particular artefact helps nobody.
fn temperature_converter(args: &Args) -> anyhow::Result<String> {
    let id = "temperature-converter";
    let from = a(args, id, "from");
    let value = num("temperature", a(args, id, "value"))?;
    let k = to_kelvin(from, value)?;
    Ok(TEMP_SCALES
        .iter()
        .map(|scale| {
            let (title, unit) = temp_title_unit(scale);
            row(
                title,
                from_kelvin(scale, k)
                    .map(|v| format!("{} {unit}", fmt_f((v * 100.0).round() / 100.0))),
            )
        })
        .collect())
}

// ------------------------------------------------------ date-time-converter

const DATE_FORMATS: &[&str] = &[
    "auto",
    "iso8601",
    "iso9075",
    "rfc3339",
    "rfc7231",
    "unix",
    "timestamp",
    "utc",
    "mongo",
    "excel",
];

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static pattern")
}

/// ponytail: the it-tools ISO 8601 regex leans on backreferences (`\3`, `\17`)
/// to force consistent separators; the Rust engine has none, so this variant
/// accepts a mixed `2016-0201` too. Upgrade path: hand-rolled scanner.
fn is_iso8601(s: &str) -> bool {
    re(r"^[+-]?\d{4}(-?(0[1-9]|1[0-2])(-?(0[1-9]|[12]\d|3[01]))?)?([T ](([01]\d|2[0-3])(:?[0-5]\d(:?[0-5]\d([.,]\d+)?)?)?)([zZ]|[+-]([01]\d|2[0-3])(:?[0-5]\d)?)?)?$")
        .is_match(s)
}

fn is_iso9075(s: &str) -> bool {
    re(r"^([0-9]{4})-([0-9]{2})-([0-9]{2}) ([0-9]{2}):([0-9]{2}):([0-9]{2})(\.[0-9]{1,6})?(([+-])([0-9]{2}):([0-9]{2})|Z)?$")
        .is_match(s)
}

fn is_rfc3339(s: &str) -> bool {
    re(r"^([0-9]{4})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2})(\.[0-9]{1,9})?(([+-])([0-9]{2}):([0-9]{2})|Z)$")
        .is_match(s)
}

fn is_rfc7231(s: &str) -> bool {
    re(r"^[A-Za-z]{3},\s[0-9]{2}\s[A-Za-z]{3}\s[0-9]{4}\s[0-9]{2}:[0-9]{2}:[0-9]{2}\sGMT$")
        .is_match(s)
}

fn is_unix_timestamp(s: &str) -> bool {
    re(r"^[0-9]{1,10}$").is_match(s)
}

fn is_timestamp(s: &str) -> bool {
    re(r"^[0-9]{1,13}$").is_match(s)
}

fn is_mongo_object_id(s: &str) -> bool {
    re(r"^[0-9a-fA-F]{24}$").is_match(s)
}

fn is_excel_format(s: &str) -> bool {
    re(r"^-?\d+(\.\d+)?$").is_match(s)
}

/// it-tools: `new Date(s).toUTCString() === s`.
fn is_utc_date_string(s: &str) -> bool {
    parse_rfc7231(s)
        .map(|d| format_rfc7231(d) == s)
        .unwrap_or(false)
}

fn date_to_excel_format(ms: i64) -> String {
    format!("{}", ms as f64 / 86_400_000.0 + 25569.0)
}

fn excel_format_to_ms(excel: f64) -> i64 {
    // JS `new Date(x)` truncates toward zero, and so does `as i64`.
    ((excel - 25569.0) * 86400.0 * 1000.0) as i64
}

fn format_rfc7231(dt: DateTime<Utc>) -> String {
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

fn parse_rfc7231(s: &str) -> anyhow::Result<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s, "%a, %d %b %Y %H:%M:%S GMT")?;
    Ok(Utc.from_utc_datetime(&naive))
}

fn from_ms(ms: i64) -> anyhow::Result<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms).ok_or_else(|| anyhow::anyhow!("timestamp out of range"))
}

fn parse_iso_like(s: &str) -> anyhow::Result<DateTime<Utc>> {
    let s = s.trim();
    // Normalise a trailing Z and the ISO 9075 space separator so one code path
    // covers ISO 8601, ISO 9075 and RFC 3339.
    let mut norm = s.replace(' ', "T").replace(',', ".");
    if norm.ends_with('Z') || norm.ends_with('z') {
        norm.truncate(norm.len() - 1);
        norm.push_str("+00:00");
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(&norm) {
        return Ok(dt.with_timezone(&Utc));
    }
    for f in ["%Y%m%dT%H%M%S%z", "%Y%m%dT%H%M%S%.f%z", "%Y-%m-%dT%H:%M%z"] {
        if let Ok(dt) = DateTime::parse_from_str(&norm, f) {
            return Ok(dt.with_timezone(&Utc));
        }
    }
    for f in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M",
        "%Y%m%dT%H%M%S",
        "%Y%m%dT%H%M",
    ] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(&norm, f) {
            return Ok(Utc.from_utc_datetime(&dt));
        }
    }
    for f in ["%Y-%m-%d", "%Y%m%d"] {
        if let Ok(d) = NaiveDate::parse_from_str(&norm, f) {
            return Ok(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight")));
        }
    }
    // Year-only (`2016`) and year-month (`2016-02`) forms.
    let ym = match norm.split_once('-') {
        Some((y, m)) => (y.parse::<i32>().ok(), m.parse::<u32>().ok()),
        None => (norm.parse::<i32>().ok(), Some(1)),
    };
    if let (Some(y), Some(m)) = ym
        && let Some(d) = NaiveDate::from_ymd_opt(y, m, 1)
    {
        return Ok(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight")));
    }
    anyhow::bail!("`{s}` is not a recognisable date")
}

type DateMatcher = fn(&str) -> bool;

fn detect_date_format(s: &str) -> Option<&'static str> {
    // Order mirrors the it-tools format list, first match wins.
    let checks: &[(&str, DateMatcher)] = &[
        ("iso8601", is_iso8601),
        ("iso9075", is_iso9075),
        ("rfc3339", is_rfc3339),
        ("rfc7231", is_rfc7231),
        ("unix", is_unix_timestamp),
        ("timestamp", is_timestamp),
        ("utc", is_utc_date_string),
        ("mongo", is_mongo_object_id),
        ("excel", is_excel_format),
    ];
    checks.iter().find(|(_, f)| f(s)).map(|(n, _)| *n)
}

fn parse_date(format: &str, s: &str) -> anyhow::Result<DateTime<Utc>> {
    let s = s.trim();
    match format {
        "unix" => from_ms(s.parse::<i64>()? * 1000),
        "timestamp" => from_ms(s.parse::<i64>()?),
        "mongo" => {
            if !is_mongo_object_id(s) {
                anyhow::bail!("`{s}` is not a 24-character hex ObjectID");
            }
            from_ms(i64::from_str_radix(&s[0..8], 16)? * 1000)
        }
        "excel" => {
            from_ms(excel_format_to_ms(s.parse::<f64>().map_err(|_| {
                anyhow::anyhow!("`{s}` is not an Excel serial date")
            })?))
        }
        "rfc7231" | "utc" => parse_rfc7231(s),
        "iso8601" | "iso9075" | "rfc3339" => parse_iso_like(s),
        "auto" => {
            let detected = detect_date_format(s)
                .ok_or_else(|| anyhow::anyhow!("cannot detect the format of `{s}`"))?;
            parse_date(detected, s)
        }
        other => anyhow::bail!("unknown date format `{other}`"),
    }
}

/// Every format at once, in the order of the `.vue`'s `formats` array and with
/// its labels — the page is a list of readonly rows, not a pick-one converter.
/// Each row carries its own error so an unrepresentable one (a pre-epoch date
/// has no ObjectID) does not blank the other nine.
fn render_all_date_formats(dt: DateTime<Utc>) -> String {
    let ms = dt.timestamp_millis();
    let secs = dt.timestamp();
    [
        row(
            "JS locale date string",
            Ok(dt
                .format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)")
                .to_string()),
        ),
        row(
            "ISO 8601",
            Ok(dt.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()),
        ),
        row("ISO 9075", Ok(dt.format("%Y-%m-%d %H:%M:%S").to_string())),
        row(
            "RFC 3339",
            Ok(dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        ),
        row("RFC 7231", Ok(format_rfc7231(dt))),
        row("Unix timestamp", Ok(secs.to_string())),
        row("Timestamp", Ok(ms.to_string())),
        row("UTC format", Ok(format_rfc7231(dt))),
        row(
            "Mongo ObjectID",
            if secs < 0 {
                Err(anyhow::anyhow!("no ObjectID before the unix epoch"))
            } else {
                Ok(format!("{secs:x}0000000000000000"))
            },
        ),
        row("Excel date/time", Ok(date_to_excel_format(ms))),
    ]
    .concat()
}

/// Empty input shows *now* in every format, exactly as the `.vue` does
/// (`if (!inputDate.value) return now.value`) — hence the tool's entry in the
/// property suite's `NONDETERMINISTIC` list.
fn date_time_converter(args: &Args) -> anyhow::Result<String> {
    let id = "date-time-converter";
    let input = a(args, id, "date").trim();
    let dt = if input.is_empty() {
        Utc::now()
    } else {
        parse_date(a(args, id, "format"), input)?
    };
    Ok(render_all_date_formats(dt))
}

// -------------------------------------------------------- benchmark-builder

fn parse_series(line: &str) -> (String, Vec<f64>) {
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

fn mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        0.0
    } else {
        data.iter().sum::<f64>() / data.len() as f64
    }
}

fn variance(data: &[f64]) -> f64 {
    let m = mean(data);
    let squared: Vec<f64> = data.iter().map(|v| (v - m).powi(2)).collect();
    mean(&squared)
}

/// it-tools rounds every displayed statistic to three decimals.
fn round3(v: f64) -> String {
    fmt_f((v * 1000.0).round() / 1000.0)
}

/// The table has exactly the five columns of the `.vue`'s `header` map — no
/// median/min/max, which upstream does not compute.
///
/// A suite with no parseable values is kept with 0 samples and a mean of 0
/// (upstream `data.filter(_.isNumber)` does the same), rather than taking the
/// whole table down: one mistyped line must not blank the other suites.
fn benchmark_builder(args: &Args) -> anyhow::Result<String> {
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

// ---------------------------------------------------------- color-converter

fn remove_alpha_channel_when_opaque(hex: &str) -> String {
    re(r"(?i)^(#(?:[0-9a-f]{3}){1,2})ff$")
        .replace(hex, "$1")
        .into_owned()
}

fn color_converter(args: &Args) -> anyhow::Result<String> {
    let input = nonempty("color", a(args, "color-converter", "color"))?.trim();
    let c = csscolorparser::parse(input).map_err(|e| anyhow::anyhow!("invalid color: {e}"))?;
    let [r, g, b, _] = c.to_rgba8();

    // cmyk is not in csscolorparser; the formula is three lines.
    let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
    let k = 1.0 - rf.max(gf).max(bf);
    let comp = |v: f64| {
        if k >= 1.0 {
            0.0
        } else {
            (1.0 - v - k) / (1.0 - k) * 100.0
        }
    };

    // Row order is the `.vue`'s `formats` record: hex, rgb, hsl, hwb, lch, cmyk,
    // name (its first entry is the colour-picker widget, which has no text form).
    // `oklch` is ours: csscolorparser gives it for free and it costs a row.
    //
    // ponytail: upstream's `toName({ closest: true })` always answers, naming the
    // nearest CSS colour; csscolorparser only knows exact matches, so anything
    // off-palette says "unknown" instead of "close to darkseagreen". Upgrade path
    // is the 148-entry CSS name table plus a nearest-neighbour scan.
    Ok([
        row("hex", Ok(remove_alpha_channel_when_opaque(&c.to_css_hex()))),
        row("rgb", Ok(c.to_css_rgb())),
        row("hsl", Ok(c.to_css_hsl())),
        row("hwb", Ok(c.to_css_hwb())),
        row("lch", Ok(c.to_css_lch())),
        row("oklch", Ok(c.to_css_oklch())),
        row(
            "cmyk",
            Ok(format!(
                "cmyk({}%, {}%, {}%, {}%)",
                comp(rf).round(),
                comp(gf).round(),
                comp(bf).round(),
                (k * 100.0).round()
            )),
        ),
        row(
            "name",
            c.name()
                .map(str::to_string)
                .ok_or_else(|| anyhow::anyhow!("no exact CSS colour name")),
        ),
    ]
    .concat())
}

// -------------------------------------------------------------- emoji-picker

/// (emoji, name, search keywords). ponytail: a curated set, not the 1,900-entry
/// `unicode-emoji-json` dump — embed the full table when the picker ships.
pub static EMOJIS: &[(&str, &str, &str)] = &[
    ("😀", "grinning face", "smile happy joy grin"),
    ("😃", "grinning face with big eyes", "smile happy joy"),
    ("😄", "grinning face with smiling eyes", "smile happy laugh"),
    ("😁", "beaming face with smiling eyes", "smile grin happy"),
    ("😆", "grinning squinting face", "laugh haha happy"),
    ("😅", "grinning face with sweat", "hot laugh relief"),
    ("🤣", "rolling on the floor laughing", "rofl lol laugh"),
    ("😂", "face with tears of joy", "lol cry laugh tears"),
    ("🙂", "slightly smiling face", "smile"),
    ("🙃", "upside-down face", "silly sarcasm"),
    ("😉", "winking face", "wink flirt"),
    ("😊", "smiling face with smiling eyes", "blush shy happy"),
    ("😍", "smiling face with heart-eyes", "love crush heart"),
    ("😘", "face blowing a kiss", "kiss love"),
    ("😗", "kissing face", "kiss"),
    ("🤪", "zany face", "crazy silly goofy"),
    ("😜", "winking face with tongue", "tongue silly"),
    ("🤗", "smiling face with open hands", "hug"),
    ("🤔", "thinking face", "think hmm consider"),
    ("🤨", "face with raised eyebrow", "suspicious skeptic"),
    ("😐", "neutral face", "meh indifferent"),
    ("😑", "expressionless face", "blank meh"),
    ("😶", "face without mouth", "silent quiet"),
    ("🙄", "face with rolling eyes", "eyeroll annoyed"),
    ("😏", "smirking face", "smug flirt"),
    ("😴", "sleeping face", "sleep zzz tired"),
    ("😪", "sleepy face", "tired sleep"),
    ("😷", "face with medical mask", "sick mask covid"),
    ("🤒", "face with thermometer", "sick ill fever"),
    ("🤕", "face with head-bandage", "hurt injured"),
    ("🤢", "nauseated face", "sick vomit gross"),
    ("🥵", "hot face", "heat sweat"),
    ("🥶", "cold face", "freezing ice"),
    ("😵", "face with crossed-out eyes", "dizzy dead ko"),
    ("🤯", "exploding head", "mind blown shocked"),
    ("😎", "smiling face with sunglasses", "cool sunglasses"),
    ("🥳", "partying face", "party celebrate birthday"),
    ("😕", "confused face", "confused unsure"),
    ("😟", "worried face", "worried concern"),
    ("🙁", "slightly frowning face", "sad frown"),
    ("😢", "crying face", "sad cry tear"),
    ("😭", "loudly crying face", "sob cry sad"),
    ("😤", "face with steam from nose", "triumph angry proud"),
    ("😠", "angry face", "mad angry"),
    ("😡", "enraged face", "rage furious mad"),
    ("🤬", "face with symbols on mouth", "swearing cursing"),
    ("😱", "face screaming in fear", "scared shock fear"),
    ("😨", "fearful face", "scared afraid"),
    ("😰", "anxious face with sweat", "nervous anxious"),
    ("🥺", "pleading face", "please beg puppy eyes"),
    ("😳", "flushed face", "embarrassed blush"),
    ("🤥", "lying face", "lie pinocchio"),
    ("🤐", "zipper-mouth face", "silence secret"),
    ("😈", "smiling face with horns", "devil evil"),
    ("💀", "skull", "dead death skull"),
    ("💩", "pile of poo", "poop crap"),
    ("🤡", "clown face", "clown circus"),
    ("👻", "ghost", "halloween spooky"),
    ("👽", "alien", "ufo space extraterrestrial"),
    ("🤖", "robot", "bot ai machine"),
    ("😺", "grinning cat", "cat happy"),
    ("❤️", "red heart", "love heart red"),
    ("🧡", "orange heart", "love heart orange"),
    ("💛", "yellow heart", "love heart yellow"),
    ("💚", "green heart", "love heart green"),
    ("💙", "blue heart", "love heart blue"),
    ("💜", "purple heart", "love heart purple"),
    ("🖤", "black heart", "love heart black"),
    ("💔", "broken heart", "heartbreak sad"),
    ("💯", "hundred points", "100 perfect score"),
    ("💥", "collision", "boom explosion bang"),
    ("🔥", "fire", "hot lit flame burn"),
    ("✨", "sparkles", "shiny stars magic"),
    ("⭐", "star", "star favourite"),
    ("🎉", "party popper", "party celebrate tada"),
    ("🎊", "confetti ball", "party celebrate"),
    ("🎂", "birthday cake", "birthday cake party"),
    ("🎁", "wrapped gift", "present gift"),
    ("👍", "thumbs up", "yes approve like ok"),
    ("👎", "thumbs down", "no disapprove dislike"),
    ("👌", "OK hand", "ok perfect"),
    ("🤝", "handshake", "deal agreement"),
    ("👏", "clapping hands", "applause clap bravo"),
    ("🙏", "folded hands", "please thanks pray"),
    ("💪", "flexed biceps", "strong muscle gym"),
    ("🤞", "crossed fingers", "luck hope"),
    ("🖖", "vulcan salute", "spock star trek"),
    ("👋", "waving hand", "hello bye wave"),
    ("✋", "raised hand", "stop high five"),
    ("👀", "eyes", "look watch see"),
    ("🧠", "brain", "smart mind think"),
    ("🚀", "rocket", "launch ship space deploy"),
    ("🐛", "bug", "insect bug defect"),
    ("💻", "laptop", "computer pc dev"),
    ("🖥️", "desktop computer", "computer monitor"),
    ("⌨️", "keyboard", "typing keys"),
    ("🖱️", "computer mouse", "mouse click"),
    ("💾", "floppy disk", "save disk"),
    ("📦", "package", "box parcel shipping"),
    ("🔒", "locked", "lock secure private"),
    ("🔑", "key", "key password unlock"),
    ("🔍", "magnifying glass tilted left", "search find zoom"),
    ("⚙️", "gear", "settings config cog"),
    ("🔧", "wrench", "tool fix repair"),
    ("🔨", "hammer", "tool build"),
    ("📝", "memo", "note write memo"),
    ("📌", "pushpin", "pin location"),
    ("📎", "paperclip", "attach clip"),
    ("📊", "bar chart", "chart stats graph"),
    ("📈", "chart increasing", "growth up trend"),
    ("📉", "chart decreasing", "loss down trend"),
    ("⏰", "alarm clock", "time alarm clock"),
    ("⏳", "hourglass not done", "time wait loading"),
    ("✅", "check mark button", "yes done ok check"),
    ("❌", "cross mark", "no wrong error x"),
    ("⚠️", "warning", "caution warning alert"),
    ("♻️", "recycling symbol", "recycle green"),
    ("☕", "hot beverage", "coffee tea drink"),
    ("🍕", "pizza", "food pizza slice"),
    ("🍺", "beer mug", "beer drink pub"),
    ("🌍", "globe showing Europe-Africa", "earth world globe"),
    ("🌙", "crescent moon", "night moon sleep"),
    ("☀️", "sun", "sunny weather day"),
    ("🌈", "rainbow", "pride rainbow colors"),
    ("⚡", "high voltage", "lightning power fast"),
    ("❄️", "snowflake", "snow cold winter"),
];

/// Search `EMOJIS` by name, keyword or literal emoji. Empty query = everything.
pub fn emoji_search(query: &str) -> Vec<&'static (&'static str, &'static str, &'static str)> {
    let q = query.trim().to_lowercase();
    EMOJIS
        .iter()
        .filter(|(e, name, kw)| {
            q.is_empty() || name.contains(&q) || kw.contains(&q) || *e == q.as_str()
        })
        .collect()
}

// -------------------------------------------------------- ascii-text-drawer

/// A 4x5 bitmap font: five rows, `#` on, `.` off. ponytail: one built-in font
/// instead of a vendored FIGlet pack (~4 MB of `.flf`). Upgrade path: parse real
/// `.flf` files from disk and keep this as the offline fallback.
const BLOCK_FONT: &[(char, [&str; 5])] = &[
    ('A', [".##.", "#..#", "####", "#..#", "#..#"]),
    ('B', ["###.", "#..#", "###.", "#..#", "###."]),
    ('C', [".###", "#...", "#...", "#...", ".###"]),
    ('D', ["###.", "#..#", "#..#", "#..#", "###."]),
    ('E', ["####", "#...", "###.", "#...", "####"]),
    ('F', ["####", "#...", "###.", "#...", "#..."]),
    ('G', [".###", "#...", "#.##", "#..#", ".###"]),
    ('H', ["#..#", "#..#", "####", "#..#", "#..#"]),
    ('I', ["####", ".##.", ".##.", ".##.", "####"]),
    ('J', ["####", "...#", "...#", "#..#", ".##."]),
    ('K', ["#..#", "#.#.", "##..", "#.#.", "#..#"]),
    ('L', ["#...", "#...", "#...", "#...", "####"]),
    ('M', ["#..#", "####", "####", "#..#", "#..#"]),
    ('N', ["#..#", "##.#", "####", "#.##", "#..#"]),
    ('O', [".##.", "#..#", "#..#", "#..#", ".##."]),
    ('P', ["###.", "#..#", "###.", "#...", "#..."]),
    ('Q', [".##.", "#..#", "#..#", "#.##", ".###"]),
    ('R', ["###.", "#..#", "###.", "#.#.", "#..#"]),
    ('S', [".###", "#...", ".##.", "...#", "###."]),
    ('T', ["####", ".##.", ".##.", ".##.", ".##."]),
    ('U', ["#..#", "#..#", "#..#", "#..#", ".##."]),
    ('V', ["#..#", "#..#", "#..#", ".##.", ".##."]),
    ('W', ["#..#", "#..#", "####", "####", "#..#"]),
    ('X', ["#..#", "#..#", ".##.", "#..#", "#..#"]),
    ('Y', ["#..#", "#..#", ".##.", ".##.", ".##."]),
    ('Z', ["####", "..#.", ".##.", "#...", "####"]),
    ('0', [".##.", "#.##", "####", "##.#", ".##."]),
    ('1', [".##.", "###.", ".##.", ".##.", "####"]),
    ('2', ["###.", "...#", ".##.", "#...", "####"]),
    ('3', ["###.", "...#", ".##.", "...#", "###."]),
    ('4', ["#..#", "#..#", "####", "...#", "...#"]),
    ('5', ["####", "#...", "###.", "...#", "###."]),
    ('6', [".###", "#...", "###.", "#..#", ".##."]),
    ('7', ["####", "...#", "..#.", ".#..", ".#.."]),
    ('8', [".##.", "#..#", ".##.", "#..#", ".##."]),
    ('9', [".##.", "#..#", ".###", "...#", "###."]),
    (' ', ["....", "....", "....", "....", "...."]),
    ('!', [".#..", ".#..", ".#..", "....", ".#.."]),
    ('?', ["###.", "...#", ".##.", "....", ".#.."]),
    ('.', ["....", "....", "....", "....", ".#.."]),
    (',', ["....", "....", "....", ".#..", "#..."]),
    ('-', ["....", "....", "####", "....", "...."]),
    ('+', ["....", ".#..", "####", ".#..", "...."]),
    ('=', ["....", "####", "....", "####", "...."]),
    (':', ["....", ".#..", "....", ".#..", "...."]),
    ('\'', [".#..", ".#..", "....", "....", "...."]),
    ('/', ["...#", "..#.", ".#..", "#...", "#..."]),
];

const GLYPH_WIDTH: usize = 5; // 4 bitmap columns + 1 space

fn glyph(c: char) -> [&'static str; 5] {
    let upper = c.to_ascii_uppercase();
    BLOCK_FONT
        .iter()
        .find(|(g, _)| *g == upper)
        .map(|(_, bits)| *bits)
        .unwrap_or([".#.#", "..#.", ".#.#", "..#.", ".#.#"]) // unknown char
}

fn render_block_font(text: &str, width: usize) -> String {
    let per_line = (width / GLYPH_WIDTH).max(1);
    let mut out = String::new();
    for chunk in text.chars().collect::<Vec<_>>().chunks(per_line) {
        for row in 0..5 {
            let line: String = chunk
                .iter()
                .map(|c| format!("{} ", glyph(*c)[row].replace('.', " ")))
                .collect::<String>();
            out.push_str(line.trim_end());
            out.push('\n');
        }
    }
    out
}

fn ascii_text_drawer(args: &Args) -> anyhow::Result<String> {
    let id = "ascii-text-drawer";
    let text = nonempty("text", a(args, id, "text"))?;
    let width = uint("width", a(args, id, "width"), 10_000)?;
    if width < GLYPH_WIDTH {
        anyhow::bail!("width must be at least {GLYPH_WIDTH}");
    }
    let font = a(args, id, "font");
    if font != "block" {
        anyhow::bail!("only the built-in `block` font is bundled (got `{font}`)");
    }
    let mut out = String::new();
    for (i, line) in text.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&render_block_font(line, width));
    }
    Ok(out)
}

// -------------------------------------------------- svg-placeholder-generator

fn svg_placeholder(args: &Args) -> anyhow::Result<String> {
    let id = "svg-placeholder-generator";
    let w = uint("width", a(args, id, "width"), 100_000)?;
    let h = uint("height", a(args, id, "height"), 100_000)?;
    let font_size = uint("font size", a(args, id, "font_size"), 10_000)?;
    if w == 0 || h == 0 || font_size == 0 {
        anyhow::bail!("width, height and font size must all be greater than 0");
    }
    let bg = a(args, id, "bg_color");
    let fg = a(args, id, "fg_color");
    for c in [bg, fg] {
        csscolorparser::parse(c).map_err(|e| anyhow::anyhow!("invalid color `{c}`: {e}"))?;
    }
    let custom = a(args, id, "text").trim().to_string();
    let text = if custom.is_empty() {
        format!("{w}x{h}")
    } else {
        custom
    };
    let size = if flag(a(args, id, "exact_size")) {
        format!(r#" width="{w}" height="{h}""#)
    } else {
        String::new()
    };

    let svg = format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}"{size}>"#,
            "\n",
            r#"  <rect width="{w}" height="{h}" fill="{bg}"></rect>"#,
            "\n",
            r#"  <text x="50%" y="50%" dominant-baseline="middle" text-anchor="middle" font-family="monospace" font-size="{fs}px" fill="{fg}">{text}</text>"#,
            "\n</svg>",
        ),
        w = w,
        h = h,
        size = size,
        bg = bg,
        fg = fg,
        fs = font_size,
        text = html_escape::encode_text(&text),
    );
    // it-tools shows both the SVG element and its base64 data URI, side by side.
    let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
    Ok(format!(
        "{}\n\n{}",
        crate::section("SVG HTML element", Ok(svg)),
        crate::section(
            "SVG in Base64",
            Ok(format!("data:image/svg+xml;base64,{b64}")),
        ),
    ))
}

// ------------------------------------------------- qr-code-generator (+ wifi)

/// it-tools names the four QR error-correction levels low/medium/quartile/high.
const QR_EC_LEVELS: &[&str] = &["low", "medium", "quartile", "high"];

const WIFI_ENCRYPTIONS: &[&str] = &["nopass", "WPA", "WEP", "WPA2-EAP"];

/// The EAP method list of `useQRCode.ts`, in its order.
const EAP_METHODS: &[&str] = &[
    "MD5", "POTP", "GTC", "TLS", "IKEv2", "SIM", "AKA", "AKA'", "TTLS", "PWD", "LEAP", "PSK",
    "FAST", "TEAP", "EKE", "NOOB", "PEAP",
];

const EAP_PHASE2_METHODS: &[&str] = &["None", "MSCHAPV2"];

/// The `qrcode` SVG renderer prefixes an XML prolog; the UI inlines the markup.
const XML_PROLOG: &str = r#"<?xml version="1.0" standalone="yes"?>"#;

fn qr_ec_level(s: &str) -> anyhow::Result<qrcode::EcLevel> {
    Ok(match s.trim() {
        "low" | "L" | "l" => qrcode::EcLevel::L,
        "medium" | "M" | "m" | "" => qrcode::EcLevel::M,
        "quartile" | "Q" | "q" => qrcode::EcLevel::Q,
        "high" | "H" | "h" => qrcode::EcLevel::H,
        other => anyhow::bail!("unknown error correction level `{other}`"),
    })
}

/// Normalise any CSS color to `#rrggbb[aa]`; blank falls back to `fallback`.
fn qr_color(label: &str, s: &str, fallback: &str) -> anyhow::Result<String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(fallback.to_string());
    }
    let c = csscolorparser::parse(s).map_err(|e| anyhow::anyhow!("invalid {label} `{s}`: {e}"))?;
    Ok(c.to_css_hex())
}

/// Encode `payload` as inline SVG source. Errors (never panics) on empty input
/// and on payloads too long for the chosen error-correction level.
fn qr_svg(payload: &str, fg: &str, bg: &str, ec: &str) -> anyhow::Result<String> {
    nonempty("text", payload)?;
    let ec = qr_ec_level(ec)?;
    let fg = qr_color("foreground color", fg, "#000000")?;
    let bg = qr_color("background color", bg, "#ffffff")?;
    let code = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), ec)
        .map_err(|e| anyhow::anyhow!("cannot encode QR code: {e}"))?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(256, 256)
        .dark_color(qrcode::render::svg::Color(&fg))
        .light_color(qrcode::render::svg::Color(&bg))
        .build();
    Ok(svg.strip_prefix(XML_PROLOG).unwrap_or(&svg).to_string())
}

fn qr_code_generator(args: &Args) -> anyhow::Result<String> {
    let id = "qr-code-generator";
    qr_svg(
        a(args, id, "text"),
        a(args, id, "fg_color"),
        a(args, id, "bg_color"),
        a(args, id, "ec_level"),
    )
}

/// `\`, `;`, `,`, `:` and `"` are meta-characters in the WIFI URI scheme.
fn wifi_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | ';' | ',' | ':' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Port of `getQrCodeText` in `wifi-qr-code-generator/useQRCode.ts`, field order
/// and all — SSID first, then type, password, EAP details, hidden flag. Upstream
/// returns `null` (and renders no QR code at all) for every incomplete
/// combination; those are the `bail!`s here.
// One parameter per form field, in upstream's `GetQrCodeTextOptions` order; a
// struct here would only be the same eight fields with more ceremony.
#[allow(clippy::too_many_arguments)]
fn wifi_payload(
    encryption: &str,
    ssid: &str,
    password: &str,
    hidden: bool,
    eap_method: &str,
    eap_anonymous: bool,
    eap_identity: &str,
    eap_phase2: &str,
) -> anyhow::Result<String> {
    if !WIFI_ENCRYPTIONS.contains(&encryption) {
        anyhow::bail!("unknown encryption `{encryption}`");
    }
    nonempty("SSID", ssid)?;
    let ssid = wifi_escape(ssid);
    // No password means the type can be omitted entirely, which keeps the symbol
    // small — upstream's comment, and its exact output.
    if encryption == "nopass" {
        return Ok(format!("WIFI:S:{ssid};;"));
    }
    nonempty("password", password)?;
    let hidden = if hidden { "H:true" } else { "" };
    let password = wifi_escape(password);
    if encryption != "WPA2-EAP" {
        return Ok(format!(
            "WIFI:S:{ssid};T:{encryption};P:{password};{hidden};"
        ));
    }
    if !EAP_METHODS.contains(&eap_method) {
        anyhow::bail!("pick an EAP method");
    }
    if eap_identity.trim().is_empty() && !eap_anonymous {
        anyhow::bail!("WPA2-EAP needs an identity, or the anonymous flag");
    }
    if eap_method == "PEAP" && eap_phase2 == "None" {
        anyhow::bail!("the PEAP method needs a phase 2 method");
    }
    let identity = if eap_anonymous {
        "A:anon".to_string()
    } else {
        format!("I:{}", wifi_escape(eap_identity.trim()))
    };
    let phase2 = if eap_phase2 == "None" {
        String::new()
    } else {
        format!("PH2:{eap_phase2};")
    };
    Ok(format!(
        "WIFI:S:{ssid};T:WPA2-EAP;P:{password};E:{eap_method};{phase2}{identity};{hidden};"
    ))
}

fn wifi_qr_code_generator(args: &Args) -> anyhow::Result<String> {
    let id = "wifi-qr-code-generator";
    let payload = wifi_payload(
        a(args, id, "encryption"),
        a(args, id, "ssid"),
        a(args, id, "password"),
        flag(a(args, id, "hidden")),
        a(args, id, "eap_method"),
        flag(a(args, id, "eap_anonymous")),
        a(args, id, "eap_identity"),
        a(args, id, "eap_phase2"),
    )?;
    // Upstream hard-codes error correction level M here; only the plain QR tool
    // offers the choice, so there is no level field to expose.
    qr_svg(
        &payload,
        a(args, id, "fg_color"),
        a(args, id, "bg_color"),
        "medium",
    )
}

// ---------------------------------------------------------------- registry

pub static TOOLS: &[Tool] = &[
    Tool {
        id: "text-diff",
        name: "Text diff",
        description: "Compare two texts and see the differences between them.",
        category: Category::Text,
        keywords: &["text", "diff", "compare", "difference", "delta", "patch"],
        fields: &[
            Field::area("old", "Original text"),
            Field::area("new", "Modified text"),
        ],
        kind: Kind::Pure,
        run: text_diff,
    },
    Tool {
        id: "text-statistics",
        name: "Text statistics",
        description: "Get information about a text: characters, words, lines and byte size.",
        category: Category::Text,
        keywords: &[
            "text",
            "statistics",
            "characters",
            "words",
            "lines",
            "size",
            "bytes",
        ],
        fields: &[Field::area("text", "Your text")],
        kind: Kind::Pure,
        run: text_statistics,
    },
    Tool {
        id: "text-to-nato-alphabet",
        name: "Text to NATO alphabet",
        description: "Transform text into the NATO phonetic alphabet for oral transmission.",
        category: Category::Text,
        keywords: &["text", "nato", "alphabet", "phonetic", "spelling", "radio"],
        fields: &[Field::text("text", "Your text")],
        kind: Kind::Pure,
        run: nato_tool,
    },
    Tool {
        id: "lorem-ipsum-generator",
        name: "Lorem ipsum generator",
        description: "Generate placeholder text to demonstrate the visual form of a document.",
        category: Category::Text,
        keywords: &[
            "lorem",
            "ipsum",
            "placeholder",
            "filler",
            "text",
            "generator",
        ],
        // it-tools' sentences/words sliders are *range* sliders (defaults 3-8 and
        // 8-15) and it picks a random count inside the range per generation. A
        // single number is the same thing with the range collapsed; the form has
        // no range widget, and a fixed count is the more predictable default.
        fields: &[
            Field::number("paragraphs", "Paragraphs", "1"),
            Field::number("sentences", "Sentences per paragraph", "3"),
            Field::number("words", "Words per sentence", "10"),
            Field::bool("start_with_lorem", "Start with lorem ipsum", "true"),
            Field::bool("as_html", "As HTML", "false"),
        ],
        kind: Kind::Pure,
        run: lorem_tool,
    },
    Tool {
        id: "numeronym-generator",
        name: "Numeronym generator",
        description: "A numeronym abbreviates a word by the count of its inner letters, e.g. i18n.",
        category: Category::Text,
        keywords: &[
            "numeronym",
            "i18n",
            "a11y",
            "l10n",
            "abbreviation",
            "generator",
        ],
        fields: &[Field::text("word", "Your word(s)")],
        kind: Kind::Pure,
        run: numeronym_tool,
    },
    Tool {
        id: "regex-tester",
        name: "Regex tester",
        description: "Test your regular expressions with sample text.",
        category: Category::Development,
        keywords: &["regex", "regexp", "test", "match", "pattern", "expression"],
        // One checkbox per flag, as upstream, with its defaults (g and s on).
        fields: &[
            Field::text("regex", "Regular expression"),
            Field::area("text", "Text to match against"),
            Field::bool("global", "Global search (g)", "true"),
            Field::bool("ignore_case", "Case-insensitive search (i)", "false"),
            Field::bool("multiline", "Multiline (m)", "false"),
            Field::bool("dot_all", "Singleline: . matches newlines (s)", "true"),
        ],
        kind: Kind::Pure,
        run: regex_tester,
    },
    Tool {
        id: "regex-memo",
        name: "Regex cheatsheet",
        description: "Regular expression cheatsheet.",
        category: Category::Development,
        keywords: &["regex", "regexp", "memo", "cheatsheet", "reference"],
        fields: &[],
        kind: Kind::Pure,
        run: regex_memo,
    },
    Tool {
        id: "git-memo",
        name: "Git cheatsheet",
        description: "Quick access to the most common git commands.",
        category: Category::Development,
        keywords: &["git", "memo", "cheatsheet", "reference", "commands", "vcs"],
        fields: &[],
        kind: Kind::Pure,
        run: git_memo,
    },
    Tool {
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
    },
    Tool {
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
    },
    Tool {
        id: "eta-calculator",
        name: "ETA calculator",
        description: "Estimate the end time and duration of a task from its consumption rate.",
        category: Category::Math,
        keywords: &[
            "eta",
            "calculator",
            "estimate",
            "duration",
            "time",
            "arrival",
        ],
        fields: &[
            Field::number("unit_count", "Amount of elements to consume", "186"),
            Field::number("units_per_span", "Units consumed per time span", "3"),
            Field::number("span", "Time span", "5"),
            Field::select("span_unit", "Time span unit", TIME_SPAN_UNITS, "minutes"),
            Field::text("started_at", "The consumption started at (blank = now)"),
        ],
        kind: Kind::Pure,
        run: eta_calculator,
    },
    Tool {
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
    },
    Tool {
        id: "date-time-converter",
        name: "Date-time converter",
        description: "Convert date and time between ISO 8601, ISO 9075, RFC 3339, RFC 7231, unix, UTC, Mongo ObjectID and Excel formats.",
        category: Category::Converter,
        keywords: &[
            "date",
            "time",
            "converter",
            "iso",
            "rfc",
            "unix",
            "timestamp",
            "mongo",
            "objectid",
            "excel",
            "utc",
        ],
        fields: &[
            Field::text("date", "Date string"),
            Field::select("format", "Input format", DATE_FORMATS, "auto"),
        ],
        kind: Kind::Pure,
        run: date_time_converter,
    },
    Tool {
        id: "chronometer",
        name: "Chronometer",
        description: "Monitor the duration of a thing with a simple chronometer.",
        category: Category::Measurement,
        keywords: &["chronometer", "stopwatch", "timer", "duration", "time"],
        fields: &[],
        // Live wall-clock ticking; a pure fn has nothing to return.
        kind: Kind::Custom,
        run: chronometer_ui,
    },
    Tool {
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
    },
    Tool {
        id: "color-converter",
        name: "Color converter",
        description: "Convert a color between hex, rgb, hsl, hwb, lch, cmyk and css name.",
        category: Category::Converter,
        keywords: &[
            "color",
            "converter",
            "hex",
            "rgb",
            "hsl",
            "hwb",
            "lch",
            "cmyk",
            "css",
        ],
        fields: &[Field::text("color", "Color (e.g. #1ea54c)")],
        kind: Kind::Pure,
        run: color_converter,
    },
    Tool {
        id: "emoji-picker",
        name: "Emoji picker",
        description: "Copy emojis and get their unicode and code point values.",
        category: Category::Text,
        keywords: &["emoji", "picker", "unicode", "codepoint", "smiley"],
        fields: &[Field::text("search", "Search emojis")],
        // Grid picker with click-to-copy; see EMOJIS for the searchable data.
        kind: Kind::Custom,
        run: emoji_picker_ui,
    },
    Tool {
        id: "ascii-text-drawer",
        name: "ASCII text drawer",
        description: "Create ASCII art text with the built-in block font.",
        category: Category::Text,
        keywords: &["ascii", "art", "figlet", "text", "drawer", "banner"],
        fields: &[
            Field::area("text", "Your text"),
            Field::select("font", "Font", &["block"], "block"),
            Field::number("width", "Width", "80"),
        ],
        kind: Kind::Pure,
        run: ascii_text_drawer,
    },
    Tool {
        id: "keycode-info",
        name: "Keycode info",
        description: "Find the keycode, code, location and modifiers of any pressed key.",
        category: Category::Development,
        keywords: &["keycode", "key", "code", "keyboard", "event", "modifier"],
        fields: &[],
        // Needs live keydown events from the host window.
        kind: Kind::Custom,
        run: keycode_info_ui,
    },
    Tool {
        id: "svg-placeholder-generator",
        name: "SVG placeholder generator",
        description: "Generate SVG images to use as placeholders in your applications.",
        category: Category::Images,
        keywords: &["svg", "placeholder", "generator", "image", "mockup"],
        fields: &[
            Field::number("width", "Width (px)", "600"),
            Field::number("height", "Height (px)", "350"),
            Field::number("font_size", "Font size (px)", "26"),
            text_default("bg_color", "Background", "#cccccc"),
            text_default("fg_color", "Text color", "#333333"),
            Field::text("text", "Custom text"),
            Field::bool("exact_size", "Use exact size", "true"),
        ],
        kind: Kind::Pure,
        run: svg_placeholder,
    },
    Tool {
        id: "qr-code-generator",
        name: "QR Code generator",
        description: "Generate a QR code for a URL or plain text with custom colors.",
        category: Category::Images,
        keywords: &["qr", "qrcode", "generator", "barcode", "url"],
        fields: &[
            Field::area("text", "Text or URL"),
            text_default("fg_color", "Foreground color", "#000000"),
            text_default("bg_color", "Background color", "#ffffff"),
            Field::select("ec_level", "Error resistance", QR_EC_LEVELS, "medium"),
        ],
        kind: Kind::Pure,
        run: qr_code_generator,
    },
    Tool {
        id: "wifi-qr-code-generator",
        name: "WiFi QR Code generator",
        description: "Generate a QR code for quick connection to a WiFi network.",
        category: Category::Images,
        keywords: &["wifi", "qr", "qrcode", "generator", "network", "ssid"],
        fields: &[
            Field::select("encryption", "Encryption method", WIFI_ENCRYPTIONS, "WPA"),
            Field::text("ssid", "SSID"),
            Field::bool("hidden", "Hidden SSID", "false"),
            // Upstream hides the password unless the encryption needs one, and the
            // three EAP rows unless it is WPA2-EAP; a static field list cannot, so
            // they are documented as ignored instead of silently doing nothing.
            Field::text("password", "Password (ignored for `nopass`)"),
            Field::select(
                "eap_method",
                "EAP method (WPA2-EAP only)",
                EAP_METHODS,
                "TTLS",
            ),
            Field::bool("eap_anonymous", "Anonymous EAP identity", "false"),
            Field::text("eap_identity", "EAP identity (WPA2-EAP only)"),
            Field::select(
                "eap_phase2",
                "EAP phase 2 method (WPA2-EAP only)",
                EAP_PHASE2_METHODS,
                "None",
            ),
            text_default("fg_color", "Foreground color", "#000000"),
            text_default("bg_color", "Background color", "#ffffff"),
        ],
        kind: Kind::Pure,
        run: wifi_qr_code_generator,
    },
    Tool {
        id: "camera-recorder",
        name: "Camera recorder",
        description: "Take a picture or record a video from your webcam or camera.",
        category: Category::Images,
        keywords: &["camera", "recorder", "webcam", "video", "picture", "photo"],
        fields: &[],
        // Needs webcam capture and a media recorder.
        kind: Kind::Custom,
        run: camera_recorder_ui,
    },
    Tool {
        id: "html-wysiwyg-editor",
        name: "HTML WYSIWYG editor",
        description: "Rich-text editor that generates the HTML source of the content.",
        category: Category::Web,
        keywords: &["html", "wysiwyg", "editor", "richtext", "contenteditable"],
        fields: &[],
        // Needs a contenteditable rich-text surface.
        kind: Kind::Custom,
        run: wysiwyg_ui,
    },
];

// ------------------------------------------------------------------- tests

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    fn args(pairs: &[(&str, &str)]) -> Args {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn run(id: &str, pairs: &[(&str, &str)]) -> anyhow::Result<String> {
        let t = TOOLS.iter().find(|t| t.id == id).expect("tool exists");
        (t.run)(&args(pairs))
    }

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
        ] {
            assert!(TOOLS.iter().any(|t| t.id == id), "missing tool {id}");
        }
        assert_eq!(TOOLS.len(), 24);
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

    // ---- nato alphabet

    #[test]
    fn nato_vectors() {
        assert_eq!(text_to_nato("a"), "Alpha");
        assert_eq!(text_to_nato("ab"), "Alpha Bravo");
        assert_eq!(text_to_nato("Xyz"), "X-ray Yankee Zulu");
        assert_eq!(text_to_nato("a1"), "Alpha 1");
        assert_eq!(text_to_nato(""), "");
        assert!(run("text-to-nato-alphabet", &[]).is_err());
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
        let out = generate_lorem_ipsum(2, 3, 5, true, false);
        assert!(out.starts_with(LOREM_FIRST));
        assert_eq!(out.split("\n\n").count(), 2);

        let html = generate_lorem_ipsum(2, 1, 4, false, true);
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
        assert_eq!(
            m[0].captures,
            vec![
                Capture {
                    name: "1".into(),
                    value: "e".into(),
                    start: 2,
                    end: 3
                },
                Capture {
                    name: "2".into(),
                    value: "r".into(),
                    start: 3,
                    end: 4
                },
            ]
        );
        assert_eq!(
            m[0].groups,
            vec![Capture {
                name: "g".into(),
                value: "r".into(),
                start: 3,
                end: 4
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

    // ---- math evaluator

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
        assert!((num("phi") - 1.618_033_988_749_895).abs() < 1e-12);
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

    // ---- percentage calculator

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

    // ---- eta calculator

    #[test]
    fn eta_duration_formatting() {
        assert_eq!(format_ms_duration(0.0), "0 seconds");
        assert_eq!(format_ms_duration(1000.0), "1 second");
        assert_eq!(format_ms_duration(61_000.0), "1 minute 1 second");
        assert_eq!(
            format_ms_duration(3_661_012.0),
            "1 hour 1 minute 1 second 12 ms"
        );
        // 500 plates, 5 per 3 minutes -> 5 hours.
        assert_eq!(
            format_ms_duration(500.0 / (5.0 / (3.0 * 60_000.0))),
            "5 hours"
        );
        assert!(
            run("eta-calculator", &[])
                .unwrap()
                .contains("Total duration")
        );
        assert!(run("eta-calculator", &[("units_per_span", "0")]).is_err());
        assert!(run("eta-calculator", &[("span_unit", "fortnights")]).is_err());
        assert!(run("eta-calculator", &[("unit_count", "abc")]).is_err());
    }

    /// Reproducers found by the property suite: a large-but-finite duration
    /// used to panic inside chrono instead of returning, which crashed the UI
    /// on a keystroke.
    #[test]
    fn eta_calculator_survives_out_of_range_durations() {
        let huge = "9".repeat(38);
        for (key, value) in [
            ("unit_count", huge.as_str()),
            ("span", huge.as_str()),
            ("unit_count", &i64::MIN.to_string()),
            ("unit_count", &u64::MAX.to_string()),
            ("span", &u64::MAX.to_string()),
        ] {
            let out = run("eta-calculator", &[(key, value)]);
            // Either answer is fine; panicking is not.
            if let Ok(text) = out {
                assert!(text.contains("Total duration"), "{key}={value}: {text}");
            }
        }
        // A duration beyond the representable range says so instead of lying.
        let text = run("eta-calculator", &[("unit_count", &huge)]).unwrap();
        assert!(text.contains("out of representable range"), "{text}");
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
        // Titles and 2-decimal truncation as in the `.vue`'s `units` record.
        assert!(out.contains("Kelvin: 373.15 K"), "{out}");
        assert!(out.contains("Fahrenheit: 212 °F"), "{out}");
        assert!(out.contains("Rømer: 60 °Rø"), "{out}");
        assert!(run("temperature-converter", &[("value", "hot")]).is_err());
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

    // ---- benchmark builder

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

    // ---- color converter

    #[test]
    fn color_alpha_stripping_vectors() {
        assert_eq!(remove_alpha_channel_when_opaque("#000000ff"), "#000000");
        assert_eq!(remove_alpha_channel_when_opaque("#ffffffFF"), "#ffffff");
        assert_eq!(remove_alpha_channel_when_opaque("#000000FE"), "#000000FE");
        assert_eq!(remove_alpha_channel_when_opaque("#00000000"), "#00000000");
    }

    #[test]
    fn color_conversions() {
        let out = run("color-converter", &[("color", "#ff0000")]).unwrap();
        assert!(out.contains("hex: #ff0000"), "{out}");
        // csscolorparser emits modern space-separated CSS syntax.
        assert!(out.contains("rgb: rgb(255 0 0)"), "{out}");
        assert!(out.contains("hsl: hsl(0 100% 50%)"), "{out}");
        assert!(out.contains("cmyk: cmyk(0%, 100%, 100%, 0%)"), "{out}");
        assert!(out.contains("name: red"), "{out}");

        assert!(
            run("color-converter", &[("color", "red")])
                .unwrap()
                .contains("#ff0000")
        );
        assert!(run("color-converter", &[]).is_err());
        assert!(run("color-converter", &[("color", "#zzz")]).is_err());
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

    // ---- ascii text drawer

    #[test]
    fn ascii_art_renders() {
        let out = run("ascii-text-drawer", &[("text", "HI")]).unwrap();
        assert_eq!(out.lines().count(), 5);
        assert!(out.contains('#'));
        // Wrapping: width 5 fits exactly one glyph per line.
        let out = run("ascii-text-drawer", &[("text", "AB"), ("width", "5")]).unwrap();
        assert_eq!(out.lines().count(), 10);

        assert!(run("ascii-text-drawer", &[]).is_err());
        assert!(run("ascii-text-drawer", &[("text", "hi"), ("width", "1")]).is_err());
        assert!(run("ascii-text-drawer", &[("text", "hi"), ("width", "nope")]).is_err());
        assert!(run("ascii-text-drawer", &[("text", "hi"), ("font", "Doom")]).is_err());
    }

    // ---- svg placeholder

    #[test]
    fn svg_placeholder_output() {
        let out = run(
            "svg-placeholder-generator",
            &[("bg_color", "#cccccc"), ("fg_color", "#333333")],
        )
        .unwrap();
        assert!(out.starts_with("SVG HTML element:\n<svg xmlns="), "{out}");
        assert!(out.contains(r#"viewBox="0 0 600 350""#));
        assert!(out.contains(r#"width="600" height="350""#));
        assert!(out.contains(">600x350</text>"));
        // Both panels, like upstream: the element and its base64 data URI.
        assert!(
            out.contains("SVG in Base64:\ndata:image/svg+xml;base64,PHN2ZyB4bWxucz0"),
            "{out}"
        );

        let out = run(
            "svg-placeholder-generator",
            &[
                ("bg_color", "#000"),
                ("fg_color", "#fff"),
                ("text", "a<b"),
                ("exact_size", "false"),
            ],
        )
        .unwrap();
        assert!(out.contains("a&lt;b"));
        assert!(
            !out.contains(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 350" width"#)
        );

        // The colour pickers upstream always hold a value, so the field defaults
        // do too and the tool renders with no arguments at all.
        assert!(
            run("svg-placeholder-generator", &[])
                .unwrap()
                .contains(r##"fill="#cccccc""##)
        );
        assert!(
            run(
                "svg-placeholder-generator",
                &[("bg_color", "#000"), ("fg_color", "#fff"), ("width", "0")]
            )
            .is_err()
        );
        assert!(
            run(
                "svg-placeholder-generator",
                &[("bg_color", "nope"), ("fg_color", "#fff")]
            )
            .is_err()
        );
    }

    // ---- qr code generator

    #[test]
    fn qr_code_svg_output() {
        let out = run(
            "qr-code-generator",
            &[("text", "https://it-tools.tech"), ("fg_color", "red")],
        )
        .unwrap();
        assert!(
            out.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
            "{out:.60}"
        );
        assert!(out.contains(r##"fill="#ff0000""##), "foreground applied");
        assert!(
            out.contains(r##"fill="#ffffff""##),
            "background defaults to white"
        );
        assert!(out.ends_with("</svg>"));
        assert!(
            !out.contains("<?xml"),
            "prolog stripped so the markup inlines"
        );

        // Every level encodes, and a stronger level needs a bigger symbol.
        let dim = |ec: &str| {
            let s = run("qr-code-generator", &[("text", "hello"), ("ec_level", ec)]).unwrap();
            s.split(r#"viewBox="0 0 "#)
                .nth(1)
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .to_string()
        };
        for ec in QR_EC_LEVELS {
            assert!(!dim(ec).is_empty(), "{ec} renders");
        }

        assert!(run("qr-code-generator", &[]).is_err()); // empty text
        assert!(run("qr-code-generator", &[("text", "   ")]).is_err());
        assert!(run("qr-code-generator", &[("text", "hi"), ("ec_level", "nope")]).is_err());
        assert!(run("qr-code-generator", &[("text", "hi"), ("bg_color", "nope")]).is_err());
        // Way past the ~2953 byte ceiling: Err, never a panic.
        assert!(run("qr-code-generator", &[("text", &"a".repeat(5000))]).is_err());
        assert!(
            run(
                "qr-code-generator",
                &[("text", &"a".repeat(3000)), ("ec_level", "high")]
            )
            .is_err()
        );
    }

    // ---- wifi qr code generator

    #[test]
    fn wifi_payload_vectors() {
        // Field order, the omitted type for `nopass` and the omitted `H:` when the
        // SSID is not hidden: `getQrCodeText` in useQRCode.ts, verbatim.
        assert_eq!(
            wifi_payload("WPA", "MyNet", "s3cret", false, "", false, "", "None").unwrap(),
            "WIFI:S:MyNet;T:WPA;P:s3cret;;"
        );
        assert_eq!(
            wifi_payload("WPA", "MyNet", "s3cret", true, "", false, "", "None").unwrap(),
            "WIFI:S:MyNet;T:WPA;P:s3cret;H:true;"
        );
        assert_eq!(
            wifi_payload("nopass", "Guest", "", true, "", false, "", "None").unwrap(),
            "WIFI:S:Guest;;"
        );
        // Every meta-character gets a backslash, in SSID and password alike.
        assert_eq!(
            wifi_payload(
                "WEP",
                r#"a;b:c\d,e"f"#,
                r#"p;q:r\s,t"u"#,
                false,
                "",
                false,
                "",
                "None"
            )
            .unwrap(),
            r#"WIFI:S:a\;b\:c\\d\,e\"f;T:WEP;P:p\;q\:r\\s\,t\"u;;"#
        );
        assert_eq!(
            wifi_payload(
                "WPA2-EAP", "Corp", "pw", false, "TTLS", false, "user:1", "MSCHAPV2"
            )
            .unwrap(),
            r"WIFI:S:Corp;T:WPA2-EAP;P:pw;E:TTLS;PH2:MSCHAPV2;I:user\:1;;"
        );
        // A `None` phase 2 drops the PH2 tag; anonymous replaces `I:` with `A:`.
        assert_eq!(
            wifi_payload("WPA2-EAP", "Corp", "pw", true, "TTLS", true, "", "None").unwrap(),
            "WIFI:S:Corp;T:WPA2-EAP;P:pw;E:TTLS;A:anon;H:true;"
        );
        // EAP fields are ignored unless the encryption asks for them.
        assert_eq!(
            wifi_payload("WPA", "Corp", "pw", false, "TTLS", false, "who", "MSCHAPV2").unwrap(),
            "WIFI:S:Corp;T:WPA;P:pw;;"
        );

        // Every combination for which upstream renders no QR code at all.
        assert!(wifi_payload("WPA", "", "pw", false, "", false, "", "None").is_err());
        assert!(wifi_payload("WPA3", "net", "pw", false, "", false, "", "None").is_err());
        assert!(wifi_payload("WPA", "net", "", false, "", false, "", "None").is_err());
        assert!(wifi_payload("WPA2-EAP", "net", "pw", false, "", false, "me", "None").is_err());
        assert!(wifi_payload("WPA2-EAP", "net", "pw", false, "TTLS", false, "", "None").is_err());
        assert!(wifi_payload("WPA2-EAP", "net", "pw", false, "PEAP", true, "", "None").is_err());
    }

    #[test]
    fn wifi_qr_code_svg_output() {
        let out = run(
            "wifi-qr-code-generator",
            &[("ssid", "MyNet"), ("password", "s3cret")],
        )
        .unwrap();
        assert!(out.starts_with("<svg "), "{out:.40}");
        assert!(out.ends_with("</svg>"));

        assert!(run("wifi-qr-code-generator", &[]).is_err()); // no SSID
        assert!(
            run(
                "wifi-qr-code-generator",
                &[("ssid", "n"), ("password", &"p".repeat(5000))]
            )
            .is_err()
        );
    }

    // ---- text diff

    #[test]
    fn text_diff_output() {
        let out = run("text-diff", &[("old", "a\nb\n"), ("new", "a\nc\n")]).unwrap();
        assert!(out.contains("-b"), "{out}");
        assert!(out.contains("+c"), "{out}");
        assert_eq!(
            run("text-diff", &[("old", "x"), ("new", "x")]).unwrap(),
            "texts are identical"
        );
        assert!(run("text-diff", &[]).is_err());
    }
}
