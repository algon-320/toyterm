use freetype::GlyphMetrics;
use glium::{texture, Display};
use std::rc::Rc;

use crate::font::{FontSet, FontStyle};
use crate::terminal::CellSize;

// NOTE: STYLES_BITS must be large enough to distinguish `FontStyle`s, that is:
// assert!( FontStyle::all().len() < (1 << STYLES_BITS) )
const STYLES_BITS: usize = 2;

fn get_ascii_index(ch: char, style: FontStyle) -> usize {
    debug_assert!(ch.is_ascii());
    let code = ch as usize;
    let style = style as u8 as usize;
    debug_assert!(style < (1 << STYLES_BITS));
    (code << STYLES_BITS) | style
}

#[derive(Debug, Clone, Copy)]
pub struct GlyphRegion {
    /// width in pixel
    pub px_w: u32,
    /// height in pixel
    pub px_h: u32,

    /// x (0.0 to 1.0)
    pub tx_x: f32,
    /// y (0.0 to 1.0)
    pub tx_y: f32,
    /// width (0.0 to 1.0)
    pub tx_w: f32,
    /// height (0.0 to 1.0)
    pub tx_h: f32,
}

impl GlyphRegion {
    pub fn is_empty(&self) -> bool {
        self.px_w == 0 || self.px_h == 0
    }
}

pub struct GlyphCache {
    texture: Rc<texture::Texture2d>,
    ascii_glyph_region: Vec<Option<(GlyphRegion, GlyphMetrics)>>,
}

impl GlyphCache {
    pub fn build_ascii_visible(display: &Display, fonts: &FontSet, mut cell_sz: CellSize) -> Self {
        // NOTE: add padding to avoid conflict with adjacent glyphs
        cell_sz.w += 1;
        cell_sz.h += 1;

        // Glyph layout in the cache texture:
        // +----------------+
        // | !"#$%&'()*+,-./| <-- Regular style
        // |0123456789:;<=>?|
        // |@ABCDEFGHIJKLMNO|
        // |PQRSTUVWXYZ[\]^_|
        // |`abcdefghijklmno|
        // |pqrstuvwxyz{|}~ |
        // +----------------+
        // | !"#$%&'()*+,-./| <-- Bold style
        // |0123456789:;<=>?|
        //        ...
        // |pqrstuvwxyz{|}~ |
        // +----------------+
        // | !"#$%&'()*+,-./| <-- Faint style
        //        ...
        // |pqrstuvwxyz{|}~ |
        // +----------------+

        let texture_w = 16 * cell_sz.w;
        let texture_h = (6 * cell_sz.h) * 3;
        log::debug!("cache texture: {}x{} (px)", texture_w, texture_h);

        let texture = texture::Texture2d::with_mipmaps(
            display,
            vec![vec![0_u8; texture_w as usize]; texture_h as usize],
            texture::MipmapsOption::NoMipmap,
        )
        .expect("Failed to create a texture");

        assert!(FontStyle::all().len() < (1 << STYLES_BITS));
        let mut ascii_glyph_region: Vec<Option<(GlyphRegion, GlyphMetrics)>> =
            vec![None; 0x80 << STYLES_BITS];

        let ascii_visible = ' '..='~';
        for ch in ascii_visible {
            let code = ch as usize;

            let col = code & 0xF;
            let row = ((code & 0x70) >> 4) - 2;

            let y = (row as u32) * cell_sz.h;
            let x = (col as u32) * cell_sz.w;

            for (i, &style) in FontStyle::all().iter().enumerate() {
                let (glyph_image, metrics) = match fonts.render(ch, style) {
                    None => continue,
                    Some(found) => found,
                };

                let y_origin = (i as u32) * (texture_h / 3);

                let rect = glium::Rect {
                    left: x,
                    bottom: y_origin + y,
                    width: glyph_image.width,
                    height: glyph_image.height,
                };

                texture.main_level().write(rect, glyph_image);

                let region = GlyphRegion {
                    px_w: rect.width,
                    px_h: rect.height,
                    tx_x: rect.left as f32 / texture_w as f32,
                    tx_y: rect.bottom as f32 / texture_h as f32,
                    tx_w: rect.width as f32 / texture_w as f32,
                    tx_h: rect.height as f32 / texture_h as f32,
                };

                let idx = get_ascii_index(ch, style);
                ascii_glyph_region[idx] = Some((region, metrics));
            }
        }

        Self {
            texture: Rc::new(texture),
            ascii_glyph_region,
        }
    }

    pub fn get(&self, ch: char, style: FontStyle) -> Option<(GlyphRegion, GlyphMetrics)> {
        if ch.is_ascii() {
            let idx = get_ascii_index(ch, style);
            self.ascii_glyph_region[idx]
        } else {
            None
        }
    }

    pub fn texture(&self) -> Rc<texture::Texture2d> {
        self.texture.clone()
    }
}
