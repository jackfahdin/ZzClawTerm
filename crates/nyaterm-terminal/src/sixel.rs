//! Minimal VT340-style Sixel decoder.
//!
//! Converts a DCS body (`params q sixel-data`) into an RGBA8 raster packed as
//! [`crate::graphics::NYAR_MAGIC`] for the GPUI paint path.

const MAX_SIXEL_DIMENSION: u32 = 4096;
const MAX_SIXEL_CURSOR_EXTENT: u32 = 8192;
const MAX_SIXEL_RASTER_BYTES: usize = 4 * 1024 * 1024;

/// Decode a Sixel DCS body (bytes between `ESC P` and `ST`, including `q`).
///
/// Returns `(width, height, rgba8)` on success. Caps raster size to avoid OOM
/// from hostile streams.
pub fn decode_sixel_rgba(dcs_body: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let q_pos = dcs_body.iter().position(|&b| b == b'q')?;
    let params = &dcs_body[..q_pos];
    let data = &dcs_body[q_pos + 1..];

    // P2: 0/2 keep background transparent-ish; 1 forces background fill.
    let p2 = parse_dcs_params(params).get(1).copied().unwrap_or(0);
    let force_bg = p2 == 1;

    let mut palette = default_palette();
    let mut color = 0usize;
    let mut x: u32 = 0;
    let mut y: u32 = 0; // top of current 6-pixel band
    let mut max_x: u32 = 0;
    let mut max_y: u32 = 0;
    let mut declared_w: Option<u32> = None;
    let mut declared_h: Option<u32> = None;

    // Sparse then compact: grow a band buffer lazily.
    // Store as flat RGBA once we know extents; use intermediate rows of width.
    let mut rows: Vec<Vec<[u8; 4]>> = Vec::new();

    let mut i = 0usize;
    while i < data.len() {
        let b = data[i];
        match b {
            b'"' => {
                // Raster attributes: "Pan;Pad;Ph;Pv
                i += 1;
                let (nums, next) = read_numbers(data, i);
                i = next;
                // Ph = nums[2], Pv = nums[3] when present.
                if nums.len() >= 3 {
                    declared_w = Some(nums[2].max(1));
                }
                if nums.len() >= 4 {
                    declared_h = Some(nums[3].max(1));
                }
            }
            b'#' => {
                i += 1;
                let (nums, next) = read_numbers(data, i);
                i = next;
                if nums.is_empty() {
                    continue;
                }
                let pc = nums[0] as usize;
                if nums.len() >= 5 {
                    let pu = nums[1];
                    let (r, g, bcol) = if pu == 1 {
                        hls_to_rgb(nums[2], nums[3], nums[4])
                    } else {
                        // RGB percentages 0..=100
                        (pct_to_u8(nums[2]), pct_to_u8(nums[3]), pct_to_u8(nums[4]))
                    };
                    if pc < palette.len() {
                        palette[pc] = [r, g, bcol, 255];
                    }
                    color = pc.min(palette.len().saturating_sub(1));
                } else {
                    color = pc.min(palette.len().saturating_sub(1));
                }
            }
            b'!' => {
                // Repeat: !Pn Ch
                i += 1;
                let (nums, next) = read_numbers(data, i);
                i = next;
                let count = nums.first().copied().unwrap_or(1).max(1);
                if i >= data.len() {
                    break;
                }
                let ch = data[i];
                i += 1;
                if (b'?'..=b'~').contains(&ch) {
                    if x.saturating_add(count) > MAX_SIXEL_CURSOR_EXTENT {
                        return None;
                    }
                    let bits = ch - b'?';
                    for _ in 0..count {
                        plot_sixel(
                            &mut rows,
                            x,
                            y,
                            bits,
                            palette[color],
                            &mut max_x,
                            &mut max_y,
                        );
                        x = x.saturating_add(1);
                        if x > MAX_SIXEL_CURSOR_EXTENT {
                            return None;
                        }
                    }
                }
            }
            b'$' => {
                x = 0;
                i += 1;
            }
            b'-' => {
                x = 0;
                y = y.saturating_add(6);
                if y > MAX_SIXEL_CURSOR_EXTENT {
                    return None;
                }
                i += 1;
            }
            b'?'..=b'~' => {
                let bits = b - b'?';
                plot_sixel(
                    &mut rows,
                    x,
                    y,
                    bits,
                    palette[color],
                    &mut max_x,
                    &mut max_y,
                );
                x = x.saturating_add(1);
                if x > MAX_SIXEL_CURSOR_EXTENT {
                    return None;
                }
                i += 1;
            }
            // Ignore CR/LF/spaces and unknown controls.
            b'\r' | b'\n' | b' ' | b'\t' => i += 1,
            _ => i += 1,
        }
    }

    let width = declared_w
        .unwrap_or(max_x)
        .max(max_x)
        .clamp(1, MAX_SIXEL_DIMENSION);
    let height = declared_h
        .unwrap_or(max_y)
        .max(max_y)
        .clamp(1, MAX_SIXEL_DIMENSION);
    if max_x == 0 && max_y == 0 {
        return None;
    }
    if raster_len(width, height)? > MAX_SIXEL_RASTER_BYTES {
        return None;
    }

    let mut rgba = vec![0u8; raster_len(width, height)?];
    if force_bg {
        // Fill with palette color 0 as opaque background.
        let bg = palette[0];
        for px in rgba.as_chunks_mut::<4>().0 {
            px.copy_from_slice(&bg);
        }
    }

    for (row_y, row) in rows.iter().enumerate() {
        if row_y as u32 >= height {
            break;
        }
        for (col_x, pixel) in row.iter().enumerate() {
            if col_x as u32 >= width {
                break;
            }
            // Only overwrite transparent default when we have a real pixel.
            if pixel[3] == 0 {
                continue;
            }
            let idx = (row_y * width as usize + col_x) * 4;
            rgba[idx..idx + 4].copy_from_slice(pixel);
        }
    }

    Some((width, height, rgba))
}

