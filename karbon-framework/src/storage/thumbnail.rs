use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::imageops;
use image::{DynamicImage, ImageFormat, ImageReader};
use std::io::Cursor;
use std::path::Path;

use crate::error::{AppError, AppResult};

/// Maximum allowed image dimensions to prevent decompression bombs
const MAX_IMAGE_DIMENSION: u32 = 16384; // 16384 x 16384

/// Maximum decoded pixel count (width * height)
const MAX_PIXEL_COUNT: u64 = 100_000_000; // 100 megapixels

/// Crop anchor point
#[derive(Debug, Clone, Copy, Default)]
pub enum CropAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    #[default]
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Resize strategy
#[derive(Debug, Clone, Copy, Default)]
pub enum ResizeMode {
    /// Resize to fit within bounds, preserving aspect ratio (may be smaller)
    #[default]
    Fit,
    /// Resize to cover bounds, then crop to exact size
    Cover,
    /// Stretch to exact dimensions (distorts aspect ratio)
    Stretch,
    /// Resize width only, compute height from aspect ratio
    Width,
    /// Resize height only, compute width from aspect ratio
    Height,
}

/// Flip direction
#[derive(Debug, Clone, Copy)]
pub enum FlipDirection {
    Horizontal,
    Vertical,
}

/// Rotation angle
#[derive(Debug, Clone, Copy)]
pub enum Rotation {
    Rotate90,
    Rotate180,
    Rotate270,
}

/// Output format with quality settings
#[derive(Debug, Clone)]
pub enum OutputFormat {
    Jpeg { quality: u8 },
    Png { compression: PngCompression },
    WebP,
    Gif,
    /// Auto-detect from output file extension
    Auto,
}

impl Default for OutputFormat {
    fn default() -> Self {
        Self::Auto
    }
}

/// PNG compression level
#[derive(Debug, Clone, Copy, Default)]
pub enum PngCompression {
    Fast,
    #[default]
    Default,
    Best,
}

/// Watermark configuration
#[derive(Debug, Clone)]
pub struct Watermark {
    pub path: std::path::PathBuf,
    pub position: CropAnchor,
    pub opacity: u8,
    pub scale_percent: u32,
    pub margin: u32,
}

/// Image processing pipeline builder
#[derive(Default)]
pub struct ImageProcessor {
    width: Option<u32>,
    height: Option<u32>,
    resize_mode: ResizeMode,
    crop_anchor: CropAnchor,
    rotation: Option<Rotation>,
    flip: Option<FlipDirection>,
    blur: Option<f32>,
    brightness: Option<i32>,
    contrast: Option<f32>,
    grayscale: bool,
    sharpen: Option<f32>,
    crop_region: Option<CropRegion>,
    output_format: OutputFormat,
    watermark: Option<Watermark>,
    upscale: bool,
}

