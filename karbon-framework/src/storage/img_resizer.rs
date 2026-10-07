use axum::Router;
use axum::extract::{OriginalUri, Path, Query, Request};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use super::thumbnail::{CropAnchor, ImageProcessor, OutputFormat, PngCompression, ResizeMode};

/// ImgResizer — on-the-fly image resizing service with disk cache.
///
/// # Usage
///
/// ```ignore
/// use karbon::storage::ImgResizer;
///
/// let app = Router::new()
///     .nest_service("/files", ImgResizer::serve("./storage", "./cache/img"));
/// ```
///
/// URLs:
/// - `/files/r/320x180/uploads/photo.jpg` → resize fit 320×180
/// - `/files/r/640x0/uploads/photo.jpg` → resize width 640, auto height
/// - `/files/r/0x400/uploads/photo.jpg` → resize height 400, auto width
/// - `/files/r/320x180_cover/uploads/photo.jpg` → cover crop
/// - `/files/r/320x180_stretch/uploads/photo.jpg` → stretch
/// - `/files/r/800x600.webp/uploads/photo.jpg` → convert to WebP (lossy, honours `_q`)
/// - `/files/r/400x400_cover.webp/uploads/photo.jpg` → cover + WebP
/// - `/files/r/320x180_q75/uploads/photo.jpg` → quality 75
/// - `/files/r/320x180_gray/uploads/photo.jpg` → grayscale
/// - `/files/r/320x180_blur3/uploads/photo.jpg` → blur sigma 3 (integer 1–20)
/// - `/files/original/uploads/photo.jpg` → serve original (passthrough)
///
/// Modifiers are written in this canonical order: mode, `q`, `gray`, `blur`, then `.format`.
/// Any other spelling of the same variant (`_fit`, `q075`, reordered modifiers) is redirected
/// to the canonical URL, and unknown modifiers are rejected, so one variant has exactly one
/// URL and one cache entry.
///
/// Query params:
/// - `?anchor=top-left` → crop anchor for cover mode (part of the cache key)
///
/// # Abuse limits
///
/// Every distinct URL used to cost a full decode + encode and a new cache file, so iterating
/// sizes could exhaust CPU and disk. Processing is bounded by
/// [`ImgResizerConfig::max_concurrent`], each source image keeps at most
/// [`ImgResizerConfig::max_variants_per_file`] cached variants, and
/// [`ImgResizerConfig::allowed_specs`] can restrict URLs to a fixed list.
pub struct ImgResizer;

/// Internal state shared across requests
struct ResizerState {
    source_dir: PathBuf,
    cache_dir: PathBuf,
    max_width: u32,
    max_height: u32,
    default_quality: u8,
    allowed_formats: Vec<String>,
    allowed_specs: Option<HashSet<String>>,
    max_variants_per_file: usize,
    permits: Semaphore,
}

/// Configuration builder for ImgResizer
pub struct ImgResizerConfig {
    source_dir: PathBuf,
    cache_dir: PathBuf,
    max_width: u32,
    max_height: u32,
    default_quality: u8,
    allowed_specs: Option<HashSet<String>>,
    max_variants_per_file: usize,
    max_concurrent: usize,
}

/// Longest wait for a processing slot before answering 503.
const QUEUE_TIMEOUT: Duration = Duration::from_secs(15);

/// Output formats the processor can actually encode.
const OUTPUT_FORMATS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "gif"];

impl ImgResizerConfig {
    pub fn new(source_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        Self {
            source_dir: source_dir.into(),
            cache_dir: cache_dir.into(),
            max_width: 3840,
            max_height: 2160,
            default_quality: 85,
            allowed_specs: None,
            max_variants_per_file: 48,
            max_concurrent: (cpus / 2).max(1),
        }
    }

    /// Max allowed resize width (default 3840)
    pub fn max_width(mut self, w: u32) -> Self {
        self.max_width = w;
        self
    }

    /// Max allowed resize height (default 2160)
    pub fn max_height(mut self, h: u32) -> Self {
        self.max_height = h;
        self
    }

