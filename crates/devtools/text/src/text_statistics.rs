//! `text-statistics`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

/// it-tools: `new TextEncoder().encode(text).buffer.byteLength`, i.e. UTF-8 len.
pub(crate) fn string_size_in_bytes(text: &str) -> usize {
    text.len()
}

/// (chars, words, lines, bytes)
///
/// The counts are the four `n-statistic` tiles of the `.vue`, including its
/// quirks: word count is `text.split(/\s+/).length`, so a leading space counts
/// as a word — that is one whitespace run plus one, and a blank `text` is
/// special-cased to 0 rather than 1.
pub(crate) fn text_stats(text: &str) -> (usize, usize, usize, usize) {
    let chars = text.chars().count();
    let mut whitespace_runs = 0;
    let mut prev_ws = false;
    for c in text.chars() {
        let ws = c.is_whitespace();
        if ws && !prev_ws {
            whitespace_runs += 1;
        }
        prev_ws = ws;
    }
    let words = if text.is_empty() {
        0
    } else {
        whitespace_runs + 1
    };
    let lines = if text.is_empty() {
        0
    } else {
        text.replace("\r\n", "\n").split(['\n', '\r']).count()
    };
    (chars, words, lines, string_size_in_bytes(text))
}

/// The words the *added* statistics count, as opposed to `text_stats`' word
/// count, which is it-tools' `split(/\s+/)` quirk and stays as it is.
///
/// A word is a run of alphanumerics plus the two characters that live inside
/// one, so "don't" and "well-known" stay whole and "1,2" does not.
pub(crate) fn words_of(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .map(|w| w.trim_matches(['\'', '-']))
        .filter(|w| !w.is_empty())
}

/// Sentence-ending punctuation, Latin and CJK. A run of them ends one sentence,
/// so "wait…!" is not three.
pub(crate) fn count_sentences(text: &str) -> usize {
    text.split(['.', '!', '?', '…', '。', '！', '？'])
        .filter(|s| s.chars().any(char::is_alphanumeric))
        .count()
}

/// A paragraph is a block separated by a blank line; a text with no blank line
/// at all is one paragraph.
pub(crate) fn count_paragraphs(text: &str) -> usize {
    text.replace("\r\n", "\n")
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .count()
}

