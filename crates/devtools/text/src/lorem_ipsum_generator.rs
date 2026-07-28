//! `lorem-ipsum-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool};

pub(crate) const LOREM_VOCAB: &[&str] = &[
    "a",
    "ac",
    "accumsan",
    "ad",
    "adipiscing",
    "aenean",
    "aliquam",
    "aliquet",
    "amet",
    "ante",
    "aptent",
    "arcu",
    "at",
    "auctor",
    "bibendum",
    "blandit",
    "class",
    "commodo",
    "condimentum",
    "congue",
    "consectetur",
    "consequat",
    "conubia",
    "convallis",
    "cras",
    "cubilia",
    "cum",
    "curabitur",
    "curae",
    "dapibus",
    "diam",
    "dictum",
    "dictumst",
    "dignissim",
    "dolor",
    "donec",
    "dui",
    "duis",
    "egestas",
    "eget",
    "eleifend",
    "elementum",
    "elit",
    "enim",
    "erat",
    "eros",
    "est",
    "et",
    "etiam",
    "eu",
    "euismod",
    "facilisi",
    "faucibus",
    "felis",
    "fermentum",
    "feugiat",
    "fringilla",
    "fusce",
    "gravida",
    "habitant",
    "habitasse",
    "hac",
    "hendrerit",
    "himenaeos",
    "iaculis",
    "id",
    "imperdiet",
    "in",
    "inceptos",
    "integer",
    "interdum",
    "ipsum",
    "justo",
    "lacinia",
    "lacus",
    "laoreet",
    "lectus",
    "leo",
    "ligula",
    "litora",
    "lobortis",
    "lorem",
    "luctus",
    "maecenas",
    "magna",
    "magnis",
    "malesuada",
    "massa",
    "mattis",
    "mauris",
    "metus",
    "mi",
    "molestie",
    "mollis",
    "montes",
    "morbi",
    "mus",
    "nam",
    "nascetur",
    "natoque",
    "nec",
    "neque",
    "netus",
    "nisi",
    "nisl",
    "non",
    "nostra",
    "nulla",
    "nullam",
    "nunc",
    "odio",
    "orci",
    "ornare",
    "parturient",
    "pellentesque",
    "penatibus",
    "per",
    "pharetra",
    "phasellus",
    "placerat",
    "platea",
    "porta",
    "porttitor",
    "posuere",
    "potenti",
    "praesent",
    "pretium",
    "primis",
    "proin",
    "pulvinar",
    "purus",
    "quam",
    "quis",
    "quisque",
    "rhoncus",
    "ridiculus",
    "risus",
    "rutrum",
    "sagittis",
    "sapien",
    "scelerisque",
    "sed",
    "sem",
    "semper",
    "senectus",
    "sit",
    "sociis",
    "sociosqu",
    "sodales",
    "sollicitudin",
    "suscipit",
    "suspendisse",
    "taciti",
    "tellus",
    "tempor",
    "tempus",
    "tincidunt",
    "torquent",
    "tortor",
    "turpis",
    "ullamcorper",
    "ultrices",
    "ultricies",
    "urna",
    "varius",
    "vehicula",
    "vel",
    "velit",
    "venenatis",
    "vestibulum",
    "vitae",
    "vivamus",
    "viverra",
    "volutpat",
    "vulputate",
];

pub(crate) const LOREM_FIRST: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit.";

/// One filler alphabet: what to draw words from, the canonical opening sentence
/// and how the script joins and ends a sentence.
///
/// it-tools PR #1184 asks for other scripts, not other typography, so each
/// vocabulary is a small embedded sample (30-odd common words) rather than a
/// corpus — the point is that the block of text *looks* like the language.
pub(crate) struct LoremLang {
    pub(crate) id: &'static str,
    pub(crate) vocab: &'static [&'static str],
    pub(crate) opening: &'static str,
    /// CJK sets no space between words, so the separator is per language.
    pub(crate) sep: &'static str,
    pub(crate) end: char,
}

pub(crate) const LOREM_LANGUAGES: &[&str] = &["latin", "german", "cyrillic", "greek", "cjk"];

