// SPDX-License-Identifier: GPL-3.0-or-later

//! The program's icon, drawn by code: a rounded teal square with three
//! piano roll notes rising from left to right. The window uses it, and
//! `--write-icons` writes it as PNG files and a Windows `.ico` for the
//! installers and the desktop entry (decision 0022), so all of them share
//! one drawing. (A designed icon replaces it before the first release.)

use std::path::Path;

use egui::IconData;

/// The drawing's own grid: it is drawn on 64 by 64 units.
const DESIGN: u32 = 64;

/// The sizes written by `--write-icons`.
pub const SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

/// The icon as pixels, for the window.
pub fn window_icon() -> IconData {
    IconData {
        rgba: render(DESIGN),
        width: DESIGN,
        height: DESIGN,
    }
}

/// The icon at any size, as RGBA pixels, smoothed by drawing each pixel
/// from sixteen samples of the design.
pub fn render(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let mut sum = [0_u32; 4];
            for sy in 0..4 {
                for sx in 0..4 {
                    let ux = (f64::from(x) + (f64::from(sx) + 0.5) / 4.0) * f64::from(DESIGN)
                        / f64::from(size);
                    let uy = (f64::from(y) + (f64::from(sy) + 0.5) / 4.0) * f64::from(DESIGN)
                        / f64::from(size);
                    let colour = pixel(ux, uy);
                    // Colours weighted by coverage, so edges do not darken.
                    let alpha = u32::from(colour[3]);
                    for (channel, value) in sum.iter_mut().take(3).zip(colour) {
                        *channel += u32::from(value) * alpha;
                    }
                    sum[3] += alpha;
                }
            }
            let alpha = sum[3];
            let channel = |c: u32| {
                c.checked_div(alpha)
                    .map_or(0, |v| u8::try_from(v).unwrap_or(u8::MAX))
            };
            rgba.extend_from_slice(&[
                channel(sum[0]),
                channel(sum[1]),
                channel(sum[2]),
                u8::try_from(alpha / 16).unwrap_or(u8::MAX),
            ]);
        }
    }
    rgba
}

/// The colour at a point of the 64-unit design.
fn pixel(x: f64, y: f64) -> [u8; 4] {
    const BACKGROUND: [u8; 4] = [23, 26, 30, 255];
    const NOTE: [u8; 4] = [72, 199, 186, 255];
    if !inside_rounded_square(x, y) {
        return [0, 0, 0, 0];
    }
    // Three notes: (left, top, width), each 8 units tall.
    let notes = [(10.0, 38.0, 18.0), (26.0, 26.0, 16.0), (40.0, 14.0, 14.0)];
    let on_note = notes
        .iter()
        .any(|&(left, top, width)| x >= left && x < left + width && y >= top && y < top + 8.0);
    if on_note { NOTE } else { BACKGROUND }
}

fn inside_rounded_square(x: f64, y: f64) -> bool {
    const RADIUS: f64 = 12.0;
    let max = f64::from(DESIGN);
    if !(0.0..max).contains(&x) || !(0.0..max).contains(&y) {
        return false;
    }
    let cx = x.clamp(RADIUS, max - RADIUS);
    let cy = y.clamp(RADIUS, max - RADIUS);
    (x - cx).powi(2) + (y - cy).powi(2) <= RADIUS * RADIUS
}

/// Writes the icon as `icon-<size>.png` for every size and as `icon.ico`
/// into `folder`.
pub fn write_icons(folder: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let mut images = Vec::new();
    for size in SIZES {
        let png = png(size, &render(size));
        std::fs::write(folder.join(format!("icon-{size}.png")), &png)?;
        images.push((size, png));
    }
    std::fs::write(folder.join("icon.ico"), ico(&images))
}

/// A PNG file of RGBA pixels (uncompressed deflate blocks: simple, and
/// icons are small).
fn png(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + size as usize);
    for row in rgba.chunks(size as usize * 4) {
        raw.push(0); // No filter.
        raw.extend_from_slice(row);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len()));
        let length = u16::try_from(block.len()).unwrap_or(u16::MAX);
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&size.to_be_bytes());
    header.extend_from_slice(&size.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8 bits, RGBA.
    chunk(&mut out, *b"IHDR", &header);
    chunk(&mut out, *b"IDAT", &zlib);
    chunk(&mut out, *b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(0).to_be_bytes());
    let mut crc_input = kind.to_vec();
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc_input);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

/// A Windows icon file holding the PNG images (Windows Vista and later
/// read PNG inside `.ico`).
fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let count = u16::try_from(images.len()).unwrap_or(0);
    let mut out = Vec::new();
    out.extend_from_slice(&0_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * u32::from(count);
    for (size, png) in images {
        let side = u8::try_from(*size).unwrap_or(0); // 256 is written as 0.
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1_u16.to_le_bytes());
        out.extend_from_slice(&32_u16.to_le_bytes());
        let length = u32::try_from(png.len()).unwrap_or(0);
        out.extend_from_slice(&length.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += length;
    }
    for (_, png) in images {
        out.extend_from_slice(png);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_known_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn icons_are_written_at_every_size() {
        let folder = std::env::temp_dir().join(format!("icons-{}", std::process::id()));
        write_icons(&folder).unwrap();
        for size in SIZES {
            let png = std::fs::read(folder.join(format!("icon-{size}.png"))).unwrap();
            assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), size);
        }
        let ico = std::fs::read(folder.join("icon.ico")).unwrap();
        assert_eq!(&ico[..4], &[0, 0, 1, 0]);
        assert_eq!(
            usize::from(u16::from_le_bytes([ico[4], ico[5]])),
            SIZES.len()
        );
        // The corners are clear, the middle is drawn.
        let pixels = render(32);
        assert_eq!(pixels[3], 0, "a clear corner");
        assert_eq!(pixels[(16 * 32 + 16) * 4 + 3], 255, "an opaque middle");
        std::fs::remove_dir_all(folder).ok();
    }
}
