use std::collections::HashMap;

use freetype::{
    face::{Face, LoadFlag},
    GlyphMetrics, Library,
};
use glium::texture::RawImage2d;

pub struct Font {
    _freetype: Library,
    face: Face,
    size: u32,
}

impl Font {
    pub fn new(ttf_data: &[u8]) -> Self {
        let font_size = crate::TOYTERM_CONFIG.font_size;

        let freetype = freetype::Library::init().expect("FreeType init");
        let face = freetype.new_memory_face(ttf_data.to_vec(), 0).unwrap();
        face.set_pixel_sizes(0, font_size).unwrap();

        Self {
            _freetype: freetype,
            face,
            size: font_size,
        }
    }

    pub fn metrics(&self, character: char) -> Option<GlyphMetrics> {
        let idx = self.face.get_char_index(character as usize);
        if idx == 0 {
            None
        } else {
            self.face.load_glyph(idx, LoadFlag::DEFAULT).expect("load");
            Some(self.face.glyph().metrics())
        }
    }

    pub fn render(&self, character: char) -> Option<(RawImage2d<u8>, GlyphMetrics)> {
        let idx = self.face.get_char_index(character as usize);
        if idx == 0 {
            None
        } else {
            self.face
                .load_glyph(idx, LoadFlag::RENDER | LoadFlag::TARGET_LIGHT)
                .expect("load");
            let glyph = self.face.glyph();

            let bitmap = glyph.bitmap();
            let metrics = glyph.metrics();

            let raw_image = {
                RawImage2d {
                    data: bitmap.buffer().to_vec().into(),
                    width: bitmap.width() as u32,
                    height: bitmap.rows() as u32,
                    format: glium::texture::ClientFormat::U8,
                }
            };

            Some((raw_image, metrics))
        }
    }

    pub fn increase_size(&mut self, inc: i32) {
        let new_size = self.size as i32 + inc;
        self.size = new_size.clamp(1, i32::MAX) as u32;
        self.face.set_pixel_sizes(0, self.size).unwrap();
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum Style {
    Regular,
    Bold,
    Faint,
}

pub struct FontSet {
    fonts: HashMap<Style, Vec<Font>>,
}

impl FontSet {
    pub fn empty() -> Self {
        FontSet {
            fonts: HashMap::new(),
        }
    }

    pub fn add(&mut self, style: Style, font: Font) {
        let fallbacks = self.fonts.entry(style).or_insert_with(Vec::new);
        fallbacks.push(font);
    }

    pub fn metrics(&self, character: char, style: Style) -> Option<GlyphMetrics> {
        self.fonts
            .get(&style)?
            .iter()
            .find_map(|f| f.metrics(character))
    }

    pub fn render(&self, character: char, style: Style) -> Option<(RawImage2d<u8>, GlyphMetrics)> {
        self.fonts
            .get(&style)?
            .iter()
            .find_map(|f| f.render(character))
    }

    pub fn increase_size(&mut self, inc: i32) {
        for fs in self.fonts.values_mut() {
            for f in fs {
                f.increase_size(inc);
            }
        }
    }
}
