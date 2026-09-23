//! The app icon, rasterized in code so the window icon and the Windows `.exe` icon
//! (written by `build.rs`, which includes this file) come from one source.
//! Uses only `std`, since the build script compiles it too.

const BG_TOP: [f32; 3] = [16.0, 54.0, 46.0];
const BG_BOTTOM: [f32; 3] = [7.0, 12.0, 16.0];
const CARD: [f32; 3] = [250.0, 250.0, 247.0];
const GOLD: [f32; 3] = [242.0, 196.0, 78.0];
const RED: [f32; 3] = [214.0, 48.0, 58.0];

/// Square RGBA image (unpremultiplied): a white card with a gold edge and a heart,
/// on a rounded felt-green tile.
pub fn rgba(size: u32) -> Vec<u8> {
    const SS: u32 = 4; // samples per axis, for anti-aliasing
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    if let Some(c) = sample(x, y) {
                        acc[0] += c[0];
                        acc[1] += c[1];
                        acc[2] += c[2];
                        acc[3] += 1.0;
                    }
                }
            }
            let n = acc[3];
            if n == 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let alpha = n / (SS * SS) as f32;
                out.extend_from_slice(&[
                    (acc[0] / n).round() as u8,
                    (acc[1] / n).round() as u8,
                    (acc[2] / n).round() as u8,
                    (alpha * 255.0).round() as u8,
                ]);
            }
        }
    }
    out
}

/// Color at a point in the unit square, or `None` where the icon is transparent.
fn sample(x: f32, y: f32) -> Option<[f32; 3]> {
    if !in_round_rect(x, y, 0.5, 0.5, 0.5, 0.5, 0.22) {
        return None;
    }
    let (cx, cy) = (0.5, 0.5);
    if in_round_rect(x, y, cx, cy, 0.30, 0.39, 0.07) {
        if !in_round_rect(x, y, cx, cy, 0.265, 0.355, 0.05) {
            return Some(GOLD);
        }
        return Some(if in_heart(x, y, cx, cy + 0.03, 0.40) {
            RED
        } else {
            CARD
        });
    }
    Some(lerp(BG_TOP, BG_BOTTOM, y))
}

fn lerp(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// Rounded rectangle given by its center, half-extents and corner radius.
fn in_round_rect(x: f32, y: f32, cx: f32, cy: f32, hw: f32, hh: f32, r: f32) -> bool {
    let dx = ((x - cx).abs() - (hw - r)).max(0.0);
    let dy = ((y - cy).abs() - (hh - r)).max(0.0);
    dx * dx + dy * dy <= r * r
}

/// Same geometry as the heart pip drawn on the cards.
fn in_heart(x: f32, y: f32, cx: f32, cy: f32, s: f32) -> bool {
    let (u, v) = ((x - cx) / s, (y - cy) / s);
    let lobe = |ox: f32| (u - ox).powi(2) + (v + 0.17).powi(2) <= 0.27 * 0.27;
    // Triangle (-0.515, -0.10), (0.515, -0.10), (0, 0.50).
    let tri = (-0.10..=0.50).contains(&v) && u.abs() <= 0.515 * (0.50 - v) / 0.60;
    lobe(-0.25) || lobe(0.25) || tri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_transparent_and_center_is_red() {
        let size = 64;
        let img = rgba(size);
        assert_eq!(img.len(), (size * size * 4) as usize);
        assert_eq!(img[3], 0, "top-left corner is transparent");
        let center = ((size / 2 * size + size / 2) * 4) as usize;
        assert_eq!(&img[center..center + 4], &[214, 48, 58, 255]);
    }
}
