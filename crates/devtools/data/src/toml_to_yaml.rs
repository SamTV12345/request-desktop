//! `toml-to-yaml`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn toml_to_yaml(args: &Args) -> Result<String> {
    to_yaml(&toml_value(args, TOOLS[10], "input")?)
}

pub static TOOL: Tool = Tool {
    id: "toml-to-yaml",
    name: "TOML to YAML",
    description: "Convert TOML to YAML.",
    category: Category::Converter,
    keywords: &["toml", "yaml", "yml", "convert"],
    fields: &[Field::area("input", "TOML")],
    kind: Kind::Pure,
    run: toml_to_yaml,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn toml_yaml_round_trip() {
        let y = ok("toml-to-yaml", &[("input", "a = 1\nb = \"x\"\n")]);
        assert!(y.contains("a: 1"), "{y}");
        let t = ok("yaml-to-toml", &[("input", &y)]);
        assert_eq!(
            ok("toml-to-json", &[("input", &t)]),
            "{\n  \"a\": 1,\n  \"b\": \"x\"\n}"
        );
    }
}
