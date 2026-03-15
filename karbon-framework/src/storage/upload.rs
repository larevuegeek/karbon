use axum::extract::Multipart;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Uploaded file info returned after processing
#[derive(Debug, Clone, serde::Serialize)]
pub struct UploadedFile {
    pub filename: String,
    pub original_name: String,
    pub filepath: String,
    pub mime_type: String,
    pub size: u64,
}

/// Upload configuration
pub struct UploadConfig {
    pub upload_dir: PathBuf,
    pub max_file_size: u64,
    pub max_files: usize,
    pub allowed_mimes: AllowedMimes,
}

impl UploadConfig {
    pub fn new(upload_dir: impl Into<PathBuf>) -> Self {
        Self {
            upload_dir: upload_dir.into(),
            max_file_size: 10 * 1024 * 1024, // 10 Mo
            max_files: 10,
            allowed_mimes: AllowedMimes::images(),
        }
    }

    pub fn max_file_size(mut self, size: u64) -> Self {
        self.max_file_size = size;
        self
    }

    pub fn max_files(mut self, count: usize) -> Self {
        self.max_files = count;
        self
    }

    pub fn allowed_mimes(mut self, mimes: AllowedMimes) -> Self {
        self.allowed_mimes = mimes;
        self
    }
}

/// Allowed MIME types for upload validation, with associated safe extensions
pub struct AllowedMimes(pub Vec<MimeEntry>);

/// A MIME type entry with its magic bytes signature and allowed extensions
pub struct MimeEntry {
    pub mime: &'static str,
    pub extensions: &'static [&'static str],
    pub magic_bytes: &'static [MagicSignature],
}

/// File signature (magic bytes) for content-based type detection
pub struct MagicSignature {
    pub offset: usize,
    pub bytes: &'static [u8],
}

impl AllowedMimes {
    pub fn images() -> Self {
        Self(vec![
            MimeEntry {
                mime: "image/jpeg",
                extensions: &["jpg", "jpeg"],
                magic_bytes: &[MagicSignature {
                    offset: 0,
                    bytes: &[0xFF, 0xD8, 0xFF],
                }],
            },
            MimeEntry {
                mime: "image/png",
                extensions: &["png"],
                magic_bytes: &[MagicSignature {
                    offset: 0,
                    bytes: &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
                }],
            },
            MimeEntry {
                mime: "image/webp",
                extensions: &["webp"],
                magic_bytes: &[
                    MagicSignature {
                        offset: 0,
                        bytes: b"RIFF",
                    },
                    MagicSignature {
                        offset: 8,
                        bytes: b"WEBP",
                    },
                ],
            },
            MimeEntry {
                mime: "image/gif",
                extensions: &["gif"],
                magic_bytes: &[MagicSignature {
                    offset: 0,
                    bytes: b"GIF8",
                }],
            },
        ])
    }

    pub fn images_with_svg() -> Self {
        let mut mimes = Self::images();
        mimes.0.push(MimeEntry {
            mime: "image/svg+xml",
            extensions: &["svg"],
            magic_bytes: &[], // SVG validated separately via content check
        });
        mimes
    }

    pub fn documents() -> Self {
        Self(vec![
            MimeEntry {
                mime: "application/pdf",
                extensions: &["pdf"],
                magic_bytes: &[MagicSignature {
                    offset: 0,
                    bytes: b"%PDF",
                }],
            },
            MimeEntry {
                mime: "text/plain",
                extensions: &["txt"],
                magic_bytes: &[],
            },
        ])
    }

    pub fn all_media() -> Self {
        let mut mimes = Self::images();
        mimes.0.extend(vec![
            MimeEntry {
                mime: "video/mp4",
                extensions: &["mp4"],
                magic_bytes: &[MagicSignature {
                    offset: 4,
                    bytes: b"ftyp",
                }],
            },
            MimeEntry {
                mime: "video/webm",
                extensions: &["webm"],
                magic_bytes: &[MagicSignature {
                    offset: 0,
                    bytes: &[0x1A, 0x45, 0xDF, 0xA3],
                }],
            },
            MimeEntry {
                mime: "audio/mpeg",
                extensions: &["mp3"],
                magic_bytes: &[
                    MagicSignature {
                        offset: 0,
                        bytes: &[0xFF, 0xFB],
                    },
                    MagicSignature {
                        offset: 0,
                        bytes: b"ID3",
                    },
                ],
            },
        ]);
        mimes
    }

