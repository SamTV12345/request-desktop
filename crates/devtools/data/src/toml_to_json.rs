//! `toml-to-json`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn toml_to_json(args: &Args) -> Result<String> {
    Ok(serde_json::to_string_pretty(&toml_value(
        args, TOOLS[9], "input",
    )?)?)
}

pub static TOOL: Tool = Tool {
    id: "toml-to-json",
    name: "TOML to JSON",
    description: "Convert TOML to JSON.",
    category: Category::Converter,
    keywords: &["toml", "json", "convert"],
    fields: &[Field::area("input", "TOML")],
    kind: Kind::Pure,
    run: toml_to_json,
};