    /// Default JPEG / WebP quality 1-100 (default 85)
    pub fn default_quality(mut self, q: u8) -> Self {
        self.default_quality = q.clamp(1, 100);
        self
    }

    /// Images processed at the same time (default: half the CPU cores, at least 1).
    /// Further requests wait up to 15 s for a slot, then get `503 Retry-After`.
    pub fn max_concurrent(mut self, n: usize) -> Self {
        self.max_concurrent = n.max(1);
        self
    }

    /// Cached variants kept per source image (default 48). A request for a new variant
    /// beyond this gets `400`; existing variants keep being served.
    pub fn max_variants_per_file(mut self, n: usize) -> Self {
        self.max_variants_per_file = n.max(1);
        self
    }

    /// Restrict resizing to these canonical specs (e.g. `"320x180_cover_q75.webp"`);
    /// anything else gets `404`. Off by default.
    pub fn allowed_specs<I, S>(mut self, specs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_specs = Some(specs.into_iter().map(Into::into).collect());
        self
    }

    /// Build the Axum router
    pub fn build(self) -> Router {
        // Ensure cache dir exists
        std::fs::create_dir_all(&self.cache_dir).ok();

        let state = Arc::new(ResizerState {
            source_dir: self.source_dir,
            cache_dir: self.cache_dir,
            max_width: self.max_width,
            max_height: self.max_height,
            default_quality: self.default_quality,
            allowed_formats: vec![
                "jpg".into(),
                "jpeg".into(),
                "png".into(),
                "gif".into(),
                "webp".into(),
                "avif".into(),
            ],
            allowed_specs: self.allowed_specs,
            max_variants_per_file: self.max_variants_per_file,
            permits: Semaphore::new(self.max_concurrent),
        });

        Router::new()
            .route(
                "/r/{spec}/{*path}",
                get({
                    let state = Arc::clone(&state);
                    move |uri, path, query| handle_resize(uri, path, query, state)
                }),
            )
            // Fallback: serve original files via tower-http ServeDir
            .fallback_service(tower_http::services::ServeDir::new(&state.source_dir))
            // Hidden files and directories (`.env`, `.git/`, temp files) are never served.
            .layer(middleware::from_fn(deny_hidden))
    }
}