/// Openings are the language's own pangram or best-known line, so the first
/// sentence reads as deliberate rather than as noise.
pub(crate) const LOREM_LANGS: &[LoremLang] = &[
    LoremLang {
        id: "latin",
        vocab: LOREM_VOCAB,
        opening: LOREM_FIRST,
        sep: " ",
        end: '.',
    },
    LoremLang {
        id: "german",
        vocab: &[
            "aber", "auch", "auf", "aus", "bei", "bis", "buch", "damit", "dann", "durch", "eine",
            "einen", "erste", "gegen", "haben", "immer", "jahr", "jetzt", "kann", "kein", "leben",
            "machen", "mehr", "nach", "nicht", "noch", "oder", "ohne", "seine", "sehr", "sind",
            "über", "unter", "viele", "vom", "vor", "wenn", "werden", "wieder", "wird", "zwei",
            "zwischen",
        ],
        opening: "Franz jagt im komplett verwahrlosten Taxi quer durch Bayern.",
        sep: " ",
        end: '.',
    },
    LoremLang {
        id: "cyrillic",
        vocab: &[
            "быть",
            "весь",
            "год",
            "для",
            "если",
            "ещё",
            "жизнь",
            "здесь",
            "знать",
            "как",
            "когда",
            "которые",
            "место",
            "может",
            "можно",
            "надо",
            "нет",
            "один",
            "она",
            "очень",
            "потом",
            "почти",
            "работа",
            "раз",
            "сам",
            "свой",
            "себя",
            "сейчас",
            "слово",
            "снова",
            "так",
            "тогда",
            "только",
            "человек",
            "через",
            "что",
            "чтобы",
            "этот",
        ],
        opening: "Съешь же ещё этих мягких французских булок, да выпей чаю.",
        sep: " ",
        end: '.',
    },
    LoremLang {
        id: "greek",
        vocab: &[
            "αλλά",
            "από",
            "αυτό",
            "γιατί",
            "δεν",
            "είναι",
            "ένα",
            "εκεί",
            "επίσης",
            "ήδη",
            "θα",
            "και",
            "κάθε",
            "κάτι",
            "μετά",
            "μια",
            "μόνο",
            "να",
            "όλα",
            "όμως",
            "όταν",
            "όχι",
            "περισσότερο",
            "πολύ",
            "πριν",
            "προς",
            "πως",
            "στη",
            "στο",
            "στον",
            "της",
            "την",
            "του",
            "των",
            "χωρίς",
            "ωστόσο",
        ],
        opening: "Ξεσκεπάζω την ψυχοφθόρα βδελυγμία.",
        sep: " ",
        end: '.',
    },
    LoremLang {
        id: "cjk",
        vocab: &[
            "中文", "文字", "排版", "示例", "内容", "设计", "版面", "阅读", "段落", "标题", "图片",
            "效果", "简体", "传统", "文化", "系统", "时间", "空间", "世界", "语言", "书写", "结构",
            "样式", "颜色", "边距", "行距", "字体", "大小", "布局", "网格",
        ],
        // Li Bai, the line every Chinese schoolchild knows.
        opening: "床前明月光，疑是地上霜。",
        sep: "",
        end: '。',
    },
];

/// Unknown ids fall back to Latin, which is what an empty or half-typed select
/// value has to mean if the tool is to keep rendering.
pub(crate) fn lorem_lang(id: &str) -> &'static LoremLang {
    LOREM_LANGS
        .iter()
        .find(|l| l.id == id.trim())
        .unwrap_or(&LOREM_LANGS[0])
}

pub(crate) fn lorem_sentence(lang: &LoremLang, words: usize) -> String {
    let mut s: String = (0..words)
        .map(|_| lang.vocab[rand::random_range(0..lang.vocab.len())])
        .collect::<Vec<_>>()
        .join(lang.sep);
    // Uppercase the first *character*, not the first byte: Greek and Cyrillic
    // are multi-byte, and slicing at 1 would panic on them.
    if let Some(first) = s.chars().next() {
        s.replace_range(0..first.len_utf8(), &first.to_uppercase().to_string());
    }
    s.push(lang.end);
    s
}

pub(crate) fn generate_lorem_ipsum(
    paragraphs: usize,
    sentences: usize,
    words: usize,
    start_with_lorem: bool,
    as_html: bool,
    lang: &LoremLang,
) -> String {
    let mut paras: Vec<Vec<String>> = (0..paragraphs)
        .map(|_| {
            (0..sentences)
                .map(|_| lorem_sentence(lang, words))
                .collect()
        })
        .collect();

    if start_with_lorem && let Some(first) = paras.first_mut() {
        if first.is_empty() {
            first.push(lang.opening.into());
        } else {
            first[0] = lang.opening.into();
        }
    }

    let joined: Vec<String> = paras.iter().map(|s| s.join(lang.sep)).collect();
    if as_html {
        format!("<p>{}</p>", joined.join("</p>\n\n<p>"))
    } else {
        joined.join("\n\n")
    }
}

pub(crate) fn lorem_tool(args: &Args) -> anyhow::Result<String> {
    let id = "lorem-ipsum-generator";
    let paragraphs = uint("paragraphs", a(args, id, "paragraphs"), 100)?;
    let sentences = uint("sentences", a(args, id, "sentences"), 200)?;
    let words = uint("words", a(args, id, "words"), 200)?;
    if paragraphs == 0 || sentences == 0 || words == 0 {
        anyhow::bail!("paragraphs, sentences and words must all be at least 1");
    }
    Ok(generate_lorem_ipsum(
        paragraphs,
        sentences,
        words,
        truthy(a(args, id, "start_with_lorem")),
        truthy(a(args, id, "as_html")),
        lorem_lang(a(args, id, "language")),
    ))
}

pub static TOOL: Tool = Tool {
    id: "lorem-ipsum-generator",
    name: "Lorem ipsum generator",
    description: "Generate placeholder text to demonstrate the visual form of a document.",
    category: Category::Text,
    keywords: &[
        "lorem",
        "ipsum",
        "placeholder",
        "filler",
        "text",
        "generator",
    ],
    // it-tools' sentences/words sliders are *range* sliders (defaults 3-8 and
    // 8-15) and it picks a random count inside the range per generation. A
    // single number is the same thing with the range collapsed; the form has
    // no range widget, and a fixed count is the more predictable default.
    fields: &[
        Field::number("paragraphs", "Paragraphs", "1"),
        Field::number("sentences", "Sentences per paragraph", "3"),
        Field::number("words", "Words per sentence", "10"),
        Field::bool(
            "start_with_lorem",
            "Start with the classic first sentence",
            "true",
        ),
        Field::bool("as_html", "As HTML", "false"),
        // it-tools PR #1184. Latin is the default, so the tool opens exactly as
        // it did before.
        Field::select("language", "Language", LOREM_LANGUAGES, "latin"),
    ],
    kind: Kind::Pure,
    run: lorem_tool,
};
