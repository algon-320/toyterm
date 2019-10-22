use std::collections::HashMap;
use std::path::Path;

use sdl2::rect::Rect;
use sdl2::render::Texture;
use sdl2::ttf;
use sdl2::ttf::FontStyle;

use crate::basics::*;
use crate::utils::*;

use super::render::*;

pub enum CursorMove {
    Up,
    Down,
    Left,
    Right,
    Next,
    Prev,
    LeftMost,
    RightMost,
}

pub struct Term<'a, 'b> {
    canvas: &'a mut sdl2::render::Canvas<sdl2::video::Window>,
    font: &'a mut sdl2::ttf::Font<'b, 'static>,
    render_cache: std::collections::HashMap<Cell, Vec<u8>>,

    screen_size: Size<usize>,
    screen_begin: usize,
    cursor: Point<ScreenCell>,
    saved_cursor_pos: Point<ScreenCell>,

    cell_attr: CellAttribute,
    char_size: Size<usize>,
    screen_pixel_buf: Vec<u8>,
    screen_texture: Texture<'a>,

    top_line: isize,
    bottom_line: isize,
}
impl<'a, 'b> Term<'a, 'b> {
    pub fn new(
        canvas: &'a mut sdl2::render::Canvas<sdl2::video::Window>,
        texture_creator: &'a mut sdl2::render::TextureCreator<sdl2::video::WindowContext>,
        font: &'a mut sdl2::ttf::Font<'b, 'static>,
        size: Size<usize>,
    ) -> Self {
        assert!(size.height > 0);

        let char_size = {
            let tmp = font.size_of_char('#').unwrap();
            Size::new(tmp.0 as usize, tmp.1 as usize)
        };

        let width = size.width * char_size.width;
        let height = size.height * char_size.height;
        let screen_texture = texture_creator
            .create_texture_streaming(
                sdl2::pixels::PixelFormatEnum::ARGB8888,
                width as u32,
                height as u32,
            )
            .unwrap();
        let screen_pixel_buf = vec![0u8; width * height * 4];

        let mut term = Term {
            canvas,
            font,
            render_cache: HashMap::new(),

            screen_size: size,
            screen_begin: 0,
            cursor: Point::new(0, 0),
            saved_cursor_pos: Point::new(0, 0),

            cell_attr: CellAttribute::default(),
            char_size,

            screen_texture,
            screen_pixel_buf,

            top_line: 0,
            bottom_line: size.height as isize - 1,
        };
        term.clear();
        term
    }

    pub fn clear(&mut self) {
        self.fill_rect_buf(
            &Rect::new(
                0,
                0,
                self.screen_size.width as u32,
                self.screen_size.height as u32,
            ),
            &self.cell_attr.bg.clone(),
        );
        self.render().unwrap();
    }

    pub fn set_cell_attribute(&mut self, cell_attr: CellAttribute) {
        self.cell_attr = cell_attr;
    }

    pub fn draw_char(&mut self, c: char, p: Point<ScreenCell>) -> Result<(), String> {
        let mut fg_color = self.cell_attr.fg.to_sdl_color();
        let mut bg_color = self.cell_attr.bg.to_sdl_color();

        if self.cell_attr.style == Style::Reverse {
            std::mem::swap(&mut fg_color, &mut bg_color);
        }

        if self.cell_attr.style == Style::Bold {
            self.font.set_style(FontStyle::BOLD);
        } else {
            self.font.set_style(FontStyle::NORMAL);
        }

        // generate texture
        let cell = Cell::new(c, self.cell_attr);
        if !self.render_cache.contains_key(&cell) {
            let mut cell_canvas = {
                let tmp = sdl2::surface::Surface::new(
                    self.char_size.width as u32,
                    self.char_size.height as u32,
                    sdl2::pixels::PixelFormatEnum::ARGB8888,
                )?;
                let mut cvs = tmp.into_canvas()?;
                cvs.set_draw_color(Color::Blue.to_sdl_color());
                // cvs.set_draw_color(bg_color);
                cvs.fill_rect(None)?;
                cvs
            };
            let surface = err_str(self.font.render_char(c).blended(fg_color))?;
            let tc = cell_canvas.texture_creator();
            let texture = err_str(tc.create_texture_from_surface(surface))?;
            cell_canvas.copy(&texture, None, None)?;
            self.render_cache.insert(
                cell.clone(),
                cell_canvas.read_pixels(None, sdl2::pixels::PixelFormatEnum::ARGB8888)?,
            );
        }
        let raw_data = &self.render_cache[&cell];

        let top_left = self.point_screen_to_pixel(p);
        assert_eq!(
            self.char_size.width * self.char_size.height * 4,
            raw_data.len()
        );
        assert_eq!(
            self.screen_size.width
                * self.screen_size.height
                * self.char_size.width
                * self.char_size.height
                * 4,
            self.screen_pixel_buf.len()
        );
        for i in 0..self.char_size.width * self.char_size.height {
            let (y, x) = (i / self.char_size.width, i % self.char_size.width);
            let (abs_y, abs_x) = (y + top_left.y as usize, x + top_left.x as usize);
            for k in 0..4 {
                self.screen_pixel_buf
                    [(abs_y * self.screen_size.width * self.char_size.width + abs_x) * 4 + k] =
                    raw_data[i * 4 + k];
            }
        }

        Ok(())
    }