impl ImgResizer {
    /// Quick setup: serve images from `source_dir` with cache in `cache_dir`
    ///
    /// ```ignore
    /// app.nest_service("/files", ImgResizer::serve("./storage", "./cache/img"));
    /// ```
    pub fn serve(source_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Router {
        ImgResizerConfig::new(source_dir, cache_dir).build()
    }

    /// Advanced setup with config builder
    ///
    /// ```ignore
    /// app.nest_service("/files", ImgResizer::config("./storage", "./cache/img")
    ///     .max_width(2560)
    ///     .default_quality(90)
    ///     .build());
    /// ```
    pub fn config(
        source_dir: impl Into<PathBuf>,
        cache_dir: impl Into<PathBuf>,
    ) -> ImgResizerConfig {
        ImgResizerConfig::new(source_dir, cache_dir)
    }
}

/// 404 for any path segment starting with a dot.
async fn deny_hidden(req: Request, next: Next) -> Response {
    if req.uri().path().split('/').any(|seg| seg.starts_with('.')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    next.run(req).await
}

// ── Resize spec parsing ──

/// Parsed resize specification from URL
#[derive(Debug)]
struct ResizeSpec {
    width: u32,
    height: u32,
    mode: ResizeMode,
    format: Option<String>,
    quality: Option<u8>,
    grayscale: bool,
    blur: Option<f32>,
}

impl ResizeSpec {
    /// The single URL spelling of this variant (mode, `q`, `gray`, `blur`, `.format`).
    fn canonical(&self) -> String {
        let mut out = format!("{}x{}", self.width, self.height);
        match self.mode {
            ResizeMode::Cover => out.push_str("_cover"),
            ResizeMode::Stretch => out.push_str("_stretch"),
            ResizeMode::Fit | ResizeMode::Width | ResizeMode::Height => {}
        }
        if let Some(q) = self.quality {
            out.push_str(&format!("_q{q}"));
        }
        if self.grayscale {
            out.push_str("_gray");
        }
        if let Some(b) = self.blur {
            out.push_str(&format!("_blur{}", b as u32));
        }
        if let Some(f) = &self.format {
            out.push('.');
            out.push_str(f);
        }
        out
    }
}

/// Parse spec like "320x180", "320x180_cover", "640x0.webp", "320x180_q75_gray".
/// `None` for "original" and for anything malformed (unknown modifier, bad value).
fn parse_spec(spec: &str) -> Option<ResizeSpec> {
    if spec == "original" {
        return None; // passthrough
    }

    // Split format extension: "320x180.webp" → ("320x180", Some("webp"))
    let (spec_part, format) = if let Some(dot_pos) = spec.rfind('.') {
        let fmt = spec[dot_pos + 1..].to_lowercase();
        if !OUTPUT_FORMATS.contains(&fmt.as_str()) {
            return None;
        }
        (&spec[..dot_pos], Some(fmt))
    } else {
        (spec, None)
    };

    // Split by underscore for modifiers: "320x180_cover_q75_gray"
    let parts: Vec<&str> = spec_part.split('_').collect();
    let dims = parts.first()?;

    // Parse dimensions: "320x180"
    let (w_str, h_str) = dims.split_once('x')?;
    let width = w_str.parse::<u32>().ok()?;
    let height = h_str.parse::<u32>().ok()?;

    if width == 0 && height == 0 {
        return None;
    }

    let mut mode = if width == 0 {
        ResizeMode::Height
    } else if height == 0 {
        ResizeMode::Width
    } else {
        ResizeMode::Fit
    };

    let mut quality = None;
    let mut grayscale = false;
    let mut blur = None;

    // Parse modifiers; anything unknown makes the spec invalid (an ignored modifier would
    // give the same image a new URL and a new cache entry).
    for part in parts.iter().skip(1) {
        match *part {
            "cover" => mode = ResizeMode::Cover,
            "fit" => {}
            "stretch" => mode = ResizeMode::Stretch,
            "gray" | "grayscale" => grayscale = true,
            p if p.starts_with("blur") => {
                let b = p[4..].parse::<u32>().ok()?;
                blur = Some(b.clamp(1, 20) as f32);
            }
            p if p.starts_with('q') => {
                quality = Some(p[1..].parse::<u8>().ok()?.clamp(1, 100));
            }
            _ => return None,
        }
    }

    Some(ResizeSpec {
        width,
        height,
        mode,
        format,
        quality,
        grayscale,
        blur,
    })
}

/// Parse crop anchor from query param (`None` if unknown).
fn parse_anchor(anchor: &str) -> Option<CropAnchor> {
    Some(match anchor {
        "top-left" => CropAnchor::TopLeft,
        "top-center" | "top" => CropAnchor::TopCenter,
        "top-right" => CropAnchor::TopRight,
        "center-left" | "left" => CropAnchor::CenterLeft,
        "center" => CropAnchor::Center,
        "center-right" | "right" => CropAnchor::CenterRight,
        "bottom-left" => CropAnchor::BottomLeft,
        "bottom-center" | "bottom" => CropAnchor::BottomCenter,
        "bottom-right" => CropAnchor::BottomRight,
        _ => return None,
    })
}

// ── Query params ──

#[derive(Debug, Deserialize, Default)]
struct ResizeQuery {
    anchor: Option<String>,
}

// ── Request handler ──

/// A single URL path segment used to build a cache file name. Rejects anything that
/// could escape the target directory (`..`, separators, absolute markers).
fn is_safe_spec(spec: &str) -> bool {
    !spec.is_empty()
        && spec.len() <= 64
        && !spec.starts_with('.')
        && spec
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// A relative file path from the URL. Rejects traversal, absolute paths, backslashes,
/// null bytes, empty components and hidden files or directories.
fn is_safe_rel_path(p: &str) -> bool {
    if p.is_empty() || p.contains('\0') || p.contains('\\') || p.starts_with('/') {
        return false;
    }
    p.split('/').all(|seg| !seg.is_empty() && !seg.starts_with('.'))
}

/// Join `rel` onto `base` and confirm the canonicalized result stays inside `base`.
/// Returns `None` if the path escapes, doesn't exist, or can't be canonicalized.
fn resolve_within(base: &std::path::Path, rel: &str) -> Option<std::path::PathBuf> {
    let candidate = base.join(rel);
    let canonical = candidate.canonicalize().ok()?;
    let canonical_base = base.canonicalize().ok()?;
    canonical.starts_with(&canonical_base).then_some(canonical)
}

/// `mtime(a) >= mtime(b)`; false if either is unreadable.
fn newer_or_same(a: &StdPath, b: &StdPath) -> bool {
    match (a.metadata().and_then(|m| m.modified()), b.metadata().and_then(|m| m.modified())) {
        (Ok(a), Ok(b)) => a >= b,
        _ => false,
    }
}

async fn handle_resize(
    OriginalUri(original_uri): OriginalUri,
    Path((spec, file_path)): Path<(String, String)>,
    Query(query): Query<ResizeQuery>,
    state: Arc<ResizerState>,
) -> Response {
    // Reject path traversal before touching the filesystem (both branches below).
    if !is_safe_rel_path(&file_path) || !is_safe_spec(&spec) {
        return (StatusCode::BAD_REQUEST, "Invalid path").into_response();
    }

    // Original passthrough (containment-checked).
    if spec == "original" {
        return match resolve_within(&state.source_dir, &file_path) {
            Some(safe) => serve_file(&safe).await,
            None => StatusCode::NOT_FOUND.into_response(),
        };
    }

    let Some(resize_spec) = parse_spec(&spec) else {
        return (StatusCode::BAD_REQUEST, "Invalid resize spec").into_response();
    };

    // One variant, one URL: other spellings are redirected (no processing, no cache entry).
    let canonical = resize_spec.canonical();
    if canonical != spec {
        // Absolute target built from the original URI (the router can be nested anywhere).
        let path = original_uri.path();
        let needle = format!("/r/{spec}/");
        let Some(pos) = path.find(&needle) else {
            return (StatusCode::BAD_REQUEST, "Invalid resize spec").into_response();
        };
        let query = original_uri.query().map(|q| format!("?{q}")).unwrap_or_default();
        let target = format!("{}/r/{canonical}/{}{query}", &path[..pos], &path[pos + needle.len()..]);
        return Redirect::permanent(&target).into_response();
    }

    if let Some(allowed) = &state.allowed_specs
        && !allowed.contains(&canonical)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Validate dimensions
    if resize_spec.width > state.max_width || resize_spec.height > state.max_height {
        return (StatusCode::BAD_REQUEST, "Dimensions too large").into_response();
    }

    // Unknown anchors are rejected: they would all mean "center" under new URLs.
    let anchor = match query.anchor.as_deref() {
        None => None,
        Some(a) => match parse_anchor(a) {
            Some(parsed) => Some((a.to_string(), parsed)),
            None => return (StatusCode::BAD_REQUEST, "Invalid anchor").into_response(),
        },
    };

    // Validate source file exists, stays inside the source dir and has an allowed format.
    let Some(source_path) = resolve_within(&state.source_dir, &file_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !source_path.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Check file extension is allowed
    let ext = source_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if !state.allowed_formats.contains(&ext) {
        return (StatusCode::BAD_REQUEST, "Unsupported format").into_response();
    }

    // Determine output extension
    let out_ext = resize_spec.format.clone().unwrap_or_else(|| ext.clone());

    // Cache layout: cache_dir/<source path>/<spec>[@anchor].<ext> — every variant of one
    // image sits in one directory, so the per-image variant cap is a cheap directory count.
    let variant_dir = state.cache_dir.join(file_path.replace('/', "_"));
    let anchor_suffix = anchor.as_ref().map(|(a, _)| format!("@{a}")).unwrap_or_default();
    let cache_path = variant_dir.join(format!("{canonical}{anchor_suffix}.{out_ext}"));
    let failed_marker = variant_dir.join(format!("{canonical}{anchor_suffix}.{out_ext}.failed"));

    // Serve from cache if exists and newer than source
    if cache_path.exists() && newer_or_same(&cache_path, &source_path) {
        return serve_file(&cache_path).await;
    }
    // Known failure for this version of the source: don't decode it again on every hit.
    if failed_marker.exists() && newer_or_same(&failed_marker, &source_path) {
        return serve_file(&source_path).await;
    }

    if !cache_path.exists() {
        let variants = std::fs::read_dir(&variant_dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| !e.file_name().to_string_lossy().ends_with(".failed"))
                    .count()
            })
            .unwrap_or(0);
        if variants >= state.max_variants_per_file {
            return (StatusCode::BAD_REQUEST, "Too many variants for this image").into_response();
        }
    }

    // Process image
    let mut processor = ImageProcessor::new()
        .resize(
            if resize_spec.width > 0 {
                resize_spec.width
            } else {
                1
            },
            if resize_spec.height > 0 {
                resize_spec.height
            } else {
                1
            },
        )
        .mode(resize_spec.mode);

    // Width-only or height-only
    if resize_spec.width == 0 {
        processor = ImageProcessor::new()
            .height(resize_spec.height)
            .mode(resize_spec.mode);
    } else if resize_spec.height == 0 {
        processor = ImageProcessor::new()
            .width(resize_spec.width)
            .mode(resize_spec.mode);
    }

    // Anchor
    if let Some((_, parsed)) = anchor {
        processor = processor.anchor(parsed);
    }

    // Filters
    if resize_spec.grayscale {
        processor = processor.grayscale();
    }
    if let Some(sigma) = resize_spec.blur {
        processor = processor.blur(sigma);
    }

    // Output format
    let quality = resize_spec.quality.unwrap_or(state.default_quality);
    processor = match out_ext.as_str() {
        "webp" => processor.webp_quality(quality),
        "png" => processor.png(PngCompression::Default),
        "gif" => processor.format(OutputFormat::Gif),
        _ => processor.jpeg(quality),
    };

    // Ensure cache directory exists
    std::fs::create_dir_all(&variant_dir).ok();

    // Bounded processing: past the queue timeout, ask the client to come back later.
    let _permit = match tokio::time::timeout(QUEUE_TIMEOUT, state.permits.acquire()).await {
        Ok(Ok(permit)) => permit,
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::RETRY_AFTER, "5")],
                "Image processing busy",
            )
                .into_response();
        }
    };

    // Process in a blocking thread to avoid starving the Tokio runtime
    let cache_path_clone = cache_path.clone();
    let source_clone = source_path.clone();

    let result =
        tokio::task::spawn_blocking(move || processor.process(&source_clone, &cache_path_clone))
            .await;

    match result {
        Ok(Ok(())) => serve_file(&cache_path).await,
        Ok(Err(e)) => {
            tracing::error!("ImgResizer: failed to process {}: {}", file_path, e);
            std::fs::remove_file(&cache_path).ok();
            std::fs::write(&failed_marker, e.to_string()).ok();
            // Fallback: serve original
            serve_file(&source_path).await
        }
        Err(e) => {
            tracing::error!("ImgResizer: task panicked for {}: {}", file_path, e);
            std::fs::remove_file(&cache_path).ok();
            serve_file(&source_path).await
        }
    }
}

