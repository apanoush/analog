//! In-memory image type and file decode/encode.
//!
//! Images are 8-bit (`Rgba8`) or 16-bit (`Rgba16`) RGBA. The GPU passes upload
//! both layouts as packed `u32` words: one word per pixel for `Rgba8`, two words
//! (R|G, B|A) per pixel for `Rgba16`.

use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageBuffer, ImageFormat, RgbImage, Rgba, RgbaImage};

use crate::error::{Error, Result};

/// 16-bit RGBA image buffer (the crate's alias is not public).
type Rgba16Image = ImageBuffer<Rgba<u16>, Vec<u16>>;

/// Pixel layout of an [`Image`] buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8-bit RGBA, one `u32` word per pixel.
    Rgba8,
    /// 16-bit RGBA, two `u32` words per pixel (R|G, B|A).
    Rgba16,
}

impl PixelFormat {
    /// Bytes per pixel in the CPU buffer.
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8 => 4,
            Self::Rgba16 => 8,
        }
    }

    /// `u32` words per pixel in the GPU buffer.
    pub fn words_per_pixel(self) -> u32 {
        match self {
            Self::Rgba8 => 1,
            Self::Rgba16 => 2,
        }
    }

    /// The bit depth of one channel.
    pub fn bit_depth(self) -> u32 {
        match self {
            Self::Rgba8 => 8,
            Self::Rgba16 => 16,
        }
    }
}

/// A decoded image in RGBA, 8- or 16-bit, row-major, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    format: PixelFormat,
    /// `width * height * format.bytes_per_pixel()` bytes, little-endian.
    data: Vec<u8>,
}

impl Image {
    /// Build an 8-bit image from raw RGBA bytes, validating the buffer length.
    pub fn from_rgba8(width: u32, height: u32, data: Vec<u8>) -> Result<Self> {
        Self::from_bytes(width, height, PixelFormat::Rgba8, data)
    }

    /// Build a 16-bit image from raw RGBA samples.
    pub fn from_rgba16(width: u32, height: u32, data: Vec<u16>) -> Result<Self> {
        let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();
        Self::from_bytes(width, height, PixelFormat::Rgba16, bytes)
    }