/// Manual crop region (in pixels from source image)
#[derive(Debug, Clone, Copy)]
pub struct CropRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ImageProcessor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set target width and height
    pub fn resize(mut self, width: u32, height: u32) -> Self {
        self.width = Some(width);
        self.height = Some(height);
        self
    }

    /// Set target width only (height computed from aspect ratio)
    pub fn width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self.resize_mode = ResizeMode::Width;
        self
    }

    /// Set target height only (width computed from aspect ratio)
    pub fn height(mut self, height: u32) -> Self {
        self.height = Some(height);
        self.resize_mode = ResizeMode::Height;
        self
    }

    /// Set resize strategy
    pub fn mode(mut self, mode: ResizeMode) -> Self {
        self.resize_mode = mode;
        self
    }

    /// Set crop anchor for Cover mode
    pub fn anchor(mut self, anchor: CropAnchor) -> Self {
        self.crop_anchor = anchor;
        self
    }

    /// Rotate the image
    pub fn rotate(mut self, rotation: Rotation) -> Self {
        self.rotation = Some(rotation);
        self
    }

    /// Flip the image
    pub fn flip(mut self, direction: FlipDirection) -> Self {
        self.flip = Some(direction);
        self
    }

    /// Apply gaussian blur (sigma value)
    pub fn blur(mut self, sigma: f32) -> Self {
        self.blur = Some(sigma);
        self
    }

    /// Adjust brightness (-100 to 100)
    pub fn brightness(mut self, value: i32) -> Self {
        self.brightness = Some(value.clamp(-100, 100));
        self
    }

    /// Adjust contrast (-100.0 to 100.0)
    pub fn contrast(mut self, value: f32) -> Self {
        self.contrast = Some(value.clamp(-100.0, 100.0));
        self
    }

    /// Convert to grayscale
    pub fn grayscale(mut self) -> Self {
        self.grayscale = true;
        self
    }

    /// Crop a specific region from the source image
    pub fn crop(mut self, x: u32, y: u32, width: u32, height: u32) -> Self {
        self.crop_region = Some(CropRegion {
            x,
            y,
            width,
            height,
        });
        self
    }

    /// Apply sharpening filter (sigma value, e.g. 1.0-5.0)
    pub fn sharpen(mut self, sigma: f32) -> Self {
        self.sharpen = Some(sigma.clamp(0.1, 10.0));
        self
    }

    /// Allow upscaling (by default images are NOT upscaled beyond original size)
    pub fn upscale(mut self, allow: bool) -> Self {
        self.upscale = allow;
        self
    }

    /// Apply a watermark image
    ///
    /// ```rust
    /// processor.watermark("./logo.png", CropAnchor::BottomRight, 50, 15, 10)
    /// ```
    /// - `path`: path to watermark image (PNG with transparency recommended)
    /// - `position`: where to place the watermark
    /// - `opacity`: 0-100 (100 = fully opaque)
    /// - `scale_percent`: watermark size as % of the output image width (e.g. 15 = 15%)
    /// - `margin`: pixel margin from edges
    pub fn watermark(
        mut self,
        path: impl Into<std::path::PathBuf>,
        position: CropAnchor,
        opacity: u8,
        scale_percent: u32,
        margin: u32,
    ) -> Self {
        self.watermark = Some(Watermark {
            path: path.into(),
            position,
            opacity: opacity.min(100),
            scale_percent: scale_percent.clamp(1, 100),
            margin,
        });
        self
    }

    /// Set output format
    pub fn format(mut self, format: OutputFormat) -> Self {
        self.output_format = format;
        self
    }

    /// Shortcut: output as JPEG with given quality (1-100)
    pub fn jpeg(self, quality: u8) -> Self {
        self.format(OutputFormat::Jpeg {
            quality: quality.clamp(1, 100),
        })
    }

    /// Shortcut: output as optimized PNG
    pub fn png(self, compression: PngCompression) -> Self {
        self.format(OutputFormat::Png { compression })
    }

    /// Shortcut: output as WebP
    pub fn webp(self) -> Self {
        self.format(OutputFormat::WebP)
    }

    /// Process a source image file and save to destination
    pub fn process(&self, source: &Path, dest: &Path) -> AppResult<()> {
        let img = self.load_safe(source)?;
        let processed = self.apply(img)?;
        self.save(&processed, dest)
    }

    /// Process from bytes and return processed bytes
    pub fn process_bytes(&self, data: &[u8], dest_ext: &str) -> AppResult<Vec<u8>> {
        let img = self.load_safe_from_bytes(data)?;
        let processed = self.apply(img)?;
        self.encode(&processed, dest_ext)
    }

    /// Load image with decompression bomb protection
    fn load_safe(&self, source: &Path) -> AppResult<DynamicImage> {
        let reader = ImageReader::open(source)
            .map_err(|e| AppError::Internal(format!("Failed to open image: {}", e)))?;

        self.decode_safe(reader)
    }

    /// Load image from bytes with decompression bomb protection
    fn load_safe_from_bytes(&self, data: &[u8]) -> AppResult<DynamicImage> {
        let cursor = Cursor::new(data);
        let reader = ImageReader::new(cursor)
            .with_guessed_format()
            .map_err(|e| AppError::Internal(format!("Failed to detect image format: {}", e)))?;

        self.decode_safe(reader)
    }

    /// Decode with dimension limits
    fn decode_safe<R: std::io::BufRead + std::io::Seek>(
        &self,
        reader: ImageReader<R>,
    ) -> AppResult<DynamicImage> {
        let mut limited_reader = reader;

        // Set decoding limits to prevent decompression bombs
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_PIXEL_COUNT * 4); // 4 bytes per pixel (RGBA)
        limited_reader.limits(limits);

        let img = limited_reader
            .decode()
            .map_err(|e| AppError::BadRequest(format!("Failed to decode image: {}", e)))?;

        // Double check pixel count
        let (w, h) = (img.width(), img.height());
        if (w as u64) * (h as u64) > MAX_PIXEL_COUNT {
            return Err(AppError::BadRequest(format!(
                "Image pixel count {} exceeds maximum allowed {}",
                (w as u64) * (h as u64),
                MAX_PIXEL_COUNT
            )));
        }

        Ok(img)
    }

    /// Apply all transformations
    fn apply(&self, mut img: DynamicImage) -> AppResult<DynamicImage> {
        // 1. Manual crop region (before resize)
        if let Some(region) = &self.crop_region {
            let (iw, ih) = (img.width(), img.height());
            let x = region.x.min(iw.saturating_sub(1));
            let y = region.y.min(ih.saturating_sub(1));
            let w = region.width.min(iw - x);
            let h = region.height.min(ih - y);
            img = img.crop_imm(x, y, w, h);
        }

        // 2. Resize
        if self.width.is_some() || self.height.is_some() {
            img = self.apply_resize(img)?;
        }

        // 3. Rotation
        if let Some(rotation) = &self.rotation {
            img = match rotation {
                Rotation::Rotate90 => img.rotate90(),
                Rotation::Rotate180 => img.rotate180(),
                Rotation::Rotate270 => img.rotate270(),
            };
        }

        // 4. Flip
        if let Some(flip) = &self.flip {
            img = match flip {
                FlipDirection::Horizontal => img.fliph(),
                FlipDirection::Vertical => img.flipv(),
            };
        }

        // 5. Filters
        if self.grayscale {
            img = img.grayscale();
        }

        if let Some(sigma) = self.blur {
            img = img.blur(sigma);
        }

        if let Some(b) = self.brightness {
            img = img.brighten(b);
        }

        if let Some(c) = self.contrast {
            img = img.adjust_contrast(c);
        }

        // 6. Sharpen
        if let Some(sigma) = self.sharpen {
            img = img.unsharpen(sigma, 1);
        }

        // 7. Watermark
        if let Some(wm) = &self.watermark {
            img = self.apply_watermark(img, wm)?;
        }

        Ok(img)
    }

    /// Apply watermark overlay
    fn apply_watermark(&self, mut base: DynamicImage, wm: &Watermark) -> AppResult<DynamicImage> {
        let wm_img = ImageReader::open(&wm.path)
            .map_err(|e| AppError::Internal(format!("Failed to open watermark: {}", e)))?
            .decode()
            .map_err(|e| AppError::Internal(format!("Failed to decode watermark: {}", e)))?;

        // Scale watermark relative to base image width
        let wm_target_w = (base.width() * wm.scale_percent) / 100;
        let wm_ratio = wm_target_w as f64 / wm_img.width() as f64;
        let wm_target_h = (wm_img.height() as f64 * wm_ratio).round() as u32;

        let wm_resized = wm_img.resize_exact(
            wm_target_w.max(1),
            wm_target_h.max(1),
            imageops::FilterType::Lanczos3,
        );

        // Apply opacity
        let mut wm_rgba = wm_resized.to_rgba8();
        if wm.opacity < 100 {
            let alpha_factor = wm.opacity as f32 / 100.0;
            for pixel in wm_rgba.pixels_mut() {
                pixel[3] = (pixel[3] as f32 * alpha_factor).round() as u8;
            }
        }

        // Compute position
        let (x, y) = self.compute_crop_offset(
            base.width(),
            base.height(),
            wm_rgba.width() + wm.margin * 2,
            wm_rgba.height() + wm.margin * 2,
        );
        let x = x + wm.margin;
        let y = y + wm.margin;

        // Overlay
        imageops::overlay(&mut base, &DynamicImage::ImageRgba8(wm_rgba), x as i64, y as i64);

        Ok(base)
    }

    /// Apply resize based on mode
    fn apply_resize(&self, img: DynamicImage) -> AppResult<DynamicImage> {
        let (src_w, src_h) = (img.width(), img.height());
        let mut target_w = self.width.unwrap_or(src_w);
        let mut target_h = self.height.unwrap_or(src_h);

        // Prevent upscaling unless explicitly allowed
        if !self.upscale {
            target_w = target_w.min(src_w);
            target_h = target_h.min(src_h);
            // If both are at source size, skip resize entirely
            if target_w == src_w && target_h == src_h {
                return Ok(img);
            }
        }

        let result = match self.resize_mode {
            ResizeMode::Fit => {
                img.resize(target_w, target_h, imageops::FilterType::Lanczos3)
            }
            ResizeMode::Stretch => {
                img.resize_exact(target_w, target_h, imageops::FilterType::Lanczos3)
            }
            ResizeMode::Width => {
                let ratio = target_w as f64 / src_w as f64;
                let new_h = (src_h as f64 * ratio).round() as u32;
                img.resize_exact(target_w, new_h.max(1), imageops::FilterType::Lanczos3)
            }
            ResizeMode::Height => {
                let ratio = target_h as f64 / src_h as f64;
                let new_w = (src_w as f64 * ratio).round() as u32;
                img.resize_exact(new_w.max(1), target_h, imageops::FilterType::Lanczos3)
            }
            ResizeMode::Cover => {
                self.apply_cover_resize(img, target_w, target_h)?
            }
        };

        Ok(result)
    }

    /// Cover resize: scale to cover area then crop to exact size
    fn apply_cover_resize(
        &self,
        img: DynamicImage,
        target_w: u32,
        target_h: u32,
    ) -> AppResult<DynamicImage> {
        let (src_w, src_h) = (img.width(), img.height());
        let scale_w = target_w as f64 / src_w as f64;
        let scale_h = target_h as f64 / src_h as f64;
        let scale = scale_w.max(scale_h);

        let scaled_w = (src_w as f64 * scale).ceil() as u32;
        let scaled_h = (src_h as f64 * scale).ceil() as u32;

        let resized = img.resize_exact(
            scaled_w.max(1),
            scaled_h.max(1),
            imageops::FilterType::Lanczos3,
        );

        // Calculate crop position based on anchor
        let (crop_x, crop_y) = self.compute_crop_offset(
            scaled_w,
            scaled_h,
            target_w.min(scaled_w),
            target_h.min(scaled_h),
        );

        Ok(resized.crop_imm(
            crop_x,
            crop_y,
            target_w.min(scaled_w),
            target_h.min(scaled_h),
        ))
    }

    /// Compute crop offset based on anchor
    fn compute_crop_offset(
        &self,
        src_w: u32,
        src_h: u32,
        target_w: u32,
        target_h: u32,
    ) -> (u32, u32) {
        let max_x = src_w.saturating_sub(target_w);
        let max_y = src_h.saturating_sub(target_h);

        match self.crop_anchor {
            CropAnchor::TopLeft => (0, 0),
            CropAnchor::TopCenter => (max_x / 2, 0),
            CropAnchor::TopRight => (max_x, 0),
            CropAnchor::CenterLeft => (0, max_y / 2),
            CropAnchor::Center => (max_x / 2, max_y / 2),
            CropAnchor::CenterRight => (max_x, max_y / 2),
            CropAnchor::BottomLeft => (0, max_y),
            CropAnchor::BottomCenter => (max_x / 2, max_y),
            CropAnchor::BottomRight => (max_x, max_y),
        }
    }

    /// Save processed image to file
    fn save(&self, img: &DynamicImage, dest: &Path) -> AppResult<()> {
        match &self.output_format {
            OutputFormat::Jpeg { quality } => {
                let file = std::fs::File::create(dest)
                    .map_err(|e| AppError::Internal(format!("Failed to create file: {}", e)))?;
                let encoder = JpegEncoder::new_with_quality(file, *quality);
                img.write_with_encoder(encoder)
                    .map_err(|e| AppError::Internal(format!("Failed to encode JPEG: {}", e)))?;
            }
            OutputFormat::Png { compression } => {
                let file = std::fs::File::create(dest)
                    .map_err(|e| AppError::Internal(format!("Failed to create file: {}", e)))?;
                let (comp, filter) = match compression {
                    PngCompression::Fast => (CompressionType::Fast, FilterType::NoFilter),
                    PngCompression::Default => (CompressionType::Default, FilterType::Adaptive),
                    PngCompression::Best => (CompressionType::Best, FilterType::Adaptive),
                };
                let encoder = PngEncoder::new_with_quality(file, comp, filter);
                img.write_with_encoder(encoder)
                    .map_err(|e| AppError::Internal(format!("Failed to encode PNG: {}", e)))?;
            }
            OutputFormat::WebP | OutputFormat::Auto => {
                let format = match &self.output_format {
                    OutputFormat::WebP => ImageFormat::WebP,
                    _ => ImageFormat::from_path(dest).unwrap_or(ImageFormat::Jpeg),
                };
                img.save_with_format(dest, format)
                    .map_err(|e| AppError::Internal(format!("Failed to save image: {}", e)))?;
            }
            OutputFormat::Gif => {
                img.save_with_format(dest, ImageFormat::Gif)
                    .map_err(|e| AppError::Internal(format!("Failed to save GIF: {}", e)))?;
            }
        }
        Ok(())
    }

    /// Encode processed image to bytes
    fn encode(&self, img: &DynamicImage, dest_ext: &str) -> AppResult<Vec<u8>> {
        let mut buf = Vec::new();
        let cursor = Cursor::new(&mut buf);

        match &self.output_format {
            OutputFormat::Jpeg { quality } => {
                let encoder = JpegEncoder::new_with_quality(cursor, *quality);
                img.write_with_encoder(encoder)
                    .map_err(|e| AppError::Internal(format!("Failed to encode JPEG: {}", e)))?;
            }
            OutputFormat::Png { compression } => {
                let (comp, filter) = match compression {
                    PngCompression::Fast => (CompressionType::Fast, FilterType::NoFilter),
                    PngCompression::Default => (CompressionType::Default, FilterType::Adaptive),
                    PngCompression::Best => (CompressionType::Best, FilterType::Adaptive),
                };
                let encoder = PngEncoder::new_with_quality(cursor, comp, filter);
                img.write_with_encoder(encoder)
                    .map_err(|e| AppError::Internal(format!("Failed to encode PNG: {}", e)))?;
            }
            OutputFormat::WebP => {
                img.write_to(&mut Cursor::new(&mut buf), ImageFormat::WebP)
                    .map_err(|e| AppError::Internal(format!("Failed to encode WebP: {}", e)))?;
            }
            OutputFormat::Gif => {
                img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Gif)
                    .map_err(|e| AppError::Internal(format!("Failed to encode GIF: {}", e)))?;
            }
            OutputFormat::Auto => {
                let format = match dest_ext.to_lowercase().as_str() {
                    "jpg" | "jpeg" => ImageFormat::Jpeg,
                    "png" => ImageFormat::Png,
                    "webp" => ImageFormat::WebP,
                    "gif" => ImageFormat::Gif,
                    _ => ImageFormat::Jpeg,
                };
                img.write_to(&mut Cursor::new(&mut buf), format)
                    .map_err(|e| AppError::Internal(format!("Failed to encode image: {}", e)))?;
            }
        }

        Ok(buf)
    }
}

