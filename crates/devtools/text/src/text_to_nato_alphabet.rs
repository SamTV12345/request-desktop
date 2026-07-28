//! `text-to-nato-alphabet`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

pub(crate) const NATO: [&str; 26] = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India", "Juliet",
    "Kilo", "Lima", "Mike", "November", "Oscar", "Papa", "Quebec", "Romeo", "Sierra", "Tango",
    "Uniform", "Victor", "Whiskey", "X-ray", "Yankee", "Zulu",
];

pub(crate) fn text_to_nato(text: &str) -> String {
    text.chars()
        .map(|c| {
            let lower = c.to_ascii_lowercase();
            if lower.is_ascii_lowercase() {
                NATO[(lower as u8 - b'a') as usize].to_string()
            } else {
                c.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn nato_tool(args: &Args) -> anyhow::Result<String> {
    let text = nonempty("text", a(args, "text-to-nato-alphabet", "text"))?;
    Ok(text_to_nato(text))
}

pub static TOOL: Tool = Tool {
    id: "text-to-nato-alphabet",
    name: "Text to NATO alphabet",
    description: "Transform text into the NATO phonetic alphabet for oral transmission.",
    category: Category::Text,
    keywords: &["text", "nato", "alphabet", "phonetic", "spelling", "radio"],
    fields: &[Field::text("text", "Your text")],
    kind: Kind::Pure,
    run: nato_tool,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn nato_vectors() {
        assert_eq!(text_to_nato("a"), "Alpha");
        assert_eq!(text_to_nato("ab"), "Alpha Bravo");
        assert_eq!(text_to_nato("Xyz"), "X-ray Yankee Zulu");
        assert_eq!(text_to_nato("a1"), "Alpha 1");
        assert_eq!(text_to_nato(""), "");
        assert!(run("text-to-nato-alphabet", &[]).is_err());
    }
}
