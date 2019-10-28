use std::collections::HashMap;

use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::ttf::Font;
use sdl2::video::{Window, WindowContext};

use crate::basics::*;
use crate::utils::*;

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
        lazy_static! {
            static ref COLOR_CONFIG_TABLE: Option<std::collections::HashMap<String, config::Value>> = {
                let mut tmp = config::Config::default();
                tmp.merge(config::File::with_name("settings.toml")).ok()?;
                tmp.get_table("color_scheme").ok()
            };
        }
        fn get_config(key: &str) -> Option<Sdl2Color> {
            let mut col = COLOR_CONFIG_TABLE
                .as_ref()?
                .get(key)?
                .clone()
                .into_array()
                .ok()?;
            let b = col.pop()?.into_int().ok()?;
            let g = col.pop()?.into_int().ok()?;
            let r = col.pop()?.into_int().ok()?;
            Some(Sdl2Color::RGB(r as u8, g as u8, b as u8))
        }
        use sdl2::pixels::Color as Sdl2Color;
        match self {
            Color::Black => get_config("black").unwrap_or(Sdl2Color::RGB(0, 0, 0)),
            Color::Red => get_config("red").unwrap_or(Sdl2Color::RGB(200, 0, 0)),
            Color::Yellow => get_config("yellow").unwrap_or(Sdl2Color::RGB(200, 200, 0)),
            Color::Green => get_config("green").unwrap_or(Sdl2Color::RGB(0, 200, 0)),
            Color::Blue => get_config("blue").unwrap_or(Sdl2Color::RGB(0, 0, 200)),
            Color::Magenta => get_config("magenta").unwrap_or(Sdl2Color::RGB(200, 0, 200)),
            Color::Cyan => get_config("cyan").unwrap_or(Sdl2Color::RGB(0, 200, 200)),
            Color::White => get_config("white").unwrap_or(Sdl2Color::RGB(200, 200, 200)),
            Color::Gray => get_config("gray").unwrap_or(Sdl2Color::RGB(120, 120, 120)),
            Color::LightRed => get_config("light_red").unwrap_or(Sdl2Color::RGB(255, 0, 0)),
            Color::LightYellow => get_config("light_yellow").unwrap_or(Sdl2Color::RGB(255, 255, 0)),
            Color::LightGreen => get_config("light_green").unwrap_or(Sdl2Color::RGB(0, 255, 0)),
            Color::LightBlue => get_config("light_blue").unwrap_or(Sdl2Color::RGB(0, 0, 255)),
            Color::LightMagenta => {
                get_config("light_magenta").unwrap_or(Sdl2Color::RGB(255, 0, 255))
            }
            Color::LightCyan => get_config("light_cyan").unwrap_or(Sdl2Color::RGB(0, 255, 255)),
            Color::LightWhite => get_config("light_white").unwrap_or(Sdl2Color::RGB(255, 255, 255)),
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
        let regular = ttf_context.load_font(font_name_regular, font_size).unwrap();
        let bold = ttf_context.load_font(font_name_bold, font_size).unwrap();
        let char_size = {
            let tmp = regular.size_of_char('#').unwrap();
            Size::new(tmp.0 as usize, tmp.1 as usize)
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
        let font = FontSet::new(
            ttf_context,
            "/usr/share/fonts/truetype/ricty-diminished/RictyDiminished-Regular.ttf",
            "/usr/share/fonts/truetype/ricty-diminished/RictyDiminished-Bold.ttf",
            10 * 2,
        );
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
            screen_pixel_size: Size::new(width as u32, height as u32),
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
    pub fn draw_char(&mut self, c: char, p: Point<ScreenCell>) -> Result<usize, String> {
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
            let surface = err_str(f.render_char(c).blended(fg_color))?;
            let cols = f.size_of_char(c).unwrap().0 as usize / self.get_char_size().width;
            let mut cell_canvas = {
                let tmp = sdl2::surface::Surface::new(
                    (self.get_char_size().width * cols) as u32,
                    self.get_char_size().height as u32,
                    PixelFormatEnum::ARGB8888,
                )?;
                let mut cvs = tmp.into_canvas()?;
                cvs.set_draw_color(bg_color);
                cvs.fill_rect(None)?;
                cvs
            };
            let tc = cell_canvas.texture_creator();
            let texture = err_str(tc.create_texture_from_surface(surface))?;
            cell_canvas.copy(&texture, None, None)?;
            self.cache.insert(
                cell.clone(),
                (
                    cols,
                    cell_canvas.read_pixels(None, PixelFormatEnum::ARGB8888)?,
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

        Ok(*cols)
    }

    pub fn render(&mut self, cursor_pos: Option<&Point<ScreenCell>>) -> Result<(), String> {
        let src = &self.screen_pixel_buf[..];
        self.screen_texture
            .with_lock(None, |dst: &mut [u8], _: usize| unsafe {
                std::ptr::copy(src.as_ptr(), dst.as_mut_ptr(), dst.len());
            })
            .unwrap();

        err_str(self.context.canvas.copy(&self.screen_texture, None, None))?;
        if let Some(c) = cursor_pos {
            let rect = Rect::new(
                (self.get_char_size().width * c.x) as i32,
                (self.get_char_size().height * c.y) as i32,
                self.get_char_size().width as u32,
                self.get_char_size().height as u32,
            );
            let col = self.cell_attr.fg.to_sdl_color();
            self.context.canvas.set_draw_color(col);
            self.context.canvas.fill_rect(rect)?;
        }
        self.context.canvas.present();
        Ok(())
    }

    fn fill_rect_buf(&mut self, rect: &Rect, c: &Color) {
        let c = c.to_sdl_color();
        let pix = [c.b, c.g, c.r, 0xFF];
        for y in 0..rect.h {
            let y = (y + rect.y) as usize;
            for x in 0..rect.w {
                let x = (x + rect.x) as usize;
                for k in 0..4 {
                    self.screen_pixel_buf
                        [(y * self.screen_pixel_size.width as usize + x) * 4 + k] = pix[k];
                }
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
        self.render(None).unwrap();
    }

    // range: [l, r)
    pub fn clear_line(&mut self, row: usize, range: Option<(usize, usize)>) -> Result<(), String> {
        let rect = {
            let top_left = self.point_screen_to_pixel(Point::new(0, row));
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
        Ok(())
    }

    fn point_screen_to_pixel(&self, sp: Point<ScreenCell>) -> Point<Pixel> {
        Point::new(
            sp.x as i32 * self.get_char_size().width as i32,
            sp.y as i32 * self.get_char_size().height as i32,
        )
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
        self.clear_line(bottom_line, None).unwrap();
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
        self.clear_line(top_line, None).unwrap();
    }
}