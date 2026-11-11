//! Language of the text Karbon itself writes for end users.
//!
//! Karbon produces a handful of messages on its own: the masked 500, the refused token,
//! route and form validation, `Idempotency-Key` errors, and a few display helpers
//! (`DateHelper::time_ago`, file sizes, ordinals). They are in English unless the process
//! says otherwise:
//!
//! ```text
//! APP_LOCALE=fr
//! ```
//!
//! or, in code, `karbon::i18n::set_locale(Locale::Fr)`.
//!
//! This is process-wide, and covers Karbon's own text only. Messages the application
//! writes (`AppError::NotFound("…")`, constraint messages, templates) are never touched —
//! translate those with [`super::I18n`].

use std::sync::atomic::{AtomicU8, Ordering};

/// A language Karbon's own messages exist in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Locale {
    #[default]
    En,
    Fr,
}

impl Locale {
    /// `fr`, `fr_FR`, `fr-CA`, `FR`… are French; anything else is English.
    pub fn from_name(name: &str) -> Self {
        let name = name.trim().to_ascii_lowercase();
        let language = name.split(['_', '-', '.']).next().unwrap_or("");
        if language == "fr" { Self::Fr } else { Self::En }
    }

    /// The text for this locale, out of the two Karbon ships.
    pub(crate) fn pick<'a>(self, en: &'a str, fr: &'a str) -> &'a str {
        match self {
            Self::En => en,
            Self::Fr => fr,
        }
    }
}

const FROM_ENV: u8 = 0;
const EN: u8 = 1;
const FR: u8 = 2;

static CHOSEN: AtomicU8 = AtomicU8::new(FROM_ENV);

/// Fix the language of Karbon's own messages, whatever `APP_LOCALE` says.
pub fn set_locale(locale: Locale) {
    let value = match locale {
        Locale::En => EN,
        Locale::Fr => FR,
    };
    CHOSEN.store(value, Ordering::Relaxed);
}

/// The language of Karbon's own messages: the one given to [`set_locale`], else
/// `APP_LOCALE`, else English.
///
/// `APP_LOCALE` is read on each call rather than cached: the first message may be built
/// before the `.env` files are loaded, and messages are built on error paths only.
pub fn locale() -> Locale {
    match CHOSEN.load(Ordering::Relaxed) {
        EN => Locale::En,
        FR => Locale::Fr,
        _ => std::env::var("APP_LOCALE").map(|v| Locale::from_name(&v)).unwrap_or_default(),
    }
}

/// `en` or `fr`, according to [`locale`].
pub(crate) fn pick<'a>(en: &'a str, fr: &'a str) -> &'a str {
    locale().pick(en, fr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_french_names_select_french() {
        for name in ["fr", "FR", "fr_FR", "fr-CA", "fr_FR.UTF-8", " fr "] {
            assert_eq!(Locale::from_name(name), Locale::Fr, "{name}");
        }
        for name in ["", "en", "en_US", "de", "french", "fra", "C"] {
            assert_eq!(Locale::from_name(name), Locale::En, "{name}");
        }
    }

    #[test]
    fn pick_returns_the_text_of_the_locale() {
        assert_eq!(Locale::En.pick("yes", "oui"), "yes");
        assert_eq!(Locale::Fr.pick("yes", "oui"), "oui");
    }
}