/// Serve a file with proper content-type and cache headers
async fn serve_file(path: &StdPath) -> Response {
    let data = match tokio::fs::read(path).await {
        Ok(d) => d,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("bin");

    let content_type = match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    };

    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
            // Prevent MIME sniffing (polyglot file attack)
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            // Images should not be framed
            (header::X_FRAME_OPTIONS, "DENY"),
            // Block if browser detects XSS in image context
            (
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'",
            ),
        ],
        data,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_spec_basic() {
        let spec = parse_spec("320x180").unwrap();
        assert_eq!(spec.width, 320);
        assert_eq!(spec.height, 180);
        assert!(matches!(spec.mode, ResizeMode::Fit));
    }

    #[test]
    fn test_parse_spec_width_only() {
        let spec = parse_spec("640x0").unwrap();
        assert_eq!(spec.width, 640);
        assert_eq!(spec.height, 0);
        assert!(matches!(spec.mode, ResizeMode::Width));
    }

    #[test]
    fn test_parse_spec_height_only() {
        let spec = parse_spec("0x400").unwrap();
        assert_eq!(spec.width, 0);
        assert_eq!(spec.height, 400);
        assert!(matches!(spec.mode, ResizeMode::Height));
    }

    #[test]
    fn test_parse_spec_cover() {
        let spec = parse_spec("320x180_cover").unwrap();
        assert!(matches!(spec.mode, ResizeMode::Cover));
    }

    #[test]
    fn test_parse_spec_with_format() {
        let spec = parse_spec("320x180.webp").unwrap();
        assert_eq!(spec.format, Some("webp".into()));
    }

    #[test]
    fn test_parse_spec_quality() {
        let spec = parse_spec("320x180_q75").unwrap();
        assert_eq!(spec.quality, Some(75));
    }

    #[test]
    fn test_parse_spec_grayscale() {
        let spec = parse_spec("320x180_gray").unwrap();
        assert!(spec.grayscale);
    }

    #[test]
    fn test_parse_spec_blur() {
        let spec = parse_spec("320x180_blur3").unwrap();
        assert_eq!(spec.blur, Some(3.0));
        assert_eq!(parse_spec("320x180_blur99").unwrap().blur, Some(20.0));
    }

    #[test]
    fn test_parse_spec_combined() {
        let spec = parse_spec("800x600_cover_q90_gray.webp").unwrap();
        assert_eq!(spec.width, 800);
        assert_eq!(spec.height, 600);
        assert!(matches!(spec.mode, ResizeMode::Cover));
        assert_eq!(spec.quality, Some(90));
        assert!(spec.grayscale);
        assert_eq!(spec.format, Some("webp".into()));
    }

    #[test]
    fn test_parse_spec_original() {
        assert!(parse_spec("original").is_none());
    }

    #[test]
    fn test_parse_spec_invalid() {
        assert!(parse_spec("0x0").is_none());
        assert!(parse_spec("abc").is_none());
    }

    #[test]
    fn test_parse_anchor() {
        assert!(matches!(parse_anchor("top-left"), Some(CropAnchor::TopLeft)));
        assert!(matches!(parse_anchor("center"), Some(CropAnchor::Center)));
        assert!(matches!(
            parse_anchor("bottom-right"),
            Some(CropAnchor::BottomRight)
        ));
        assert!(matches!(parse_anchor("top"), Some(CropAnchor::TopCenter)));
        assert!(parse_anchor("invalid").is_none());
    }

    #[test]
    fn test_unknown_or_malformed_modifiers_are_rejected() {
        // An ignored modifier used to give the same image an unlimited number of URLs.
        assert!(parse_spec("320x180_foo").is_none());
        assert!(parse_spec("320x180_q").is_none());
        assert!(parse_spec("320x180_blur2.5").is_none());
        assert!(parse_spec("320x180.exe").is_none());
    }

    #[test]
    fn test_canonical_spelling() {
        assert_eq!(
            parse_spec("800x600_cover_q90_gray.webp").unwrap().canonical(),
            "800x600_cover_q90_gray.webp"
        );
        // Equivalent spellings collapse to one URL (the handler redirects to it).
        assert_eq!(parse_spec("320x180_fit").unwrap().canonical(), "320x180");
        assert_eq!(parse_spec("320x180_q075").unwrap().canonical(), "320x180_q75");
        assert_eq!(
            parse_spec("320x180_gray_q75_cover").unwrap().canonical(),
            "320x180_cover_q75_gray"
        );
        assert_eq!(parse_spec("320x180_grayscale").unwrap().canonical(), "320x180_gray");
        assert_eq!(parse_spec("320x180.WEBP").unwrap().canonical(), "320x180.webp");
    }

    #[test]
    fn test_hidden_paths_rejected() {
        assert!(!is_safe_rel_path(".env"));
        assert!(!is_safe_rel_path("images/.git/config"));
        assert!(!is_safe_rel_path("images/../secret.jpg"));
        assert!(is_safe_rel_path("images/content/photo.jpg"));
        assert!(!is_safe_spec(".."));
    }

    // ── Router-level behaviour ──

    use axum::body::Body;
    use axum::http::Request as HttpRequest;
    use tower::ServiceExt;

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let root = std::env::temp_dir().join(format!("karbon-resizer-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&root).ok();
            std::fs::create_dir_all(root.join("src")).unwrap();
            let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(120, 80, |x, y| {
                image::Rgb([(x * 2) as u8, (y * 3) as u8, 90])
            }));
            // A PNG behind a .jpg name: must still be resized, not served whole.
            img.save_with_format(root.join("src/photo.jpg"), image::ImageFormat::Png).unwrap();
            std::fs::write(root.join("src/.env"), "SECRET=1").unwrap();
            Self { root }
        }

        fn router(&self, config: impl FnOnce(ImgResizerConfig) -> ImgResizerConfig) -> Router {
            config(ImgResizer::config(self.root.join("src"), self.root.join("cache"))).build()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    async fn get(router: &Router, uri: &str) -> Response {
        router.clone().oneshot(HttpRequest::get(uri).body(Body::empty()).unwrap()).await.unwrap()
    }

    #[tokio::test]
    async fn test_disguised_png_is_resized_to_webp() {
        let fx = Fixture::new("png");
        let router = fx.router(|c| c);
        let res = get(&router, "/r/60x40_cover_q75.webp/photo.jpg").await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "image/webp");
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        assert_eq!(image::guess_format(&body).unwrap(), image::ImageFormat::WebP);
        assert_eq!(image::load_from_memory(&body).unwrap().width(), 60);
    }

    #[tokio::test]
    async fn test_equivalent_spelling_redirects_to_canonical_url() {
        let fx = Fixture::new("redirect");
        let router = fx.router(|c| c);
        let res = get(&router, "/r/60x40_q75_cover/photo.jpg?anchor=top").await;
        assert_eq!(res.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(res.headers()[header::LOCATION], "/r/60x40_cover_q75/photo.jpg?anchor=top");
        assert_eq!(get(&router, "/r/60x40_bogus/photo.jpg").await.status(), StatusCode::BAD_REQUEST);
        assert_eq!(get(&router, "/r/60x40/photo.jpg?anchor=nowhere").await.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_hidden_files_are_never_served() {
        let fx = Fixture::new("hidden");
        let router = fx.router(|c| c);
        assert_eq!(get(&router, "/.env").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(get(&router, "/r/original/.env").await.status(), StatusCode::NOT_FOUND);
        assert_eq!(get(&router, "/photo.jpg").await.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_variant_cap_and_allowed_specs() {
        let fx = Fixture::new("cap");
        let router = fx.router(|c| c.max_variants_per_file(2));
        assert_eq!(get(&router, "/r/60x40/photo.jpg").await.status(), StatusCode::OK);
        assert_eq!(get(&router, "/r/50x40/photo.jpg").await.status(), StatusCode::OK);
        assert_eq!(get(&router, "/r/40x40/photo.jpg").await.status(), StatusCode::BAD_REQUEST);
        // Cached variants keep being served.
        assert_eq!(get(&router, "/r/60x40/photo.jpg").await.status(), StatusCode::OK);

        let strict = fx.router(|c| c.allowed_specs(["30x20"]));
        assert_eq!(get(&strict, "/r/30x20/photo.jpg").await.status(), StatusCode::OK);
        assert_eq!(get(&strict, "/r/31x20/photo.jpg").await.status(), StatusCode::NOT_FOUND);
    }
}
