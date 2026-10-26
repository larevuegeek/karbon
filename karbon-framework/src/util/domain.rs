/// Domain names cut into name and extension, extension without its leading dot.
pub struct DomainHelper;

impl DomainHelper {
    /// `" Exemple.FR. "` → `"exemple.fr"`: trimmed, lowercased, trailing dot of a fully
    /// qualified name removed.
    pub fn normalize(domain: &str) -> String {
        domain.trim().trim_end_matches('.').to_lowercase()
    }

    /// Everything after the first dot is the extension: `"exemple.co.uk"` →
    /// `Some(("exemple", "co.uk"))`. `None` without a dot or with an empty label
    /// (`"exemple"`, `".fr"`, `"a..fr"`).
    pub fn split(domain: &str) -> Option<(String, String)> {
        let domain = Self::normalize(domain);

        if !Self::has_valid_labels(&domain) {
            return None;
        }

        let (name, extension) = domain.split_once('.')?;

        Some((name.to_string(), extension.to_string()))
    }

    /// The longest extension of `known` the domain ends with, otherwise everything after
    /// the first dot: with `["fr", "uk", "co.uk"]`, `"www.exemple.fr"` → `"fr"`,
    /// `"exemple.co.uk"` → `"co.uk"`, `"exemple.com"` → `"com"`. Entries of `known` may
    /// carry a leading dot and any case; an empty list gives the first-dot cut. `None`
    /// without a dot or with an empty label.
    pub fn extension<S: AsRef<str>>(domain: &str, known: &[S]) -> Option<String> {
        let domain = Self::normalize(domain);

        if !Self::has_valid_labels(&domain) {
            return None;
        }

        let is_known = |extension: &str| {
            known.iter().any(|k| {
                k.as_ref()
                    .trim()
                    .trim_start_matches('.')
                    .eq_ignore_ascii_case(extension)
            })
        };

        // Candidates from the longest: after the first dot, then after the second…
        let mut candidates = domain.match_indices('.').map(|(dot, _)| &domain[dot + 1..]);
        let first_dot = candidates.clone().next()?;

        Some(
            candidates
                .find(|c| is_known(c))
                .unwrap_or(first_dot)
                .to_string(),
        )
    }

    fn has_valid_labels(domain: &str) -> bool {
        domain.contains('.') && domain.split('.').all(|label| !label.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::DomainHelper;

    const KNOWN: [&str; 3] = ["uk", "co.uk", ".FR"];

    fn pair(name: &str, extension: &str) -> Option<(String, String)> {
        Some((name.to_string(), extension.to_string()))
    }

    fn extension(domain: &str) -> Option<String> {
        DomainHelper::extension(domain, &KNOWN)
    }

    #[test]
    fn normalize_trims_lowercases_and_drops_the_root_dot() {
        assert_eq!(DomainHelper::normalize(" Exemple.FR. "), "exemple.fr");
    }

    #[test]
    fn split_keeps_everything_after_the_first_dot() {
        assert_eq!(DomainHelper::split("exemple.fr"), pair("exemple", "fr"));
        assert_eq!(
            DomainHelper::split("Exemple.CO.UK"),
            pair("exemple", "co.uk")
        );
        assert_eq!(DomainHelper::split("exemple.fr."), pair("exemple", "fr"));
    }

    #[test]
    fn split_refuses_a_name_without_extension_or_with_an_empty_label() {
        for domain in ["exemple", "exemple.", ".fr", "a..fr", "", "  "] {
            assert_eq!(DomainHelper::split(domain), None, "{domain:?}");
        }
    }

    #[test]
    fn extension_takes_the_longest_known_one() {
        assert_eq!(extension("exemple.co.uk").as_deref(), Some("co.uk"));
        assert_eq!(extension("Exemple.UK").as_deref(), Some("uk"));
        assert_eq!(extension("www.exemple.fr").as_deref(), Some("fr"));
    }

    #[test]
    fn an_unknown_extension_falls_back_to_the_first_dot() {
        assert_eq!(extension("exemple.com").as_deref(), Some("com"));
        assert_eq!(
            DomainHelper::extension("exemple.gouv.xyz", &[] as &[&str]).as_deref(),
            Some("gouv.xyz")
        );
    }

    #[test]
    fn extension_refuses_a_name_without_extension_or_with_an_empty_label() {
        for domain in ["exemple", "exemple.", ".fr", "a..fr", ""] {
            assert_eq!(extension(domain), None, "{domain:?}");
        }
    }
}
