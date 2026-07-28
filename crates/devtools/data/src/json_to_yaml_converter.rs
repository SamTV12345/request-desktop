//! `json-to-yaml-converter`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn json_to_yaml(args: &Args) -> Result<String> {
    to_yaml(&json(args, TOOLS[5], "input")?)
}

pub static TOOL: Tool = Tool {
    id: "json-to-yaml-converter",
    name: "JSON to YAML",
    description: "Convert JSON to YAML.",
    category: Category::Converter,
    keywords: &["json", "yaml", "yml", "convert"],
    fields: &[Field::area("input", "JSON")],
    kind: Kind::Pure,
    run: json_to_yaml,
};
