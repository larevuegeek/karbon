/// URL slug utilities
pub struct Slug;

impl Slug {
    /// Generate a URL-safe slug from a string
    pub fn generate(input: &str) -> String {
        slug::slugify(input)
    }

    /// Generate a unique slug by appending a short random suffix
    pub fn unique(input: &str) -> String {
        let base = Self::generate(input);
        let suffix: u32 = rand::random::<u32>() % 99999;
        format!("{}-{}", base, suffix)
    }
}
