use std::collections::HashMap;
use std::sync::Arc;

/// Simple translation system backed by in-memory key-value maps.
///
/// ```ignore
/// let mut i18n = I18n::new("fr");
///
/// i18n.add_translations("fr", &[
///     ("user.not_found", "Utilisateur introuvable"),
///     ("user.created", "Utilisateur créé avec succès"),
///     ("validation.required", "Ce champ est obligatoire"),
/// ]);
///
/// i18n.add_translations("en", &[
///     ("user.not_found", "User not found"),
///     ("user.created", "User created successfully"),
///     ("validation.required", "This field is required"),
/// ]);
///
/// assert_eq!(i18n.t("user.not_found"), "Utilisateur introuvable");
/// assert_eq!(i18n.t_locale("user.not_found", "en"), "User not found");
///
/// // With interpolation:
/// assert_eq!(
///     i18n.t_with("welcome", &[("name", "David")]),
///     "Bienvenue David"
/// );
/// ```
#[derive(Clone)]
pub struct I18n {
    default_locale: String,
    translations: Arc<HashMap<String, HashMap<String, String>>>,
}

impl I18n {
    /// Create a new I18n instance with a default locale
    pub fn new(default_locale: &str) -> Self {
        Self {
            default_locale: default_locale.to_string(),
            translations: Arc::new(HashMap::new()),
        }
    }

    /// Add translations for a locale
    pub fn add_translations(&mut self, locale: &str, entries: &[(&str, &str)]) {
        let translations = Arc::make_mut(&mut self.translations);
        let map = translations.entry(locale.to_string()).or_default();
        for (key, value) in entries {
            map.insert(key.to_string(), value.to_string());
        }
    }

    /// Load translations from a JSON string: `{ "key": "value", ... }`
    pub fn load_json(&mut self, locale: &str, json: &str) -> Result<(), serde_json::Error> {
        let map: HashMap<String, String> = serde_json::from_str(json)?;
        let translations = Arc::make_mut(&mut self.translations);
        let locale_map = translations.entry(locale.to_string()).or_default();
        locale_map.extend(map);
        Ok(())
    }

    /// Translate a key using the default locale
    pub fn t<'a>(&'a self, key: &'a str) -> &'a str {
        self.t_locale(key, &self.default_locale)
    }

    /// Translate a key for a specific locale
    pub fn t_locale<'a>(&'a self, key: &'a str, locale: &str) -> &'a str {
        self.translations
            .get(locale)
            .and_then(|m| m.get(key))
            .or_else(|| {
                self.translations
                    .get(&self.default_locale)
                    .and_then(|m| m.get(key))
            })
            .map(|s| s.as_str())
            .unwrap_or(key)
    }

    /// Translate with variable interpolation: `{name}` → replaced by value
    pub fn t_with(&self, key: &str, vars: &[(&str, &str)]) -> String {
        let mut result = self.t(key).to_string();
        for (name, value) in vars {
            result = result.replace(&format!("{{{}}}", name), value);
        }
        result
    }

    /// Translate with interpolation for a specific locale
    pub fn t_locale_with(&self, key: &str, locale: &str, vars: &[(&str, &str)]) -> String {
        let mut result = self.t_locale(key, locale).to_string();
        for (name, value) in vars {
            result = result.replace(&format!("{{{}}}", name), value);
        }
        result
    }

    /// Get the default locale
    pub fn locale(&self) -> &str {
        &self.default_locale
    }

    /// Set the default locale
    pub fn set_locale(&mut self, locale: &str) {
        self.default_locale = locale.to_string();
    }

    /// Check if a key exists in the given locale
    pub fn has(&self, key: &str, locale: &str) -> bool {
        self.translations
            .get(locale)
            .is_some_and(|m| m.contains_key(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_translation() {
        let mut i18n = I18n::new("fr");
        i18n.add_translations("fr", &[("hello", "Bonjour")]);
        i18n.add_translations("en", &[("hello", "Hello")]);

        assert_eq!(i18n.t("hello"), "Bonjour");
        assert_eq!(i18n.t_locale("hello", "en"), "Hello");
    }

    #[test]
    fn test_fallback_to_default() {
        let mut i18n = I18n::new("fr");
        i18n.add_translations("fr", &[("hello", "Bonjour")]);

        assert_eq!(i18n.t_locale("hello", "de"), "Bonjour");
    }

    #[test]
    fn test_missing_key_returns_key() {
        let i18n = I18n::new("fr");
        assert_eq!(i18n.t("missing.key"), "missing.key");
    }

    #[test]
    fn test_interpolation() {
        let mut i18n = I18n::new("fr");
        i18n.add_translations("fr", &[("welcome", "Bienvenue {name}")]);

        assert_eq!(
            i18n.t_with("welcome", &[("name", "David")]),
            "Bienvenue David"
        );
    }

    #[test]
    fn test_load_json() {
        let mut i18n = I18n::new("fr");
        i18n.load_json("fr", r#"{"hello": "Bonjour", "bye": "Au revoir"}"#)
            .unwrap();

        assert_eq!(i18n.t("hello"), "Bonjour");
        assert_eq!(i18n.t("bye"), "Au revoir");
    }

    #[test]
    fn test_has_key() {
        let mut i18n = I18n::new("fr");
        i18n.add_translations("fr", &[("hello", "Bonjour")]);

        assert!(i18n.has("hello", "fr"));
        assert!(!i18n.has("hello", "en"));
        assert!(!i18n.has("missing", "fr"));
    }
}
