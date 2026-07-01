use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use serde::Serialize;

/// Header sent by the Inertia.js client adapter on subsequent visits
pub const INERTIA_HEADER: &str = "x-inertia";

/// Header for asset versioning — triggers full reload when version changes
pub const INERTIA_VERSION_HEADER: &str = "x-inertia-version";

/// The page object returned to the Inertia client
#[derive(Debug, Clone, Serialize)]
pub struct InertiaPage {
    pub component: String,
    pub props: serde_json::Value,
    pub url: String,
    pub version: String,
}

/// Configuration for the Inertia adapter
#[derive(Clone)]
pub struct InertiaConfig {
    /// HTML template with a `{{INERTIA_PAGE}}` placeholder for the page data
    /// and `{{INERTIA_HEAD}}` for optional head tags
    pub root_template: String,
    /// Asset version string — change this to force a full page reload
    pub version: String,
}

impl InertiaConfig {
    pub fn new(root_template: &str) -> Self {
        Self {
            root_template: root_template.to_string(),
            version: String::new(),
        }
    }

    pub fn version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }
}

impl Default for InertiaConfig {
    fn default() -> Self {
        Self::new(DEFAULT_TEMPLATE)
    }
}

/// Default HTML shell that works with both SvelteKit and React Inertia adapters
const DEFAULT_TEMPLATE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {{INERTIA_HEAD}}
</head>
<body>
    <div id="app" data-page='{{INERTIA_PAGE}}'></div>
    {{INERTIA_SCRIPTS}}
</body>
</html>"#;

/// Main Inertia helper for building responses from controllers.
///
/// ```ignore
/// use karbon::inertia::Inertia;
///
/// #[karbon::get("/dashboard")]
/// async fn dashboard(State(state): State<AppState>) -> impl IntoResponse {
///     Inertia::render("Dashboard", serde_json::json!({
///         "user": current_user,
///         "stats": stats,
///     }))
/// }
/// ```
pub struct Inertia;

impl Inertia {
    /// Create an Inertia response with a component name and props.
    ///
    /// - If the request has an `X-Inertia` header → returns JSON (partial response)
    /// - Otherwise → returns full HTML page with embedded page data
    pub fn render<T: Serialize>(component: &str, props: T) -> InertiaResponse {
        let props = serde_json::to_value(props).unwrap_or_default();
        InertiaResponse {
            component: component.to_string(),
            props,
        }
    }

    /// Create an Inertia redirect (303 See Other) — used after POST/PUT/DELETE
    pub fn location(url: &str) -> Response {
        (StatusCode::SEE_OTHER, [(header::LOCATION, url)]).into_response()
    }
}

/// An Inertia response that adapts its format based on request headers.
/// Convert to an actual response using `into_response_with(headers, url, config)`.
pub struct InertiaResponse {
    pub component: String,
    pub props: serde_json::Value,
}

impl InertiaResponse {
    /// Build the final HTTP response.
    ///
    /// Called by the Inertia middleware which has access to the request headers and URL.
    pub fn into_response_with(
        self,
        request_headers: &HeaderMap,
        url: &str,
        config: &InertiaConfig,
    ) -> Response {
        let page = InertiaPage {
            component: self.component,
            props: self.props,
            url: url.to_string(),
            version: config.version.clone(),
        };

        let is_inertia = request_headers.contains_key(INERTIA_HEADER);

        if is_inertia {
            // Partial response — JSON only
            let mut response = axum::Json(&page).into_response();
            response
                .headers_mut()
                .insert(INERTIA_HEADER, "true".parse().unwrap());
            response
                .headers_mut()
                .insert("vary", "X-Inertia".parse().unwrap());
            response
        } else {
            // Full page — render HTML with embedded page data
            let page_json = serde_json::to_string(&page).unwrap_or_default();
            // Full HTML attribute escaping (covers both single and double quote contexts)
            let page_escaped = page_json
                .replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('\'', "&#39;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");

            let html = config
                .root_template
                .replace("{{INERTIA_PAGE}}", &page_escaped)
                .replace("{{INERTIA_HEAD}}", "")
                .replace("{{INERTIA_SCRIPTS}}", "");

            Html(html).into_response()
        }
    }
}
