//! `regex-memo`

use crate::{Args, Category, Kind, Tool};

pub(crate) const REGEX_MEMO: &str = r#"### Normal characters

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
`\p{L}` | any Unicode letter, in any script
`\p{Lu}` or `\p{Ll}` | any uppercase or lowercase Unicode letter
`\p{N}` | any Unicode number, including non-Latin digits
`\p{Greek}` | any character of the named Unicode script
`\P{L}` | inverse of `\p{L}`; capital `P` negates any Unicode class

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
`*?` | 0 or more, non-greedy: stops at the first match that works
`+?` | 1 or more, non-greedy
`??` | 0 or 1, preferring 0
`{2,7}?` | at least 2 but no more than 7, non-greedy

- The quantifier goes *after* the expression to be quantified.
- A quantifier is greedy by default: `<.+>` on `<a><b>` matches the whole string, `<.+?>` matches `<a>`.

### Boundaries

Expression | Description
:--|:--
`^` | start of string
`$` | end of string
`\b` | word boundary
`\B` | not a word boundary
`\A` | start of the whole text, whatever the multiline flag says
`\z` | end of the whole text, whatever the multiline flag says

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
`(?<name>foo)` | named capturing group; the group is also called `name`
`(?P<name>foo)` | the same, in Python and Rust notation
`\k<name>` | backreference to the group named `name`
`(a)?(b)` | an optional group that does not take part still holds its number; group 1 is *absent*, not empty

- `\N` is a backreference to the `Nth` capturing group. Capturing groups are numbered starting from 1.
- A named group keeps its number as well, so `(?<a>x)(y)` has groups `1`/`a` and `2`.
- Note: the Rust `regex` engine used by the regex tester supports neither lookaround nor backreferences.

### Flags

Expression | Description
:--|:--
`g` | global; keep looking after the first match instead of stopping
`i` | case-insensitive
`m` | multiline; `^` and `$` match at every line break
`s` | dot-all; `.` also matches a newline
`x` | extended; whitespace and `#` comments in the pattern are ignored
`U` | swap the meaning of greedy and non-greedy quantifiers
`(?i)foo` | set a flag inside the pattern instead of alongside it
`(?i:foo)bar` | set a flag for one group only

- The regex tester offers `g`, `i`, `m` and `s` as checkboxes; the rest go inline as `(?flags)`.

## References and tools

- [MDN](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Regular_Expressions)
- [RegExplained](https://leaverou.github.io/regexplained/)
"#;

pub(crate) fn regex_memo(_: &Args) -> anyhow::Result<String> {
    Ok(REGEX_MEMO.into())
}

pub static TOOL: Tool = Tool {
    id: "regex-memo",
    name: "Regex cheatsheet",
    description: "Regular expression cheatsheet.",
    category: Category::Development,
    keywords: &["regex", "regexp", "memo", "cheatsheet", "reference"],
    fields: &[],
    kind: Kind::Pure,
    run: regex_memo,
};
