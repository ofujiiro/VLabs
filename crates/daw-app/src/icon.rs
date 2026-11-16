// SPDX-License-Identifier: GPL-3.0-or-later

//! The window icon, drawn by code: a rounded teal square with three piano
//! roll notes rising from left to right. (A designed icon replaces it before
//! the first release.)

use egui::IconData;

const SIZE: u32 = 64;

/// The icon as pixels.
pub fn window_icon() -> IconData {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            rgba.extend_from_slice(&pixel(x, y));
        }
    }
    IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

fn pixel(x: u32, y: u32) -> [u8; 4] {
    const BACKGROUND: [u8; 4] = [23, 26, 30, 255];
    const NOTE: [u8; 4] = [72, 199, 186, 255];
    if !inside_rounded_square(x, y) {
        return [0, 0, 0, 0];
    }
    // Three notes: (left, top, width), each 8 pixels tall.
    let notes = [(10, 38, 18), (26, 26, 16), (40, 14, 14)];
    let on_note = notes
        .iter()
        .any(|&(left, top, width)| x >= left && x < left + width && y >= top && y < top + 8);
    if on_note { NOTE } else { BACKGROUND }
}

fn inside_rounded_square(x: u32, y: u32) -> bool {
    const RADIUS: i64 = 12;
    let (x, y) = (i64::from(x), i64::from(y));
    let max = i64::from(SIZE) - 1;
    let cx = x.clamp(RADIUS, max - RADIUS);
    let cy = y.clamp(RADIUS, max - RADIUS);
    (x - cx).pow(2) + (y - cy).pow(2) <= RADIUS * RADIUS
}
