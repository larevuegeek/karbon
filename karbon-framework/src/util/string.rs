/// String manipulation helpers
pub struct StringHelper;

impl StringHelper {
    /// Truncate a string to `max_len` characters, appending a suffix (default "...")
    pub fn truncate(value: &str, max_len: usize, suffix: Option<&str>) -> String {
        let suffix = suffix.unwrap_or("...");
        let chars: Vec<char> = value.chars().collect();

        if chars.len() <= max_len {
            return value.to_string();
        }

        let truncated: String = chars[..max_len].iter().collect();
        format!("{}{}", truncated.trim_end(), suffix)
    }

    /// Extract an excerpt around a keyword with surrounding context
    pub fn excerpt(value: &str, keyword: &str, radius: usize, suffix: Option<&str>) -> String {
        let suffix = suffix.unwrap_or("...");
        let lower = value.to_lowercase();
        let keyword_lower = keyword.to_lowercase();

        let pos = match lower.find(&keyword_lower) {
            Some(p) => p,
            None => return Self::truncate(value, radius * 2, Some(suffix)),
        };

        let chars: Vec<char> = value.chars().collect();
        let char_pos = value[..pos].chars().count();

        let start = char_pos.saturating_sub(radius);
        let end = (char_pos + keyword.chars().count() + radius).min(chars.len());

        let mut result = String::new();

        if start > 0 {
            result.push_str(suffix);
        }

        let slice: String = chars[start..end].iter().collect();
        result.push_str(slice.trim());

        if end < chars.len() {
            result.push_str(suffix);
        }

        result
    }

