//! Upload image processing: decode, normalize (orientation, alpha, EXIF) and resize.

use std::io::Cursor;
use std::sync::Arc;

use bytes::Bytes;
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageError as LibError, ImageFormat,
    ImageReader, Limits, RgbImage,
};
use tokio::sync::Semaphore;

use crate::error::AppError;
use crate::models::ImageKind;

pub const MAX_INPUT_BYTES: usize = 20 * 1024 * 1024;

const MAX_SIDE: u32 = 12_000;
const MAX_PIXELS: u64 = 50_000_000;
const MAX_ALLOC: u64 = 512 * 1024 * 1024;
/// Decoded size cap; bounds RSS for deep or alpha formats within the pixel limit.
const MAX_DECODE_BYTES: u64 = 150 * 1024 * 1024;

const COVER_FULL_SIDE: u32 = 1600;
const COVER_QUALITY: u8 = 82;
const SCREENSHOT_FULL_WIDTH: u32 = 1440;
const SCREENSHOT_QUALITY: u8 = 90;
const THUMB_SIZE: u32 = 400;
const THUMB_QUALITY: u8 = 80;

pub struct Processed {
    pub full: Vec<u8>,
    pub thumb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ImageError {
    #[error("unsupported image format")]
    UnsupportedFormat,
    #[error("image too large")]
    TooLarge,
    #[error("image could not be decoded")]
    Decode,
}

impl From<ImageError> for AppError {
    fn from(e: ImageError) -> Self {
        AppError::BadRequest(
            match e {
                ImageError::UnsupportedFormat => {
                    "Nepodporovaný formát obrázku (povoleno JPEG, PNG, WebP)"
                }
                ImageError::TooLarge => "Obrázek je příliš velký",
                ImageError::Decode => "Obrázek se nepodařilo načíst",
            }
            .to_string(),
        )
    }
}

fn map_lib_err(e: LibError) -> ImageError {
    match e {
        LibError::Limits(_) => ImageError::TooLarge,
        _ => ImageError::Decode,
    }
}

fn reader(bytes: &[u8]) -> Result<ImageReader<Cursor<&[u8]>>, ImageError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ImageError::Decode)?;
    match reader.format() {
        Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP) => Ok(reader),
        _ => Err(ImageError::UnsupportedFormat),
    }
}

/// Decodes (with limits), applies EXIF orientation and flattens alpha onto white.
fn decode(bytes: &[u8]) -> Result<RgbImage, ImageError> {
    let (w, h) = reader(bytes)?.into_dimensions().map_err(map_lib_err)?;
    if w > MAX_SIDE || h > MAX_SIDE || u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(ImageError::TooLarge);
    }

    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    let mut reader = reader(bytes)?;
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(map_lib_err)?;
    if decoder.total_bytes() > MAX_DECODE_BYTES {
        return Err(ImageError::TooLarge);
    }
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(map_lib_err)?;
    img.apply_orientation(orientation);

    if !img.color().has_alpha() {
        return Ok(img.into_rgb8());
    }
    let rgba = img.into_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (src, dst) in rgba.pixels().zip(out.pixels_mut()) {
        let a = u32::from(src[3]);
        for c in 0..3 {
            dst[c] = ((u32::from(src[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
    }
    Ok(out)
}

fn encode(img: &RgbImage, quality: u8) -> Result<Vec<u8>, ImageError> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, quality)
        .encode(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgb8,
        )
        .map_err(|_| ImageError::Decode)?;
    Ok(out)
}

fn resize(img: &RgbImage, w: u32, h: u32) -> RgbImage {
    image::imageops::resize(img, w, h, FilterType::Lanczos3)
}

/// Scales `v` by `num/den`, rounded, at least 1.
fn scale(v: u32, num: u32, den: u32) -> u32 {
    ((u64::from(v) * u64::from(num) + u64::from(den) / 2) / u64::from(den)).max(1) as u32
}

fn cover(img: RgbImage) -> (RgbImage, RgbImage) {
    let (w, h) = img.dimensions();
    let long = w.max(h);
    let full = if long > COVER_FULL_SIDE {
        resize(
            &img,
            scale(w, COVER_FULL_SIDE, long),
            scale(h, COVER_FULL_SIDE, long),
        )
    } else {
        img
    };

    // Center-cropped square, never upscaled.
    let (w, h) = full.dimensions();
    let side = w.min(h);
    let square =
        image::imageops::crop_imm(&full, (w - side) / 2, (h - side) / 2, side, side).to_image();
    let thumb = if side > THUMB_SIZE {
        resize(&square, THUMB_SIZE, THUMB_SIZE)
    } else {
        square
    };
    (full, thumb)
}

fn screenshot(img: RgbImage) -> (RgbImage, RgbImage) {
    let (w, h) = img.dimensions();
    let full = if w > SCREENSHOT_FULL_WIDTH {
        resize(
            &img,
            SCREENSHOT_FULL_WIDTH,
            scale(h, SCREENSHOT_FULL_WIDTH, w),
        )
    } else {
        img
    };

    // Top crop: scaled to thumb width, at most THUMB_SIZE tall.
    let (w, h) = full.dimensions();
    let scaled = if w > THUMB_SIZE {
        resize(&full, THUMB_SIZE, scale(h, THUMB_SIZE, w))
    } else {
        full.clone()
    };
    let crop_h = scaled.height().min(THUMB_SIZE);
    let thumb = image::imageops::crop_imm(&scaled, 0, 0, scaled.width(), crop_h).to_image();
    (full, thumb)
}

pub fn process(bytes: &[u8], kind: ImageKind) -> Result<Processed, ImageError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(ImageError::TooLarge);
    }
    let img = decode(bytes)?;
    let (full, thumb, quality) = match kind {
        ImageKind::Cover => {
            let (f, t) = cover(img);
            (f, t, COVER_QUALITY)
        }
        ImageKind::Screenshot => {
            let (f, t) = screenshot(img);
            (f, t, SCREENSHOT_QUALITY)
        }
    };
    Ok(Processed {
        width: full.width(),
        height: full.height(),
        full: encode(&full, quality)?,
        thumb: encode(&thumb, THUMB_QUALITY)?,
    })
}

