use axum::extract::{Path, Query, Request};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use std::path::{Path as StdPath, PathBuf};
use std::sync::Arc;

use super::thumbnail::{ImageProcessor, ResizeMode, CropAnchor, OutputFormat, PngCompression};

/// ImgResizer — on-the-fly image resizing service with disk cache.
///
/// # Usage
///
/// ```rust
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
/// - `/files/r/800x600.webp/uploads/photo.jpg` → convert to WebP
/// - `/files/r/400x400_cover.webp/uploads/photo.jpg` → cover + WebP
/// - `/files/r/320x180_q75/uploads/photo.jpg` → JPEG quality 75
/// - `/files/r/320x180_gray/uploads/photo.jpg` → grayscale
/// - `/files/r/320x180_blur3/uploads/photo.jpg` → blur sigma 3
/// - `/files/original/uploads/photo.jpg` → serve original (passthrough)
///
/// Query params (advanced):
/// - `?anchor=top-left` → crop anchor for cover mode
/// - `?watermark=/path/to/logo.png&wpos=bottom-right&wopacity=50&wscale=20`
pub struct ImgResizer;

/// Internal state shared across requests
struct ResizerState {
    source_dir: PathBuf,
    cache_dir: PathBuf,
    max_width: u32,
    max_height: u32,
    default_quality: u8,
    allowed_formats: Vec<String>,
}

/// Configuration builder for ImgResizer
pub struct ImgResizerConfig {
    source_dir: PathBuf,
    cache_dir: PathBuf,
    max_width: u32,
    max_height: u32,
    default_quality: u8,
}

impl ImgResizerConfig {
    pub fn new(source_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            source_dir: source_dir.into(),
            cache_dir: cache_dir.into(),
            max_width: 3840,
            max_height: 2160,
            default_quality: 85,
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

    /// Default JPEG quality 1-100 (default 85)
    pub fn default_quality(mut self, q: u8) -> Self {
        self.default_quality = q.clamp(1, 100);
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
                "jpg".into(), "jpeg".into(), "png".into(),
                "gif".into(), "webp".into(), "avif".into(),
            ],
        });

        Router::new()
            .route("/r/{spec}/{*path}", get({
                let state = Arc::clone(&state);
                move |path, query| handle_resize(path, query, state)
            }))
            // Fallback: serve original files via tower-http ServeDir
            .fallback_service(tower_http::services::ServeDir::new(&state.source_dir))
    }
}

