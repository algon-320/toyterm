use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};
use nix::sys::stat::Mode;
use nix::unistd;

use std::os::unix::io::RawFd;
use std::path::Path;

use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::ttf;

use crate::basics::conv_err;
use crate::basics::PositionType::{BufferCell, Pixel, ScreenCell};
use crate::basics::{Position, Size};

mod buffer;
use buffer::*;

#[derive(Debug, Clone, Copy)]
pub struct PTY {
    pub master: RawFd,
    pub slave: RawFd,
}

impl PTY {
    pub fn open() -> Result<Self, String> {
        // Open a new PTY master
        let master_fd = conv_err(posix_openpt(OFlag::O_RDWR))?;

        // Allow a slave to be generated for it
        conv_err(grantpt(&master_fd))?;
        conv_err(unlockpt(&master_fd))?;

        // Get the name of the slave
        let slave_name = conv_err(unsafe { ptsname(&master_fd) })?;

        // Try to open the slave
        let slave_fd = conv_err(open(Path::new(&slave_name), OFlag::O_RDWR, Mode::empty()))?;

        use std::os::unix::io::IntoRawFd;
        Ok(PTY {
            master: master_fd.into_raw_fd(),
            slave: slave_fd.into(),
        })
    }
}

pub struct Term<'ttf> {
    canvas: sdl2::render::Canvas<sdl2::video::Window>,
    font: sdl2::ttf::Font<'ttf, 'static>,
    buf: Buffer,
    screen_size: Size<usize>,
    screen_begin: usize,
    char_size: Size<usize>,
}
impl<'ttf> Term<'ttf> {
    pub fn new<P: AsRef<Path>>(
        sdl_context: &sdl2::Sdl,
        ttf_context: &'ttf sdl2::ttf::Sdl2TtfContext,
        size: Size<usize>,
        font_path: P,
        font_size: u16,
    ) -> Self {
        let font = ttf_context.load_font(font_path, font_size).unwrap();
        let char_size = font.size_of_char('#').unwrap();
        let char_size = Size::new(char_size.0 as usize, char_size.1 as usize);
        println!("font char size: {:?}", char_size);

        let window = {
            let video = sdl_context.video().unwrap();
            video
                .window(
                    "toyterm",
                    (char_size.width * size.width) as u32,
                    (char_size.height * size.height) as u32,
                )
                .position_centered()
                .build()
                .unwrap()
        };
        let canvas = window.into_canvas().build().unwrap();

        let mut term = Term {
            canvas,
            font,
            buf: Buffer::new(size.width),
            screen_size: size,
            screen_begin: 0,
            char_size,
        };
        term.clear();
        term
    }
    pub fn clear(&mut self) {
        self.canvas.set_draw_color(Color::RGB(0, 0, 32));
        self.canvas.clear();
    }

    fn draw_char(&mut self, c: char, p: Position<ScreenCell>) -> Result<(), String> {
        let surface = conv_err(
            self.font
                .render(&c.to_string())
                .blended(Color::RGB(255, 255, 255)),
        )?;

        {
            let tc = self.canvas.texture_creator();
            let texture = conv_err(tc.create_texture_from_surface(surface))?;
            let rect = Rect::new(
                (p.x * self.char_size.width) as i32,
                (p.y * self.char_size.height) as i32,
                texture.query().width,
                texture.query().height,
            );
            conv_err(self.canvas.copy(&texture, None, rect))?;
        }

        Ok(())
    }

    // get current screen-cursor position (from buffer-cursor position)
    fn get_cursor_pos(&self) -> Position<ScreenCell> {
        Position::new(self.buf.cursor.x, self.buf.cursor.y - self.screen_begin)
    }

    pub fn render_all(&mut self) -> Result<(), String> {
        self.clear();

        // draw entire screen
        for r in 0..self.screen_size.height {
            for c in 0..self.screen_size.width {
                if self.buf.data.len() <= r + self.screen_begin {
                    break;
                }
                self.draw_char(self.buf.data[r + self.screen_begin][c], Position::new(c, r))?;
            }
        }

        self.canvas.set_draw_color(Color::RGB(200, 200, 200));
        let cursor_p = self.get_cursor_pos();
        // draw cursor
        self.canvas.draw_rect(Rect::new(
            (cursor_p.x * self.char_size.width) as i32,
            (cursor_p.y * self.char_size.height) as i32,
            self.char_size.width as u32,
            self.char_size.height as u32,
        ))?;

        self.canvas.present();
        Ok(())
    }

    pub fn write(&mut self, buf: &[u8]) {
        for c in buf.iter() {
            match *c {
                0 => break,
                b'\n' => {
                    self.buf.move_cursor(CursorMove::Down);
                    if self.get_cursor_pos().y >= self.screen_size.height {
                        self.screen_begin += self.get_cursor_pos().y - self.screen_size.height + 1;
                    }
                }
                b'\r' => {
                    self.buf.move_cursor(CursorMove::LeftMost);
                }
                b'\x08' => {
                    self.buf.move_cursor(CursorMove::Left);
                }
                x => {
                    self.buf.put_char(char::from(x));
                    self.buf.move_cursor(CursorMove::Next);
                }
            }
        }
    }
}
