//! The NIRUFLOW logo: chunky 5x7 pixel letters in the theme accent with a
//! drop shadow, the retro look of the suite (adapted from NIRUFM), and the
//! window icon: three pixel bars flowing into a tray.
use crate::canvas::{Canvas, Rect};
use crate::theme::{blend, Colors};

/// 5x7 glyphs, one row per byte, bit 4 = leftmost column.
fn glyph(c: char) -> [u8; 7] {
    match c {
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'I' => [0x0e, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0e],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x1b, 0x11],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        '.' => [0, 0, 0, 0, 0, 0x0c, 0x0c],
        _ => [0; 7],
    }
}

pub fn word_width(s: &str, px: i32) -> i32 {
    let n = s.chars().count() as i32;
    (n * 6 - 1).max(0) * px
}

/// Pixel letters of `px` screen pixels per dot, top-left at (x, y).
pub fn word(cv: &mut Canvas, x: i32, y: i32, s: &str, px: i32, color: u32, shadow: Option<u32>) {
    for pass in 0..2 {
        let (dx, dy, col) = match (pass, shadow) {
            (0, Some(sh)) => (px.max(1) / 2 + 1, px.max(1) / 2 + 1, sh),
            (0, None) => continue,
            _ => (0, 0, color),
        };
        for (i, c) in s.chars().enumerate() {
            let g = glyph(c.to_ascii_uppercase());
            let gx = x + i as i32 * 6 * px;
            for (row, bits) in g.iter().enumerate() {
                for colm in 0..5 {
                    if bits & (0x10 >> colm) != 0 {
                        cv.fill(
                            Rect::new(gx + colm * px + dx, y + row as i32 * px + dy, px, px),
                            col,
                        );
                    }
                }
            }
        }
    }
}

/// ARGB window icons: descending bars (a cleanup flow) in the accent.
pub fn window_icons(p: &Colors) -> Vec<(u32, u32, Vec<u32>)> {
    [16u32, 32, 48]
        .iter()
        .map(|&s| {
            let mut cv = Canvas::new(s as i32, s as i32);
            let u = (s / 16) as i32;
            let dark = blend(p.accent, 0x000000, 0.35);
            for (i, w) in [12, 9, 6].iter().enumerate() {
                cv.fill(
                    Rect::new(2 * u, (2 + 3 * i as i32) * u, w * u, 2 * u),
                    p.accent,
                );
            }
            cv.fill(Rect::new(u, 12 * u, 14 * u, u), dark);
            cv.fill(Rect::new(u, 13 * u, 14 * u, 2 * u), p.accent);
            let argb = cv
                .px
                .iter()
                .map(|v| if *v == 0 { 0 } else { 0xff00_0000 | v })
                .collect();
            (s, s, argb)
        })
        .collect()
}
