pub mod pty;

use std::collections::HashMap;
use std::path::Path;

use sdl2::rect::Rect;
use sdl2::render::Texture;
use sdl2::ttf;
use sdl2::ttf::FontStyle;

use crate::basics::*;
use crate::utils::*;

mod render;
use render::*;

mod term;
pub use term::*;

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
    ChangeCellAttribute(CellAttribute),
    SetCursorMode(bool),
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
                    let (args, fin_char) = {
                        let mut args = Vec::new();
                        let mut fin_char = None;
                        let mut tmp = None;
                        while let Some(c) = itr.next() {
                            read_bytes += 1;
                            match *c {
                                x if b'0' <= x && x <= b'9' => {
                                    if tmp.is_none() {
                                        tmp = Some(0);
                                    } else {
                                        tmp = Some(tmp.unwrap() * 10);
                                    }
                                    tmp = Some(tmp.unwrap() + (x - b'0') as u32);
                                }
                                b';' => {
                                    args.push(tmp);
                                    tmp = None;
                                }
                                x => {
                                    fin_char = Some(x);
                                    break;
                                }
                            }
                        }
                        if tmp.is_some() {
                            args.push(tmp);
                        }
                        (args, fin_char)
                    };
                    #[cfg(debug_assertions)]
                    println!("args:{:?}", args);

                    match fin_char {
                        // Cursor Home
                        Some(b'f') | Some(b'H') => match args.len() {
                            0 => Some(ControlOp::CursorHome(Point::new(1, 1))),
                            2 => Some(ControlOp::CursorHome(Point::new(
                                args[1].unwrap_or(1) as usize,
                                args[0].unwrap_or(1) as usize,
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

                        Some(b'm') => {
                            let mut style = CellAttribute::default();
                            for a in args.iter() {
                                match a {
                                    Some(0) => {
                                        // reset
                                        style = CellAttribute::default();
                                    }
                                    Some(1) => {
                                        style.style = Style::Bold;
                                    }
                                    Some(4) => {
                                        style.style = Style::UnderLine;
                                    }
                                    Some(5) => {
                                        style.style = Style::Blink;
                                    }
                                    Some(7) => {
                                        style.style = Style::Reverse;
                                    }
                                    _ => {}
                                }
                            }
                            Some(ControlOp::ChangeCellAttribute(style))
                        }

                        Some(b'?') => {
                            read_bytes += 1;
                            let p = || -> Option<(u8, u8)> { Some((*itr.next()?, *itr.next()?)) }();
                            match p {
                                Some((b'1', b'h')) => Some(ControlOp::SetCursorMode(true)),
                                Some((b'1', b'l')) => Some(ControlOp::SetCursorMode(false)),
                                _ => None,
                            }
                        }

                        Some(x) => {
                            #[cfg(debug_assertions)]
                            println!("unsupported: \\E[{}", char::from(x));
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

// type raw_pixels = Vec<u32>;

// pub struct Term<'a, 'b> {
//     canvas: &'a mut sdl2::render::Canvas<sdl2::video::Window>,
//     font: &'a mut sdl2::ttf::Font<'b, 'static>,
//     buf: Buffer,
//     screen_size: Size<usize>,
//     char_size: Size<usize>,

//     screen_begin: usize,
//     saved_cursor_pos: Point<ScreenCell>,

//     top_line: isize,
//     bottom_line: isize,
//     cell_style: CellStyle,

//     texture_creator: &'a sdl2::render::TextureCreator<sdl2::video::WindowContext>,
//     render_cache: std::collections::HashMap<(char, CellStyle), Texture<'a>>,
// }
// impl<'a, 'b> Term<'a, 'b> {
//     pub fn new(
//         canvas: &'a mut sdl2::render::Canvas<sdl2::video::Window>,
//         texture_creator: &'a mut sdl2::render::TextureCreator<sdl2::video::WindowContext>,
//         font: &'a mut sdl2::ttf::Font<'b, 'static>,
//         size: Size<usize>,
//     ) -> Self {
//         let char_size = {
//             let tmp = font.size_of_char('#').unwrap();
//             Size::new(tmp.0 as usize, tmp.1 as usize)
//         };
//         println!("font char size: {:?}", char_size);

//         assert!(size.height > 0);

//         let mut term = Term {
//             canvas,
//             texture_creator,
//             font,

//             buf: Buffer::new(size.width),
//             screen_size: size,
//             screen_begin: 0,
//             char_size,
//             saved_cursor_pos: Point::new(0, 0),
//             top_line: 0,
//             bottom_line: size.height as isize - 1,
//             cell_style: CellStyle::default(),

//             render_cache: HashMap::new(),
//         };
//         term.clear();
//         term
//     }

//     pub fn reset(&mut self) {
//         self.buf.reset();
//         self.screen_begin = 0;
//         self.saved_cursor_pos = Point::new(0, 0);
//         self.top_line = 0;
//         self.bottom_line = self.screen_size.height as isize - 1;
//         self.cell_style = CellStyle::default();
//         self.change_cell_style(CellStyle::default());
//     }

//     pub fn clear(&mut self) {
//         self.canvas
//             .set_draw_color(sdl2::pixels::Color::RGB(0, 0, 32));
//         self.canvas.clear();
//     }

//     fn point_screen_to_pixel(&self, sp: Point<ScreenCell>) -> Point<Pixel> {
//         Point::new(
//             sp.x as i32 * self.char_size.width as i32,
//             sp.y as i32 * self.char_size.height as i32,
//         )
//     }

//     fn change_cell_style(&mut self, cell_style: CellStyle) {
//         self.cell_style = cell_style;
//     }

//     fn draw_char(&mut self, c: char, p: Point<ScreenCell>, style: CellStyle) -> Result<(), String> {
//         let mut fg_color = style.fg.to_sdl_color();
//         let mut bg_color = style.bg.to_sdl_color();

//         if style.style == Style::Reverse {
//             println!("reversed");
//             std::mem::swap(&mut fg_color, &mut bg_color);
//         }

//         // clear
//         self.canvas.set_draw_color(bg_color);
//         let top_left = self.point_screen_to_pixel(p);
//         self.canvas.fill_rect(Some(Rect::new(
//             top_left.x,
//             top_left.y,
//             self.char_size.width as u32,
//             self.char_size.height as u32,
//         )))?;

//         if style.style == Style::Bold {
//             self.font.set_style(FontStyle::BOLD);
//         } else {
//             self.font.set_style(FontStyle::NORMAL);
//         }

//         // generate texture
//         if !self.render_cache.contains_key(&(c, style)) {
//             let surface = err_str(self.font.render_char(c).blended(fg_color))?;
//             let texture = err_str(self.texture_creator.create_texture_from_surface(surface))?;
//             self.render_cache.insert((c, style), texture);
//         }
//         let texture = &self.render_cache[&(c, style)];
//         let top_left = self.point_screen_to_pixel(p);
//         let rect = Rect::new(
//             top_left.x,
//             top_left.y,
//             self.char_size.width as u32,
//             self.char_size.height as u32,
//         );
//         err_str(self.canvas.copy(&texture, None, rect))?;

//         Ok(())
//     }

//     // get current screen-cursor position (from buffer-cursor position)
//     fn get_cursor_pos(&self) -> Point<ScreenCell> {
//         Point::new(
//             self.buf.cursor.x as isize,
//             (self.buf.cursor.y - self.screen_begin) as isize,
//         )
//     }
//     // return buffer cell position in the buffer from the screen cell position
//     fn get_buffer_cell_pos(&self, pos: Point<ScreenCell>) -> Option<Point<BufferCell>> {
//         if pos.x < 0 || pos.x >= self.buf.width as isize || pos.y < 0 {
//             None
//         } else {
//             Some(Point::new(
//                 pos.x as usize,
//                 (self.screen_begin + pos.y as usize) as usize,
//             ))
//         }
//     }
//     fn set_cursor_pos_wrap(&mut self, p: Point<ScreenCell>) {
//         let (w, h) = (
//             self.screen_size.width as isize,
//             self.screen_size.height as isize,
//         );
//         let wrapped = Point::new(
//             if p.x < 0 {
//                 0
//             } else if p.x >= w {
//                 w - 1
//             } else {
//                 p.x
//             },
//             if p.y < self.top_line {
//                 0
//             } else if p.y >= h {
//                 h - 1
//             } else {
//                 p.y
//             },
//         );

//         #[cfg(debug_assertions)]
//         println!("set cursor (wrapped): {:?}", wrapped);

//         self.buf
//             .set_cursor_pos(self.get_buffer_cell_pos(wrapped).unwrap());
//     }

//     pub fn render_all(&mut self) -> Result<(), String> {
//         self.clear();

//         // draw entire screen
//         for r in 0..self.screen_size.height {
//             if self.buf.data.len() <= r + self.screen_begin {
//                 break;
//             }
//             for c in 0..self.screen_size.width {
//                 let (ch, style) = self.buf.data[r + self.screen_begin][c];
//                 self.draw_char(ch, Point::new(c as isize, r as isize), style)?;
//             }
//         }

//         // draw cursor
//         self.canvas
//             .set_draw_color(sdl2::pixels::Color::RGB(200, 200, 200));
//         let top_left = self.point_screen_to_pixel(self.get_cursor_pos());
//         self.canvas.fill_rect(Some(Rect::new(
//             top_left.x,
//             top_left.y,
//             self.char_size.width as u32,
//             self.char_size.height as u32,
//         )))?;

//         self.canvas.present();
//         Ok(())
//     }

//     pub fn insert_char(&mut self, c: u8) {
//         self.buf.put_char(char::from(c), self.cell_style);
//         self.buf.move_cursor(CursorMove::Next);
//     }
//     pub fn insert_chars(&mut self, chars: &[u8]) {
//         for c in chars.iter() {
//             self.insert_char(*c);
//         }
//     }

//     pub fn write(&mut self, buf: &[u8]) {
//         let mut itr = buf.iter();
//         while let Some(c) = itr.next() {
//             match *c {
//                 0 => break,
//                 b'\n' => {
//                     self.buf.move_cursor(CursorMove::Down);
//                     if self.get_cursor_pos().y >= self.screen_size.height as isize {
//                         self.screen_begin +=
//                             self.get_cursor_pos().y as usize - self.screen_size.height + 1;
//                     }
//                 }
//                 b'\r' => {
//                     self.buf.move_cursor(CursorMove::LeftMost);
//                 }
//                 b'\x08' => {
//                     self.buf.move_cursor(CursorMove::Left);
//                 }

//                 b'\x1B' => {
//                     // start escape sequence

//                     use ControlOp::*;
//                     match parse_escape_sequence(&mut itr) {
//                         (Some(op), _) => {
//                             println!("{:?}", op);
//                             match op {
//                                 CursorHome(p) => {
//                                     self.set_cursor_pos_wrap(Point::new(p.x - 1, p.y - 1))
//                                 }
//                                 CursorUp(am) => {
//                                     let am = std::cmp::min(am, self.get_cursor_pos().y as usize);
//                                     for _ in 0..am {
//                                         self.buf.move_cursor(CursorMove::Up);
//                                     }
//                                 }
//                                 CursorDown(am) => {
//                                     let am = std::cmp::min(
//                                         am,
//                                         self.screen_size.height
//                                             - 1
//                                             - self.get_cursor_pos().y as usize,
//                                     );
//                                     for _ in 0..am {
//                                         self.buf.move_cursor(CursorMove::Down);
//                                     }
//                                 }
//                                 CursorForward(am) => {
//                                     let am = std::cmp::min(
//                                         am,
//                                         self.screen_size.width
//                                             - 1
//                                             - self.get_cursor_pos().x as usize,
//                                     );
//                                     for _ in 0..am {
//                                         self.buf.move_cursor(CursorMove::Right);
//                                     }
//                                 }
//                                 CursorBackward(am) => {
//                                     let am = std::cmp::min(am, self.get_cursor_pos().x as usize);
//                                     for _ in 0..am {
//                                         self.buf.move_cursor(CursorMove::Left);
//                                     }
//                                 }

//                                 SaveCursor => {
//                                     self.saved_cursor_pos = self.get_cursor_pos();
//                                 }
//                                 RestoreCursor => {
//                                     self.set_cursor_pos_wrap(self.saved_cursor_pos);
//                                 }

//                                 EraseEndOfLine => {
//                                     self.buf.clear_line(
//                                         self.buf.cursor.y,
//                                         (self.buf.cursor.x, self.buf.width),
//                                     );
//                                 }
//                                 EraseStartOfLine => {
//                                     self.buf
//                                         .clear_line(self.buf.cursor.y, (0, self.buf.cursor.x));
//                                 }
//                                 EraseLine => {
//                                     self.buf.clear_line(self.buf.cursor.y, (0, self.buf.width));
//                                 }
//                                 EraseDown => {
//                                     // erase end of line
//                                     self.buf.clear_line(
//                                         self.buf.cursor.y,
//                                         (self.buf.cursor.x, self.buf.width),
//                                     );
//                                     // erase down
//                                     for row in self.buf.cursor.y
//                                         ..(self.screen_begin + self.screen_size.height)
//                                     {
//                                         self.buf.clear_line(row, (0, self.buf.width));
//                                     }
//                                 }
//                                 EraseUp => {
//                                     // erase start of line
//                                     self.buf
//                                         .clear_line(self.buf.cursor.y, (0, self.buf.cursor.x));
//                                     // erase up
//                                     for row in self.screen_begin..self.buf.cursor.y {
//                                         self.buf.clear_line(row, (0, self.buf.width));
//                                     }
//                                 }
//                                 EraseScreen => {
//                                     // erase entire screen
//                                     for row in self.screen_begin
//                                         ..(self.screen_begin + self.screen_size.height)
//                                     {
//                                         self.buf.clear_line(row, (0, self.buf.width));
//                                     }
//                                 }
//                                 Reset => {
//                                     self.reset();
//                                 }
//                                 SetTopBottom(top, bottom) => {
//                                     self.top_line = top;
//                                     self.bottom_line = bottom;
//                                     // TODO
//                                 }
//                                 ChangeCellAttribute(style) => {
//                                     self.change_cell_style(style);
//                                 }
//                                 Ignore => {}

//                                 ScrollDown => {
//                                     self.buf.move_cursor(CursorMove::Down);
//                                 }
//                                 ScrollUp => {
//                                     self.screen_begin -= 1;
//                                     self.buf.move_cursor(CursorMove::Up);
//                                 }

//                                 SetCursorMode(to_set) => {
//                                     // currently, it is not meaningful
//                                     // TODO
//                                 }
//                             }
//                         }
//                         (None, sz) => {
//                             // print sequence as string
//                             self.insert_chars(b"^[");
//                             self.insert_chars(&itr.as_slice()[..sz]);
//                             itr.nth(sz - 1);
//                         }
//                     }
//                 }
//                 x => self.insert_char(x),
//             }
//         }
//     }
// }