    pub(crate) fn from_bytes(
        width: u32,
        height: u32,
        format: PixelFormat,
        data: Vec<u8>,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(Error::Image("image dimensions must be non-zero".into()));
        }
        let expected = width as usize * height as usize * format.bytes_per_pixel();
        if data.len() != expected {
            return Err(Error::Image(format!(
                "expected {expected} bytes for {width}x{height} {format:?}, got {}",
                data.len()
            )));
        }
        Ok(Self {
            width,
            height,
            format,
            data,
        })
    }

    /// Image width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Image height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Pixel layout.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// Raw bytes (`width * height * bytes_per_pixel`).
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consume the image, returning its raw byte buffer.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    /// The buffer as `u32` words in GPU layout.
    pub fn as_words(&self) -> Vec<u32> {
        let pixels = (self.width * self.height) as usize;
        let mut words = Vec::with_capacity(pixels * self.format.words_per_pixel() as usize);
        match self.format {
            PixelFormat::Rgba8 => {
                for px in self.data.chunks_exact(4) {
                    words.push(
                        px[0] as u32
                            | (px[1] as u32) << 8
                            | (px[2] as u32) << 16
                            | (px[3] as u32) << 24,
                    );
                }
            }
            PixelFormat::Rgba16 => {
                for px in self.data.chunks_exact(8) {
                    let r = u16::from_le_bytes([px[0], px[1]]) as u32;
                    let g = u16::from_le_bytes([px[2], px[3]]) as u32;
                    let b = u16::from_le_bytes([px[4], px[5]]) as u32;
                    let a = u16::from_le_bytes([px[6], px[7]]) as u32;
                    words.push(r | (g << 16));
                    words.push(b | (a << 16));
                }
            }
        }
        words
    }

    /// Convert to 8-bit RGBA (16-bit is scaled down by truncation).
    pub fn to_rgba8(&self) -> Image {
        match self.format {
            PixelFormat::Rgba8 => self.clone(),
            PixelFormat::Rgba16 => {
                let out: Vec<u8> = self
                    .data
                    .chunks_exact(2)
                    .map(|c| (u16::from_le_bytes([c[0], c[1]]) >> 8) as u8)
                    .collect();
                Image {
                    width: self.width,
                    height: self.height,
                    format: PixelFormat::Rgba8,
                    data: out,
                }
            }
        }
    }

    /// Convert to 16-bit RGBA (8-bit is expanded by `v * 257`).
    pub fn to_rgba16(&self) -> Image {
        match self.format {
            PixelFormat::Rgba16 => self.clone(),
            PixelFormat::Rgba8 => {
                let mut out = Vec::with_capacity(self.data.len() * 2);
                for &b in &self.data {
                    out.extend_from_slice(&((b as u16 * 257).to_le_bytes()));
                }
                Image {
                    width: self.width,
                    height: self.height,
                    format: PixelFormat::Rgba16,
                    data: out,
                }
            }
        }
    }

    /// A downscaled copy that fits within `max_dim` on its longest side.
    pub fn thumbnail(&self, max_dim: u32) -> Self {
        if self.width <= max_dim && self.height <= max_dim {
            return self.clone();
        }
        let scale = max_dim as f32 / self.width.max(self.height) as f32;
        let nw = ((self.width as f32 * scale).round() as u32).max(1);
        let nh = ((self.height as f32 * scale).round() as u32).max(1);

        let decoded = match self.format {
            PixelFormat::Rgba8 => DynamicImage::ImageRgba8(
                RgbaImage::from_raw(self.width, self.height, self.data.clone())
                    .expect("consistent RGBA buffer"),
            ),
            PixelFormat::Rgba16 => DynamicImage::ImageRgba16(
                Rgba16Image::from_raw(self.width, self.height, self.samples16())
                    .expect("consistent RGBA buffer"),
            ),
        };
        let resized = decoded.resize_exact(nw, nh, image::imageops::FilterType::Triangle);
        match self.format {
            PixelFormat::Rgba8 => Image {
                width: nw,
                height: nh,
                format: PixelFormat::Rgba8,
                data: resized.to_rgba8().into_raw(),
            },
            PixelFormat::Rgba16 => {
                let raw: Vec<u8> = resized
                    .to_rgba16()
                    .into_raw()
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect();
                Image {
                    width: nw,
                    height: nh,
                    format: PixelFormat::Rgba16,
                    data: raw,
                }
            }
        }
    }

    /// The buffer as 16-bit samples (little-endian pairs).
    fn samples16(&self) -> Vec<u16> {
        self.data
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect()
    }

    /// Decode an image file, preserving 16-bit depth when present.
    ///
    /// PSD files are decoded to their flattened (composited) 8-bit RGBA image.
    /// PSB is not supported by the reader. Camera RAW files are developed to
    /// 16-bit sRGB RGBA when the `raw` feature is enabled (see [`crate::raw`]).
    pub fn decode(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if is_psd_path(path) {
            let bytes = std::fs::read(path).map_err(|source| Error::Io {
                path: path.to_path_buf(),
                source,
            })?;
            return decode_psd(&bytes);
        }
        if crate::raw::is_raw_path(path) {
            return crate::raw::decode_raw(path);
        }
        let reader = image::ImageReader::open(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let decoded = reader.decode().map_err(|source| Error::ImageIo {
            path: path.to_path_buf(),
            source,
        })?;

        if is_16_bit(decoded.color()) {
            let rgba = decoded.to_rgba16();
            let (w, h) = rgba.dimensions();
            let bytes: Vec<u8> = rgba
                .into_raw()
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect();
            Image::from_bytes(w, h, PixelFormat::Rgba16, bytes)
        } else {
            let rgba = decoded.to_rgba8();
            let (w, h) = rgba.dimensions();
            Image::from_rgba8(w, h, rgba.into_raw())
        }
    }

    /// Encode the image to `path`, inferring the format from the extension.
    ///
    /// 16-bit data is written as 16-bit PNG/TIFF; formats without 16-bit support
    /// (JPEG, WebP) fall back to 8-bit. Formats without alpha (JPEG) composite
    /// over white; WebP keeps its alpha channel.
    pub fn encode(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        if is_psd_path(path) {
            return Err(Error::Image(
                "PSD/PSB export is not implemented; use PNG or TIFF".into(),
            ));
        }
        if crate::raw::is_raw_path(path) {
            return Err(Error::Image(
                "camera RAW is input only; export as PNG, JPEG, TIFF or WebP".into(),
            ));
        }
        let format = format_from_path(path)?;

        let output = match (self.format, format) {
            (PixelFormat::Rgba16, ImageFormat::Png) | (PixelFormat::Rgba16, ImageFormat::Tiff) => {
                DynamicImage::ImageRgba16(
                    Rgba16Image::from_raw(self.width, self.height, self.samples16())
                        .ok_or_else(|| Error::Image("inconsistent RGBA16 buffer".into()))?,
                )
            }
            _ => {
                let eight = self.to_rgba8();
                let rgba = RgbaImage::from_raw(eight.width, eight.height, eight.data)
                    .ok_or_else(|| Error::Image("inconsistent RGBA buffer".into()))?;
                match format {
                    ImageFormat::Jpeg => DynamicImage::ImageRgb8(composite_over_white(&rgba)),
                    _ => DynamicImage::ImageRgba8(rgba),
                }
            }
        };

        output.save_with_format(path, format).map_err(|source| {
            if let image::ImageError::IoError(io) = &source {
                Error::Io {
                    path: path.to_path_buf(),
                    source: std::io::Error::new(io.kind(), io.to_string()),
                }
            } else {
                Error::ImageIo {
                    path: path.to_path_buf(),
                    source,
                }
            }
        })
    }
}