/// Pack RGBA into the shared NYAR container used by size peek + GPUI decode.
pub fn pack_nyar_rgba(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let need = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    let pixels = if rgba.len() >= need {
        &rgba[..need]
    } else {
        rgba
    };
    let mut out = Vec::with_capacity(12 + pixels.len());
    out.extend_from_slice(b"NYAR");
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(pixels);
    out
}

/// Read NYAR width/height without copying pixel bytes.
pub fn nyar_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 12 || &data[..4] != b"NYAR" {
        return None;
    }
    let width = u32::from_le_bytes(data[4..8].try_into().ok()?);
    let height = u32::from_le_bytes(data[8..12].try_into().ok()?);
    if width == 0 || height == 0 || width > 4096 || height > 4096 {
        return None;
    }
    Some((width, height))
}

/// Decode NYAR container into `(width, height, rgba)`.
#[cfg(test)]
pub fn unpack_nyar_rgba(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let (width, height) = nyar_dimensions(data)?;
    let need = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if data.len() < 12 + need {
        return None;
    }
    Some((width, height, data[12..12 + need].to_vec()))
}

fn plot_sixel(
    rows: &mut Vec<Vec<[u8; 4]>>,
    x: u32,
    y: u32,
    bits: u8,
    color: [u8; 4],
    max_x: &mut u32,
    max_y: &mut u32,
) {
    if bits == 0 {
        // Still advances cursor in caller; nothing to paint.
        *max_x = (*max_x).max(x.saturating_add(1));
        *max_y = (*max_y).max(y.saturating_add(1));
        return;
    }
    for bit in 0..6u32 {
        if bits & (1 << bit) == 0 {
            continue;
        }
        let py = y.saturating_add(bit);
        let px = x;
        ensure_pixel(rows, px, py, color);
        *max_x = (*max_x).max(px.saturating_add(1));
        *max_y = (*max_y).max(py.saturating_add(1));
    }
}

fn ensure_pixel(rows: &mut Vec<Vec<[u8; 4]>>, x: u32, y: u32, color: [u8; 4]) {
    let y = y as usize;
    let x = x as usize;
    if y >= MAX_SIXEL_DIMENSION as usize || x >= MAX_SIXEL_DIMENSION as usize {
        return;
    }
    if rows.len() <= y {
        rows.resize_with(y + 1, Vec::new);
    }
    let row = &mut rows[y];
    if row.len() <= x {
        row.resize(x + 1, [0, 0, 0, 0]);
    }
    row[x] = color;
}

fn raster_len(width: u32, height: u32) -> Option<usize> {
    (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)
}

fn parse_dcs_params(params: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut cur: Option<u32> = None;
    for &b in params {
        match b {
            b'0'..=b'9' => {
                let d = (b - b'0') as u32;
                cur = Some(cur.unwrap_or(0).saturating_mul(10).saturating_add(d));
            }
            b';' => {
                out.push(cur.unwrap_or(0));
                cur = None;
            }
            _ => {}
        }
    }
    if cur.is_some() || params.ends_with(b";") {
        out.push(cur.unwrap_or(0));
    } else if let Some(v) = cur {
        out.push(v);
    }
    out
}

