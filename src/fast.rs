//! One-candidate PNG preset for synchronous gateways; no perceptual scoring or oxipng.

use std::time::Instant;

use crate::{codec, palette, remap};

pub(crate) const MAX_PIXELS: usize = 8_000_000;

pub struct FastCompressed {
    pub png: Vec<u8>,
    pub outcome: &'static str,
    pub colors: usize,
    pub elapsed_ms: u128,
}

impl FastCompressed {
    pub fn json(&self) -> String {
        format!(
            r#"{{"preset":"fast","outcome":"{}","colors":{},"score":null,"bytes":{},"elapsed_ms":{}}}"#,
            self.outcome,
            self.colors,
            self.png.len(),
            self.elapsed_ms
        )
    }
}

/// Produce one palette at full resolution, keeping an exact palette where possible.
/// Return the smaller of that candidate and the input with unrelated metadata removed.
/// Reject animation, images above the pixel limit and results that do not reduce size.
pub fn compress_fast(input: &[u8]) -> Result<FastCompressed, codec::Error> {
    let started = Instant::now();
    let chunks = codec::checked_chunks(input)?;
    let finish = |png, outcome, colors| FastCompressed { png, outcome, colors, elapsed_ms: started.elapsed().as_millis() };
    if chunks.iter().any(|(kind, _)| *kind == b"acTL") {
        return Err("fast PNG does not support animation; use the quality preset for APNG".into());
    }
    let width = u32::from_be_bytes(input[16..20].try_into()?);
    let height = u32::from_be_bytes(input[20..24].try_into()?);
    if width == 0 || height == 0 {
        return Err("invalid PNG dimensions".into());
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS as u64 {
        return Err("fast PNG exceeds the 8000000 pixel limit; use the quality preset".into());
    }
    // Decode before stripping: corrupt image data must not become a successful output.
    let bitmap = codec::decode(input)?;
    let mut display = codec::color_chunks(input);
    for (kind, full) in &chunks {
        if *kind == b"eXIf" {
            if let Some(orientation) = exif_orientation(&full[8..full.len() - 4])? {
                // Rebuild a minimal EXIF directory: keep orientation, not camera/GPS/comment data.
                let mut exif = b"II\x2a\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00".to_vec();
                exif.extend_from_slice(&orientation.to_le_bytes());
                exif.extend_from_slice(&[0; 6]);
                codec::chunk(&mut display, b"eXIf", &exif);
            }
        }
    }
    let mut stripped = input[..33].to_vec();
    stripped.extend_from_slice(&display);
    for (kind, full) in &chunks {
        if matches!(*kind, b"PLTE" | b"tRNS" | b"IDAT" | b"IEND") {
            stripped.extend_from_slice(full);
        }
    }
    let (palette, indices, outcome) = if let Some((palette, indices)) = palette::exact_palette(&bitmap.pixels) {
        (palette, indices, "exact")
    } else {
        let palette = palette::PLAIN.palette(&bitmap.pixels, bitmap.width, bitmap.height, 256);
        let indices = remap::remap(&bitmap.pixels, bitmap.width, bitmap.height, &palette, 0.5);
        (palette, indices, "quantized")
    };
    let candidate = codec::with_chunks(&codec::encode_palette(&palette, &indices, bitmap.width, bitmap.height), &display);
    let (png, outcome) = if candidate.len() < stripped.len() { (candidate, outcome) } else { (stripped, "metadata_only") };
    crate::require_smaller(input, &png)?;
    Ok(finish(png, outcome, palette.len()))
}

pub(crate) fn exif_orientation(exif: &[u8]) -> Result<Option<u16>, codec::Error> {
    if exif.len() < 8 {
        return Err("truncated EXIF".into());
    }
    let little = match &exif[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return Err("invalid EXIF byte order".into()),
    };
    let short = |bytes: &[u8]| if little { u16::from_le_bytes([bytes[0], bytes[1]]) } else { u16::from_be_bytes([bytes[0], bytes[1]]) };
    let long = |bytes: &[u8]| {
        let bytes = [bytes[0], bytes[1], bytes[2], bytes[3]];
        if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        }
    };
    if short(&exif[2..4]) != 42 {
        return Err("invalid EXIF TIFF header".into());
    }
    let offset = long(&exif[4..8]) as usize;
    let directory = exif.get(offset..).filter(|d| d.len() >= 2).ok_or("invalid EXIF directory")?;
    let count = short(&directory[..2]) as usize;
    let entries = directory.get(2..2 + count * 12).ok_or("truncated EXIF directory")?;
    for entry in entries.as_chunks::<12>().0 {
        if short(&entry[..2]) == 0x112 {
            let orientation = short(&entry[8..10]);
            if short(&entry[2..4]) != 3 || long(&entry[4..8]) != 1 || !(1..=8).contains(&orientation) {
                return Err("invalid EXIF orientation".into());
            }
            return Ok((orientation != 1).then_some(orientation));
        }
    }
    Ok(None)
}
