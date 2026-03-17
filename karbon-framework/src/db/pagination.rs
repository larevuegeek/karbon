use serde::{Deserialize, Deserializer, Serialize};

/// Pagination query parameters (from URL: ?page=1&per_page=20&sort=id&order=desc)
#[derive(Debug, Clone, Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page", deserialize_with = "string_or_u32")]
    pub page: u32,
    #[serde(default = "default_per_page", deserialize_with = "string_or_u32", alias = "limit")]
    pub per_page: u32,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default = "default_order")]
    pub order: String,
    #[serde(default)]
    pub search: Option<String>,
}

/// Deserialize a u32 from either a number or a string (query strings are always strings)
fn string_or_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrU32 {
        Num(u32),
        Str(String),
    }

    match StringOrU32::deserialize(deserializer)? {
        StringOrU32::Num(n) => Ok(n),
        StringOrU32::Str(s) => s.parse::<u32>().map_err(serde::de::Error::custom),
    }
}

fn default_page() -> u32 {
    1
}
fn default_per_page() -> u32 {
    20
}
fn default_order() -> String {
    "desc".to_string()
}

/// Maximum allowed per_page to prevent DoS via huge result sets
const MAX_PER_PAGE: u32 = 200;

impl PaginationParams {
    /// Returns per_page clamped to MAX_PER_PAGE (200)
    pub fn safe_per_page(&self) -> u32 {
        self.per_page.min(MAX_PER_PAGE).max(1)
    }

    /// Calculate SQL OFFSET (uses safe_per_page)
    pub fn offset(&self) -> u32 {
        (self.page.saturating_sub(1)) * self.safe_per_page()
    }

    /// Validated ORDER direction (only asc/desc)
    pub fn order_direction(&self) -> &str {
        match self.order.to_lowercase().as_str() {
            "asc" => "ASC",
            _ => "DESC",
        }
    }

    /// Validate and return the sort column against an allow-list
    pub fn sort_column<'a>(&'a self, allowed: &[&'a str], default: &'a str) -> &'a str {
        match &self.sort {
            Some(col) if allowed.contains(&col.as_str()) => col.as_str(),
            _ => default,
        }
    }
}

/// Paginated response wrapper
#[derive(Debug, Serialize)]
pub struct Paginated<T: Serialize> {
    pub data: Vec<T>,
    pub meta: PaginationMeta,
}

#[derive(Debug, Serialize)]
pub struct PaginationMeta {
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
}

impl<T: Serialize> Paginated<T> {
    /// Create a paginated response from data and total count
    pub fn new(data: Vec<T>, total: u64, params: &PaginationParams) -> Self {
        let total_pages = ((total as f64) / (params.per_page as f64)).ceil() as u32;
        Self {
            data,
            meta: PaginationMeta {
                page: params.page,
                per_page: params.per_page,
                total,
                total_pages,
                has_next: params.page < total_pages,
                has_prev: params.page > 1,
            },
        }
    }
}