fn read_numbers(data: &[u8], mut i: usize) -> (Vec<u32>, usize) {
    let mut nums = Vec::new();
    let mut cur: Option<u32> = None;
    while i < data.len() {
        let b = data[i];
        match b {
            b'0'..=b'9' => {
                let d = (b - b'0') as u32;
                cur = Some(cur.unwrap_or(0).saturating_mul(10).saturating_add(d));
                i += 1;
            }
            b';' => {
                nums.push(cur.unwrap_or(0));
                cur = None;
                i += 1;
            }
            _ => break,
        }
    }
    if let Some(v) = cur {
        nums.push(v);
    }
    (nums, i)
}

fn pct_to_u8(v: u32) -> u8 {
    let v = v.min(100);
    ((v * 255) / 100) as u8
}

fn hls_to_rgb(h: u32, l: u32, s: u32) -> (u8, u8, u8) {
    // VT HLS: H 0..=360, L/S 0..=100.
    let h = (h % 360) as f32;
    let l = (l.min(100) as f32) / 100.0;
    let s = (s.min(100) as f32) / 100.0;
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hk = h / 360.0;
    let tr = (hk + 1.0 / 3.0).rem_euclid(1.0);
    let tg = hk.rem_euclid(1.0);
    let tb = (hk - 1.0 / 3.0).rem_euclid(1.0);
    let r = hls_channel(p, q, tr);
    let g = hls_channel(p, q, tg);
    let b = hls_channel(p, q, tb);
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

fn hls_channel(p: f32, q: f32, t: f32) -> f32 {
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

fn default_palette() -> [[u8; 4]; 256] {
    let mut palette = [[0u8, 0, 0, 255]; 256];
    // VT340-ish primary colors for low indexes.
    let primaries: [[u8; 3]; 16] = [
        [0, 0, 0],
        [128, 0, 0],
        [0, 128, 0],
        [128, 128, 0],
        [0, 0, 128],
        [128, 0, 128],
        [0, 128, 128],
        [192, 192, 192],
        [128, 128, 128],
        [255, 0, 0],
        [0, 255, 0],
        [255, 255, 0],
        [0, 0, 255],
        [255, 0, 255],
        [0, 255, 255],
        [255, 255, 255],
    ];
    for (i, rgb) in primaries.iter().enumerate() {
        palette[i] = [rgb[0], rgb[1], rgb[2], 255];
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::{decode_sixel_rgba, pack_nyar_rgba, unpack_nyar_rgba};

    #[test]
    fn decodes_solid_red_band() {
        // Color 0 = RGB 100%,0,0; one column of all 6 bits set (`~` = 63).
        let body = b"0;0;0q#0;2;100;0;0#0~";
        let (w, h, rgba) = decode_sixel_rgba(body).expect("decode");
        assert!(w >= 1);
        assert!(h >= 6);
        // Top pixel should be red opaque.
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
        let packed = pack_nyar_rgba(w, h, &rgba);
        let (w2, h2, rgba2) = unpack_nyar_rgba(&packed).expect("unpack");
        assert_eq!((w, h), (w2, h2));
        assert_eq!(rgba, rgba2);
    }

    #[test]
    fn repeat_and_graphics_newline() {
        // Two red pixels, then `-` next band with one green pixel.
        let body = b"q#0;2;100;0;0#0!2@-#1;2;0;100;0#1@";
        let (w, h, rgba) = decode_sixel_rgba(body).expect("decode");
        assert!(w >= 2);
        assert!(h >= 7);
        // bit0 of `@` (value 1) is the top pixel of the band.
        assert_eq!(&rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[4..8], &[255, 0, 0, 255]);
        let idx = (6 * w as usize) * 4;
        assert_eq!(&rgba[idx..idx + 4], &[0, 255, 0, 255]);
    }

    #[test]
    fn oversized_declared_raster_is_rejected() {
        // Declared 4096x4096 would require 64MiB of RGBA even with one pixel.
        assert!(decode_sixel_rgba(b"q\"1;1;4096;4096~").is_none());
    }

    #[test]
    fn oversized_repeat_is_rejected_without_long_loop() {
        assert!(decode_sixel_rgba(b"q!999999999~").is_none());
    }
}
