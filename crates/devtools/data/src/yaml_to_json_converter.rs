//! `yaml-to-json-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn yaml_to_json(args: &Args) -> Result<String> {
    Ok(serde_json::to_string_pretty(&yaml(
        args, TOOLS[11], "input",
    )?)?)
}

pub static TOOL: Tool = Tool {
    id: "yaml-to-json-converter",
    name: "YAML to JSON",
    description: "Convert YAML to JSON.",
    category: Category::Converter,
    keywords: &["yaml", "yml", "json", "convert"],
    fields: &[Field::area("input", "YAML")],
    kind: Kind::Pure,
    run: yaml_to_json,
};
