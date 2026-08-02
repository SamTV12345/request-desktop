//! Runtime translations for user-facing application text.
//!
//! Translation resources are embedded in the executable. This keeps the desktop
//! app self-contained while still allowing translators to work in ordinary FTL
//! files.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use gpui::{App, Global};
use unic_langid::LanguageIdentifier;

const EN: &str = include_str!("../locales/en-US.ftl");
const DE: &str = include_str!("../locales/de-DE.ftl");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locale {
    EnUs,
    DeDe,
}

impl Locale {
    pub fn from_environment() -> Self {
        let value = std::env::var("RP_LOCALE")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if value.starts_with("de") {
            Self::DeDe
        } else {
            Self::EnUs
        }
    }

    fn id(self) -> LanguageIdentifier {
        match self {
            Self::EnUs => "en-US".parse().expect("valid locale"),
            Self::DeDe => "de-DE".parse().expect("valid locale"),
        }
    }

    fn source(self) -> &'static str {
        match self {
            Self::EnUs => EN,
            Self::DeDe => DE,
        }
    }

    pub fn stored(value: &str) -> Self {
        match value {
            "de-DE" | "de" => Self::DeDe,
            _ => Self::EnUs,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::DeDe => "de-DE",
        }
    }
}

pub struct I18n {
    locale: Locale,
    bundle: FluentBundle<FluentResource>,
}

impl I18n {
    pub fn new(locale: Locale) -> Self {
        let mut bundle = FluentBundle::new(vec![locale.id()]);
        let resource = FluentResource::try_new(locale.source().to_owned())
            .expect("embedded translation resource must be valid Fluent");
        bundle
            .add_resource(resource)
            .expect("locale must not be duplicated");
        Self { locale, bundle }
    }

    pub fn locale(&self) -> Locale {
        self.locale
    }

    pub fn text(&self, key: &str) -> String {
        self.text_args(key, &[])
    }

    pub fn text_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        let Some(message) = self.bundle.get_message(key) else {
            return key.to_owned();
        };
        let Some(pattern) = message.value() else {
            return key.to_owned();
        };
        let mut values = FluentArgs::new();
        for (name, value) in args {
            values.set(*name, *value);
        }
        let mut errors = Vec::new();
        self.bundle
            .format_pattern(pattern, Some(&values), &mut errors)
            .into_owned()
    }
}

impl Global for I18n {}

pub fn install(cx: &mut App) {
    cx.set_global(I18n::new(Locale::from_environment()));
}

pub fn set_locale(locale: Locale, cx: &mut App) {
    cx.set_global(I18n::new(locale));
    cx.refresh_windows();
}

pub fn text(cx: &App, key: &str) -> String {
    cx.global::<I18n>().text(key)
}

pub fn text_args(cx: &App, key: &str, args: &[(&str, &str)]) -> String {
    cx.global::<I18n>().text_args(key, args)
}
