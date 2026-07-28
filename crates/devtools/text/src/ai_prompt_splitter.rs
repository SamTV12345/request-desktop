//! `ai-prompt-splitter` — it-tools PR #1266.
//!
//! Cut a document into parts small enough to paste, each wrapped in the
//! instruction that stops the model answering before the last one arrives.

use crate::*;
use anyhow::{Result, bail};
use std::fmt::Write as _;

/// Split on the largest boundary that fits: paragraph, then line, then word,
/// and only mid-word when a single word is longer than the whole chunk.
pub(crate) fn chunks(text: &str, size: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        if rest.chars().count() <= size {
            out.push(rest.to_string());
            break;
        }
        // `size` is in characters; find the byte index that many chars in.
        let limit = rest.char_indices().nth(size).map_or(rest.len(), |(i, _)| i);
        let head = &rest[..limit];
        let cut = head
            .rfind("\n\n")
            .map(|i| i + 2)
            .or_else(|| head.rfind('\n').map(|i| i + 1))
            .or_else(|| head.rfind(' ').map(|i| i + 1))
            .unwrap_or(limit);
        out.push(rest[..cut].to_string());
        rest = &rest[cut..];
    }
    out
}

fn split(a: &Args) -> Result<String> {
    let input = s(a, "input", "");
    if input.trim().is_empty() {
        bail!("paste the text that is too long for one message");
    }
    let size = n(a, "size", 8000);
    if !(200..=100_000).contains(&size) {
        bail!("the part size must be between 200 and 100000 characters");
    }
    let parts = chunks(input, size as usize);
    let total = parts.len();
    if total == 1 {
        return Ok(format!(
            "It fits in one message: {} characters, under the {size} you asked for.\n\n{input}",
            input.chars().count()
        ));
    }
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        let no = i + 1;
        let _ = writeln!(out, "[START PART {no}/{total}]");
        out.push_str(part.trim_end());
        let _ = writeln!(out, "\n[END PART {no}/{total}]");
        if no < total {
            let _ = writeln!(
                out,
                "Remember this is part {no} of {total}. Reply only with \
                 \"received part {no}/{total}\" and wait for the rest.\n"
            );
        } else {
            let _ = writeln!(
                out,
                "That was the last part. Now act on the whole document.\n"
            );
        }
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "ai-prompt-splitter",
    name: "AI prompt splitter",
    description: "Cut a long document into numbered parts, each with the \"wait for the rest\" wrapper.",
    category: Category::Text,
    keywords: &["ai", "prompt", "split", "chunk", "llm", "context", "paste"],
    fields: &[
        Field::area("input", "Long text"),
        Field::number("size", "Characters per part", "8000"),
    ],
    kind: Kind::Pure,
    run: split,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn numbers_every_part_and_marks_the_last() {
        // Four paragraphs of ~130 characters, so 200 per part really splits.
        let text = &["para one", "para two", "para three", "para four"]
            .map(|p| format!("{p} {}", "x".repeat(120)))
            .join("\n\n");
        let out = ok("ai-prompt-splitter", &[("input", text), ("size", "5000")]);
        assert!(
            out.contains("It fits in one message"),
            "a short document is not split: {out}"
        );

        let out = ok("ai-prompt-splitter", &[("input", text), ("size", "200")]);
        assert!(out.contains("[START PART 1/"), "{out}");
        assert!(out.contains("Reply only with \"received part 1/"), "{out}");
        assert!(out.contains("That was the last part."), "{out}");
        // Nothing is lost and nothing is duplicated.
        let joined: String = chunks(text, 200).concat();
        assert_eq!(&joined, text);
    }

    #[test]
    fn cuts_on_the_largest_boundary_that_fits() {
        // Paragraph boundary wins over line, line over word.
        assert_eq!(chunks("aaa\n\nbbb ccc", 8), ["aaa\n\n", "bbb ccc"]);
        assert_eq!(chunks("aaa\nbbb\nccc", 8), ["aaa\nbbb\n", "ccc"]);
        assert_eq!(chunks("aaa bbb ccc", 8), ["aaa bbb ", "ccc"]);
        // A single word longer than the chunk is cut where it must be.
        assert_eq!(chunks("aaaaaaaaaa", 4), ["aaaa", "aaaa", "aa"]);
        // Multi-byte characters are counted, not sliced.
        assert_eq!(chunks("äöüß", 2), ["äö", "üß"]);
        assert!(run("ai-prompt-splitter", &[("input", "x"), ("size", "10")]).is_err());
    }
}
