use std::collections::HashMap;

use freetype::{
    face::{Face, LoadFlag},
    GlyphMetrics, Library,
};
use glium::texture::RawImage2d;

pub struct Font {
    _freetype: Library,
    face: Face,
}

impl Font {
    pub fn new(ttf_data: &[u8], index: isize) -> Self {
        let freetype = freetype::Library::init().expect("FreeType init");
        let face = freetype.new_memory_face(ttf_data.to_vec(), index).unwrap();
        Self {
            _freetype: freetype,
            face,
        }
    }

    fn set_fontsize(&mut self, size: u32) {
        self.face.set_pixel_sizes(0, size).unwrap();
    }

    fn metrics(&self, ch: char) -> Option<GlyphMetrics> {
        if let idx @ 1.. = self.face.get_char_index(ch as usize) {
            self.face.load_glyph(idx, LoadFlag::DEFAULT).expect("load");
            Some(self.face.glyph().metrics())
        } else {
            None
        }
    }

    fn render(&self, ch: char) -> Option<(RawImage2d<u8>, GlyphMetrics)> {
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
    font_size: u32,
}

impl FontSet {
    pub fn new(font_size: u32) -> Self {
        FontSet {
            fonts: HashMap::new(),
            font_size,
        }
    }

    pub fn add(&mut self, style: FontStyle, mut font: Font) {
        font.set_fontsize(self.font_size);
        let list = self.fonts.entry(style).or_insert_with(Vec::new);
        list.push(font);
    }

    pub fn metrics(&self, ch: char, style: FontStyle) -> Option<GlyphMetrics> {
        self.fonts.get(&style)?.iter().find_map(|f| f.metrics(ch))
    }

    pub fn render(&self, ch: char, style: FontStyle) -> Option<(RawImage2d<u8>, GlyphMetrics)> {
        self.fonts.get(&style)?.iter().find_map(|f| f.render(ch))
    }

    pub fn fontsize(&self) -> u32 {
        self.font_size
    }

    pub fn set_fontsize(&mut self, new_size: u32) {
        self.font_size = new_size;
        for list in self.fonts.values_mut() {
            for f in list.iter_mut() {
                f.set_fontsize(new_size);
            }
        }
    }
}
