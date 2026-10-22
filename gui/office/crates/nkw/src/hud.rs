//! The HUD of phase 0: the mode label (§3.1), rasterised on the CPU into
//! a `MemoryRenderBuffer` only when it changes. A 5×7 pixel font for the
//! letters the labels use keeps the binary free of font files for now.

/// Rows of a 5×7 glyph, top first; bit 4 is the leftmost pixel.
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        ' ' => [0; 7],
        _ => return None,
    })
}

/// Cream-coloured badge with brown ink, RGBA (premultiplied: opaque).
const BG: [u8; 4] = [0xf6, 0xec, 0xd8, 0xff];
const INK: [u8; 4] = [0x5a, 0x3e, 0x2b, 0xff];

/// A label bitmap: `(width, height, rgba)`. Each font pixel is `px`×`px`
/// screen pixels; `px` px of padding around; corners cut for a rounded look.
pub fn label(text: &str, px: u32) -> (u32, u32, Vec<u8>) {
    let chars: Vec<[u8; 7]> = text.chars().filter_map(glyph).collect();
    let pad = 2 * px;
    let cell = 6 * px;
    let w = (chars.len() as u32 * cell).saturating_sub(px) + 2 * pad;
    let h = 7 * px + 2 * pad;
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let corner = px;
    for y in 0..h {
        for x in 0..w {
            let cx = x.min(w - 1 - x);
            let cy = y.min(h - 1 - y);
            if cx + cy < corner {
                continue; // transparent corner
            }
            let i = ((y * w + x) * 4) as usize;
            buf[i..i + 4].copy_from_slice(&BG);
        }
    }
    for (k, g) in chars.iter().enumerate() {
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5u32 {
                if bits & (0x10 >> col) == 0 {
                    continue;
                }
                for dy in 0..px {
                    for dx in 0..px {
                        let x = pad + k as u32 * cell + col * px + dx;
                        let y = pad + row as u32 * px + dy;
                        let i = ((y * w + x) * 4) as usize;
                        buf[i..i + 4].copy_from_slice(&INK);
                    }
                }
            }
        }
    }
    (w, h, buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_label_has_glyphs() {
        for l in ["NAVEGAR", "ANDAR", "FOCO", "CONVERSA", "SALTO", "COMANDOS"] {
            assert!(l.chars().all(|c| glyph(c).is_some()), "{l}");
        }
    }

    #[test]
    fn label_size_and_pixels() {
        let (w, h, buf) = label("FOCO", 2);
        assert_eq!((w, h), (4 * 12 - 2 + 8, 14 + 8));
        assert_eq!(buf.len(), (w * h * 4) as usize);
        let at = |x: u32, y: u32| &buf[((y * w + x) * 4) as usize..][..4];
        assert_eq!(at(0, 0), [0, 0, 0, 0], "rounded corner");
        assert_eq!(at(w / 2, 1), BG);
        // top-left pixel of the F stroke
        assert_eq!(at(4, 4), INK);
        assert_eq!(at(5, 5), INK);
    }
}
