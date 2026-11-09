use crate::canvas::Canvas;
use crate::color::Rgba;
use anyhow::{bail, Context, Result};
use std::io::BufReader;
use std::path::Path;
use std::sync::OnceLock;

/// The ColorOS About hero, fitted to the card it is laid in, once a build
/// has one. The public builds carry none and the card is drawn instead.
static ART: OnceLock<Canvas> = OnceLock::new();

/// Install the art the About page's hero card shows. Only the first art a
/// process installs is kept.
pub fn install(art: Canvas) {
    let _ = ART.set(art);
}

/// The installed art, fitted to the card.
pub fn installed() -> Option<&'static Canvas> {
    ART.get()
}

/// Read the art from a PNG and fit it to a `w`×`h` card whose corners are
/// `radius` across: bilinear sampling, then the card's rounded shape as the
/// picture's alpha so it sits over the page.
pub fn load(path: &Path, w: u32, h: u32, radius: f32) -> Result<Canvas> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    let (sw, sh) = (info.width, info.height);
    let pixels: Vec<[u8; 4]> = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].as_chunks::<4>().0.iter().map(|p| [p[0], p[1], p[2], p[3]]).collect(),
        png::ColorType::Rgb => buf[..info.buffer_size()].as_chunks::<3>().0.iter().map(|p| [p[0], p[1], p[2], 255]).collect(),
        other => bail!("the About art is {other:?}, not RGB or RGBA"),
    };
    Ok(fit(&pixels, sw, sh, w, h, radius))
}

/// `src`, `sw`×`sh`, fitted to `w`×`h` and cut to a card `radius` across.
pub fn fit(src: &[[u8; 4]], sw: u32, sh: u32, w: u32, h: u32, radius: f32) -> Canvas {
    let mut art = Canvas::new(w, h);
    if sw == 0 || sh == 0 || w == 0 || h == 0 {
        return art;
    }
    let (fw, fh) = (w as f32, h as f32);
    for y in 0..h {
        let sy = ((y as f32 + 0.5) * sh as f32 / fh - 0.5).clamp(0.0, sh as f32 - 1.0);
        let (y0, ty) = (sy.floor(), sy - sy.floor());
        let y1 = (y0 as u32 + 1).min(sh - 1);
        for x in 0..w {
            let sx = ((x as f32 + 0.5) * sw as f32 / fw - 0.5).clamp(0.0, sw as f32 - 1.0);
            let (x0, tx) = (sx.floor(), sx - sx.floor());
            let x1 = (x0 as u32 + 1).min(sw - 1);
            let at = |px: u32, py: u32| src[(py * sw + px) as usize];
            let (a, b, c, d) = (at(x0 as u32, y0 as u32), at(x1, y0 as u32), at(x0 as u32, y1), at(x1, y1));
            let blend = |k: usize| {
                let top = a[k] as f32 + (b[k] as f32 - a[k] as f32) * tx;
                let bottom = c[k] as f32 + (d[k] as f32 - c[k] as f32) * tx;
                (top + (bottom - top) * ty).round() as u8
            };
            let i = ((y * w + x) * 4) as usize;
            art.data[i] = blend(0);
            art.data[i + 1] = blend(1);
            art.data[i + 2] = blend(2);
            art.data[i + 3] = blend(3);
        }
    }
    // The card's rounded shape, as alpha.
    let mut mask = Canvas::new(w, h);
    mask.fill_round_rect(0.0, 0.0, fw, fh, radius, Rgba::rgb(255, 255, 255));
    for i in 0..(w * h) as usize {
        let m = mask.data[i * 4 + 3] as u32;
        art.data[i * 4 + 3] = ((art.data[i * 4 + 3] as u32 * m) / 255) as u8;
    }
    art
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_fills_the_card_but_its_corners_are_cut_away() {
        let src = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]];
        let art = fit(&src, 2, 2, 40, 40, 10.0);
        let at = |x: u32, y: u32| {
            let i = ((y * 40 + x) * 4) as usize;
            [art.data[i], art.data[i + 1], art.data[i + 2], art.data[i + 3]]
        };
        assert_eq!(at(0, 0)[3], 0, "the corner is cut away");
        assert_eq!(at(20, 20)[3], 255, "the middle is whole");
        assert_eq!(at(20, 2)[3], 255, "and so is the top edge");
        // The four quarters keep the colours they were sampled from.
        assert!(at(6, 6)[0] > 200, "top left red: {:?}", at(6, 6));
        assert!(at(34, 6)[1] > 200, "top right green: {:?}", at(34, 6));
        assert!(at(6, 34)[2] > 200, "bottom left blue: {:?}", at(6, 34));
    }
}