    /// Find a MIME entry matching the given content type
    fn find_entry(&self, mime: &str) -> Option<&MimeEntry> {
        self.0.iter().find(|e| e.mime == mime)
    }

    /// Detect the real MIME type by inspecting magic bytes
    fn detect_mime_from_bytes(&self, data: &[u8]) -> Option<&str> {
        for entry in &self.0 {
            if entry.magic_bytes.is_empty() {
                continue;
            }
            let all_match = entry.magic_bytes.iter().all(|sig| {
                if sig.offset + sig.bytes.len() > data.len() {
                    return false;
                }
                &data[sig.offset..sig.offset + sig.bytes.len()] == sig.bytes
            });
            if all_match {
                return Some(entry.mime);
            }
        }
        None
    }
}

/// Sanitize an extension: lowercase, alphanumeric only
fn sanitize_extension(ext: &str) -> String {
    ext.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

/// Validate that SVG content doesn't contain dangerous elements
fn validate_svg_content(data: &[u8]) -> AppResult<()> {
    let content = std::str::from_utf8(data)
        .map_err(|_| AppError::BadRequest("Invalid SVG: not valid UTF-8".to_string()))?;

    let lower = content.to_lowercase();

    let dangerous = [
        "<script",
        "javascript:",
        "onerror",
        "onload",
        "onclick",
        "onmouseover",
        "onfocus",
        "onblur",
        "eval(",
        "expression(",
        "url(data:",
        "<!entity",
        "<!doctype",
        "xlink:href=\"data:",
        "xlink:href=\"javascript:",
    ];

    for pattern in &dangerous {
        if lower.contains(pattern) {
            return Err(AppError::BadRequest(format!(
                "SVG contains forbidden content: {}",
                pattern
            )));
        }
    }

    Ok(())
}

/// Safely resolve a path within a base directory, preventing path traversal
fn safe_resolve(base: &Path, filename: &str) -> AppResult<PathBuf> {
    let resolved = base.join(filename);
    let canonical_base = base
        .canonicalize()
        .unwrap_or_else(|_| base.to_path_buf());
    let canonical_resolved = resolved
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .unwrap_or_else(|| canonical_base.clone());

    if !canonical_resolved.starts_with(&canonical_base) {
        return Err(AppError::BadRequest(
            "Invalid file path: path traversal detected".to_string(),
        ));
    }

    Ok(resolved)
}

/// Handle file upload from multipart form data
pub async fn handle_upload(
    multipart: Multipart,
    config: &UploadConfig,
) -> AppResult<Vec<UploadedFile>> {
    handle_upload_inner(multipart, config).await
}

async fn handle_upload_inner(
    mut multipart: Multipart,
    config: &UploadConfig,
) -> AppResult<Vec<UploadedFile>> {
    let mut files = Vec::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        // Limit number of files
        if files.len() >= config.max_files {
            return Err(AppError::BadRequest(format!(
                "Too many files. Maximum allowed: {}",
                config.max_files
            )));
        }

        let original_name = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let claimed_content_type = field
            .content_type()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());

        // Read file data
        let data = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read file: {}", e)))?;

        // Validate size
        if data.len() as u64 > config.max_file_size {
            return Err(AppError::BadRequest(format!(
                "File '{}' exceeds maximum size of {} bytes",
                original_name, config.max_file_size
            )));
        }

        // Reject empty files
        if data.is_empty() {
            return Err(AppError::BadRequest("Empty file uploaded".to_string()));
        }

        // Detect real MIME type from magic bytes
        let detected_mime = config.allowed_mimes.detect_mime_from_bytes(&data);

        // Determine actual MIME type: prefer detected, fallback to claimed for types
        // without magic bytes (like SVG, TXT)
        let actual_mime = if let Some(detected) = detected_mime {
            detected.to_string()
        } else if config.allowed_mimes.find_entry(&claimed_content_type).is_some() {
            // Only trust claimed type if the entry has no magic bytes to check
            let entry = config.allowed_mimes.find_entry(&claimed_content_type).unwrap();
            if entry.magic_bytes.is_empty() {
                claimed_content_type.clone()
            } else {
                return Err(AppError::BadRequest(format!(
                    "File '{}': content does not match declared type '{}'",
                    original_name, claimed_content_type
                )));
            }
        } else {
            return Err(AppError::BadRequest(format!(
                "File type '{}' is not allowed",
                claimed_content_type
            )));
        };

        // Verify the MIME type is in the allowed list
        let mime_entry = config
            .allowed_mimes
            .find_entry(&actual_mime)
            .ok_or_else(|| {
                AppError::BadRequest(format!("File type '{}' is not allowed", actual_mime))
            })?;

        // Validate extension against whitelist for this MIME type
        let original_ext = Path::new(&original_name)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| sanitize_extension(e))
            .unwrap_or_default();

        let safe_extension = if mime_entry.extensions.contains(&original_ext.as_str()) {
            original_ext
        } else {
            mime_entry.extensions[0].to_string()
        };

        // SVG-specific content validation
        if actual_mime == "image/svg+xml" {
            validate_svg_content(&data)?;
        }

        // Generate unique filename with safe extension
        let filename = format!("{}.{}", Uuid::new_v4(), safe_extension);

        // Ensure directory exists
        tokio::fs::create_dir_all(&config.upload_dir)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create upload dir: {}", e)))?;

        // Resolve path safely (prevents traversal)
        let filepath = safe_resolve(&config.upload_dir, &filename)?;

        // Write file
        tokio::fs::write(&filepath, &data)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to write file: {}", e)))?;

        files.push(UploadedFile {
            filename,
            original_name,
            filepath: config.upload_dir.to_string_lossy().to_string(),
            mime_type: actual_mime,
            size: data.len() as u64,
        });
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_extension() {
        assert_eq!(sanitize_extension("jpg"), "jpg");
        assert_eq!(sanitize_extension("JPG"), "jpg");
        assert_eq!(sanitize_extension("ph.p"), "php");
        assert_eq!(sanitize_extension("j/p\\g"), "jpg");
    }

    #[test]
    fn test_detect_jpeg() {
        let allowed = AllowedMimes::images();
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        assert_eq!(allowed.detect_mime_from_bytes(&jpeg_header), Some("image/jpeg"));
    }

    #[test]
    fn test_detect_png() {
        let allowed = AllowedMimes::images();
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(allowed.detect_mime_from_bytes(&png_header), Some("image/png"));
    }

    #[test]
    fn test_detect_gif() {
        let allowed = AllowedMimes::images();
        assert_eq!(
            allowed.detect_mime_from_bytes(b"GIF89a"),
            Some("image/gif")
        );
    }

    #[test]
    fn test_detect_unknown() {
        let allowed = AllowedMimes::images();
        assert_eq!(allowed.detect_mime_from_bytes(b"random data"), None);
    }

    #[test]
    fn test_svg_validation_safe() {
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"10\"/></svg>";
        assert!(validate_svg_content(svg).is_ok());
    }

    #[test]
    fn test_svg_validation_xss_script() {
        let svg = b"<svg><script>alert('xss')</script></svg>";
        assert!(validate_svg_content(svg).is_err());
    }

    #[test]
    fn test_svg_validation_xss_event() {
        let svg = b"<svg><rect onload=\"alert('xss')\"/></svg>";
        assert!(validate_svg_content(svg).is_err());
    }

    #[test]
    fn test_svg_validation_xss_javascript_uri() {
        let svg = b"<svg><a xlink:href=\"javascript:alert(1)\">click</a></svg>";
        assert!(validate_svg_content(svg).is_err());
    }

    #[test]
    fn test_extension_whitelist() {
        let allowed = AllowedMimes::images();
        let entry = allowed.find_entry("image/jpeg").unwrap();
        assert!(entry.extensions.contains(&"jpg"));
        assert!(entry.extensions.contains(&"jpeg"));
    }

    #[test]
    fn test_safe_resolve_normal() {
        let base = Path::new("/uploads");
        let result = safe_resolve(base, "file.jpg");
        assert!(result.is_ok());
    }
}
