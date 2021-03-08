use lazy_static::lazy_static;
use std::collections::HashMap;

use sdl2::pixels::Color as Sdl2Color;
use sdl2::pixels::PixelFormatEnum;
use sdl2::rect::{self, Rect};
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::surface::Surface;
use sdl2::ttf::{Font, Sdl2TtfContext};
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
    pub fn to_sdl_color(self) -> Sdl2Color {
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
struct Cell {
    c: char,
    attribute: CellAttribute,
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

pub struct FontSet<'ttf> {
    pub regular: Font<'ttf, 'static>,
    pub bold: Font<'ttf, 'static>,
    pub char_size: Size<usize>,
}
impl<'ttf> FontSet<'ttf> {
    fn new(
        ttf_context: &'ttf Sdl2TtfContext,
        font_name_regular: &str,
        font_name_bold: &str,
        font_size: u16,
    ) -> Self {
        let fc = fontconfig::Fontconfig::new().expect("fontconfig");

        let font_path_regular = fc.find(font_name_regular, Some("Regular")).unwrap().path;
        log::info!("Regular font: {:?}", font_path_regular);
        let mut regular = ttf_context
            .load_font(font_path_regular, font_size)
            .map_err(|_| {
                "Cannot open the regular font: please check your `settings.toml`".to_string()
            })
            .unwrap();
        regular.set_hinting(sdl2::ttf::Hinting::Light);

        let font_path_bold = fc.find(font_name_bold, Some("Bold")).unwrap().path;
        log::info!("Bold font: {:?}", font_path_bold);
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
        log::debug!("char_size = {:?}", char_size);

        FontSet {
            regular,
            bold,
            char_size,
        }
    }
}

pub fn load_fonts(ttf_context: &Sdl2TtfContext) -> FontSet<'_> {
    let font_config: HashMap<String, config::Value> = {
        config::Config::default()
            .merge(config::File::with_name("settings.toml"))
            .and_then(|c| c.get_table("font"))
            .unwrap_or_else(|_| HashMap::new())
    };
    FontSet::new(
        ttf_context,
        &config_get!(font_config, "regular", String).unwrap_or_else(|| {
            log::warn!("Regular font not specified");
            String::new()
        }),
        &config_get!(font_config, "bold", String).unwrap_or_else(|| {
            log::warn!("Bold font not specified");
            String::new()
        }),
        config_get!(font_config, "size", u16).unwrap_or_else(|| {
            log::warn!("font size not specified");
            log::info!("default font size: {}", 20);
            20
        }),
    )
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

pub struct Renderer<'ttf, 'texture> {
    fonts: FontSet<'ttf>,
    canvas: Canvas<Window>,
    texture_creator: &'texture TextureCreator<WindowContext>,
    cache: HashMap<Cell, (usize, Surface<'static>)>,
    screen_texture: Texture<'texture>,
    cell_attr: CellAttribute,
    screen_pixel_size: Size<u32>,
}
impl<'ttf, 'texture> Renderer<'ttf, 'texture> {
    pub fn new(
        fonts: FontSet<'ttf>,
        canvas: Canvas<Window>,
        texture_creator: &'texture TextureCreator<WindowContext>,
        screen_size: Size<usize>,
    ) -> Self {
        let char_size = fonts.char_size;
        let width = screen_size.width * char_size.width;
        let height = screen_size.height * char_size.height;
        let texture = texture_creator
            .create_texture_target(PixelFormatEnum::ARGB8888, width as u32, height as u32)
            .unwrap();
        // let texture_creator = canvas.texture_creator();
        Renderer {
            fonts,
            canvas,
            texture_creator,
            cache: std::collections::HashMap::new(),
            screen_texture: texture,
            cell_attr: CellAttribute::default(),
            screen_pixel_size: Size {
                width: width as u32,
                height: height as u32,
            },
        }
    }

    pub fn cell_size(&self) -> Size<usize> {
        self.fonts.char_size
    }
    pub fn char_size(&self, c: char) -> Size<usize> {
        let width = CharWidth::from_char(c).columns();
        let cell = self.fonts.char_size;
        Size {
            width: cell.width * width,
            height: cell.height,
        }
    }

    pub fn set_cell_attribute(&mut self, cell_attr: CellAttribute) {
        self.cell_attr = cell_attr;
    }
    pub fn get_cell_attribute(&self) -> CellAttribute {
        self.cell_attr
    }

    /// Draw the character and return its width
    pub fn draw_char(&mut self, c: char, p: Point<ScreenCell>) -> usize {
        let (fg_color, bg_color) = if self.cell_attr.style == Style::Reverse {
            (
                self.cell_attr.bg.to_sdl_color(),
                self.cell_attr.fg.to_sdl_color(),
            )
        } else {
            (
                self.cell_attr.fg.to_sdl_color(),
                self.cell_attr.bg.to_sdl_color(),
            )
        };

        // generate surface
        let cell = Cell::new(c, self.cell_attr);
        if !self.cache.contains_key(&cell) {
            let f = match self.cell_attr.style {
                Style::Bold => &self.fonts.bold,
                _ => &self.fonts.regular,
            };

            // draw � if the font doesn't have a glyph of the character.
            let c = f.find_glyph(c).map(|_| c).unwrap_or('�');
            let mut surface = f.render_char(c).blended(fg_color).expect("sdl2");
            let char_width = CharWidth::from_char(c).columns();

            // draw under line
            if self.cell_attr.style == Style::UnderLine {
                let char_size = self.char_size(c);
                let mut canvas = surface.into_canvas().unwrap();
                canvas.set_draw_color(fg_color);
                canvas
                    .draw_line(
                        // FIXME: underline position
                        rect::Point::new(0, char_size.height as i32 - 3),
                        rect::Point::new(char_size.width as i32 - 1, char_size.height as i32 - 3),
                    )
                    .unwrap();
                surface = canvas.into_surface();
            }
            self.cache.insert(cell, (char_width, surface));
        }

        let (char_width, cell_surface) = self.cache.get(&cell).unwrap();
        let char_width = *char_width;
        let cell_texture = Texture::from_surface(&cell_surface, &self.texture_creator).unwrap();
        let cell_rect = {
            let top_left = self.point_screen_to_pixel(p);
            let cell_size = self.cell_size();
            Rect::new(
                top_left.x,
                top_left.y,
                (char_width * cell_size.width) as u32,
                cell_size.height as u32,
            )
        };

        self.draw_on_screen_texture(|canvas| {
            canvas.set_draw_color(bg_color);
            canvas.fill_rect(cell_rect).unwrap();
            // copy texture
            canvas.copy(&cell_texture, None, Some(cell_rect)).unwrap();
        });
        char_width
    }

    pub fn render(&mut self, cursor_pos: Option<&Point<ScreenCell>>) {
        self.canvas.copy(&self.screen_texture, None, None).unwrap();

        // draw a cursor on the current position
        if let Some(c) = cursor_pos {
            let Size {
                width: w,
                height: h,
            } = self.cell_size();
            let rect = Rect::new((w * c.x) as i32, (h * c.y) as i32, w as u32, h as u32);
            let fg = self.cell_attr.fg.to_sdl_color();
            self.canvas.set_draw_color(fg);
            self.canvas.fill_rect(rect).expect("driver error");
        }

        self.canvas.present();
    }

    fn fill_rect(&mut self, rect: Rect, c: &Color) {
        let c = c.to_sdl_color();
        self.draw_on_screen_texture(|canvas| {
            canvas.set_draw_color(c);
            canvas.fill_rect(rect).unwrap();
        });
    }

    pub fn clear_entire_screen(&mut self) {
        let bg = self.cell_attr.bg;
        self.fill_rect(
            Rect::new(
                0,
                0,
                self.screen_pixel_size.width,
                self.screen_pixel_size.height,
            ),
            &bg,
        );
        self.render(None);
    }

    /// Fill the given area with the current background color
    pub fn clear_area(&mut self, top_left: Point<Pixel>, size: Size<u32>) {
        let rect = Rect::new(top_left.x, top_left.y, size.width, size.height);
        let bg = self.cell_attr.bg;
        self.fill_rect(rect, &bg);
    }

    fn point_screen_to_pixel(&self, sp: Point<ScreenCell>) -> Point<Pixel> {
        let cell_sz = self.cell_size();
        Point {
            x: sp.x as i32 * cell_sz.width as i32,
            y: sp.y as i32 * cell_sz.height as i32,
        }
    }

    fn shift_texture(&mut self, src: Rect, dst: Point<Pixel>) {
        log::trace!("shift_texture(src={:?}, dst={:?})", src, dst);
        let mut new_texture = self
            .texture_creator
            .create_texture_target(
                PixelFormatEnum::ARGB8888,
                self.screen_pixel_size.width,
                self.screen_pixel_size.height,
            )
            .unwrap();

        let bg_color = self.cell_attr.bg.to_sdl_color();
        let screen_texture = &self.screen_texture;
        self.canvas
            .with_texture_canvas(&mut new_texture, |canvas| {
                canvas.copy(screen_texture, None, None).unwrap();

                let mut rect = src;
                rect.set_x(dst.x);
                rect.set_y(dst.y);

                canvas.set_draw_color(bg_color);
                canvas.fill_rect(src | rect).unwrap();
                canvas.copy(screen_texture, src, rect).unwrap();
            })
            .unwrap();

        self.screen_texture = new_texture;
    }

    pub fn scroll_up(&mut self, top_line: usize, bottom_line: usize) {
        let cell_size = self.cell_size();
        let src = Rect::new(
            0,
            ((top_line + 1) * cell_size.height) as i32,
            self.screen_pixel_size.width,
            ((bottom_line - top_line) * cell_size.height) as u32,
        );
        let dst = Point {
            x: 0,
            y: (top_line * cell_size.height) as i32,
        };
        self.shift_texture(src, dst);
    }
    pub fn scroll_down(&mut self, top_line: usize, bottom_line: usize) {
        let cell_size = self.cell_size();
        let src = Rect::new(
            0,
            (top_line * cell_size.height) as i32,
            self.screen_pixel_size.width,
            ((bottom_line - top_line) * cell_size.height) as u32,
        );
        let dst = Point {
            x: 0,
            y: ((top_line + 1) * cell_size.height) as i32,
        };
        self.shift_texture(src, dst);
    }

    fn draw_on_screen_texture<F>(&mut self, fun: F)
    where
        F: FnOnce(&mut Canvas<Window>),
    {
        self.canvas
            .with_texture_canvas(&mut self.screen_texture, fun)
            .expect("invalid screen texture")
    }

    // draw sixel graphic on the screen texture
    pub fn draw_sixel(&mut self, img: &sixel::Image, at: Point<Pixel>) {
        let mut surface = Surface::new(
            img.width as u32,
            img.height as u32,
            PixelFormatEnum::ARGB8888,
        )
        .expect("too large to allocate");

        surface
            .without_lock_mut()
            .expect("must lock")
            .copy_from_slice(&img.buf);

        let texture = Texture::from_surface(&surface, &self.texture_creator).unwrap();
        self.draw_on_screen_texture(|canvas| {
            canvas
                .copy(
                    &texture,
                    None,
                    Rect::new(at.x, at.y, surface.width(), surface.height()),
                )
                .unwrap();
        });
    }
}