/// The luminance range of an opaque image, in 0..1, using BT.709 weights.
///
/// Fully transparent pixels are ignored. A degenerate range falls back to
/// `(0.0, 1.0)` so the grain zones can still use absolute luminance.
pub fn luma_range(image: &Image) -> (f32, f32) {
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;

    match image.format() {
        PixelFormat::Rgba8 => {
            for px in image.data.as_chunks::<4>().0 {
                if px[3] == 0 {
                    continue;
                }
                let luma = 0.2126 * px[0] as f32 + 0.7152 * px[1] as f32 + 0.0722 * px[2] as f32;
                let luma = luma / 255.0;
                min = min.min(luma);
                max = max.max(luma);
            }
        }
        PixelFormat::Rgba16 => {
            for px in image.data.as_chunks::<8>().0 {
                let r = u16::from_le_bytes([px[0], px[1]]) as f32 / 65535.0;
                let g = u16::from_le_bytes([px[2], px[3]]) as f32 / 65535.0;
                let b = u16::from_le_bytes([px[4], px[5]]) as f32 / 65535.0;
                let a = u16::from_le_bytes([px[6], px[7]]);
                if a == 0 {
                    continue;
                }
                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                min = min.min(luma);
                max = max.max(luma);
            }
        }
    }

    if !min.is_finite() || !max.is_finite() || max - min < 0.01 {
        return (0.0, 1.0);
    }
    (min, max)
}

fn is_16_bit(color: image::ColorType) -> bool {
    use image::ColorType::*;
    matches!(color, L16 | La16 | Rgb16 | Rgba16 | Rgb32F | Rgba32F)
}

/// Whether a path names a PSD/PSB file (handled by the `psd` reader).
fn is_psd_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("psd") || e.eq_ignore_ascii_case("psb"))
        .unwrap_or(false)
}

