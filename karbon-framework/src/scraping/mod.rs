//! Web scraping toolkit: an HTTP client with throttling and a CSS-selector
//! HTML parser, plus a minimal `robots.txt` check.
//!
//! ```ignore
//! use karbon::scraping::Scraper;
//! use std::time::Duration;
//!
//! let scraper = Scraper::new()
//!     .user_agent("MyBot/1.0")
//!     .throttle(Duration::from_millis(500)); // be polite
//!
//! // Extract structured data without holding the (non-Send) document across awaits.
//! let titles: Vec<String> = scraper
//!     .scrape("https://example.com", |doc| doc.select_text("h2.title"))
//!     .await?;
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::error::{AppError, AppResult};

/// Polite HTTP client for scraping: optional throttling and a custom user-agent.
#[derive(Clone)]
pub struct Scraper {
    client: reqwest::Client,
    user_agent: String,
    throttle: Option<Duration>,
    last: Arc<Mutex<Option<Instant>>>,
}

impl Default for Scraper {
    fn default() -> Self {
        Self::new()
    }
}

impl Scraper {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            user_agent: concat!("KarbonScraper/", env!("CARGO_PKG_VERSION")).to_string(),
            throttle: None,
            last: Arc::new(Mutex::new(None)),
        }
    }

    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }

    /// Minimum delay between requests (politeness / rate-limit).
    pub fn throttle(mut self, delay: Duration) -> Self {
        self.throttle = Some(delay);
        self
    }

    async fn wait_throttle(&self) {
        let Some(delay) = self.throttle else { return };
        let mut last = self.last.lock().await;
        if let Some(prev) = *last {
            let elapsed = prev.elapsed();
            if elapsed < delay {
                tokio::time::sleep(delay - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }

    /// Fetch the raw body text of a URL.
    pub async fn fetch_text(&self, url: &str) -> AppResult<String> {
        self.wait_throttle().await;
        let resp = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, &self.user_agent)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("GET {url} failed: {e}")))?
            .error_for_status()
            .map_err(|e| AppError::Internal(format!("GET {url} returned error: {e}")))?;
        resp.text()
            .await
            .map_err(|e| AppError::Internal(format!("reading body of {url} failed: {e}")))
    }

    /// Fetch a URL, parse it as HTML, and run `extract` over the document.
    ///
    /// The parsed [`Document`] is created **after** the network await and never
    /// crosses an `.await`, so the returned future stays `Send` even though the
    /// underlying parser is not. Return owned data (`String`, `Vec<…>`) from `extract`.
    pub async fn scrape<T, F>(&self, url: &str, extract: F) -> AppResult<T>
    where
        F: FnOnce(&Document) -> T + Send,
        T: Send,
    {
        let text = self.fetch_text(url).await?;
        let doc = Document::parse(&text);
        Ok(extract(&doc))
    }

    /// Check the host's `robots.txt` for whether `url`'s path is allowed for the
    /// configured user-agent. Missing/unreadable `robots.txt` is treated as allowed.
    pub async fn is_allowed(&self, url: &str) -> bool {
        let Ok(parsed) = reqwest::Url::parse(url) else {
            return true;
        };
        let Some(host) = parsed.host_str() else {
            return true;
        };
        let robots_url = format!("{}://{}/robots.txt", parsed.scheme(), host);
        match self.fetch_text(&robots_url).await {
            Ok(body) => RobotsTxt::parse(&body).allowed(parsed.path()),
            Err(_) => true,
        }
    }
}

/// A parsed HTML document with CSS-selector helpers.
///
/// Not `Send` (the parser uses `Rc` internally) — extract owned data from it
/// synchronously rather than holding it across `.await`.
pub struct Document {
    html: scraper::Html,
}

impl Document {
    pub fn parse(html: &str) -> Self {
        Self {
            html: scraper::Html::parse_document(html),
        }
    }

    fn selector(css: &str) -> Option<scraper::Selector> {
        scraper::Selector::parse(css).ok()
    }

    /// Trimmed text content of every element matching `css`.
    pub fn select_text(&self, css: &str) -> Vec<String> {
        let Some(sel) = Self::selector(css) else {
            return Vec::new();
        };
        self.html
            .select(&sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Text of the first element matching `css`.
    pub fn select_first_text(&self, css: &str) -> Option<String> {
        self.select_text(css).into_iter().next()
    }

    /// Value of `attr` for every element matching `css`.
    pub fn select_attr(&self, css: &str, attr: &str) -> Vec<String> {
        let Some(sel) = Self::selector(css) else {
            return Vec::new();
        };
        self.html
            .select(&sel)
            .filter_map(|e| e.value().attr(attr).map(String::from))
            .collect()
    }

    /// Number of elements matching `css`.
    pub fn count(&self, css: &str) -> usize {
        Self::selector(css)
            .map(|sel| self.html.select(&sel).count())
            .unwrap_or(0)
    }

    /// All `href`s from `<a>` elements.
    pub fn links(&self) -> Vec<String> {
        self.select_attr("a[href]", "href")
    }

    /// The document `<title>`.
    pub fn title(&self) -> Option<String> {
        self.select_first_text("title")
    }
}

/// Minimal `robots.txt` model — collects `Disallow` rules under the `User-agent: *`
/// group (sufficient for politeness checks; not a full RFC 9309 implementation).
pub struct RobotsTxt {
    disallow: Vec<String>,
}

impl RobotsTxt {
    pub fn parse(body: &str) -> Self {
        let mut disallow = Vec::new();
        let mut applies = false;
        for line in body.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let lower = line.to_lowercase();
            if let Some(ua) = lower.strip_prefix("user-agent:") {
                applies = ua.trim() == "*";
            } else if applies && let Some(idx) = line.find(':') {
                let (key, value) = line.split_at(idx);
                if key.trim().eq_ignore_ascii_case("disallow") {
                    let path = value[1..].trim();
                    if !path.is_empty() {
                        disallow.push(path.to_string());
                    }
                }
            }
        }
        Self { disallow }
    }

    pub fn allowed(&self, path: &str) -> bool {
        !self.disallow.iter().any(|d| path.starts_with(d.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = r#"<html><head><title>Hello</title></head>
        <body>
          <h2 class="title">First</h2>
          <h2 class="title">Second</h2>
          <a href="/a">A</a><a href="https://x.com/b">B</a>
        </body></html>"#;

    #[test]
    fn extracts_text_and_attrs() {
        let doc = Document::parse(HTML);
        assert_eq!(doc.title().as_deref(), Some("Hello"));
        assert_eq!(doc.select_text("h2.title"), vec!["First", "Second"]);
        assert_eq!(doc.select_first_text("h2.title").as_deref(), Some("First"));
        assert_eq!(doc.count("h2.title"), 2);
        assert_eq!(doc.links(), vec!["/a", "https://x.com/b"]);
    }

    #[test]
    fn unknown_selector_is_empty() {
        let doc = Document::parse(HTML);
        assert!(doc.select_text(":::bad").is_empty());
    }
}

#[cfg(test)]
mod robots_tests {
    use super::*;

    #[test]
    fn robots_disallow() {
        let robots = RobotsTxt::parse("User-agent: *\nDisallow: /admin\nDisallow: /private\n");
        assert!(!robots.allowed("/admin/users"));
        assert!(!robots.allowed("/private"));
        assert!(robots.allowed("/public"));
    }

    #[test]
    fn robots_only_other_agents() {
        // Rules under a specific agent don't apply to the `*` group.
        let robots = RobotsTxt::parse("User-agent: BadBot\nDisallow: /\n");
        assert!(robots.allowed("/anything"));
    }
}
