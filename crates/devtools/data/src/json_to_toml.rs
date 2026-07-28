//! `json-to-toml`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};
use anyhow::Result;

pub(crate) fn json_to_toml(args: &Args) -> Result<String> {
    to_toml(&json(args, TOOLS[3], "input")?)
}

pub static TOOL: Tool = Tool {
    id: "json-to-toml",
    name: "JSON to TOML",
    description: "Convert JSON to TOML.",
    category: Category::Converter,
    keywords: &["json", "toml", "convert"],
    fields: &[Field::area("input", "JSON")],
    kind: Kind::Pure,
    run: json_to_toml,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;
    #[test]
    fn json_toml_round_trip() {
        let t = ok("json-to-toml", &[("input", J)]);
        assert!(t.contains("n = 1"), "{t}");
        let back = ok("toml-to-json", &[("input", &t)]);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&back).unwrap(),
            serde_json::from_str::<serde_json::Value>(J).unwrap()
        );
    }
}
