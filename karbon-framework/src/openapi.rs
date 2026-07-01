//! Minimal OpenAPI 3.0 generation from controller route metadata.
//!
//! Each `#[controller]` impl exposes `openapi_paths() -> Vec<(&str, &str)>`
//! ((HTTP method, full path)). Feed those into [`spec`] to produce a basic
//! OpenAPI document (paths + methods; no request/response schemas yet) that you
//! can serve as JSON.
//!
//! ```ignore
//! let routes = [PostController::openapi_paths(), UserController::openapi_paths()].concat();
//! let doc = karbon::openapi::spec("My API", "1.0.0", &routes);
//! Router::new().route("/openapi.json", get(|| async move { Json(doc) }));
//! ```

use serde_json::{Map, Value, json};

/// Build an OpenAPI 3.0 document from `(method, path)` route pairs.
///
/// Path templates (`/posts/{id}`) become `parameters` (path, required); operations
/// are grouped by a `tag` derived from the path (e.g. `posts`, `admin`), and each
/// gets a `summary` and `operationId`.
pub fn spec(title: &str, version: &str, routes: &[(&str, &str)]) -> Value {
    let mut paths: Map<String, Value> = Map::new();
    let mut tags: Vec<String> = Vec::new();

    for (method, path) in routes {
        let tag = path_tag(path);
        if !tags.contains(&tag) {
            tags.push(tag.clone());
        }

        let parameters: Vec<Value> = path_params(path)
            .into_iter()
            .map(|p| {
                // Sensible default constraints, documented in the spec: integer ids are
                // positive, string params are non-empty. Override per-route as needed.
                let schema = if p == "id" || p.ends_with("_id") {
                    json!({ "type": "integer", "format": "int64", "minimum": 1 })
                } else {
                    json!({ "type": "string", "minLength": 1 })
                };
                json!({
                    "name": p,
                    "in": "path",
                    "required": true,
                    "schema": schema,
                })
            })
            .collect();

        let operation = json!({
            "tags": [tag],
            "summary": format!("{} {}", method.to_uppercase(), path),
            "operationId": operation_id(method, path),
            "parameters": parameters,
            "responses": { "200": { "description": "OK" } },
        });

        let entry = paths
            .entry(path.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if let Value::Object(methods) = entry {
            methods.insert(method.to_lowercase(), operation);
        }
    }

    let tag_objects: Vec<Value> = tags.iter().map(|t| json!({ "name": t })).collect();

    json!({
        "openapi": "3.0.0",
        "info": { "title": title, "version": version },
        "tags": tag_objects,
        "paths": Value::Object(paths),
    })
}

/// Extract `{param}` names from a path template.
fn path_params(path: &str) -> Vec<String> {
    let mut params = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('{') {
        if let Some(close) = rest[open..].find('}') {
            params.push(rest[open + 1..open + close].to_string());
            rest = &rest[open + close + 1..];
        } else {
            break;
        }
    }
    params
}

/// Group tag = first path segment that isn't `api` or a version (`v1`, `v2`…) or a
/// `{param}`; falls back to `default`.
fn path_tag(path: &str) -> String {
    path.split('/')
        .find(|seg| {
            !(seg.is_empty()
                || *seg == "api"
                || seg.starts_with('{')
                || (seg.starts_with('v') && seg[1..].chars().all(|c| c.is_ascii_digit())))
        })
        .unwrap_or("default")
        .to_string()
}

/// Stable operationId, e.g. `get_api_v1_posts_id`.
fn operation_id(method: &str, path: &str) -> String {
    let slug: String = path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{}{}", method.to_lowercase(), slug)
        .trim_end_matches('_')
        .replace("__", "_")
}

/// A self-contained Swagger UI HTML page that renders the spec served at
/// `spec_url` (e.g. `/openapi.json`). Swagger UI assets load from a CDN, so this
/// needs network access in the browser; ideal for dev/API exploration.
pub fn swagger_ui_html(spec_url: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>API — Swagger UI</title>
<link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css">
<style>body{{margin:0;background:#0b0d18}}</style>
</head>
<body>
<div id="swagger-ui"></div>
<script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js" crossorigin></script>
<script>
window.ui = SwaggerUIBundle({{ url: "{spec_url}", dom_id: "#swagger-ui", deepLinking: true }});
</script>
</body>
</html>"##
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_paths_and_methods() {
        let routes = [
            ("GET", "/posts"),
            ("POST", "/posts"),
            ("GET", "/posts/{id}"),
        ];
        let doc = spec("API", "1.0.0", &routes);
        assert_eq!(doc["openapi"], "3.0.0");
        assert_eq!(doc["info"]["title"], "API");
        assert!(doc["paths"]["/posts"]["get"].is_object());
        assert!(doc["paths"]["/posts"]["post"].is_object());
        assert!(doc["paths"]["/posts/{id}"]["get"].is_object());
    }

    #[test]
    fn path_params_become_parameters() {
        let doc = spec("API", "1.0.0", &[("GET", "/api/v1/posts/{id}")]);
        let params = &doc["paths"]["/api/v1/posts/{id}"]["get"]["parameters"];
        assert_eq!(params[0]["name"], "id");
        assert_eq!(params[0]["in"], "path");
        assert_eq!(params[0]["required"], true);
        assert_eq!(params[0]["schema"]["type"], "integer");
    }

    #[test]
    fn tags_grouped_from_path() {
        assert_eq!(path_tag("/api/v1/posts/{id}"), "posts");
        assert_eq!(path_tag("/admin/posts"), "admin");
        let doc = spec("API", "1.0.0", &[("GET", "/api/v1/posts")]);
        assert_eq!(doc["paths"]["/api/v1/posts"]["get"]["tags"][0], "posts");
    }
}
