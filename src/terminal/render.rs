use lazy_static::lazy_static;
use std::collections::HashMap;

use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::ttf::Font;
use sdl2::video::{Window, WindowContext};

use crate::basics::*;
use crate::config_get;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Gray,
    LightRed,
    LightGreen,
    LightYellow,
    LightBlue,
    LightMagenta,
    LightCyan,
    LightWhite,
    RGB(u8, u8, u8),
}
impl Color {
    pub fn to_sdl_color(self) -> sdl2::pixels::Color {
        use sdl2::pixels::Color as Sdl2Color;

        lazy_static! {
            static ref COLOR_CONFIG: HashMap<String, config::Value> = {
                config::Config::default()
                    .merge(config::File::with_name("settings.toml"))
                    .and_then(|c| c.get_table("color_scheme"))
                    .unwrap_or_else(|_| HashMap::new())
            };
        }

        fn get_sdl2_color(key: &str) -> Option<Sdl2Color> {
            let col = config_get!(COLOR_CONFIG, key, Vec<u8>)?;
            if col.len() < 3 {
                None
            } else {
                Some(Sdl2Color::RGB(col[0], col[1], col[2]))
            }
        }

        lazy_static! {
            static ref COLOR_BLACK: Sdl2Color =
                get_sdl2_color("black").unwrap_or_else(|| Sdl2Color::RGB(0, 0, 0));
            static ref COLOR_RED: Sdl2Color =
                get_sdl2_color("red").unwrap_or_else(|| Sdl2Color::RGB(200, 0, 0));
            static ref COLOR_YELLOW: Sdl2Color =
                get_sdl2_color("yellow").unwrap_or_else(|| Sdl2Color::RGB(200, 200, 0));
            static ref COLOR_GREEN: Sdl2Color =
                get_sdl2_color("green").unwrap_or_else(|| Sdl2Color::RGB(0, 200, 0));
            static ref COLOR_BLUE: Sdl2Color =
                get_sdl2_color("blue").unwrap_or_else(|| Sdl2Color::RGB(0, 0, 200));
            static ref COLOR_MAGENTA: Sdl2Color =
                get_sdl2_color("magenta").unwrap_or_else(|| Sdl2Color::RGB(200, 0, 200));
            static ref COLOR_CYAN: Sdl2Color =
                get_sdl2_color("cyan").unwrap_or_else(|| Sdl2Color::RGB(0, 200, 200));
            static ref COLOR_WHITE: Sdl2Color =
                get_sdl2_color("white").unwrap_or_else(|| Sdl2Color::RGB(200, 200, 200));
            static ref COLOR_GRAY: Sdl2Color =
                get_sdl2_color("gray").unwrap_or_else(|| Sdl2Color::RGB(120, 120, 120));
            static ref COLOR_LIGHTRED: Sdl2Color =
                get_sdl2_color("light_red").unwrap_or_else(|| Sdl2Color::RGB(255, 0, 0));
            static ref COLOR_LIGHTYELLOW: Sdl2Color =
                get_sdl2_color("light_yellow").unwrap_or_else(|| Sdl2Color::RGB(255, 255, 0));
            static ref COLOR_LIGHTGREEN: Sdl2Color =
                get_sdl2_color("light_green").unwrap_or_else(|| Sdl2Color::RGB(0, 255, 0));
            static ref COLOR_LIGHTBLUE: Sdl2Color =
                get_sdl2_color("light_blue").unwrap_or_else(|| Sdl2Color::RGB(0, 0, 255));
            static ref COLOR_LIGHTMAGENTA: Sdl2Color =
                get_sdl2_color("light_magenta").unwrap_or_else(|| Sdl2Color::RGB(255, 0, 255));
            static ref COLOR_LIGHTCYAN: Sdl2Color =
                get_sdl2_color("light_cyan").unwrap_or_else(|| Sdl2Color::RGB(0, 255, 255));
            static ref COLOR_LIGHTWHITE: Sdl2Color =
                get_sdl2_color("light_white").unwrap_or_else(|| Sdl2Color::RGB(0, 255, 255));
        }

        match self {
            Color::Black => *COLOR_BLACK,
            Color::Red => *COLOR_RED,
            Color::Yellow => *COLOR_YELLOW,
            Color::Green => *COLOR_GREEN,
            Color::Blue => *COLOR_BLUE,
            Color::Magenta => *COLOR_MAGENTA,
            Color::Cyan => *COLOR_CYAN,
            Color::White => *COLOR_WHITE,
            Color::Gray => *COLOR_GRAY,
            Color::LightRed => *COLOR_LIGHTRED,
            Color::LightYellow => *COLOR_LIGHTYELLOW,
            Color::LightGreen => *COLOR_LIGHTGREEN,
            Color::LightBlue => *COLOR_LIGHTBLUE,
            Color::LightMagenta => *COLOR_LIGHTMAGENTA,
            Color::LightCyan => *COLOR_LIGHTCYAN,
            Color::LightWhite => *COLOR_LIGHTWHITE,
            Color::RGB(r, g, b) => Sdl2Color::RGB(r, g, b),
        }
    }
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Yellow,
            3 => Color::Green,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::White,
            8 => Color::Gray,
            9 => Color::LightRed,
            10 => Color::LightYellow,
            11 => Color::LightGreen,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            15 => Color::LightWhite,
            _ => Color::LightWhite,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Style {
    Normal,
    Bold,
    UnderLine,
    Blink,
    Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellAttribute {
    pub style: Style,
    pub fg: Color,
    pub bg: Color,
}
impl Default for CellAttribute {
    fn default() -> Self {
        CellAttribute {
            style: Style::Normal,
            fg: Color::LightWhite,
            bg: Color::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell {
    pub c: char,
    pub attribute: CellAttribute,
}
impl Cell {
    pub fn new(c: char, attr: CellAttribute) -> Self {
        Cell { c, attribute: attr }
    }
}
impl Default for Cell {
    fn default() -> Self {
        Cell {
            c: ' ',
            attribute: CellAttribute::default(),
        }
    }
}

pub struct FontSet<'a> {
    pub regular: Font<'a, 'static>,
    pub bold: Font<'a, 'static>,
    pub char_size: Size<usize>,
}
impl<'a> FontSet<'a> {
    fn new(
        ttf_context: &'a sdl2::ttf::Sdl2TtfContext,
        font_name_regular: &str,
        font_name_bold: &str,
        font_size: u16,
    ) -> Self {
        let fc = fontconfig::Fontconfig::new().expect("fontconfig");

        let font_path_regular = fc.find(font_name_regular, Some("Regular")).unwrap().path;

        #[cfg(debug_assertions)]
        println!("Regular font: {:?}", font_path_regular);

        let mut regular = ttf_context
            .load_font(font_path_regular, font_size)
            .map_err(|_| {
                "Cannot open the regular font: please check your `settings.toml`".to_string()
            })
            .unwrap();
        regular.set_hinting(sdl2::ttf::Hinting::Light);

        let font_path_bold = fc.find(font_name_bold, Some("Bold")).unwrap().path;

        #[cfg(debug_assertions)]
        println!("Bold font: {:?}", font_path_bold);

        let mut bold = ttf_context
            .load_font(font_path_bold, font_size)
            .map_err(|_| "Cannot open the bold font: please check your `settings.toml`".to_string())
            .unwrap();
        bold.set_hinting(sdl2::ttf::Hinting::Light);

        let char_size = {
            let tmp = regular.size_of_char('#').unwrap();
            Size {
                width: tmp.0 as usize,
                height: tmp.1 as usize,
            }
        };

        FontSet {
            regular,
            bold,
            char_size,
        }
    }
}

pub struct RenderContext<'a> {
    pub font: FontSet<'a>,
    pub canvas: Canvas<Window>,
    pub texture_creator: TextureCreator<WindowContext>,
}
impl<'a> RenderContext<'a> {
    pub fn new(
        window_title: &str,
        sdl_context: &sdl2::Sdl,
        ttf_context: &'a sdl2::ttf::Sdl2TtfContext,
        screen_size: Size<usize>,
    ) -> Self {
        let font = {
            let font_config: HashMap<String, config::Value> = {
                config::Config::default()
                    .merge(config::File::with_name("settings.toml"))
                    .and_then(|c| c.get_table("font"))
                    .unwrap_or_else(|_| HashMap::new())
            };
            FontSet::new(
                ttf_context,
                &config_get!(font_config, "regular", String).unwrap_or_else(String::new),
                &config_get!(font_config, "bold", String).unwrap_or_else(String::new),
                2 * config_get!(font_config, "size", u16).unwrap_or(10),
            )
        };
        let window = {
            let video = sdl_context.video().unwrap();
            video
                .window(
                    window_title,
                    (font.char_size.width * screen_size.width) as u32,
                    (font.char_size.height * screen_size.height) as u32,
                )
                .position_centered()
                .build()
                .unwrap()
        };
        let canvas = window
            .into_canvas()
            .accelerated()
            .target_texture()
            .build()
            .unwrap();
        let texture_creator = canvas.texture_creator();
        RenderContext {
            font,
            canvas,
            texture_creator,
        }
    }
}

pub enum CharWidth {
    Half,
    Full,
}
impl CharWidth {
    pub fn from_char(c: char) -> Self {
        use ucd::tables::misc::EastAsianWidth::*;
        use ucd::Codepoint;
        match c.east_asian_width() {
            Ambiguous | Neutral | HalfWidth | Narrow => CharWidth::Half,
            FullWidth | Wide => CharWidth::Full,
        }
    }
    pub fn columns(self) -> usize {
        match self {
            CharWidth::Half => 1,
            CharWidth::Full => 2,
        }
    }
}

pub struct Renderer<'a, 'b> {
    pub context: &'a mut RenderContext<'b>,
    pub cache: HashMap<Cell, (usize, Vec<u8>)>,
    pub screen_texture: Texture,
    pub screen_size: Size<usize>,
    pub screen_pixel_buf: Vec<u8>,
    pub cell_attr: CellAttribute,
    pub screen_pixel_size: Size<u32>,
}
impl<'a, 'b> Renderer<'a, 'b> {
    pub fn new(render_context: &'a mut RenderContext<'b>, screen_size: Size<usize>) -> Self {
        let char_size = render_context.font.char_size;
        let width = screen_size.width * char_size.width;
        let height = screen_size.height * char_size.height;
        let texture = render_context
            .texture_creator
            .create_texture_streaming(PixelFormatEnum::ARGB8888, width as u32, height as u32)
            .unwrap();
        Renderer {
            context: render_context,
            cache: std::collections::HashMap::new(),
            screen_texture: texture,
            screen_size,
            screen_pixel_buf: vec![0u8; width * height * 4],
            cell_attr: CellAttribute::default(),
            screen_pixel_size: Size {
                width: width as u32,
                height: height as u32,
            },
        }
    }

    pub fn get_char_size(&self) -> Size<usize> {
        self.context.font.char_size
    }

    pub fn set_cell_attribute(&mut self, cell_attr: CellAttribute) {
        self.cell_attr = cell_attr;
    }
    pub fn get_cell_attribute(&self) -> CellAttribute {
        self.cell_attr
    }

    // return char width
    pub fn draw_char(&mut self, c: char, p: Point<ScreenCell>) -> usize {
        let mut fg_color = self.cell_attr.fg.to_sdl_color();
        let mut bg_color = self.cell_attr.bg.to_sdl_color();

        if self.cell_attr.style == Style::Reverse {
            std::mem::swap(&mut fg_color, &mut bg_color);
        }

        // generate texture
        let cell = Cell::new(c, self.cell_attr);
        if !self.cache.contains_key(&cell) {
            let f = if self.cell_attr.style == Style::Bold {
                &self.context.font.bold
            } else {
                &self.context.font.regular
            };
            // draw □ if the font doesn't have this glyph
            let c = if f.find_glyph(c).is_none() { '?' } else { c };
            let surface = f.render_char(c).blended(fg_color).expect("sdl2");

            let cols = CharWidth::from_char(c).columns();
            let mut cell_canvas = {
                let tmp = sdl2::surface::Surface::new(
                    (self.get_char_size().width * cols) as u32,
                    self.get_char_size().height as u32,
                    PixelFormatEnum::ARGB8888,
                )
                .expect("sdl2");
                let mut cvs = tmp.into_canvas().unwrap();
                cvs.set_draw_color(bg_color);
                cvs.fill_rect(None).unwrap();
                cvs
            };
            let tc = cell_canvas.texture_creator();
            let texture = tc.create_texture_from_surface(surface).unwrap();
            cell_canvas.copy(&texture, None, None).unwrap();

            // draw under line
            if self.cell_attr.style == Style::UnderLine {
                let sz = self.get_char_size();
                cell_canvas.set_draw_color(fg_color);
                cell_canvas
                    .draw_line(
                        sdl2::rect::Point::new(0, sz.height as i32 - 3),
                        sdl2::rect::Point::new(sz.width as i32 - 1, sz.height as i32 - 3),
                    )
                    .unwrap();
            }

            self.cache.insert(
                cell,
                (
                    cols,
                    cell_canvas
                        .read_pixels(None, PixelFormatEnum::ARGB8888)
                        .unwrap(),
                ),
            );
        }
        let (cols, raw_data) = &self.cache[&cell];

        let top_left = self.point_screen_to_pixel(p);
        assert_eq!(self.get_char_size().area() * 4 * cols, raw_data.len());
        let width_px = self.get_char_size().width * cols;
        for i in 0..self.get_char_size().area() * cols {
            let (y, x) = (i / width_px, i % width_px);
            let (abs_y, abs_x) = (y + top_left.y as usize, x + top_left.x as usize);
            for k in 0..4 {
                self.screen_pixel_buf
                    [(abs_y * self.screen_pixel_size.width as usize + abs_x) * 4 + k] =
                    raw_data[i * 4 + k];
            }
        }
        *cols
    }

    pub fn render(&mut self, cursor_pos: Option<&Point<ScreenCell>>) {
        let src = &self.screen_pixel_buf[..];
        self.screen_texture
            .with_lock(None, |dst: &mut [u8], _: usize| unsafe {
                std::ptr::copy(src.as_ptr(), dst.as_mut_ptr(), dst.len());
            })
            .unwrap();

        self.context
            .canvas
            .copy(&self.screen_texture, None, None)
            .expect("driver error");
        if let Some(c) = cursor_pos {
            let rect = Rect::new(
                (self.get_char_size().width * c.x) as i32,
                (self.get_char_size().height * c.y) as i32,
                self.get_char_size().width as u32,
                self.get_char_size().height as u32,
            );
            let col = self.cell_attr.fg.to_sdl_color();
            self.context.canvas.set_draw_color(col);
            self.context.canvas.fill_rect(rect).expect("driver error");
        }
        self.context.canvas.present();
    }

    fn fill_rect_buf(&mut self, rect: &Rect, c: &Color) {
        let c = c.to_sdl_color();
        let pix = [c.b, c.g, c.r, 0xFF];
        for y in 0..rect.h {
            let y = (y + rect.y) as usize;
            for x in 0..rect.w {
                let x = (x + rect.x) as usize;
                let pixel_begin = (y * self.screen_pixel_size.width as usize + x) * 4;
                self.screen_pixel_buf[pixel_begin..pixel_begin + 4].copy_from_slice(&pix);
            }
        }
    }

    // draw sixel graphic on the screen texture
    pub fn draw_sixel(&mut self, img: &sixel::Image) {
        for iy in 0..img.height {
            let src = &img.buf[iy * img.width * 4..(iy + 1) * img.width * 4];
            // println!("src={:?}", src);
            let dst = &mut self.screen_pixel_buf[iy * self.screen_pixel_size.width as usize * 4..];
            unsafe {
                std::ptr::copy(src.as_ptr(), dst.as_mut_ptr(), img.width * 4);
            }
        }
    }

    pub fn clear(&mut self) {
        self.fill_rect_buf(
            &Rect::new(
                0,
                0,
                self.screen_pixel_size.width,
                self.screen_pixel_size.height,
            ),
            &self.cell_attr.bg.clone(),
        );
        self.render(None);
    }

    // range: [l, r)
    pub fn clear_line(&mut self, row: usize, range: Option<(usize, usize)>) {
        let rect = {
            let top_left = self.point_screen_to_pixel(Point { x: 0, y: row });
            if let Some(r) = range {
                Rect::new(
                    (self.get_char_size().width * r.0) as i32,
                    top_left.y,
                    (self.get_char_size().width * (r.1 - r.0)) as u32,
                    self.get_char_size().height as u32,
                )
            } else {
                Rect::new(
                    top_left.x,
                    top_left.y,
                    self.screen_pixel_size.width as u32,
                    self.get_char_size().height as u32,
                )
            }
        };
        let bg = self.cell_attr.bg;
        self.fill_rect_buf(&rect, &bg);
    }

    fn point_screen_to_pixel(&self, sp: Point<ScreenCell>) -> Point<Pixel> {
        Point {
            x: sp.x as i32 * self.get_char_size().width as i32,
            y: sp.y as i32 * self.get_char_size().height as i32,
        }
    }

    pub fn scroll_up(&mut self, top_line: usize, bottom_line: usize) {
        let line_bytes = (self.screen_pixel_size.width * 4) as usize;
        let row_block = line_bytes * self.get_char_size().height;
        unsafe {
            std::ptr::copy(
                self.screen_pixel_buf
                    [((top_line + 1) * row_block)..((bottom_line + 1) * row_block)]
                    .as_ptr(),
                self.screen_pixel_buf[(top_line * row_block)..].as_mut_ptr(),
                row_block * (bottom_line - top_line),
            );
        }
        self.clear_line(bottom_line, None);
    }
    pub fn scroll_down(&mut self, top_line: usize, bottom_line: usize) {
        let line_bytes = (self.screen_pixel_size.width * 4) as usize;
        let row_block = line_bytes * self.get_char_size().height;
        unsafe {
            std::ptr::copy(
                self.screen_pixel_buf[(top_line * row_block)..(bottom_line * row_block)].as_ptr(),
                self.screen_pixel_buf[((top_line + 1) * row_block)..].as_mut_ptr(),
                row_block * (bottom_line - top_line),
            );
        }
        self.clear_line(top_line, None);
    }
}
