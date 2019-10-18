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

use crate::basics::*;

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

#[derive(Debug)]
enum ControlOp {
    CursorHome(Point<ScreenCell>),
    CursorUp(usize),
    CursorDown(usize),
    CursorForward(usize),
    CursorBackward(usize),
    SaveCursor,
    RestoreCursor,
    ScrollDown,
    ScrollUp,
    EraseEndOfLine,
    EraseStartOfLine,
    EraseLine,
    EraseDown,
    EraseUp,
    EraseScreen,
    SetTopBottom(isize, isize),
    Reset,
    Ignore,
}
fn parse_escape_sequence<'a>(itr: &mut std::slice::Iter<'a, u8>) -> (Option<ControlOp>, usize) {
    let backup = itr.clone();
    match itr.next() {
        Some(c) => {
            let mut read_bytes = 1;
            let op = match c {
                // escape sequences
                b'[' => {
                    use regex::bytes::Regex;
                    lazy_static! {
                        static ref ARGUMENTS: Regex = Regex::new(r"^(\d*(;\d*)*)?").unwrap();
                    }
                    let args = match ARGUMENTS
                        .captures(itr.as_slice())
                        .and_then(|cap| cap.get(1))
                    {
                        Some(mt) if mt.as_bytes().len() > 0 => {
                            let bytes = mt.as_bytes();
                            read_bytes += bytes.len();
                            itr.nth(bytes.len() - 1); // skip
                            bytes
                                .split(|b| *b == b';')
                                .map(|b| parse_int_from_ascii(b))
                                .collect::<Vec<_>>()
                        }
                        _ => Vec::new(),
                    };
                    #[cfg(debug_assertions)]
                    println!("args:{:?}", args);

                    match itr.next() {
                        // Cursor Home
                        Some(b'f') | Some(b'H') => match args.len() {
                            0 => Some(ControlOp::CursorHome(Point::new(0, 0))),
                            2 => Some(ControlOp::CursorHome(Point::new(
                                args[1].unwrap_or(0) as isize,
                                args[0].unwrap_or(0) as isize,
                            ))),
                            _ => None,
                        },
                        // Cursor Up
                        Some(b'A') => match args.len() {
                            0 => Some(ControlOp::CursorUp(1)),
                            1 => Some(ControlOp::CursorUp(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Down
                        Some(b'B') => match args.len() {
                            0 => Some(ControlOp::CursorDown(1)),
                            1 => Some(ControlOp::CursorDown(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Forward
                        Some(b'C') => match args.len() {
                            0 => Some(ControlOp::CursorForward(1)),
                            1 => Some(ControlOp::CursorForward(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Backward
                        Some(b'D') => match args.len() {
                            0 => Some(ControlOp::CursorBackward(1)),
                            1 => Some(ControlOp::CursorBackward(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },

                        // Save cursor position
                        Some(b's') => match args.len() {
                            0 => Some(ControlOp::SaveCursor),
                            _ => None,
                        },
                        // Restore cursor position
                        Some(b'u') => match args.len() {
                            0 => Some(ControlOp::RestoreCursor),
                            _ => None,
                        },

                        // Erase end of line
                        Some(b'K') => match args.len() {
                            0 => Some(ControlOp::EraseEndOfLine),
                            1 => match args[0] {
                                Some(0) => Some(ControlOp::EraseEndOfLine),
                                Some(1) => Some(ControlOp::EraseStartOfLine),
                                Some(2) => Some(ControlOp::EraseLine),
                                _ => None,
                            },
                            _ => None,
                        },
                        Some(b'J') => match args.len() {
                            0 => Some(ControlOp::EraseDown),
                            1 => match args[0] {
                                Some(0) => Some(ControlOp::EraseDown),
                                Some(1) => Some(ControlOp::EraseUp),
                                Some(2) => Some(ControlOp::EraseScreen),
                                _ => None,
                            },
                            _ => None,
                        },

                        Some(b'r') => match args.len() {
                            2 => match (args[0], args[1]) {
                                (Some(x), Some(y)) => {
                                    Some(ControlOp::SetTopBottom(x as isize, y as isize))
                                }
                                _ => None,
                            },
                            _ => None,
                        },
                        Some(x) => {
                            #[cfg(debug_assertions)]
                            println!("unsupported: \\E[{}", char::from(*x));
                            None
                        }
                        None => None,
                    }
                }
                b'D' => Some(ControlOp::ScrollDown),
                b'M' => Some(ControlOp::ScrollUp),
                b'=' => Some(ControlOp::Ignore),
                b'>' => Some(ControlOp::Ignore),
                b'c' => Some(ControlOp::Reset),
                x => {
                    #[cfg(debug_assertions)]
                    println!("unsupported: \\E{}", char::from(*x));
                    None
                }
            };
            // revert iterator if it is followed by a invalid sequence
            if op.is_none() {
                *itr = backup;
            }
            (op, read_bytes)
        }
        None => (None, 0),
    }
}

pub struct Term<'ttf> {
    canvas: sdl2::render::Canvas<sdl2::video::Window>,
    font: sdl2::ttf::Font<'ttf, 'static>,
    buf: Buffer,
    screen_size: Size<usize>,
    char_size: Size<usize>,

    screen_begin: usize,
    saved_cursor_pos: Point<ScreenCell>,

    top_line: isize,
    bottom_line: isize,
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
        let canvas = window
            .into_canvas()
            .accelerated()
            .target_texture()
            .build()
            .unwrap();

        assert!(size.height > 0);

        let mut term = Term {
            canvas,
            font,
            buf: Buffer::new(size.width),
            screen_size: size,
            screen_begin: 0,
            char_size,
            saved_cursor_pos: Point::new(0, 0),
            top_line: 0,
            bottom_line: size.height as isize - 1,
        };
        term.clear();
        term
    }
    pub fn clear(&mut self) {
        self.canvas.set_draw_color(Color::RGB(0, 0, 32));
        self.canvas.clear();
    }

    fn point_screen_to_pixel(&self, sp: Point<ScreenCell>) -> Point<Pixel> {
        Point::new(
            sp.x as i32 * self.char_size.width as i32,
            sp.y as i32 * self.char_size.height as i32,
        )
    }

    fn draw_char(&mut self, c: char, p: Point<ScreenCell>) -> Result<(), String> {
        let surface = conv_err(
            self.font
                .render(&c.to_string())
                .blended(Color::RGB(255, 255, 255)),
        )?;

        let tc = self.canvas.texture_creator();
        let texture = conv_err(tc.create_texture_from_surface(surface))?;
        let top_left = self.point_screen_to_pixel(p);
        let rect = Rect::new(
            top_left.x,
            top_left.y,
            texture.query().width,
            texture.query().height,
        );
        conv_err(self.canvas.copy(&texture, None, rect))?;

        Ok(())
    }

    // get current screen-cursor position (from buffer-cursor position)
    fn get_cursor_pos(&self) -> Point<ScreenCell> {
        Point::new(
            self.buf.cursor.x as isize,
            (self.buf.cursor.y - self.screen_begin) as isize,
        )
    }
    // return buffer cell position in the buffer from the screen cell position
    fn get_buffer_cell_pos(&self, pos: Point<ScreenCell>) -> Option<Point<BufferCell>> {
        if pos.x < 0 || pos.x >= self.buf.width as isize || pos.y < 0 {
            None
        } else {
            Some(Point::new(
                pos.x as usize,
                (self.screen_begin + pos.y as usize) as usize,
            ))
        }
    }
    fn set_cursor_pos_wrap(&mut self, p: Point<ScreenCell>) {
        let (w, h) = (
            self.screen_size.width as isize,
            self.screen_size.height as isize,
        );
        let wrapped = Point::new(
            if p.x < 0 {
                0
            } else if p.x >= w {
                w - 1
            } else {
                p.x
            },
            if p.y < self.top_line {
                0
            } else if p.y >= h {
                h - 1
            } else {
                p.y
            },
        );

        #[cfg(debug_assertions)]
        println!("set cursor (wrapped): {:?}", wrapped);

        self.buf
            .set_cursor_pos(self.get_buffer_cell_pos(wrapped).unwrap());
    }

    pub fn render_all(&mut self) -> Result<(), String> {
        self.clear();

        // draw entire screen
        for r in 0..self.screen_size.height {
            if self.buf.data.len() <= r + self.screen_begin {
                break;
            }
            for c in 0..self.screen_size.width {
                self.draw_char(
                    self.buf.data[r + self.screen_begin][c],
                    Point::new(c as isize, r as isize),
                )?;
            }
        }

        self.canvas.set_draw_color(Color::RGB(200, 200, 200));

        // draw cursor
        let top_left = self.point_screen_to_pixel(self.get_cursor_pos());
        self.canvas.fill_rect(Some(Rect::new(
            top_left.x,
            top_left.y,
            self.char_size.width as u32,
            self.char_size.height as u32,
        )))?;

        self.canvas.present();
        Ok(())
    }

    pub fn insert_char(&mut self, c: u8) {
        self.buf.put_char(char::from(c));
        self.buf.move_cursor(CursorMove::Next);
    }
    pub fn insert_chars(&mut self, chars: &[u8]) {
        for c in chars.iter() {
            self.insert_char(*c);
        }
    }

    pub fn reset(&mut self) {
        self.buf.reset();
        self.screen_begin = 0;
        self.saved_cursor_pos = Point::new(0, 0);
        self.top_line = 0;
        self.bottom_line = self.screen_size.height as isize - 1;
    }

    pub fn write(&mut self, buf: &[u8]) {
        let mut itr = buf.iter();
        while let Some(c) = itr.next() {
            match *c {
                0 => break,
                b'\n' => {
                    self.buf.move_cursor(CursorMove::Down);
                    if self.get_cursor_pos().y >= self.screen_size.height as isize {
                        self.screen_begin +=
                            self.get_cursor_pos().y as usize - self.screen_size.height + 1;
                    }
                }
                b'\r' => {
                    self.buf.move_cursor(CursorMove::LeftMost);
                }
                b'\x08' => {
                    self.buf.move_cursor(CursorMove::Left);
                }

                // escape char
                b'\x1B' => {
                    use ControlOp::*;
                    match parse_escape_sequence(&mut itr) {
                        (Some(op), _) => {
                            println!("{:?}", op);
                            match op {
                                CursorHome(p) => {
                                    self.set_cursor_pos_wrap(Point::new(p.x - 1, p.y - 1))
                                }
                                CursorUp(am) => {
                                    let am = std::cmp::min(am, self.get_cursor_pos().y as usize);
                                    for _ in 0..am {
                                        self.buf.move_cursor(CursorMove::Up);
                                    }
                                }
                                CursorDown(am) => {
                                    let am = std::cmp::min(
                                        am,
                                        self.screen_size.height
                                            - 1
                                            - self.get_cursor_pos().y as usize,
                                    );
                                    for _ in 0..am {
                                        self.buf.move_cursor(CursorMove::Down);
                                    }
                                }
                                CursorForward(am) => {
                                    let am = std::cmp::min(
                                        am,
                                        self.screen_size.width
                                            - 1
                                            - self.get_cursor_pos().x as usize,
                                    );
                                    for _ in 0..am {
                                        self.buf.move_cursor(CursorMove::Right);
                                    }
                                }
                                CursorBackward(am) => {
                                    let am = std::cmp::min(am, self.get_cursor_pos().x as usize);
                                    for _ in 0..am {
                                        self.buf.move_cursor(CursorMove::Left);
                                    }
                                }

                                SaveCursor => {
                                    self.saved_cursor_pos = self.get_cursor_pos();
                                }
                                RestoreCursor => {
                                    self.set_cursor_pos_wrap(self.saved_cursor_pos);
                                }

                                EraseEndOfLine => {
                                    self.buf.clear_line(
                                        self.buf.cursor.y,
                                        (self.buf.cursor.x, self.buf.width),
                                    );
                                }
                                EraseStartOfLine => {
                                    self.buf
                                        .clear_line(self.buf.cursor.y, (0, self.buf.cursor.x));
                                }
                                EraseLine => {
                                    self.buf.clear_line(self.buf.cursor.y, (0, self.buf.width));
                                }
                                EraseDown => {
                                    // erase end of line
                                    self.buf.clear_line(
                                        self.buf.cursor.y,
                                        (self.buf.cursor.x, self.buf.width),
                                    );
                                    // erase down
                                    for row in self.buf.cursor.y
                                        ..(self.screen_begin + self.screen_size.height)
                                    {
                                        self.buf.clear_line(row, (0, self.buf.width));
                                    }
                                }
                                EraseUp => {
                                    // erase start of line
                                    self.buf
                                        .clear_line(self.buf.cursor.y, (0, self.buf.cursor.x));
                                    // erase up
                                    for row in self.screen_begin..self.buf.cursor.y {
                                        self.buf.clear_line(row, (0, self.buf.width));
                                    }
                                }
                                EraseScreen => {
                                    // erase entire screen
                                    for row in self.screen_begin
                                        ..(self.screen_begin + self.screen_size.height)
                                    {
                                        self.buf.clear_line(row, (0, self.buf.width));
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
                                Ignore => {}
                                _ => unimplemented!(),
                            }
                        }
                        (None, sz) => {
                            // print sequence
                            self.insert_chars(b"^[");
                            self.insert_chars(&itr.as_slice()[..sz]);
                            itr.nth(sz);
                        }
                    }
                }
                x => self.insert_char(x),
            }
        }
    }
}
