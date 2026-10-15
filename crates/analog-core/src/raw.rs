//! Camera RAW decoding (input only) through LibRaw.
//!
//! RAW files are recognised by extension and developed to 16-bit sRGB RGBA so
//! they enter the same pipeline as any other 16-bit input. LibRaw applies the
//! per-camera white balance, colour matrix and sRGB output transform, so the
//! result is a normal developed image rather than linear sensor data.
//!
//! RAW is never an output format: [`Image::encode`](crate::Image::encode)
//! rejects it. Decoding is behind the `raw` cargo feature; without it the
//! extension list and the encode rejection stay active and [`decode_raw`]
//! returns a clear error instead of attempting a decode.

use std::path::Path;

use crate::error::{Error, Result};
use crate::image::Image;

/// File extensions treated as camera RAW.
///
/// This is a superset of what any single LibRaw build accepts; extensions that
/// LibRaw does not know still surface as a decode error rather than a panic.
pub const RAW_EXTENSIONS: &[&str] = &[
    "3fr", "ari", "arw", "cr2", "cr3", "crw", "dcr", "dcs", "dng", "erf", "fff", "iiq", "kdc",
    "mef", "mos", "mrw", "nef", "nkd", "nrw", "orf", "pef", "qtk", "raf", "rw2", "rwl", "sr2",
    "srf", "srw", "tfr", "x3f",
];

/// Whether a path names a camera RAW file, by extension (case-insensitive).
pub fn is_raw_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            RAW_EXTENSIONS.contains(&e.as_str())
        })
        .unwrap_or(false)
}

/// Develop a RAW file to a 16-bit RGBA image via LibRaw.
///
/// Requires the `raw` feature. Errors from LibRaw (unsupported camera, corrupt
/// file) are surfaced as [`Error::Image`].
#[cfg(feature = "raw")]
pub fn decode_raw(path: &Path) -> Result<Image> {
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let processor = libraw::Processor::new();
    let processed = processor
        .process_16bit(&bytes)
        .map_err(|e| Error::Image(format!("raw: {e}")))?;
    let (width, height) = (processed.width(), processed.height());
    let pixels: &[u16] = &processed;
    rgba16_from_rgb(width, height, pixels)
}

/// Clear error when RAW support was not compiled in.
#[cfg(not(feature = "raw"))]
pub fn decode_raw(path: &Path) -> Result<Image> {
    Err(Error::Image(format!(
        "camera RAW support is not compiled in; rebuild with the `raw` feature to open {}",
        path.display()
    )))
}

/// Convert LibRaw's interleaved RGB(A) output into our packed RGBA16 buffer.
#[cfg(feature = "raw")]
fn rgba16_from_rgb(width: u32, height: u32, pixels: &[u16]) -> Result<Image> {
    let count = width as usize * height as usize;
    if count == 0 || !pixels.len().is_multiple_of(count) {
        return Err(Error::Image(format!(
            "raw: unexpected buffer of {} samples for {width}x{height}",
            pixels.len()
        )));
    }
    let channels = pixels.len() / count;
    if channels < 3 {
        return Err(Error::Image(format!(
            "raw: expected at least 3 colour channels, got {channels}"
        )));
    }
    let mut rgba = Vec::with_capacity(count * 4);
    for px in pixels.chunks_exact(channels) {
        rgba.push(px[0]);
        rgba.push(px[1]);
        rgba.push(px[2]);
        rgba.push(if channels >= 4 { px[3] } else { u16::MAX });
    }
    Image::from_rgba16(width, height, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_extensions_detected_case_insensitively() {
        assert!(is_raw_path(Path::new("a/b.CR2")));
        assert!(is_raw_path(Path::new("a/b.nef")));
        assert!(is_raw_path(Path::new("shot/photo.dng")));
        assert!(!is_raw_path(Path::new("a/b.png")));
        assert!(!is_raw_path(Path::new("a/b")));
    }

    #[cfg(feature = "raw")]
    #[test]
    fn rgb_buffer_becomes_opaque_rgba16() {
        // 2 pixels, 3 channels each.
        let img = rgba16_from_rgb(2, 1, &[1, 2, 3, 4, 5, 6]).unwrap();
        assert_eq!(img.format(), crate::PixelFormat::Rgba16);
        assert_eq!(
            img.data(),
            &[1u16, 2, 3, u16::MAX, 4, 5, 6, u16::MAX]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>()[..]
        );
    }

    #[cfg(feature = "raw")]
    #[test]
    fn mismatched_buffer_is_rejected() {
        assert!(rgba16_from_rgb(2, 1, &[1, 2, 3]).is_err());
    }
}
