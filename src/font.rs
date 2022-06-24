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
    pub fn new() -> Self {
        let freetype = freetype::Library::init().expect("FreeType init");

        let ttf_data = include_bytes!("../fonts/Mplus1Code-Regular.ttf");
        let face = freetype.new_memory_face(ttf_data.to_vec(), 0).unwrap();
        face.set_pixel_sizes(0, 32).unwrap();

        Self {
            _freetype: freetype,
            face,
            size: 32,
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
            self.face.load_glyph(idx, LoadFlag::RENDER).expect("render");
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

    pub fn increase_size(&mut self, inc: u32) {
        self.size += inc;
        self.face.set_pixel_sizes(0, self.size).unwrap();
    }
    pub fn decrease_size(&mut self, dec: u32) {
        self.size -= dec;
        self.face.set_pixel_sizes(0, self.size).unwrap();
    }
}