/// Simple shortcut: generate a thumbnail (backward compatible)
pub fn generate_thumbnail(
    source: &Path,
    dest: &Path,
    width: u32,
    height: u32,
) -> AppResult<()> {
    ImageProcessor::new()
        .resize(width, height)
        .mode(ResizeMode::Fit)
        .process(source, dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crop_offset_center() {
        let processor = ImageProcessor::new();
        let (x, y) = processor.compute_crop_offset(200, 200, 100, 100);
        assert_eq!(x, 50);
        assert_eq!(y, 50);
    }

    #[test]
    fn test_crop_offset_top_left() {
        let processor = ImageProcessor::new().anchor(CropAnchor::TopLeft);
        let (x, y) = processor.compute_crop_offset(200, 200, 100, 100);
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }

    #[test]
    fn test_crop_offset_bottom_right() {
        let processor = ImageProcessor::new().anchor(CropAnchor::BottomRight);
        let (x, y) = processor.compute_crop_offset(200, 200, 100, 100);
        assert_eq!(x, 100);
        assert_eq!(y, 100);
    }

    #[test]
    fn test_builder_chain() {
        let processor = ImageProcessor::new()
            .resize(800, 600)
            .mode(ResizeMode::Cover)
            .anchor(CropAnchor::Center)
            .jpeg(85)
            .grayscale()
            .blur(1.5);

        assert_eq!(processor.width, Some(800));
        assert_eq!(processor.height, Some(600));
        assert!(processor.grayscale);
        assert_eq!(processor.blur, Some(1.5));
    }

    #[test]
    fn test_width_only_mode() {
        let processor = ImageProcessor::new().width(400);
        assert_eq!(processor.width, Some(400));
        assert!(matches!(processor.resize_mode, ResizeMode::Width));
    }

    #[test]
    fn test_height_only_mode() {
        let processor = ImageProcessor::new().height(300);
        assert_eq!(processor.height, Some(300));
        assert!(matches!(processor.resize_mode, ResizeMode::Height));
    }

    #[test]
    fn test_brightness_clamped() {
        let processor = ImageProcessor::new().brightness(200);
        assert_eq!(processor.brightness, Some(100));

        let processor = ImageProcessor::new().brightness(-200);
        assert_eq!(processor.brightness, Some(-100));
    }

    #[test]
    fn test_contrast_clamped() {
        let processor = ImageProcessor::new().contrast(200.0);
        assert_eq!(processor.contrast, Some(100.0));
    }

    #[test]
    fn test_jpeg_quality_clamped() {
        let processor = ImageProcessor::new().jpeg(150);
        assert!(matches!(processor.output_format, OutputFormat::Jpeg { quality: 100 }));
    }

    #[test]
    fn test_default_format_is_auto() {
        let processor = ImageProcessor::new();
        assert!(matches!(processor.output_format, OutputFormat::Auto));
    }
}
