//! Photo preparation: read, check, fix EXIF rotation, downscale, re-encode as JPEG.
//!
//! The vision model gains nothing from a 12 MP original, so the long edge is capped
//! at `MAX_EDGE`. Dimensions are checked before decoding, so a decompression bomb is
//! refused instead of allocated.
use crate::error::LensError;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader};
use std::io::Cursor;
use std::path::Path;

pub const MAX_FILE_BYTES: u64 = 40 * 1024 * 1024;
pub const MAX_EDGE: u32 = 1536;
pub const MAX_PIXELS: u64 = 120_000_000;
const JPEG_QUALITY: u8 = 88;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PreparedImage {
    /// JPEG bytes as sent to the model.
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub source_format: String,
}

/// Read and prepare a photo file.
pub fn prepare_file(path: &Path) -> Result<PreparedImage, LensError> {
    let meta = std::fs::metadata(path)
        .map_err(|e| LensError::InvalidInput(format!("cannot read {}: {e}", path.display())))?;
    if !meta.is_file() {
        return Err(LensError::InvalidInput(format!(
            "{} is not a file",
            path.display()
        )));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(LensError::InvalidInput(format!(
            "{} is {} MB; the limit is {} MB",
            path.display(),
            meta.len() / (1024 * 1024),
            MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| LensError::Io(format!("reading {}: {e}", path.display())))?;
    prepare_bytes(&bytes)
}

/// Container brands we can name but not decode.
fn unsupported_container(bytes: &[u8]) -> Option<&'static str> {
    let brand = bytes.get(8..12)?;
    if bytes.get(4..8)? != b"ftyp" {
        return None;
    }
    match brand {
        b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => {
            Some("HEIC/HEIF photos are not supported yet: share the photo as JPEG (iPhone: Settings > Camera > Formats > Most Compatible)")
        }
        b"avif" | b"avis" => Some("AVIF is not supported: use JPEG, PNG or WebP"),
        _ => None,
    }
}

/// Prepare photo bytes already in memory (uploads, tests).
pub fn prepare_bytes(bytes: &[u8]) -> Result<PreparedImage, LensError> {
    if bytes.is_empty() {
        return Err(LensError::InvalidInput("image is empty".to_string()));
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(LensError::InvalidInput(format!(
            "image is larger than {} MB",
            MAX_FILE_BYTES / (1024 * 1024)
        )));
    }
    if let Some(why) = unsupported_container(bytes) {
        return Err(LensError::Image(why.to_string()));
    }
    let not_image = |e: &dyn std::fmt::Display| {
        LensError::Image(format!("not a supported image (JPEG, PNG, WebP): {e}"))
    };
    let probe = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| not_image(&e))?;
    let source_format = probe
        .format()
        .map(|f| format!("{f:?}").to_lowercase())
        .unwrap_or_else(|| "unknown".to_string());
    let (w, h) = probe.into_dimensions().map_err(|e| not_image(&e))?;
    if w == 0 || h == 0 {
        return Err(LensError::Image(
            "image has zero width or height".to_string(),
        ));
    }
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(LensError::Image(format!(
            "image is {w}x{h}; the limit is {} megapixels",
            MAX_PIXELS / 1_000_000
        )));
    }
    let mut decoder = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| not_image(&e))?
        .into_decoder()
        .map_err(|e| not_image(&e))?;
    let orientation = decoder.orientation().map_err(|e| not_image(&e))?;
    let mut img = DynamicImage::from_decoder(decoder).map_err(|e| not_image(&e))?;
    img.apply_orientation(orientation);
    let (source_width, source_height) = (img.width(), img.height());
    if source_width.max(source_height) > MAX_EDGE {
        img = img.resize(MAX_EDGE, MAX_EDGE, FilterType::Triangle);
    }
    let rgb = img.to_rgb8();
    let mut jpeg = Vec::new();
    JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| LensError::Image(format!("jpeg encode failed: {e}")))?;
    Ok(PreparedImage {
        width: rgb.width(),
        height: rgb.height(),
        source_width,
        source_height,
        source_format,
        jpeg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb, RgbImage};

    fn png(w: u32, h: u32) -> Result<Vec<u8>, LensError> {
        let img = RgbImage::from_fn(w, h, |x, y| Rgb([(x % 256) as u8, (y % 256) as u8, 90]));
        let mut buf = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .map_err(|e| LensError::Image(e.to_string()))?;
        Ok(buf)
    }

    #[test]
    fn large_photo_is_downscaled_to_a_jpeg() -> Result<(), LensError> {
        let out = prepare_bytes(&png(3000, 2000)?)?;
        assert_eq!((out.width, out.height), (1536, 1024));
        assert_eq!((out.source_width, out.source_height), (3000, 2000));
        assert_eq!(out.source_format, "png");
        assert_eq!(out.jpeg.get(0..2), Some(&[0xFF, 0xD8][..]));
        Ok(())
    }

    #[test]
    fn small_photo_is_not_upscaled() -> Result<(), LensError> {
        let out = prepare_bytes(&png(400, 300)?)?;
        assert_eq!((out.width, out.height), (400, 300));
        Ok(())
    }

    #[test]
    fn garbage_is_an_image_error() {
        let out = prepare_bytes(b"this is not a picture at all");
        assert_eq!(out.map_err(|e| e.kind()), Err("image"));
    }

    #[test]
    fn heic_gets_a_clear_actionable_error() {
        let mut fake = vec![0, 0, 0, 24];
        fake.extend_from_slice(b"ftypheic");
        fake.extend_from_slice(&[0; 32]);
        let out = prepare_bytes(&fake);
        let msg = match &out {
            Err(LensError::Image(m)) => m.clone(),
            _ => String::new(),
        };
        assert!(msg.contains("HEIC") && msg.contains("JPEG"), "got {out:?}");
    }

    #[test]
    fn empty_missing_and_directory_inputs_are_invalid() {
        assert_eq!(prepare_bytes(&[]).map_err(|e| e.kind()), Err("invalid"));
        let missing = prepare_file(Path::new("definitely-not-here-xyz.jpg"));
        assert_eq!(missing.map_err(|e| e.kind()), Err("invalid"));
        let dir = prepare_file(&std::env::temp_dir());
        assert_eq!(dir.map_err(|e| e.kind()), Err("invalid"));
    }
}
