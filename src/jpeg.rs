//! JPEG decoding, orientation and color-profile handling for WebP conversion.

use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use crate::codec::Error;
#[cfg(feature = "webp")]
use crate::search::AUTO_CAP;
use crate::RGBA;

#[cfg(feature = "webp")]
const AUTO_AT_1MP: f64 = 72.3;
#[cfg(feature = "webp")]
const AUTO_SLOPE: f64 = 3.2;

/// A JPEG photo, decoded.
pub struct Photo {
    /// The pixels as RGBA (gray ones as gray RGBA), for scoring.
    pub pixels: Vec<RGBA>,
    pub width: usize,
    pub height: usize,
    /// Its ICC profile, if it has one.
    pub icc: Option<Vec<u8>>,
    /// Its EXIF orientation (1 if none).
    pub orientation: u16,
}

/// Whether a file is a JPEG: its start-of-image marker and the next marker's.
pub fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8, 0xff])
}

pub fn decode(bytes: &[u8]) -> Result<Photo, Error> {
    decode_limited(bytes, usize::MAX)
}

pub(crate) fn decode_limited(bytes: &[u8], max_pixels: usize) -> Result<Photo, Error> {
    // Retain the existing dimension limit without depending on the optional encoder.
    let options = DecoderOptions::default().set_max_width(65_500).set_max_height(65_500);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(unreadable)?;
    let (width, height) = decoder.dimensions().ok_or("not a JPEG pngdeer can read")?;
    if width.checked_mul(height).is_none_or(|pixels| pixels > max_pixels) {
        return Err("JPEG exceeds the pixel limit".into());
    }
    let gray = match decoder.input_colorspace() {
        Some(ColorSpace::Luma) => true,
        Some(ColorSpace::YCbCr | ColorSpace::RGB) => false,
        Some(ColorSpace::CMYK | ColorSpace::YCCK) => return Err("CMYK JPEGs aren't supported".into()),
        other => return Err(format!("JPEGs in {other:?} aren't supported").into()),
    };
    let icc = decoder.icc_profile();
    let orientation = decoder.exif().and_then(|exif| exif_orientation(exif)).unwrap_or(1);
    let options = decoder.options().jpeg_set_out_colorspace(if gray { ColorSpace::Luma } else { ColorSpace::RGB });
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    let samples = decoder.decode().map_err(unreadable)?;
    let (width, height) = decoder.dimensions().ok_or("not a JPEG pngdeer can read")?;
    let pixels = if gray {
        samples.iter().map(|&g| RGBA::new(g, g, g, 255)).collect()
    } else {
        samples.as_chunks::<3>().0.iter().map(|p| RGBA::new(p[0], p[1], p[2], 255)).collect()
    };
    Ok(Photo { pixels, width, height, icc, orientation })
}

fn unreadable(e: zune_jpeg::errors::DecodeErrors) -> Error {
    format!("not a JPEG pngdeer can read: {}", e.to_string().trim()).into()
}

/// The orientation tag (0x0112) in IFD0 of EXIF data starting at its TIFF header.
fn exif_orientation(tiff: &[u8]) -> Option<u16> {
    let big = match tiff.get(..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |at: usize| tiff.get(at..at + 2).map(|b| if big { u16::from_be_bytes([b[0], b[1]]) } else { u16::from_le_bytes([b[0], b[1]]) });
    let u32_at =
        |at: usize| tiff.get(at..at + 4).map(|b| if big { u32::from_be_bytes(b.try_into().unwrap()) } else { u32::from_le_bytes(b.try_into().unwrap()) });
    let ifd = u32_at(4)? as usize;
    (0..u16_at(ifd)? as usize)
        .map(|i| ifd + 2 + i * 12)
        .find(|&entry| u16_at(entry) == Some(0x0112))
        .and_then(|entry| u16_at(entry + 8))
        .filter(|o| (1..=8).contains(o))
}

/// An EXIF APP1 payload holding only an orientation.
#[cfg(test)]
fn orientation_exif(orientation: u16) -> Vec<u8> {
    let mut exif = b"Exif\0\0MM\0\x2a\0\0\0\x08".to_vec(); // big-endian TIFF, IFD0 right after
    exif.extend_from_slice(&1u16.to_be_bytes()); // one entry:
    exif.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1]); // orientation, SHORT, one value
    exif.extend_from_slice(&orientation.to_be_bytes());
    exif.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // value padding, no next IFD
    exif
}

