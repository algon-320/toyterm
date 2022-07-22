use freetype::GlyphMetrics;
use glium::{texture, Display};
use std::rc::Rc;

use crate::font::{FontSet, Style};
use crate::terminal::CellSize;

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

fn get_ascii_index(ch: char, style: Style) -> usize {
    debug_assert!(ch.is_ascii());
    ((ch as u8 as usize) << 2) | (style as u8 as usize)
}

impl GlyphCache {
    pub fn build_ascii_visible(display: &Display, fonts: &FontSet, cell_sz: CellSize) -> Self {
        let texture_w = 16 * cell_sz.w;
        let texture_h = (8 - 2) * cell_sz.h * 3;
        log::debug!("cache texture: {}x{} (px)", texture_w, texture_h);

        let texture = texture::Texture2d::with_mipmaps(
            display,
            vec![vec![0_u8; texture_w as usize]; texture_h as usize],
            texture::MipmapsOption::NoMipmap,
        )
        .expect("Failed to create texture");

        // FIXME: the size is dependent on the fact that font::Style has only 3 variants.
        let mut ascii_glyph_region: Vec<Option<(GlyphRegion, GlyphMetrics)>> =
            vec![None; 0x80 << 2];

        let ascii_visible = ' '..='~';
        for ch in ascii_visible {
            let code = ch as usize;

            let row = ((code & 0x70) >> 4) - 2;
            let col = code & 0xF;

            let y = row as u32 * cell_sz.h;
            let x = col as u32 * cell_sz.w;

            if let Some((glyph_image, metrics)) = fonts.render(ch, Style::Regular) {
                let rect = glium::Rect {
                    left: x,
                    bottom: y,
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

                let idx = get_ascii_index(ch, Style::Regular);
                ascii_glyph_region[idx] = Some((region, metrics));
            }

            if let Some((glyph_image, metrics)) = fonts.render(ch, Style::Bold) {
                let rect = glium::Rect {
                    left: x,
                    bottom: y + texture_h / 3,
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

                let idx = get_ascii_index(ch, Style::Bold);
                ascii_glyph_region[idx] = Some((region, metrics));
            }

            if let Some((glyph_image, metrics)) = fonts.render(ch, Style::Faint) {
                let rect = glium::Rect {
                    left: x,
                    bottom: y + texture_h / 3 * 2,
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

                let idx = get_ascii_index(ch, Style::Faint);
                ascii_glyph_region[idx] = Some((region, metrics));
            }
        }

        Self {
            texture: Rc::new(texture),
            ascii_glyph_region,
        }
    }

    pub fn get(&self, ch: char, style: Style) -> Option<(GlyphRegion, GlyphMetrics)> {
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
