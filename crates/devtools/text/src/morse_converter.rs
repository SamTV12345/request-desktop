//! `morse-converter` — it-tools PR #1021.
//!
//! Both panels at once, the way the rest of the registry does it: text in one
//! field, code in the other, no direction toggle.

use crate::*;
use anyhow::Result;

/// ITU-R M.1677-1, plus the prosigns that turn up in practice.
#[rustfmt::skip]
pub(crate) const MORSE: &[(char, &str)] = &[
    ('A', ".-"), ('B', "-..."), ('C', "-.-."), ('D', "-.."), ('E', "."), ('F', "..-."),
    ('G', "--."), ('H', "...."), ('I', ".."), ('J', ".---"), ('K', "-.-"), ('L', ".-.."),
    ('M', "--"), ('N', "-."), ('O', "---"), ('P', ".--."), ('Q', "--.-"), ('R', ".-."),
    ('S', "..."), ('T', "-"), ('U', "..-"), ('V', "...-"), ('W', ".--"), ('X', "-..-"),
    ('Y', "-.--"), ('Z', "--.."),
    ('0', "-----"), ('1', ".----"), ('2', "..---"), ('3', "...--"), ('4', "....-"),
    ('5', "....."), ('6', "-...."), ('7', "--..."), ('8', "---.."), ('9', "----."),
    ('.', ".-.-.-"), (',', "--..--"), ('?', "..--.."), ('\'', ".----."), ('!', "-.-.--"),
    ('/', "-..-."), ('(', "-.--."), (')', "-.--.-"), ('&', ".-..."), (':', "---..."),
    (';', "-.-.-."), ('=', "-...-"), ('+', ".-.-."), ('-', "-....-"), ('_', "..--.-"),
    ('"', ".-..-."), ('$', "...-..-"), ('@', ".--.-."),
    // Accented letters the ITU table keeps.
    ('Ä', ".-.-"), ('Ö', "---."), ('Ü', "..--"), ('É', "..-.."), ('Ñ', "--.--"),
];

fn encode(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            word.to_uppercase()
                .chars()
                .map(|c| {
                    MORSE
                        .iter()
                        .find(|(k, _)| *k == c)
                        // An unknown character becomes `#`, not a silent gap:
                        // the reader should see that something was lost.
                        .map_or("#".to_string(), |(_, code)| (*code).to_string())
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

fn decode(code: &str) -> String {
    // Words are separated by `/`, `|` or a run of spaces; letters by a space.
    code.replace('|', "/")
        .replace('_', "-")
        .replace('·', ".")
        .split('/')
        .map(|word| {
            word.split_whitespace()
                .map(|letter| {
                    MORSE
                        .iter()
                        .find(|(_, c)| *c == letter)
                        .map_or('#', |(k, _)| *k)
                })
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn morse(a: &Args) -> Result<String> {
    let text = s(a, "text", "SOS help");
    let code = s(a, "morse", "");
    Ok([
        section("Morse from text", Ok(format!("{}\n", encode(text)))),
        section(
            "Text from morse",
            Ok(if code.trim().is_empty() {
                String::new()
            } else {
                format!("{}\n", decode(code))
            }),
        ),
    ]
    .join("\n\n"))
}

pub static TOOL: Tool = Tool {
    id: "morse-converter",
    name: "Morse code converter",
    description: "Text to morse and back, ITU table, with `/` between words.",
    category: Category::Converter,
    keywords: &["morse", "code", "telegraph", "itu", "sos", "convert"],
    fields: &[
        Field::text("text", "Text").with_default("SOS help"),
        Field::text("morse", "Morse code"),
    ],
    kind: Kind::Pure,
    run: morse,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    #[test]
    fn the_famous_one_and_back() {
        let out = ok("morse-converter", &[]);
        assert!(out.contains("... --- ... / .... . .-.. .--."), "{out}");
        let back = ok(
            "morse-converter",
            &[("morse", "... --- ... / .... . .-.. .--.")],
        );
        assert!(back.contains("SOS HELP"), "{back}");
        // Round trip over the whole table, punctuation included.
        let text = "HELLO, WORLD! 42 (A@B)";
        assert_eq!(decode(&encode(text)), text);
        // Unknown characters are marked, not dropped.
        assert!(encode("%").contains('#'));
        // ß upper-cases to SS, which the table does know.
        assert_eq!(encode("ß"), "... ...");
        assert_eq!(decode("..---- ..."), "#S");
        // `_`, `·` and `|` are the other common spellings.
        assert_eq!(decode("··· ___ ··· | ·"), "SOS E");
    }
}