/// The automatic target for a photo of `w` × `h` pixels (see the module docs).
#[cfg(feature = "webp")]
pub(crate) fn auto_target(w: usize, h: usize) -> f64 {
    // libm: the same bits on every target
    (AUTO_AT_1MP - AUTO_SLOPE * libm::log((w * h) as f64 / 1e6)).min(AUTO_CAP)
}

pub use crate::codec::oriented;

/// Whether an ICC profile is sRGB's: its description says so (the profile cameras, editors and
/// Wikimedia embed: "sRGB IEC61966-2.1" and the like). Viewers show an untagged JPEG as sRGB.
#[cfg(feature = "webp")]
pub(crate) fn is_srgb(icc: &[u8]) -> bool {
    let u32_at = |at: usize| icc.get(at..at + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()));
    let Some(count) = u32_at(128) else { return false };
    (0..count.min(256) as usize).any(|i| {
        let at = 132 + i * 12;
        if icc.get(at..at + 4) != Some(b"desc") {
            return false;
        }
        let (Some(offset), Some(size)) = (u32_at(at + 4), u32_at(at + 8)) else { return false };
        let Some(tag) = icc.get(offset as usize..(offset as usize).saturating_add(size as usize)) else { return false };
        // 'desc' holds ASCII; 'mluc' (ICC v4) UTF-16: either way, look for the letters
        let ascii: Vec<u8> = tag.iter().copied().filter(|&b| b != 0).collect();
        ascii.windows(4).any(|w| w == b"sRGB")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_an_orientation() {
        let exif = orientation_exif(6);
        assert_eq!(exif_orientation(&exif[6..]), Some(6));
        // little-endian, two entries, orientation second
        let mut tiff = b"II\x2a\0\x08\0\0\0\x02\0".to_vec();
        tiff.extend_from_slice(&[0x0f, 0x01, 2, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
        tiff.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0, 8, 0, 0, 0]);
        assert_eq!(exif_orientation(&tiff), Some(8));
        assert_eq!(exif_orientation(b"nope"), None);
    }

    #[test]
    fn turns_as_the_orientation_says() {
        // 3 × 2: a b c / d e f
        let px = |v: u8| RGBA::new(v, 0, 0, 255);
        let pixels: Vec<RGBA> = (1..=6).map(px).collect();
        let shown = |o| {
            let (p, w, h) = oriented(pixels.clone(), 3, 2, o);
            (p.iter().map(|p| p.r).collect::<Vec<_>>(), w, h)
        };
        assert_eq!(shown(1), (vec![1, 2, 3, 4, 5, 6], 3, 2));
        assert_eq!(shown(2), (vec![3, 2, 1, 6, 5, 4], 3, 2));
        assert_eq!(shown(3), (vec![6, 5, 4, 3, 2, 1], 3, 2));
        assert_eq!(shown(4), (vec![4, 5, 6, 1, 2, 3], 3, 2));
        assert_eq!(shown(5), (vec![1, 4, 2, 5, 3, 6], 2, 3));
        // 6: a quarter turn clockwise, so the left column, bottom up, becomes the top row
        assert_eq!(shown(6), (vec![4, 1, 5, 2, 6, 3], 2, 3));
        assert_eq!(shown(7), (vec![6, 3, 5, 2, 4, 1], 2, 3));
        assert_eq!(shown(8), (vec![3, 6, 2, 5, 1, 4], 2, 3));
    }

    #[cfg(feature = "webp")]
    #[test]
    fn tells_srgb_profiles() {
        let profile = |desc: &[u8]| {
            let mut icc = vec![0u8; 128];
            icc.extend_from_slice(&1u32.to_be_bytes());
            icc.extend_from_slice(b"desc");
            icc.extend_from_slice(&144u32.to_be_bytes());
            icc.extend_from_slice(&(desc.len() as u32).to_be_bytes());
            icc.extend_from_slice(desc);
            icc
        };
        assert!(is_srgb(&profile(b"desc\0\0\0\0\0\0\0\x12sRGB IEC61966-2.1\0")));
        assert!(!is_srgb(&profile(b"desc\0\0\0\0\0\0\0\x0aAdobe RGB\0")));
        assert!(!is_srgb(b"short"));
    }
}