    pub fn fill_rect_buf(&mut self, rect: &Rect, c: &Color) {
        let c = c.to_sdl_color();
        let pix = [c.b, c.g, c.r, 0xFF];
        for y in 0..rect.h {
            let y = (y + rect.y) as usize;
            for x in 0..rect.w {
                let x = (x + rect.x) as usize;
                for k in 0..4 {
                    self.screen_pixel_buf
                        [(y * self.screen_size.width * self.char_size.width + x) * 4 + k] = pix[k];
                }
            }
        }
    }

    pub fn clear_cell(&mut self, p: Point<ScreenCell>) -> Result<(), String> {
        let bg = self.cell_attr.bg;
        let top_left = self.point_screen_to_pixel(p);
        let rect = Rect::new(
            top_left.x,
            top_left.y,
            self.char_size.width as u32,
            self.char_size.height as u32,
        );
        self.fill_rect_buf(&rect, &bg);
        Ok(())
    }
    // range: [l, r)
    pub fn clear_line(&mut self, row: usize, range: Option<(usize, usize)>) -> Result<(), String> {
        let rect = {
            let top_left = self.point_screen_to_pixel(Point::new(0, row));
            if let Some(r) = range {
                Rect::new(
                    (self.char_size.width * r.0) as i32,
                    top_left.y,
                    (self.char_size.width * (r.1 - r.0)) as u32,
                    self.char_size.height as u32,
                )
            } else {
                Rect::new(
                    top_left.x,
                    top_left.y,
                    (self.char_size.width * self.screen_size.width) as u32,
                    self.char_size.height as u32,
                )
            }
        };
        let bg = self.cell_attr.bg;
        self.fill_rect_buf(&rect, &bg);
        Ok(())
    }

    pub fn render(&mut self) -> Result<(), String> {
        let src = &self.screen_pixel_buf[..];
        self.screen_texture
            .with_lock(None, |dst: &mut [u8], _: usize| unsafe {
                std::ptr::copy(src.as_ptr(), dst.as_mut_ptr(), dst.len());
            })
            .unwrap();

        err_str(self.canvas.copy(&self.screen_texture, None, None))?;
        self.canvas.present();
        Ok(())
    }

    fn point_screen_to_pixel(&self, sp: Point<ScreenCell>) -> Point<Pixel> {
        Point::new(
            sp.x as i32 * self.char_size.width as i32,
            sp.y as i32 * self.char_size.height as i32,
        )
    }

    pub fn reset(&mut self) {
        self.screen_begin = 0;
        self.saved_cursor_pos = Point::new(0, 0);
        self.top_line = 0;
        self.bottom_line = self.screen_size.height as isize - 1;
        self.clear();
    }

    pub fn move_cursor(&mut self, m: CursorMove) -> bool {
        use CursorMove::*;
        match m {
            Up => {
                if self.cursor.y > 0 {
                    self.cursor.y -= 1;
                    true
                } else {
                    false
                }
            }
            Down => {
                if self.cursor.y + 1 < self.screen_size.height {
                    self.cursor.y += 1;
                    true
                } else {
                    false
                }
            }
            Left => {
                if self.cursor.x > 0 {
                    self.cursor.x -= 1;
                    true
                } else {
                    false
                }
            }
            LeftMost => {
                self.cursor.x = 0;
                true
            }
            RightMost => {
                self.cursor.x = self.screen_size.width - 1;
                true
            }
            Right => {
                if self.cursor.x + 1 < self.screen_size.width {
                    self.cursor.x += 1;
                    true
                } else {
                    false
                }
            }
            Next => {
                if !self.move_cursor(Right) {
                    self.move_cursor(LeftMost);
                    if !self.move_cursor(Down) {
                        // scroll up
                        let line_px = self.screen_size.width * self.char_size.width * 4;
                        unsafe {
                            std::ptr::copy(
                                self.screen_pixel_buf[line_px * self.char_size.height..].as_ptr(),
                                self.screen_pixel_buf[0..].as_mut_ptr(),
                                line_px * self.char_size.height * (self.screen_size.height - 1),
                            );
                        }
                        self.clear_line(self.screen_size.height - 1, None).unwrap();
                    }
                }
                true
            }
            Prev => {
                if !self.move_cursor(Left) {
                    self.move_cursor(RightMost);
                    if !self.move_cursor(Up) {
                        // scroll down
                        let line_px = self.screen_size.width * self.char_size.width * 4;
                        unsafe {
                            std::ptr::copy(
                                self.screen_pixel_buf[0..].as_ptr(),
                                self.screen_pixel_buf[line_px * self.char_size.height..]
                                    .as_mut_ptr(),
                                line_px * self.char_size.height * (self.screen_size.height - 1),
                            );
                        }
                        self.clear_line(0, None).unwrap();
                    }
                }
                true
            }
        }
    }