impl ImgResizer {
    /// Quick setup: serve images from `source_dir` with cache in `cache_dir`
    ///
    /// ```rust
    /// app.nest_service("/files", ImgResizer::serve("./storage", "./cache/img"));
    /// ```
    pub fn serve(source_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Router {
        ImgResizerConfig::new(source_dir, cache_dir).build()
    }

    /// Advanced setup with config builder
    ///
    /// ```rust
    /// app.nest_service("/files", ImgResizer::config("./storage", "./cache/img")
    ///     .max_width(2560)
    ///     .default_quality(90)
    ///     .build());
    /// ```
    pub fn config(source_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> ImgResizerConfig {
        ImgResizerConfig::new(source_dir, cache_dir)
    }
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

/// Parse spec like "320x180", "320x180_cover", "640x0.webp", "320x180_q75_gray"
fn parse_spec(spec: &str) -> Option<ResizeSpec> {
    if spec == "original" {
        return None; // passthrough
    }

    // Split format extension: "320x180.webp" → ("320x180", Some("webp"))
    let (spec_part, format) = if let Some(dot_pos) = spec.rfind('.') {
        let fmt = &spec[dot_pos + 1..];
        (&spec[..dot_pos], Some(fmt.to_lowercase()))
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

    // Parse modifiers
    for part in parts.iter().skip(1) {
        match *part {
            "cover" => mode = ResizeMode::Cover,
            "fit" => mode = ResizeMode::Fit,
            "stretch" => mode = ResizeMode::Stretch,
            "gray" | "grayscale" => grayscale = true,
            p if p.starts_with('q') => {
                quality = p[1..].parse::<u8>().ok().map(|q| q.clamp(1, 100));
            }
            p if p.starts_with("blur") => {
                blur = p[4..].parse::<f32>().ok().map(|b| b.clamp(0.1, 20.0));
            }
            _ => {}
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

/// Parse crop anchor from query param
fn parse_anchor(anchor: &str) -> CropAnchor {
    match anchor {
        "top-left" => CropAnchor::TopLeft,
        "top-center" | "top" => CropAnchor::TopCenter,
        "top-right" => CropAnchor::TopRight,
        "center-left" | "left" => CropAnchor::CenterLeft,
        "center" => CropAnchor::Center,
        "center-right" | "right" => CropAnchor::CenterRight,
        "bottom-left" => CropAnchor::BottomLeft,
        "bottom-center" | "bottom" => CropAnchor::BottomCenter,
        "bottom-right" => CropAnchor::BottomRight,
        _ => CropAnchor::Center,
    }
}

// ── Query params ──

#[derive(Debug, Deserialize, Default)]
struct ResizeQuery {
    anchor: Option<String>,
    watermark: Option<String>,
    wpos: Option<String>,
    wopacity: Option<u8>,
    wscale: Option<u32>,
}

// ── Request handler ──

async fn handle_resize(
    Path((spec, file_path)): Path<(String, String)>,
    Query(query): Query<ResizeQuery>,
    state: Arc<ResizerState>,
) -> Response {
    // Parse spec
    let resize_spec = match parse_spec(&spec) {
        Some(s) => s,
        None => {
            // "original" or invalid spec → serve original
            return serve_file(&state.source_dir.join(&file_path)).await;
        }
    };

    // Validate dimensions
    if resize_spec.width > state.max_width || resize_spec.height > state.max_height {
        return (StatusCode::BAD_REQUEST, "Dimensions too large").into_response();
    }

    // Validate source file exists and is an allowed format
    let source_path = state.source_dir.join(&file_path);
    if !source_path.exists() || !source_path.is_file() {
        return StatusCode::NOT_FOUND.into_response();
    }

    // Security: verify path doesn't escape source dir
    let canonical_source = match source_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let canonical_base = match state.source_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if !canonical_source.starts_with(&canonical_base) {
        return StatusCode::FORBIDDEN.into_response();
    }

    // Check file extension is allowed
    let ext = source_path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    if !state.allowed_formats.contains(&ext) {
        return (StatusCode::BAD_REQUEST, "Unsupported format").into_response();
    }

    // Determine output extension
    let out_ext = resize_spec.format.as_deref().unwrap_or(&ext);

    // Build cache path: cache_dir/spec/path.ext
    let cache_path = state.cache_dir
        .join(&spec)
        .join(format!("{}.{}", file_path.replace('/', "_"), out_ext));

    // Serve from cache if exists and newer than source
    if cache_path.exists() {
        if let (Ok(cache_meta), Ok(src_meta)) = (cache_path.metadata(), source_path.metadata()) {
            if let (Ok(cache_mod), Ok(src_mod)) = (cache_meta.modified(), src_meta.modified()) {
                if cache_mod >= src_mod {
                    return serve_file(&cache_path).await;
                }
            }
        }
    }

    // Process image
    let mut processor = ImageProcessor::new()
        .resize(
            if resize_spec.width > 0 { resize_spec.width } else { 1 },
            if resize_spec.height > 0 { resize_spec.height } else { 1 },
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
    if let Some(anchor) = &query.anchor {
        processor = processor.anchor(parse_anchor(anchor));
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
    processor = match out_ext {
        "webp" => processor.webp(),
        "png" => processor.png(PngCompression::Default),
        "gif" => processor.format(OutputFormat::Gif),
        _ => processor.jpeg(quality),
    };

    // Ensure cache directory exists
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Process in a blocking thread to avoid starving the Tokio runtime
    let cache_path_clone = cache_path.clone();
    let source_clone = source_path.clone();
    let file_path_clone = file_path.clone();

    let result = tokio::task::spawn_blocking(move || {
        processor.process(&source_clone, &cache_path_clone)
    }).await;

    match result {
        Ok(Ok(())) => serve_file(&cache_path).await,
        Ok(Err(e)) => {
            tracing::error!("ImgResizer: failed to process {}: {}", file_path, e);
            // Fallback: serve original
            serve_file(&source_path).await
        }
        Err(e) => {
            tracing::error!("ImgResizer: task panicked for {}: {}", file_path, e);
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

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");

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
            (header::CONTENT_SECURITY_POLICY, "default-src 'none'; style-src 'unsafe-inline'"),
        ],
        data,
    ).into_response()
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
        assert!(matches!(parse_anchor("top-left"), CropAnchor::TopLeft));
        assert!(matches!(parse_anchor("center"), CropAnchor::Center));
        assert!(matches!(parse_anchor("bottom-right"), CropAnchor::BottomRight));
        assert!(matches!(parse_anchor("top"), CropAnchor::TopCenter));
        assert!(matches!(parse_anchor("invalid"), CropAnchor::Center));
    }
}
