//! `conventional-commits-cheatsheet`

use crate::{Args, Category, Kind, Tool};
use anyhow::Result;

// it-tools PR #1626.

pub(crate) const CONVENTIONAL_COMMITS: &str = r#"## The shape

```text
<type>[optional scope][!]: <description>

[optional body]

[optional footer(s)]
```

A description is imperative and lowercase, under ~72 characters, with no
trailing full stop.

```shell
git commit -m "feat(parser): accept arrays as the top-level value"
```

## Types

Type | When to use | Bumps
:--|:--|:--
`feat` | a new feature | MINOR
`fix` | a bug fix | PATCH
`docs` | documentation only | none
`style` | formatting, whitespace, semicolons | none
`refactor` | neither fixes a bug nor adds a feature | none
`perf` | a change that improves performance | PATCH
`test` | adding or correcting tests | none
`build` | build system or dependencies | none
`ci` | CI configuration and scripts | none
`chore` | anything else that touches no source | none
`revert` | reverts a previous commit | none

## Breaking changes

Either a `!` before the colon, or a `BREAKING CHANGE:` footer. Both bump MAJOR.

```text
feat(api)!: send the whole envelope rather than the body alone

BREAKING CHANGE: clients now have to unwrap `data` themselves.
```

## Footers

Footers use a `Token: value` form, with `-` instead of spaces in the token.

```text
Reviewed-by: Ada Lovelace
Refs: #133
Co-authored-by: Grace Hopper <grace@example.com>
```

## Rules worth remembering

Rule | Detail
:--|:--
scope | optional, a noun for the area touched, in parentheses
case | the type and scope are lowercase; `BREAKING CHANGE` is uppercase
revert | describe the reverted commit in the body, refs in the footer
multiple | one commit does one thing; split unrelated changes
"#;

pub(crate) fn conventional_commits(_: &Args) -> Result<String> {
    Ok(CONVENTIONAL_COMMITS.into())
}

pub static TOOL: Tool = Tool {
    id: "conventional-commits-cheatsheet",
    name: "Conventional commits cheat sheet",
    description: "The commit types, the breaking-change forms and the footers, on one page.",
    category: Category::Development,
    keywords: &[
        "commit",
        "conventional",
        "git",
        "semver",
        "changelog",
        "memo",
    ],
    fields: &[],
    kind: Kind::Pure,
    run: conventional_commits,
};
