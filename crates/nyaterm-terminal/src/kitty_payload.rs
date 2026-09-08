//! Finalize Kitty transmit payloads (`f=`, `o=`, `s=`, `v=`) into store/paint bytes.
//!
//! - `f=100`: PNG (optionally zlib-wrapped with `o=z`)
//! - `f=24`: RGB8 raw → NYAR RGBA (`s`/`v` pixel size required)
//! - `f=32`: RGBA8 raw → NYAR (`s`/`v` required)
//! - missing `f`: leave bytes unchanged (legacy / already-encoded payloads)

use crate::sixel::pack_nyar_rgba;

/// Kitty pixel format codes used by the graphics protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KittyFormat {
    Rgb24 = 24,
    Rgba32 = 32,
    Png = 100,
}

impl KittyFormat {
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            24 => Some(Self::Rgb24),
            32 => Some(Self::Rgba32),
            100 => Some(Self::Png),
            _ => None,
        }
    }
}

/// Decompress and/or convert a completed Kitty transfer into paint-ready bytes.
pub fn finalize_kitty_payload(
    data: Vec<u8>,
    format: Option<u32>,
    compressed: bool,
    pixel_width: Option<u32>,
    pixel_height: Option<u32>,
) -> Vec<u8> {
    if data.is_empty() {
        return data;
    }
    let data = if compressed {
        match zlib_inflate(&data) {
            Some(raw) => raw,
            None => return data, // keep compressed blob; GPUI will placeholder
        }
    } else {
        data
    };

    let Some(fmt) = format.and_then(KittyFormat::from_u32) else {
        return data;
    };

    match fmt {
        KittyFormat::Png => data,
        KittyFormat::Rgb24 => {
            let (w, h) = match (pixel_width, pixel_height) {
                (Some(w), Some(h)) if w > 0 && h > 0 => (w, h),
                _ => return data,
            };
            rgb24_to_nyar(&data, w, h).unwrap_or(data)
        }
        KittyFormat::Rgba32 => {
            let (w, h) = match (pixel_width, pixel_height) {
                (Some(w), Some(h)) if w > 0 && h > 0 => (w, h),
                _ => return data,
            };
            rgba32_to_nyar(&data, w, h).unwrap_or(data)
        }
    }
}

fn zlib_inflate(data: &[u8]) -> Option<Vec<u8>> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;
    const MAX: usize = 16 * 1024 * 1024;
    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = decoder.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        if out.len().saturating_add(n) > MAX {
            return None;
        }
        out.extend_from_slice(&buf[..n]);
    }
    Some(out)
}

fn rgb24_to_nyar(data: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let need = (w as usize).checked_mul(h as usize)?.checked_mul(3)?;
    if data.len() < need || w > 4096 || h > 4096 {
        return None;
    }
    let mut rgba = Vec::with_capacity(need / 3 * 4);
    for px in data[..need].as_chunks::<3>().0 {
        rgba.extend_from_slice(&[px[0], px[1], px[2], 255]);
    }
    Some(pack_nyar_rgba(w, h, &rgba))
}

fn rgba32_to_nyar(data: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let need = (w as usize).checked_mul(h as usize)?.checked_mul(4)?;
    if data.len() < need || w > 4096 || h > 4096 {
        return None;
    }
    Some(pack_nyar_rgba(w, h, &data[..need]))
}

#[cfg(test)]
mod tests {
    use super::finalize_kitty_payload;
    use crate::sixel::nyar_dimensions;
    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use std::io::Write;

    #[test]
    fn rgb24_becomes_nyar_red_pixel() {
        let data = vec![255, 0, 0];
        let out = finalize_kitty_payload(data, Some(24), false, Some(1), Some(1));
        assert!(out.starts_with(b"NYAR"));
        let (w, h) = nyar_dimensions(&out).unwrap();
        assert_eq!((w, h), (1, 1));
        assert_eq!(&out[12..16], &[255, 0, 0, 255]);
    }

    #[test]
    fn rgba32_becomes_nyar() {
        let data = vec![0, 255, 0, 128];
        let out = finalize_kitty_payload(data, Some(32), false, Some(1), Some(1));
        assert_eq!(&out[12..16], &[0, 255, 0, 128]);
    }

    #[test]
    fn zlib_png_passthrough_after_inflate() {
        // Tiny synthetic "PNG" header bytes compressed.
        let raw = b"\x89PNG\r\n\x1a\nnot-a-real-png".to_vec();
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&raw).unwrap();
        let compressed = enc.finish().unwrap();
        let out = finalize_kitty_payload(compressed, Some(100), true, None, None);
        assert_eq!(out, raw);
    }

    #[test]
    fn missing_format_leaves_bytes() {
        let data = b"ABC".to_vec();
        assert_eq!(
            finalize_kitty_payload(data.clone(), None, false, None, None),
            data
        );
    }
}