/// Decode a PSD to its flattened (composited) 8-bit RGBA image.
///
/// The `psd` crate can panic on some files, so the call is isolated with
/// `catch_unwind` and surfaced as an error instead of crashing the process.
fn decode_psd(bytes: &[u8]) -> Result<Image> {
    let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let psd = psd::Psd::from_bytes(bytes).map_err(|e| Error::Image(format!("psd: {e}")))?;
        let (w, h) = (psd.width(), psd.height());
        let rgba = psd
            .flatten_layers_rgba(&|_| true)
            .map_err(|e| Error::Image(format!("psd: {e}")))?;
        Image::from_rgba8(w, h, rgba)
    }));
    match decoded {
        Ok(result) => result,
        Err(_) => Err(Error::Image(
            "psd: the decoder panicked on this file (unsupported PSD variant)".into(),
        )),
    }
}

/// Flatten RGBA over an opaque white background into RGB.
fn composite_over_white(rgba: &RgbaImage) -> RgbImage {
    let (w, h) = rgba.dimensions();
    let mut rgb = RgbImage::new(w, h);
    for (dst, src) in rgb.pixels_mut().zip(rgba.pixels()) {
        let [r, g, b, a] = src.0;
        let alpha = a as u16;
        let blend = |c: u8| -> u8 { (((c as u16 * alpha) + 255 * (255 - alpha)) / 255) as u8 };
        dst.0 = [blend(r), blend(g), blend(b)];
    }
    rgb
}

