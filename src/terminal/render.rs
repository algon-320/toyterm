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

trait ToSdl2Color {
    fn to_sdl2_color(&self) -> Sdl2Color;
}

trait ToSdl2Rect {
    fn to_sdl2_rect(&self) -> Rect;
}

impl ToSdl2Rect for Range2d<Pixel> {
    fn to_sdl2_rect(&self) -> Rect {
        Rect::new(
            self.left() as i32,
            self.top() as i32,
            self.width() as u32,
            self.height() as u32,
        )
    }
}

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

impl ToSdl2Color for Color {
    fn to_sdl2_color(&self) -> Sdl2Color {
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

        match *self {
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
    c: char,
    attr: CellAttribute,
}
impl Cell {
    pub fn new(c: char, attr: CellAttribute) -> Self {
        Cell { c, attr }
    }
}
impl Default for Cell {
    fn default() -> Self {
        Cell {
            c: ' ',
            attr: CellAttribute::default(),
        }
    }
}

pub struct FontSet<'ttf> {
    pub regular: Font<'ttf, 'static>,
    pub bold: Font<'ttf, 'static>,
    pub char_size: Size<Pixel>,
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
        if !regular.face_is_fixed_width() {
            log::warn!("{:?} isn't a monospace font", regular.face_family_name());
        }

        let font_path_bold = fc.find(font_name_bold, Some("Bold")).unwrap().path;
        log::info!("Bold font: {:?}", font_path_bold);
        let mut bold = ttf_context
            .load_font(font_path_bold, font_size)
            .map_err(|_| "Cannot open the bold font: please check your `settings.toml`".to_string())
            .unwrap();
        bold.set_hinting(sdl2::ttf::Hinting::Light);
        if !bold.face_is_fixed_width() {
            log::warn!("{:?} isn't a monospace font", bold.face_family_name());
        }

        let char_size = {
            let tmp = regular.size_of_char('#').unwrap();
            Size {
                width: tmp.0 as PixelIdx,
                height: tmp.1 as PixelIdx,
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
            Ambiguous => CharWidth::Half, // TODO: config
            Neutral | HalfWidth | Narrow => CharWidth::Half,
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
    cache: HashMap<Cell, Texture<'texture>>,
    screen_texture: Texture<'texture>,
    screen_pixel_size: Size<Pixel>,
}
impl<'ttf, 'texture> Renderer<'ttf, 'texture> {
    pub fn new(
        fonts: FontSet<'ttf>,
        canvas: Canvas<Window>,
        texture_creator: &'texture TextureCreator<WindowContext>,
        screen_pixel_size: Size<Pixel>,
    ) -> Self {
        let texture = texture_creator
            .create_texture_target(
                PixelFormatEnum::ARGB8888,
                screen_pixel_size.width as u32,
                screen_pixel_size.height as u32,
            )
            .unwrap();
        assert_eq!(
            unsafe { sdl2::sys::SDL_SetRenderTarget(canvas.raw(), texture.raw()) },
            0
        );
        Renderer {
            fonts,
            canvas,
            texture_creator,
            cache: std::collections::HashMap::new(),
            screen_texture: texture,
            screen_pixel_size,
        }
    }

    pub fn cell_size(&self) -> Size<Pixel> {
        self.fonts.char_size
    }

    fn char_size(&self, c: char) -> Size<Pixel> {
        let width = CharWidth::from_char(c).columns();
        let cell = self.fonts.char_size;
        Size {
            width: cell.width * (width as PixelIdx),
            height: cell.height,
        }
    }

    pub fn draw_cell(&mut self, cell: Cell, top_left: Point<Pixel>) {
        let (fg_color, bg_color) = if cell.attr.style == Style::Reverse {
            (cell.attr.bg.to_sdl2_color(), cell.attr.fg.to_sdl2_color())
        } else {
            (cell.attr.fg.to_sdl2_color(), cell.attr.bg.to_sdl2_color())
        };

        let char_size = self.char_size(cell.c);

        if !self.cache.contains_key(&cell) {
            // generate surface
            let font = match cell.attr.style {
                Style::Bold => &self.fonts.bold,
                _ => &self.fonts.regular,
            };

            // draw � if the font doesn't have a glyph of the character.
            let c = font.find_glyph(cell.c).map(|_| cell.c).unwrap_or('�');
            let mut surface = font.render_char(c).blended(fg_color).expect("sdl2");

            // draw under line
            if cell.attr.style == Style::UnderLine {
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
            let texture = self
                .texture_creator
                .create_texture_from_surface(&surface)
                .unwrap();
            self.cache.insert(cell, texture);
        }

        let cell_texture = self.cache.get(&cell).unwrap();
        let cell_rect = Range2d::new(top_left, char_size).to_sdl2_rect();
        self.canvas.set_draw_color(bg_color);
        self.canvas.fill_rect(cell_rect).unwrap();
        self.canvas.copy(&cell_texture, None, cell_rect).unwrap();
        // self.canvas
        //     .with_texture_canvas(&mut self.screen_texture, |canvas| {
        //         // self.draw_on_screen_texture(|canvas| {
        //         canvas.set_draw_color(bg_color);
        //         canvas.fill_rect(cell_rect).unwrap();
        //         canvas.copy(&cell_texture, None, cell_rect).unwrap();
        //     })
        //     .unwrap();
    }

    pub fn render(&mut self) {
        assert!(self.canvas.render_target_supported());

        // switch back to default (window)
        assert_eq!(
            unsafe { sdl2::sys::SDL_SetRenderTarget(self.canvas.raw(), std::ptr::null_mut()) },
            0
        );

        self.canvas.copy(&self.screen_texture, None, None).unwrap();
        self.canvas.present();

        // switch to the screen texture
        assert_eq!(
            unsafe { sdl2::sys::SDL_SetRenderTarget(self.canvas.raw(), self.screen_texture.raw()) },
            0
        );
    }

    pub fn clear_range(&mut self, color: &Color, range: &Range2d<Pixel>) {
        self.draw_on_screen_texture(|canvas| {
            canvas.set_draw_color(color.to_sdl2_color());
            canvas.fill_rect(range.to_sdl2_rect()).unwrap();
        });
    }

    pub fn shift_texture(&mut self, bg: &Color, src: &Range2d<Pixel>, dst: Point<Pixel>) {
        log::trace!("shift_texture(src={:?}, dst={:?})", src, dst);
        let bg = bg.to_sdl2_color();
        let src = src.to_sdl2_rect();
        let mut new_texture = self
            .texture_creator
            .create_texture_target(
                PixelFormatEnum::ARGB8888,
                self.screen_pixel_size.width as u32,
                self.screen_pixel_size.height as u32,
            )
            .unwrap();

        let screen_texture = &self.screen_texture;
        self.canvas
            .with_texture_canvas(&mut new_texture, |canvas| {
                canvas.copy(screen_texture, None, None).unwrap();

                let mut rect = src;
                rect.set_x(dst.x as i32);
                rect.set_y(dst.y as i32);

                canvas.set_draw_color(bg);
                canvas.fill_rect(src | rect).unwrap();
                canvas.copy(screen_texture, src, rect).unwrap();
            })
            .unwrap();

        assert_eq!(
            unsafe { sdl2::sys::SDL_SetRenderTarget(self.canvas.raw(), new_texture.raw()) },
            0
        );
        self.screen_texture = new_texture;
    }

    fn draw_on_screen_texture<F>(&mut self, fun: F)
    where
        F: FnOnce(&mut Canvas<Window>),
    {
        fun(&mut self.canvas);
        // self.canvas
        //     .with_texture_canvas(&mut self.screen_texture, fun)
        //     .expect("invalid screen texture")
    }

    // draw sixel graphic on the screen texture
    pub fn draw_sixel(&mut self, img: &sixel::Image, at: Point<Pixel>) {
        let img_size = Size {
            width: img.width as PixelIdx,
            height: img.height as PixelIdx,
        };
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
                .copy(&texture, None, Range2d::new(at, img_size).to_sdl2_rect())
                .unwrap();
        });
    }
}
