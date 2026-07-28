//! `numeronym-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

pub(crate) fn generate_numeronym(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() <= 3 {
        return word.to_string();
    }
    format!("{}{}{}", chars[0], chars.len() - 2, chars[chars.len() - 1])
}

pub(crate) fn numeronym_tool(args: &Args) -> anyhow::Result<String> {
    let text = nonempty("word", a(args, "numeronym-generator", "word"))?;
    // ponytail: it-tools treats the whole input as one word; per-word is a
    // strict superset and single-word input behaves identically.
    Ok(text
        .split_whitespace()
        .map(generate_numeronym)
        .collect::<Vec<_>>()
        .join(" "))
}

pub static TOOL: Tool = Tool {
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
};
