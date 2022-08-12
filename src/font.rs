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
    pub fn new(ttf_data: &[u8], index: isize, font_size: u32) -> Self {
        let freetype = freetype::Library::init().expect("FreeType init");
        let face = freetype.new_memory_face(ttf_data.to_vec(), index).unwrap();
        face.set_pixel_sizes(0, font_size).unwrap();

        Self {
            _freetype: freetype,
            face,
            size: font_size,
        }
    }

    pub fn metrics(&self, ch: char) -> Option<GlyphMetrics> {
        if let idx @ 1.. = self.face.get_char_index(ch as usize) {
            self.face.load_glyph(idx, LoadFlag::DEFAULT).expect("load");
            Some(self.face.glyph().metrics())
        } else {
            None
        }
    }

    pub fn render(&self, ch: char) -> Option<(RawImage2d<u8>, GlyphMetrics)> {
        if let idx @ 1.. = self.face.get_char_index(ch as usize) {
            let flags = LoadFlag::RENDER | LoadFlag::TARGET_LIGHT;
            self.face.load_glyph(idx, flags).expect("render");
            let glyph = self.face.glyph();

            let bitmap = glyph.bitmap();
            let metrics = glyph.metrics();

            let raw_image = RawImage2d {
                data: bitmap.buffer().to_vec().into(),
                width: bitmap.width() as u32,
                height: bitmap.rows() as u32,
                format: glium::texture::ClientFormat::U8,
            };

            Some((raw_image, metrics))
        } else {
            None
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
pub enum FontStyle {
    Regular,
    Bold,
    Faint,
}

impl FontStyle {
    pub const fn all() -> [FontStyle; 3] {
        [FontStyle::Regular, FontStyle::Bold, FontStyle::Faint]
    }
}

pub struct FontSet {
    fonts: HashMap<FontStyle, Vec<Font>>,
}

impl FontSet {
    pub fn empty() -> Self {
        FontSet {
            fonts: HashMap::new(),
        }
    }

    pub fn add(&mut self, style: FontStyle, font: Font) {
        let list = self.fonts.entry(style).or_insert_with(Vec::new);
        list.push(font);
    }

    pub fn metrics(&self, ch: char, style: FontStyle) -> Option<GlyphMetrics> {
        self.fonts.get(&style)?.iter().find_map(|f| f.metrics(ch))
    }

    pub fn render(&self, ch: char, style: FontStyle) -> Option<(RawImage2d<u8>, GlyphMetrics)> {
        self.fonts.get(&style)?.iter().find_map(|f| f.render(ch))
    }

    pub fn increase_size(&mut self, inc: i32) {
        for fs in self.fonts.values_mut() {
            for f in fs {
                f.increase_size(inc);
            }
        }
    }
}