/// The ten most frequent words, case-folded. Ties break alphabetically so two
/// runs on the same text never disagree.
pub(crate) fn word_frequencies(text: &str) -> Vec<(String, usize)> {
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    for w in words_of(text) {
        *counts.entry(w.to_lowercase()).or_default() += 1;
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked.truncate(10);
    ranked
}

/// A pasted log line can be a megabyte with no space in it; the longest word is
/// a statistic, not a place to re-render the input.
fn ellipsize(word: &str, max: usize) -> String {
    if word.chars().count() <= max {
        return word.to_string();
    }
    format!("{}…", word.chars().take(max).collect::<String>())
}

/// it-tools shows the four tiles for any input, empty included (all zeroes), so
/// there is nothing to fail on and nothing to bail out of. The rows below them
/// are PR #1010; they follow the same rule, and only the reading time can fail
/// (a words-per-minute of zero), which it does in its own row.
pub(crate) fn text_statistics(args: &Args) -> anyhow::Result<String> {
    let text = a(args, "text-statistics", "text");
    let (chars, words, lines, bytes) = text_stats(text);

    let real: Vec<&str> = words_of(text).collect();
    let letters: usize = real.iter().map(|w| w.chars().count()).sum();
    // First of the longest, not the last: `max_by_key` keeps the later of two
    // equal keys, which makes the row jump about as the user types.
    let longest = real.iter().copied().reduce(|a, b| {
        if b.chars().count() > a.chars().count() {
            b
        } else {
            a
        }
    });

    let head = [
        // The four it-tools tiles, unchanged and in their original order.
        format!("Character count: {chars}\n"),
        format!("Word count: {words}\n"),
        format!("Line count: {lines}\n"),
        format!("Byte size: {bytes} B\n"),
        format!("Sentence count: {}\n", count_sentences(text)),
        format!("Paragraph count: {}\n", count_paragraphs(text)),
        row(
            "Average word length",
            Ok(if real.is_empty() {
                "0".to_string()
            } else {
                fmt_f((letters as f64 / real.len() as f64 * 100.0).round() / 100.0)
            }),
        ),
        row(
            "Longest word",
            Ok(longest.map_or_else(|| "—".to_string(), |w| ellipsize(w, 60))),
        ),
        row(
            "Reading time",
            reading_time(real.len(), a(args, "text-statistics", "words_per_minute")),
        ),
    ]
    .concat();

    let freq = word_frequencies(text);
    let list = if freq.is_empty() {
        "  (no words)".to_string()
    } else {
        freq.iter()
            .map(|(w, n)| format!("  {}: {n}", ellipsize(w, 60)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(format!(
        "{head}\n{}",
        crate::section("Most frequent words", Ok(list))
    ))
}

fn reading_time(words: usize, wpm: &str) -> anyhow::Result<String> {
    let wpm = parse_num("words per minute", wpm)?;
    if !(wpm.is_finite() && wpm >= 1.0) {
        anyhow::bail!("words per minute must be at least 1");
    }
    // Seconds, not fractional minutes: "0.42 min" is not a reading time.
    let seconds = (words as f64 * 60.0 / wpm).round();
    let seconds = if seconds.is_finite() { seconds } else { 0.0 } as u64;
    Ok(format!(
        "{} min {} s (at {} words/minute)",
        seconds / 60,
        seconds % 60,
        fmt_f(wpm)
    ))
}

pub static TOOL: Tool = Tool {
    id: "text-statistics",
    name: "Text statistics",
    description: "Get information about a text: characters, words, sentences, reading time and byte size.",
    category: Category::Text,
    keywords: &[
        "text",
        "statistics",
        "characters",
        "words",
        "lines",
        "size",
        "bytes",
        "sentences",
        "paragraphs",
        "reading time",
        "frequency",
    ],
    fields: &[
        Field::area("text", "Your text"),
        Field::number("words_per_minute", "Reading speed (words/minute)", "200"),
    ],
    kind: Kind::Pure,
    run: text_statistics,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn the_added_statistics() {
        let out = ok(
            "text-statistics",
            &[("text", "The cat sat. The cat ran!\n\nThe end.")],
        );
        // The it-tools tiles keep their values and their place at the top.
        assert!(out.starts_with("Character count: 35\n"), "{out}");
        assert!(out.contains("Word count: 8\n"), "{out}");
        assert!(out.contains("Sentence count: 3\n"), "{out}");
        assert!(out.contains("Paragraph count: 2\n"), "{out}");
        assert!(out.contains("Longest word: The\n"), "{out}");
        // 8 words at 200 wpm is 2.4 s.
        assert!(
            out.contains("Reading time: 0 min 2 s (at 200 words/minute)"),
            "{out}"
        );
        assert!(
            out.contains("Most frequent words:\n  the: 3\n  cat: 2"),
            "{out}"
        );

        // The speed is a field, and it only moves the reading time.
        let slow = ok(
            "text-statistics",
            &[("text", "one two three"), ("words_per_minute", "1")],
        );
        assert!(
            slow.contains("Reading time: 3 min 0 s (at 1 words/minute)"),
            "{slow}"
        );
        // A speed that makes no sense fails in its own row, not the whole tool.
        let bad = ok(
            "text-statistics",
            &[("text", "one"), ("words_per_minute", "0")],
        );
        assert!(
            bad.contains("Reading time: (words per minute must be at least 1)"),
            "{bad}"
        );
        assert!(bad.contains("Character count: 3"), "{bad}");
    }

    #[test]
    fn statistics_of_nothing() {
        let out = ok("text-statistics", &[]);
        assert!(out.contains("Character count: 0"), "{out}");
        assert!(out.contains("Sentence count: 0"), "{out}");
        assert!(out.contains("Paragraph count: 0"), "{out}");
        assert!(out.contains("Average word length: 0"), "{out}");
        assert!(out.contains("Longest word: —"), "{out}");
        assert!(out.contains("(no words)"), "{out}");
    }

    #[test]
    fn word_helpers() {
        assert_eq!(
            words_of("don't stop well-known 1,2").collect::<Vec<_>>(),
            ["don't", "stop", "well-known", "1", "2"]
        );
        assert_eq!(count_sentences("a. b! c? d"), 4);
        assert_eq!(count_sentences("wait…! really"), 2);
        assert_eq!(count_paragraphs("a\nb\n\nc"), 2);
        assert_eq!(count_paragraphs(""), 0);
        // Ties break alphabetically, so the list never reorders itself.
        assert_eq!(
            word_frequencies("b a"),
            [("a".to_string(), 1), ("b".to_string(), 1)]
        );
        // A megabyte without a space is a statistic, not a second copy of it.
        let out = ok("text-statistics", &[("text", &"a".repeat(1_000_000))]);
        assert!(
            out.contains(&format!("Longest word: {}…", "a".repeat(60))),
            "{out:.200}"
        );
    }
}
