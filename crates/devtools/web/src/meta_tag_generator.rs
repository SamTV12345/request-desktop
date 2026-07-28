//! `meta-tag-generator`

use crate::*;
use crate::{Args, Category, Field, Kind, Tool, arg};
use anyhow::{Result, bail};
use std::fmt::Write as _;

pub(crate) fn meta_tag_generator(args: &Args) -> Result<String> {
    let t = tool("meta-tag-generator");
    let get = |k: &str| arg(args, t, k).trim().to_string();
    let title = get("title");
    let description = get("description");
    let mut out = String::new();
    if !title.is_empty() {
        let _ = writeln!(
            out,
            "<title>{}</title>",
            html_escape::encode_quoted_attribute(&title)
        );
    }
    if !description.is_empty() {
        let _ = writeln!(
            out,
            "<meta name=\"description\" content=\"{}\" />",
            html_escape::encode_quoted_attribute(&description)
        );
    }
    let og = [
        ("og:type", get("type")),
        ("og:title", title.clone()),
        ("og:description", description.clone()),
        ("og:url", get("url")),
        ("og:image", get("image")),
        ("og:image:alt", get("image_alt")),
        ("og:image:width", get("image_width")),
        ("og:image:height", get("image_height")),
    ];
    for (prop, value) in og.iter().filter(|(_, v)| !v.is_empty()) {
        let _ = writeln!(
            out,
            "<meta property=\"{prop}\" content=\"{}\" />",
            html_escape::encode_quoted_attribute(value)
        );
    }
    let tw = [
        ("twitter:card", get("twitter_card")),
        ("twitter:site", get("twitter_site")),
        ("twitter:creator", get("twitter_creator")),
        ("twitter:title", title),
        ("twitter:description", description),
        ("twitter:image", get("image")),
    ];
    for (name, value) in tw.iter().filter(|(_, v)| !v.is_empty()) {
        let _ = writeln!(
            out,
            "<meta name=\"{name}\" content=\"{}\" />",
            html_escape::encode_quoted_attribute(value)
        );
    }
    if out.is_empty() {
        bail!("nothing to generate — fill in at least a title");
    }
    Ok(out)
}

pub static TOOL: Tool = Tool {
    id: "meta-tag-generator",
    name: "Open graph meta generator",
    description: "Generate open-graph and social meta tags for your website.",
    category: Category::Web,
    keywords: &[
        "og", "open", "graph", "meta", "tags", "twitter", "seo", "social",
    ],
    // it-tools' section order: General information, Image, Twitter.
    fields: &[
        Field::select(
            "type",
            "Page type",
            &[
                "website",
                "article",
                "book",
                "profile",
                "music.song",
                "music.album",
                "music.playlist",
                "music.radio_station",
                "video.movie",
                "video.episode",
                "video.tv_show",
                "video.other",
            ],
            "website",
        ),
        Field::text("title", "Title"),
        Field::text("description", "Description"),
        Field::text("url", "Page URL"),
        Field::text("image", "Image url"),
        Field::text("image_alt", "Image alt"),
        Field::text("image_width", "Width"),
        Field::text("image_height", "Height"),
        Field::select(
            "twitter_card",
            "Twitter card type",
            &["summary", "summary_large_image", "app", "player"],
            "summary_large_image",
        ),
        Field::text("twitter_site", "Site account"),
        Field::text("twitter_creator", "Creator acc."),
    ],
    kind: Kind::Pure,
    run: meta_tag_generator,
};

#[cfg(test)]
mod tests {

    use crate::testing::*;

    #[test]
    fn meta_tags_are_generated_and_escaped() {
        let out = run(
            "meta-tag-generator",
            &[
                ("title", "A & B"),
                ("type", "website"),
                ("twitter_card", "summary"),
            ],
        )
        .unwrap();
        assert!(out.contains("<title>A &amp; B</title>"), "{out}");
        assert!(
            out.contains("property=\"og:type\" content=\"website\""),
            "{out}"
        );
        assert!(
            out.contains("name=\"twitter:card\" content=\"summary\""),
            "{out}"
        );
        assert!(
            run(
                "meta-tag-generator",
                &[("title", ""), ("type", ""), ("twitter_card", "")]
            )
            .is_err()
        );
    }
}
