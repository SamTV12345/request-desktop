//! `yaml-to-toml`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn yaml_to_toml(args: &Args) -> Result<String> {
    to_toml(&yaml(args, TOOLS[12], "input")?)
}

pub static TOOL: Tool = Tool {
    id: "yaml-to-toml",
    name: "YAML to TOML",
    description: "Convert YAML to TOML.",
    category: Category::Converter,
    keywords: &["yaml", "yml", "toml", "convert"],
    fields: &[Field::area("input", "YAML")],
    kind: Kind::Pure,
    run: yaml_to_toml,
};