/// Infer an [`ImageFormat`] from a path's extension.
///
/// Supported for output: JPEG, PNG, TIFF and WebP. WebP is written lossless
/// (the `image` encoder has no lossy mode). PSD/PSB and camera RAW are
/// input-only.
pub fn format_from_path(path: &Path) -> Result<ImageFormat> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .ok_or_else(|| Error::UnknownFormat(path.to_path_buf()))?;
    match ext.as_str() {
        "jpg" | "jpeg" => Ok(ImageFormat::Jpeg),
        "png" => Ok(ImageFormat::Png),
        "tif" | "tiff" => Ok(ImageFormat::Tiff),
        "webp" => Ok(ImageFormat::WebP),
        _ => Err(Error::UnknownFormat(PathBuf::from(path))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_rgba8_validates_length() {
        assert!(Image::from_rgba8(2, 2, vec![0; 16]).is_ok());
        assert!(Image::from_rgba8(2, 2, vec![0; 15]).is_err());
        assert!(Image::from_rgba8(0, 2, vec![]).is_err());
    }

    #[test]
    fn format_inference() {
        assert_eq!(
            format_from_path(Path::new("a/b.JPG")).unwrap(),
            ImageFormat::Jpeg
        );
        assert_eq!(
            format_from_path(Path::new("a/b.tiff")).unwrap(),
            ImageFormat::Tiff
        );
        assert_eq!(
            format_from_path(Path::new("a/b.WEBP")).unwrap(),
            ImageFormat::WebP
        );
        assert!(format_from_path(Path::new("a/b.psd")).is_err());
    }

    #[test]
    fn words_layout_matches_shader_packing() {
        let img = Image::from_rgba8(1, 1, vec![1, 2, 3, 4]).unwrap();
        assert_eq!(img.as_words(), vec![1 | 2 << 8 | 3 << 16 | 4 << 24]);
        let img16 = Image::from_rgba16(1, 1, vec![0x0102, 0x0304, 0x0506, 0x0708]).unwrap();
        assert_eq!(
            img16.as_words(),
            vec![0x0102 | 0x0304 << 16, 0x0506 | 0x0708 << 16]
        );
    }

    #[test]
    fn depth_conversions_roundtrip_opaque() {
        let img = Image::from_rgba8(1, 1, vec![10, 20, 30, 255]).unwrap();
        let back = img.to_rgba16().to_rgba8();
        assert_eq!(back, img);
    }

    #[test]
    fn jpeg_flattens_alpha_over_white() {
        let img = Image::from_rgba8(2, 1, vec![10, 20, 30, 255, 10, 20, 30, 0]).unwrap();
        let path = std::env::temp_dir().join("analog_core_flatten.jpg");
        img.encode(&path).unwrap();
        let back = Image::decode(&path).unwrap();
        let px = back.data();
        assert!(px[0].abs_diff(10) <= 2 && px[3] == 255);
        assert!(px[4] >= 250 && px[5] >= 250 && px[6] >= 250);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn psd_paths_are_detected() {
        assert!(is_psd_path(Path::new("a/b.psd")));
        assert!(is_psd_path(Path::new("a/b.PSB")));
        assert!(!is_psd_path(Path::new("a/b.png")));
    }

    #[test]
    fn psd_export_is_rejected() {
        let img = Image::from_rgba8(1, 1, vec![0, 0, 0, 255]).unwrap();
        assert!(
            img.encode(Path::new("/tmp/analog_should_not_write.psd"))
                .is_err()
        );
    }

    #[test]
    fn raw_export_is_rejected() {
        let img = Image::from_rgba8(1, 1, vec![0, 0, 0, 255]).unwrap();
        assert!(
            img.encode(Path::new("/tmp/analog_should_not_write.cr2"))
                .is_err()
        );
    }

    #[test]
    fn roundtrip_png() {
        let img = Image::from_rgba8(3, 2, (0..24).map(|i| (i * 7) as u8).collect()).unwrap();
        let path = std::env::temp_dir().join("analog_core_roundtrip.png");
        img.encode(&path).unwrap();
        assert_eq!(Image::decode(&path).unwrap(), img);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_webp() {
        // WebP output is lossless, so an 8-bit RGBA image round-trips exactly.
        let img = Image::from_rgba8(3, 2, (0..24).map(|i| (i * 9) as u8).collect()).unwrap();
        let path = std::env::temp_dir().join("analog_core_roundtrip.webp");
        img.encode(&path).unwrap();
        assert_eq!(Image::decode(&path).unwrap(), img);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_png16() {
        let data: Vec<u16> = (0..12).map(|i| (i * 5000) as u16).collect();
        let img = Image::from_rgba16(3, 1, data).unwrap();
        let path = std::env::temp_dir().join("analog_core_roundtrip16.png");
        img.encode(&path).unwrap();
        let back = Image::decode(&path).unwrap();
        assert_eq!(back.format(), PixelFormat::Rgba16);
        assert_eq!(back, img);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn thumbnail_downscales_within_bounds() {
        let img = Image::from_rgba8(100, 50, vec![128; 100 * 50 * 4]).unwrap();
        let small = img.thumbnail(40);
        assert_eq!(small.width(), 40);
        assert_eq!(small.height(), 20);
        assert_eq!(small.format(), PixelFormat::Rgba8);
        let same = Image::from_rgba8(10, 10, vec![0; 10 * 10 * 4]).unwrap();
        assert_eq!(same.thumbnail(40), same);
    }

    #[test]
    fn luma_range_ignores_transparent_pixels() {
        let data = vec![
            0, 0, 0, 255, // black
            255, 255, 255, 255, // white
            0, 0, 0, 0, // transparent, must not widen the range
        ];
        let img = Image::from_rgba8(3, 1, data).unwrap();
        let (min, max) = luma_range(&img);
        assert!(min.abs() < 1e-6, "min {min}");
        assert!((max - 1.0).abs() < 1e-6, "max {max}");
    }

    #[test]
    fn luma_range_falls_back_for_flat_or_empty_images() {
        let flat = Image::from_rgba8(2, 1, vec![128; 8]).unwrap();
        assert_eq!(luma_range(&flat), (0.0, 1.0));
        let empty = Image::from_rgba8(1, 1, vec![0, 0, 0, 0]).unwrap();
        assert_eq!(luma_range(&empty), (0.0, 1.0));
    }

    #[test]
    fn luma_range_reads_16_bit_samples() {
        let data = vec![0u16, 0, 0, 65535, 65535, 65535, 65535, 65535];
        let img = Image::from_rgba16(2, 1, data).unwrap();
        let (min, max) = luma_range(&img);
        assert!(min.abs() < 1e-6, "min {min}");
        assert!((max - 1.0).abs() < 1e-6, "max {max}");
    }
}
