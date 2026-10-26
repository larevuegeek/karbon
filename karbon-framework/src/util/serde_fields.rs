//! Small serde helpers for request bodies and query strings, to use in attributes:
//!
//! ```ignore
//! #[derive(Deserialize)]
//! struct ListQuery {
//!     #[serde(default = "karbon::util::serde_fields::default_page")]
//!     page: i64,
//!     #[serde(default = "karbon::util::serde_fields::default_limit::<50>")]
//!     limit: i64,
//!     #[serde(default = "karbon::util::serde_fields::default_true")]
//!     active: bool,
//!     #[serde(deserialize_with = "karbon::util::serde_fields::trimmed")]
//!     name: String,
//! }
//! ```

/// `true` when the field is absent.
pub fn default_true() -> bool {
    true
}

/// First page when absent.
pub fn default_page() -> i64 {
    1
}

/// Page size `N` when absent: `default_limit::<50>`.
pub fn default_limit<const N: i64>() -> i64 {
    N
}

/// The string arrives trimmed, so a length rule measures the actual text.
pub fn trimmed<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;

    Ok(value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Query {
        #[serde(default = "crate::util::serde_fields::default_page")]
        page: i64,
        #[serde(default = "crate::util::serde_fields::default_limit::<50>")]
        limit: i64,
        #[serde(default = "crate::util::serde_fields::default_true")]
        active: bool,
        #[serde(default, deserialize_with = "crate::util::serde_fields::trimmed")]
        name: String,
    }

    #[test]
    fn absent_fields_take_their_default() {
        let query: Query = serde_json::from_str("{}").unwrap();

        assert_eq!((query.page, query.limit, query.active), (1, 50, true));
    }

    #[test]
    fn strings_arrive_trimmed() {
        let query: Query = serde_json::from_str(r#"{"name": "  Domaine  ", "limit": 10}"#).unwrap();

        assert_eq!(query.name, "Domaine");
        assert_eq!(query.limit, 10);
    }
}
