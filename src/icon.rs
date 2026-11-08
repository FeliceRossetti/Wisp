//! App and tray icons, embedded from `assets/icons`.
//!
//! The tray version can carry a small red badge for unread messages.

use std::io::Cursor;

const WINDOW_PNG: &[u8] = include_bytes!("../assets/icons/wisp-256.png");
const TRAY_PNG: &[u8] = include_bytes!("../assets/icons/wisp-64.png");

/// Straight (non-premultiplied) RGBA pixels.
pub struct Rgba {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The icon for the window and taskbar.
pub fn window() -> Rgba {
    decode(WINDOW_PNG)
}

/// The tray icon, optionally with an unread badge.
pub fn tray(badge: bool) -> Rgba {
    let mut icon = decode(TRAY_PNG);
    if badge {
        draw_badge(&mut icon);
    }
    icon
}

fn decode(bytes: &[u8]) -> Rgba {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().expect("embedded icon is a valid PNG");
    let mut buf = vec![0; reader.output_buffer_size().expect("icon size fits in memory")];
    let info = reader.next_frame(&mut buf).expect("embedded icon decodes");
    buf.truncate(info.buffer_size());
    let data = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => unreachable!("EXPAND turns palettes into RGB(A)"),
    };
    Rgba {
        data,
        width: info.width,
        height: info.height,
    }
}

/// A red dot with a white ring in the top-right corner.
fn draw_badge(icon: &mut Rgba) {
    let s = icon.width as f32;
    let (cx, cy) = (s * 0.78, s * 0.22);
    let (ring, fill) = (s * 0.22, s * 0.17);
    for y in 0..icon.height {
        for x in 0..icon.width {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let ring_a = (ring - d + 0.5).clamp(0.0, 1.0);
            if ring_a == 0.0 {
                continue;
            }
            let fill_a = (fill - d + 0.5).clamp(0.0, 1.0);
            let i = ((y * icon.width + x) * 4) as usize;
            let px = &mut icon.data[i..i + 4];
            for (c, red) in px.iter_mut().take(3).zip([239.0, 68.0, 68.0]) {
                let base = *c as f32 * (1.0 - ring_a) + 255.0 * ring_a;
                *c = (base * (1.0 - fill_a) + red * fill_a).round() as u8;
            }
            px[3] = px[3].max((ring_a * 255.0) as u8);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_decode_to_rgba() {
        let w = window();
        assert_eq!((w.width, w.height), (256, 256));
        assert_eq!(w.data.len(), 256 * 256 * 4);
        assert_eq!(w.data[3], 0, "corner should be transparent");
        let t = tray(false);
        assert_eq!(t.data.len(), (t.width * t.height * 4) as usize);
    }

    #[test]
    fn badge_changes_pixels() {
        assert_ne!(tray(false).data, tray(true).data);
    }
}
