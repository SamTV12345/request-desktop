//! `slugify-string`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};

/// `slug::slugify` with the lowercasing left out (it-tools PR #1232).
///
/// The crate lowercases unconditionally and there is no way back from its
/// output, so the case-preserving path is done here: ASCII passes through
/// untouched, anything else goes through `slug` one character at a time and is
/// re-uppercased if the source character was. Separators are still `-`, so the
/// replacement below stays safe.
fn slugify_keeping_case(input: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('-');
            }
            gap = false;
            out.push(c);
            continue;
        }
        let mapped = slug::slugify(c.to_string());
        if mapped.is_empty() {
            gap = true;
            continue;
        }
        if gap && !out.is_empty() {
            out.push('-');
        }
        gap = false;
        if c.is_uppercase() {
            out.push_str(&mapped.to_uppercase());
        } else {
            out.push_str(&mapped);
        }
    }
    out
}

pub(crate) fn slugify_string(args: &Args) -> Result<String> {
    let t = tool("slugify-string");
    let input = arg(args, t, "input");
    if input.trim().is_empty() {
        bail!("nothing to slugify");
    }
    let separator = arg(args, t, "separator");
    // A pasted megabyte in this field would multiply into every gap in the slug.
    if separator.chars().count() > 16 {
        bail!("separator is too long: at most 16 characters");
    }
    let s = if b(args, "lowercase", true) {
        slug::slugify(input)
    } else {
        slugify_keeping_case(input)
    };
    if s.is_empty() {
        bail!("nothing sluggable in this input");
    }
    // Both paths emit `[A-Za-z0-9-]` only, so every `-` in `s` is a separator
    // this produced — there is none left over from the text to damage.
    Ok(if separator == "-" {
        s
    } else {
        s.replace('-', separator)
    })
}

pub static TOOL: Tool = Tool {
    id: "slugify-string",
    name: "Slugify string",
    description: "Make a string url, filename and id safe.",
    category: Category::Text,
    keywords: &[
        "slugify",
        "string",
        "escape",
        "emoji",
        "special",
        "character",
        "space",
        "trim",
    ],
    fields: &[
        Field::area("input", "Your string to slugify"),
        Field::text("separator", "Separator").with_default("-"),
        Field::bool("lowercase", "Lowercase", "true"),
    ],
    kind: Kind::Pure,
    run: slugify_string,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn slugify_vectors() {
        assert_eq!(
            run("slugify-string", &[("input", "My file path")]).unwrap(),
            "my-file-path"
        );
        assert_eq!(
            run("slugify-string", &[("input", "Héllo Wörld! ")]).unwrap(),
            "hello-world"
        );
        assert!(run("slugify-string", &[("input", "   ")]).is_err());
        // slug maps unknown scalars away; the tool refuses an empty slug.
        assert!(run("slugify-string", &[("input", "---")]).is_err());
    }

    #[test]
    fn separator_and_case() {
        let go = |input: &str, sep: &str, lower: &str| {
            ok(
                "slugify-string",
                &[("input", input), ("separator", sep), ("lowercase", lower)],
            )
        };
        assert_eq!(go("My file path", "_", "true"), "my_file_path");
        assert_eq!(go("My file path", "", "true"), "myfilepath");
        assert_eq!(go("My file path", "::", "true"), "my::file::path");
        // Case kept, transliteration still folds — Ä has no uppercase slug form
        // other than the one we put back.
        assert_eq!(go("Héllo Wörld! ", "-", "false"), "Hello-World");
        assert_eq!(go("My File Path", "_", "false"), "My_File_Path");
        // A dash typed by the user is a word gap, not a separator to preserve.
        assert_eq!(go("a - b", "_", "true"), "a_b");
        assert!(
            run(
                "slugify-string",
                &[("input", "a b"), ("separator", &"x".repeat(17))]
            )
            .is_err()
        );
    }
}