/// Runs `process` on the blocking pool, bounded by `sem` to cap concurrent decodes.
/// The permit moves into the blocking task, so it is held until the decode really ends
/// even if the calling future is dropped.
pub async fn process_async(
    sem: &Arc<Semaphore>,
    bytes: Bytes,
    kind: ImageKind,
) -> Result<Processed, ImageError> {
    let permit = sem
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| ImageError::Decode)?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        process(&bytes, kind)
    })
    .await
    .map_err(|_| ImageError::Decode)?
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbaImage};

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, Rgb([200, 100, 50]));
        let mut out = Vec::new();
        JpegEncoder::new_with_quality(&mut out, 80)
            .encode(img.as_raw(), w, h, ExtendedColorType::Rgb8)
            .unwrap();
        out
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, Rgb([10, 20, 30]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn decode_out(bytes: &[u8]) -> DynamicImage {
        image::load_from_memory_with_format(bytes, ImageFormat::Jpeg).unwrap()
    }

    #[test]
    fn cover_downscales_longest_side() {
        let p = process(&jpeg(4000, 3000), ImageKind::Cover).unwrap();
        assert_eq!((p.width, p.height), (1600, 1200));
        let full = decode_out(&p.full);
        assert_eq!((full.width(), full.height()), (1600, 1200));
        let thumb = decode_out(&p.thumb);
        assert_eq!((thumb.width(), thumb.height()), (400, 400));
    }

    #[test]
    fn screenshot_keeps_width() {
        for ((w, h), (ew, eh)) in [
            ((1170, 2532), (1170, 2532)),
            ((1080, 9000), (1080, 9000)),
            ((2000, 12000), (1440, 8640)),
        ] {
            let p = process(&png(w, h), ImageKind::Screenshot).unwrap();
            assert_eq!((p.width, p.height), (ew, eh), "input {w}x{h}");
            let full = decode_out(&p.full);
            assert_eq!((full.width(), full.height()), (ew, eh));
            let thumb = decode_out(&p.thumb);
            assert!(thumb.width() <= 400 && thumb.height() <= 400);
        }
    }

    #[test]
    fn screenshot_thumb_is_top_crop() {
        let p = process(&png(1170, 2532), ImageKind::Screenshot).unwrap();
        let thumb = decode_out(&p.thumb);
        assert_eq!((thumb.width(), thumb.height()), (400, 400));
    }

    #[test]
    fn never_upscales() {
        for kind in [ImageKind::Cover, ImageKind::Screenshot] {
            let p = process(&jpeg(300, 200), kind).unwrap();
            assert_eq!((p.width, p.height), (300, 200));
            let thumb = decode_out(&p.thumb);
            assert!(thumb.width() <= 300 && thumb.height() <= 200);
        }
    }

    fn with_exif_orientation(jpeg: &[u8], orientation: u8) -> Vec<u8> {
        let mut seg = vec![0xFF, 0xE1, 0x00, 34];
        seg.extend_from_slice(b"Exif\0\0");
        seg.extend_from_slice(b"MM\0*\0\0\0\x08");
        seg.extend_from_slice(&[0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, orientation, 0, 0]);
        seg.extend_from_slice(&[0, 0, 0, 0]);
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&seg);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    #[test]
    fn applies_exif_orientation() {
        let input = with_exif_orientation(&jpeg(40, 20), 6);
        let p = process(&input, ImageKind::Cover).unwrap();
        assert_eq!((p.width, p.height), (20, 40));
    }

    #[test]
    fn output_has_no_exif() {
        let input = with_exif_orientation(&jpeg(40, 20), 6);
        assert!(input.windows(6).any(|w| w == b"Exif\0\0"));
        let p = process(&input, ImageKind::Cover).unwrap();
        for out in [&p.full, &p.thumb] {
            assert!(!out.windows(6).any(|w| w == b"Exif\0\0"));
        }
    }

    #[test]
    fn flattens_alpha_onto_white() {
        let img = RgbaImage::from_pixel(10, 10, image::Rgba([0, 0, 0, 0]));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        for kind in [ImageKind::Cover, ImageKind::Screenshot] {
            let p = process(buf.get_ref(), kind).unwrap();
            let px = decode_out(&p.full).to_rgb8().get_pixel(5, 5).0;
            assert!(px.iter().all(|&c| c >= 250), "got {px:?}");
        }
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(
            process(b"not an image", ImageKind::Cover).err(),
            Some(ImageError::UnsupportedFormat)
        );
        let j = jpeg(64, 64);
        assert_eq!(
            process(&j[..200.min(j.len())], ImageKind::Cover).err(),
            Some(ImageError::Decode)
        );
    }

    #[test]
    fn rejects_gif() {
        let gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";
        assert_eq!(
            process(gif, ImageKind::Cover).err(),
            Some(ImageError::UnsupportedFormat)
        );
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    }

    /// Valid PNG chunk structure claiming `w`x`h`, with an empty (undecodable) pixel stream.
    fn png_header(w: u32, h: u32) -> Vec<u8> {
        png_header_with(w, h, 8, 2)
    }

    fn png_header_with(w: u32, h: u32, depth: u8, color_type: u8) -> Vec<u8> {
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[depth, color_type, 0, 0, 0]);
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png_chunk(&mut out, b"IHDR", &ihdr);
        png_chunk(&mut out, b"IDAT", &[0x78, 0x9C, 0x03, 0, 0, 0, 0, 1]);
        png_chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn rejects_oversized_dimensions() {
        let input = png_header(30_000, 30_000);
        let dims = reader(&input).unwrap().into_dimensions().unwrap();
        assert_eq!(dims, (30_000, 30_000), "crafted header must parse");
        let start = std::time::Instant::now();
        assert_eq!(
            process(&input, ImageKind::Cover).err(),
            Some(ImageError::TooLarge)
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn rejects_too_many_pixels() {
        // Within the per-side and alloc limits, so only the pixel-count guard
        // can yield TooLarge (decoding the truncated data would give Decode).
        let input = png_header(10_000, 10_000);
        let dims = reader(&input).unwrap().into_dimensions().unwrap();
        assert_eq!(dims, (10_000, 10_000), "crafted header must parse");
        assert_eq!(
            process(&input, ImageKind::Cover).err(),
            Some(ImageError::TooLarge)
        );
    }

    #[test]
    fn rejects_large_decoded_size_within_pixel_limit() {
        // 7071x7071 16-bit RGBA: ~2 MB on disk as zeros, 400 MB decoded; only the
        // decoded-size guard can yield TooLarge (the empty stream would give Decode).
        let input = png_header_with(7071, 7071, 16, 6);
        assert!(u64::from(7071u32) * 7071 <= MAX_PIXELS);
        assert_eq!(
            process(&input, ImageKind::Cover).err(),
            Some(ImageError::TooLarge)
        );
    }

    #[test]
    fn accepts_12mp_images() {
        for input in [jpeg(4000, 3000), png(4000, 3000)] {
            let p = process(&input, ImageKind::Cover).unwrap();
            assert_eq!((p.width, p.height), (1600, 1200));
        }
    }

    #[test]
    fn rejects_input_over_20mb() {
        let input = vec![0u8; MAX_INPUT_BYTES + 1];
        assert_eq!(
            process(&input, ImageKind::Cover).err(),
            Some(ImageError::TooLarge)
        );
    }

    #[test]
    fn maps_to_czech_bad_request() {
        let e: AppError = ImageError::TooLarge.into();
        assert!(matches!(e, AppError::BadRequest(m) if m == "Obrázek je příliš velký"));
    }

    #[tokio::test]
    async fn process_async_works() {
        let sem = Arc::new(Semaphore::new(1));
        let p = process_async(&sem, Bytes::from(jpeg(100, 50)), ImageKind::Cover)
            .await
            .unwrap();
        assert_eq!((p.width, p.height), (100, 50));
    }

    #[tokio::test]
    async fn permit_outlives_dropped_caller() {
        let sem = Arc::new(Semaphore::new(1));
        let fut = process_async(&sem, Bytes::from(jpeg(3000, 3000)), ImageKind::Cover);
        // Poll until the blocking task owns the permit, then drop the caller.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(5), fut).await;
        assert_eq!(sem.available_permits(), 0, "permit must stay with the task");
        let _p = sem.acquire().await.unwrap();
    }
}
