use freetype::GlyphMetrics;
use glium::{texture, Display};
use lru::LruCache;
use std::rc::Rc;

use crate::font::{FontSet, FontStyle};
use crate::terminal::CellSize;
use crate::view::PixelRect;

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

pub type GlyphRegion = PixelRect;

fn glyph_region_to_glium_rect(rect: GlyphRegion) -> glium::Rect {
    glium::Rect {
        left: rect.x as u32,
        bottom: rect.y as u32,
        width: rect.w,
        height: rect.h,
    }
}

pub struct GlyphCache {
    texture: Rc<texture::Texture2d>,
    ascii_glyph_region: Vec<Option<(GlyphRegion, GlyphMetrics)>>,
    other_glyph_region: LruCache<(char, FontStyle), (GlyphRegion, GlyphMetrics, Option<u64>)>,
}

impl GlyphCache {
    pub fn build_ascii_visible(display: &Display, fonts: &FontSet, mut cell_sz: CellSize) -> Self {
        use glium::backend::Facade as _;
        use glium::CapabilitiesSource as _;
        let caps = display.get_context().get_capabilities();
        let max_texture_size = caps.max_texture_size;
        log::info!("max_texture_size = {max_texture_size}");

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
        // +----------------+---------+
        // |                          |
        // | (space for other glyphs) |
        // |                          |
        // +--------------------------+

        let styles = FontStyle::all().len() as u32;

        let texture_w = {
            let target = 16 * cell_sz.w + 1024;
            let mut w = 1;
            while w < target {
                w <<= 1;
            }
            w
        };
        let texture_h = {
            let target = (6 * cell_sz.h) * styles + 1024 /* space for other glyphs */;
            let mut h = 1;
            while h < target {
                h <<= 1;
            }
            h
        };
        log::debug!("cache texture: {}x{} (px)", texture_w, texture_h);

        let ascii_region_height = (6 * cell_sz.h) * styles;

        let zeros = vec![vec![0_u8; texture_w as usize]; texture_h as usize];
        let texture =
            texture::Texture2d::with_mipmaps(display, zeros, texture::MipmapsOption::NoMipmap)
                .expect("Failed to create a texture");

        assert!(styles < (1 << STYLES_BITS));
        let mut ascii_glyph_region: Vec<Option<(GlyphRegion, GlyphMetrics)>> =
            vec![None; 0x80 << STYLES_BITS];

        let ascii_visible = ' '..='~';
        for ch in ascii_visible {
            let code = ch as usize;

            let col = code & 0xF;
            let row = ((code & 0x70) >> 4) - 2;

            for (i, &style) in FontStyle::all().iter().enumerate() {
                let (glyph_image, metrics) = match fonts.render(ch, style) {
                    None => continue,
                    Some(found) => found,
                };

                let y_origin = 6 * cell_sz.h * (i as u32);
                let y = (row as u32) * cell_sz.h;
                let x = (col as u32) * cell_sz.w;

                let region = GlyphRegion {
                    x: x as i32,
                    y: (y_origin + y) as i32,
                    w: glyph_image.width,
                    h: glyph_image.height,
                };

                let rect = glyph_region_to_glium_rect(region);
                texture.main_level().write(rect, glyph_image);

                let idx = get_ascii_index(ch, style);
                ascii_glyph_region[idx] = Some((region, metrics));
            }
        }

        // Split the rest of texture into "slots" and store a non-ASCII glyph in a slot.
        // These slots are managed in the LRU manner.
        let other_glyph_region = {
            let height = texture.height() - ascii_region_height;
            let width = texture.width();
            let slot_height = (cell_sz.h as f32 * 1.5).round() as u32;
            let slot_width = (cell_sz.w as f32 * 2.5).round() as u32;

            let rows = (height / slot_height) as usize;
            let cols = (width / slot_width) as usize;
            let capacity = rows * cols;

            log::info!(
                "{capacity} slots (rows:{rows}, cols:{cols}, each: {slot_width}x{slot_height} px)"
            );

            let mut lru = LruCache::new(capacity);

            let mut dummy_next = 0_u32;

            let dummy_metrics = {
                let idx = get_ascii_index(' ', FontStyle::Regular);
                ascii_glyph_region[idx].unwrap().1
            };

            for row in 0..rows {
                for col in 0..cols {
                    let y_origin = ascii_region_height;
                    let y = (row as u32) * slot_height;
                    let x = (col as u32) * slot_width;

                    let region = GlyphRegion {
                        x: x as i32,
                        y: (y_origin + y) as i32,
                        w: 0,
                        h: 0,
                    };

                    let dummy_char = loop {
                        dummy_next += 1;
                        if let Some(ch) = char::from_u32(dummy_next) {
                            break ch;
                        }
                    };

                    let key = (dummy_char, FontStyle::Regular);
                    let val = (region, dummy_metrics, None);
                    lru.push(key, val);
                }
            }

            lru
        };

        Self {
            texture: Rc::new(texture),
            ascii_glyph_region,
            other_glyph_region,
        }
    }

    pub fn get(
        &mut self,
        ch: char,
        style: FontStyle,
        tag: u64,
    ) -> Option<(GlyphRegion, GlyphMetrics)> {
        if ch.is_ascii() {
            let idx = get_ascii_index(ch, style);
            self.ascii_glyph_region[idx]
        } else {
            let (region, metrics, tag_mut) = self.other_glyph_region.get_mut(&(ch, style))?;

            // dummy slot
            if *tag_mut == None {
                return None;
            }

            // update tag
            *tag_mut = Some(tag);

            Some((*region, *metrics))
        }
    }

    pub fn get_or_insert<'a>(
        &'_ mut self,
        ch: char,
        style: FontStyle,
        fonts: &FontSet,
        tag: u64,
    ) -> Result<Option<(GlyphRegion, GlyphMetrics)>, ()> {
        match self.get(ch, style, tag) {
            Some(found) => Ok(Some(found)),
            None => {
                let (_, next) = self.other_glyph_region.peek_lru().unwrap();
                if next.2 == Some(tag) {
                    // Evicting a slot with the same tag is not desirable.
                    // NOTE: This situation can be happen
                    //       when too many glyphs are drawn on a single same frame.
                    return Err(());
                }

                let (image, metrics) = match fonts.render(ch, style) {
                    None => return Ok(None), // cannot cache this glyph
                    Some(got) => got,
                };

                // update
                {
                    let (_, (mut region, _, _)) = self.other_glyph_region.pop_lru().unwrap();

                    region.w = image.width;
                    region.h = image.height;

                    let rect = glyph_region_to_glium_rect(region);
                    self.texture.main_level().write(rect, image);

                    let key = (ch, style);
                    let val = (region, metrics, Some(tag));
                    self.other_glyph_region.push(key, val);
                }

                Ok(Some(self.get(ch, style, tag).unwrap()))
            }
        }
    }

    pub fn texture(&self) -> Rc<texture::Texture2d> {
        self.texture.clone()
    }
}