    /// Capitalize the first letter of a string
    pub fn capitalize(value: &str) -> String {
        let mut chars = value.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => {
                let upper: String = c.to_uppercase().collect();
                format!("{}{}", upper, chars.as_str())
            }
        }
    }

    /// Title case: capitalize the first letter of each word
    pub fn title_case(value: &str) -> String {
        value
            .split_whitespace()
            .map(Self::capitalize)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Convert to camelCase
    pub fn camel_case(value: &str) -> String {
        let parts: Vec<&str> = value.split(['_', '-', ' ']).collect();
        let mut result = String::new();
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if i == 0 {
                result.push_str(&part.to_lowercase());
            } else {
                result.push_str(&Self::capitalize(&part.to_lowercase()));
            }
        }
        result
    }

    /// Convert to snake_case
    pub fn snake_case(value: &str) -> String {
        let mut result = String::new();
        for (i, c) in value.chars().enumerate() {
            if c.is_uppercase() {
                if i > 0 {
                    result.push('_');
                }
                for lc in c.to_lowercase() {
                    result.push(lc);
                }
            } else if c == '-' || c == ' ' {
                result.push('_');
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Mask a string, showing only the first and last `visible` characters
    /// e.g., mask("secret@email.com", 3) => "sec**********com"
    pub fn mask(value: &str, visible: usize) -> String {
        let chars: Vec<char> = value.chars().collect();
        let len = chars.len();

        if len <= visible * 2 {
            return "*".repeat(len);
        }

        let start: String = chars[..visible].iter().collect();
        let end: String = chars[len - visible..].iter().collect();
        let middle = "*".repeat(len - visible * 2);

        format!("{}{}{}", start, middle, end)
    }

    /// Mask an email address: "us***@***.com"
    pub fn mask_email(value: &str) -> String {
        let parts: Vec<&str> = value.splitn(2, '@').collect();
        if parts.len() != 2 {
            return Self::mask(value, 2);
        }

        let local = parts[0];
        let domain = parts[1];

        let masked_local = if local.len() <= 2 {
            format!("{}***", &local[..1.min(local.len())])
        } else {
            format!("{}***", &local[..2])
        };

        let domain_parts: Vec<&str> = domain.rsplitn(2, '.').collect();
        let masked_domain = if domain_parts.len() == 2 {
            format!("***.{}", domain_parts[0])
        } else {
            "***".to_string()
        };

        format!("{}@{}", masked_local, masked_domain)
    }

    /// Get the initials from a name
    /// e.g., "Jean-Pierre Dupont" => "JD"
    pub fn initials(value: &str) -> String {
        value
            .split(|c: char| c.is_whitespace() || c == '-')
            .filter(|w| !w.is_empty())
            .filter_map(|w| w.chars().next())
            .map(|c| c.to_uppercase().to_string())
            .collect()
    }

    /// Count words in a string
    pub fn word_count(value: &str) -> usize {
        value.split_whitespace().count()
    }

    /// Check if a string contains only ASCII characters
    pub fn is_ascii(value: &str) -> bool {
        value.is_ascii()
    }

    /// Remove all HTML tags from a string
    pub fn strip_tags(value: &str) -> String {
        let mut result = String::with_capacity(value.len());
        let mut in_tag = false;

        for c in value.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => result.push(c),
                _ => {}
            }
        }

        result
    }

    /// Pad a string to a given length from the left
    pub fn pad_left(value: &str, length: usize, pad_char: char) -> String {
        let current_len = value.chars().count();
        if current_len >= length {
            return value.to_string();
        }
        let padding: String = std::iter::repeat_n(pad_char, length - current_len).collect();
        format!("{}{}", padding, value)
    }

    /// Pad a string to a given length from the right
    pub fn pad_right(value: &str, length: usize, pad_char: char) -> String {
        let current_len = value.chars().count();
        if current_len >= length {
            return value.to_string();
        }
        let padding: String = std::iter::repeat_n(pad_char, length - current_len).collect();
        format!("{}{}", value, padding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(StringHelper::truncate("Hello World", 5, None), "Hello...");
        assert_eq!(StringHelper::truncate("Hi", 5, None), "Hi");
        assert_eq!(
            StringHelper::truncate("Hello World", 5, Some("…")),
            "Hello…"
        );
    }

    #[test]
    fn test_truncate_unicode() {
        assert_eq!(StringHelper::truncate("Café résumé", 4, None), "Café...");
    }

    #[test]
    fn test_excerpt() {
        let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit";
        let result = StringHelper::excerpt(text, "dolor", 10, None);
        assert!(result.contains("dolor"));
        assert!(result.starts_with("..."));
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_excerpt_keyword_not_found() {
        let text = "Hello World";
        let result = StringHelper::excerpt(text, "xyz", 5, None);
        assert_eq!(result, "Hello Worl...");
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(StringHelper::capitalize("hello"), "Hello");
        assert_eq!(StringHelper::capitalize("Hello"), "Hello");
        assert_eq!(StringHelper::capitalize(""), "");
        assert_eq!(StringHelper::capitalize("é"), "É");
    }

    #[test]
    fn test_title_case() {
        assert_eq!(StringHelper::title_case("hello world"), "Hello World");
        assert_eq!(StringHelper::title_case("HELLO WORLD"), "HELLO WORLD");
    }

    #[test]
    fn test_camel_case() {
        assert_eq!(StringHelper::camel_case("hello_world"), "helloWorld");
        assert_eq!(StringHelper::camel_case("hello-world"), "helloWorld");
        assert_eq!(StringHelper::camel_case("hello world"), "helloWorld");
    }

    #[test]
    fn test_snake_case() {
        assert_eq!(StringHelper::snake_case("helloWorld"), "hello_world");
        assert_eq!(StringHelper::snake_case("HelloWorld"), "hello_world");
        assert_eq!(StringHelper::snake_case("hello-world"), "hello_world");
    }

    #[test]
    fn test_mask() {
        assert_eq!(StringHelper::mask("secret", 2), "se**et");
        assert_eq!(StringHelper::mask("hi", 2), "**");
        assert_eq!(StringHelper::mask("password123", 3), "pas*****123");
    }

    #[test]
    fn test_mask_email() {
        assert_eq!(
            StringHelper::mask_email("user@example.com"),
            "us***@***.com"
        );
        assert_eq!(StringHelper::mask_email("a@b.io"), "a***@***.io");
    }

    #[test]
    fn test_initials() {
        assert_eq!(StringHelper::initials("Jean Dupont"), "JD");
        assert_eq!(StringHelper::initials("Jean-Pierre Dupont"), "JPD");
        assert_eq!(StringHelper::initials("alice"), "A");
    }

    #[test]
    fn test_word_count() {
        assert_eq!(StringHelper::word_count("Hello World"), 2);
        assert_eq!(StringHelper::word_count("  spaces  everywhere  "), 2);
        assert_eq!(StringHelper::word_count(""), 0);
    }

    #[test]
    fn test_strip_tags() {
        assert_eq!(
            StringHelper::strip_tags("<p>Hello <b>World</b></p>"),
            "Hello World"
        );
        assert_eq!(StringHelper::strip_tags("No tags here"), "No tags here");
    }

    #[test]
    fn test_pad_left() {
        assert_eq!(StringHelper::pad_left("42", 5, '0'), "00042");
        assert_eq!(StringHelper::pad_left("hello", 3, '0'), "hello");
    }

    #[test]
    fn test_pad_right() {
        assert_eq!(StringHelper::pad_right("42", 5, ' '), "42   ");
        assert_eq!(StringHelper::pad_right("hello", 3, ' '), "hello");
    }
}
