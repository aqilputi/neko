//! `--shot` / `shot`: the composed frame as an RGBA PNG.

use std::io::BufWriter;
use std::path::Path;

/// Reverse the row order of a tightly packed RGBA image (GL's origin is
/// bottom-left).
pub fn flip_rows(rgba: &mut [u8], width: u32, height: u32) {
    let stride = width as usize * 4;
    let h = height as usize;
    for y in 0..h / 2 {
        let (top, bottom) = rgba.split_at_mut((h - 1 - y) * stride);
        top[y * stride..(y + 1) * stride].swap_with_slice(&mut bottom[..stride]);
    }
}

/// Force alpha to opaque (the framebuffer's alpha is not meaningful).
pub fn opaque(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        px[3] = 255;
    }
}

pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let f = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(BufWriter::new(f), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(std::io::Error::other)?;
    w.write_image_data(rgba).map_err(std::io::Error::other)?;
    Ok(())
}

#[cfg(test)]
pub fn read_png(path: &Path) -> (u32, u32, Vec<u8>) {
    let dec = png::Decoder::new(std::fs::File::open(path).unwrap());
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_rows() {
        // 1×3: rows 0, 1, 2
        let mut px: Vec<u8> = (0..3).flat_map(|r| [r, r, r, 255]).collect();
        flip_rows(&mut px, 1, 3);
        assert_eq!(px.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), [2, 1, 0]);
        let mut px: Vec<u8> = (0..4).flat_map(|r| [r, 0, 0, 0, r, 0, 0, 0]).collect();
        flip_rows(&mut px, 2, 4);
        assert_eq!(px.chunks(8).map(|p| p[0]).collect::<Vec<_>>(), [3, 2, 1, 0]);
    }

    #[test]
    fn png_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("sub/a.png");
        let mut px = vec![10u8, 20, 30, 0, 40, 50, 60, 7];
        opaque(&mut px);
        write_png(&p, 2, 1, &px).unwrap();
        let (w, h, back) = read_png(&p);
        assert_eq!((w, h), (2, 1));
        assert_eq!(back, [10, 20, 30, 255, 40, 50, 60, 255]);
    }
}