    pub fn insert_char(&mut self, c: u8) {
        self.draw_char(char::from(c), self.cursor).unwrap();
        self.render();
        self.move_cursor(CursorMove::Next);
    }
    pub fn insert_chars(&mut self, chars: &[u8]) {
        chars.iter().for_each(|c| self.insert_char(*c));
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<(), String> {
        let mut itr = buf.iter();
        while let Some(c) = itr.next() {
            match *c {
                0 => break,
                b'\n' => {
                    self.move_cursor(CursorMove::RightMost);
                    self.move_cursor(CursorMove::Next);
                }
                b'\r' => {
                    self.move_cursor(CursorMove::LeftMost);
                }
                b'\x08' => {
                    self.move_cursor(CursorMove::Left);
                }

                b'\x1B' => {
                    // begin of escape sequence
                    use super::parse_escape_sequence;
                    use super::ControlOp::*;
                    match parse_escape_sequence(&mut itr) {
                        (Some(op), _) => {
                            println!("{:?}", op);
                            match op {
                                CursorHome(p) => {
                                    let x = wrap_range(p.x - 1, 0, self.screen_size.width - 1);
                                    let y = wrap_range(p.y - 1, 0, self.screen_size.height - 1);
                                    self.cursor = Point::new(x, y);
                                }
                                CursorUp(am) => {
                                    let am = std::cmp::min(am, self.cursor.y as usize);
                                    for _ in 0..am {
                                        self.move_cursor(CursorMove::Up);
                                    }
                                }
                                CursorDown(am) => {
                                    let am = std::cmp::min(
                                        am,
                                        self.screen_size.height - 1 - self.cursor.y as usize,
                                    );
                                    for _ in 0..am {
                                        self.move_cursor(CursorMove::Down);
                                    }
                                }
                                CursorForward(am) => {
                                    let am = std::cmp::min(
                                        am,
                                        self.screen_size.width - 1 - self.cursor.x as usize,
                                    );
                                    for _ in 0..am {
                                        self.move_cursor(CursorMove::Right);
                                    }
                                }
                                CursorBackward(am) => {
                                    let am = std::cmp::min(am, self.cursor.x as usize);
                                    for _ in 0..am {
                                        self.move_cursor(CursorMove::Left);
                                    }
                                }

                                SaveCursor => {
                                    self.saved_cursor_pos = self.cursor;
                                }
                                RestoreCursor => {
                                    self.cursor = self.saved_cursor_pos;
                                }

                                EraseEndOfLine => {
                                    self.clear_line(
                                        self.cursor.y,
                                        Some((self.cursor.x, self.screen_size.width)),
                                    )?;
                                }
                                EraseStartOfLine => {
                                    self.clear_line(self.cursor.y, Some((0, self.cursor.x + 1)))?;
                                }
                                EraseLine => {
                                    self.clear_line(self.cursor.y, None)?;
                                }
                                EraseDown => {
                                    // erase end of line
                                    self.clear_line(
                                        self.cursor.y,
                                        Some((self.cursor.x, self.screen_size.width)),
                                    )?;
                                    // erase down
                                    for row in self.cursor.y + 1..self.screen_size.height {
                                        self.clear_line(row, None)?;
                                    }
                                }
                                EraseUp => {
                                    // erase start of line
                                    self.clear_line(self.cursor.y, Some((0, self.cursor.x + 1)))?;
                                    // erase up
                                    for row in 0..self.cursor.y {
                                        self.clear_line(row, None)?;
                                    }
                                }
                                EraseScreen => {
                                    // erase entire screen
                                    for row in 0..self.screen_size.height {
                                        self.clear_line(row, None)?;
                                    }
                                }
                                Reset => {
                                    self.reset();
                                }
                                SetTopBottom(top, bottom) => {
                                    self.top_line = top;
                                    self.bottom_line = bottom;
                                    // TODO
                                }
                                ChangeCellAttribute(attr) => {
                                    self.set_cell_attribute(attr);
                                }
                                Ignore => {}

                                ScrollDown => {
                                    if !self.move_cursor(CursorMove::Down) {
                                        // next line
                                        let x = self.cursor.x;
                                        self.move_cursor(CursorMove::RightMost);
                                        self.move_cursor(CursorMove::Next);
                                        self.cursor.x = x;
                                    }
                                }
                                ScrollUp => {
                                    if !self.move_cursor(CursorMove::Up) {
                                        // prev line
                                        let x = self.cursor.x;
                                        self.move_cursor(CursorMove::LeftMost);
                                        self.move_cursor(CursorMove::Prev);
                                        self.cursor.x = x;
                                    }
                                }

                                SetCursorMode(to_set) => {
                                    // currently, it is not meaningful
                                    // TODO
                                }
                            }
                        }
                        (None, sz) => {
                            // print sequence as string
                            self.insert_chars(b"^[");
                            self.insert_chars(&itr.as_slice()[..sz]);
                            if sz > 0 {
                                itr.nth(sz - 1);
                            }
                        }
                    }
                }
                x => self.insert_char(x),
            }
        }
        Ok(())
    }
}
