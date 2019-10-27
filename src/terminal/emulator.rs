use crate::basics::*;
use crate::utils::*;

use super::control::parse_escape_sequence;
use super::control::ControlOp;
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
    renderer: Renderer<'a, 'b>,

    screen_size: Size<usize>,
    screen_begin: usize,
    cursor: Point<ScreenCell>,
    saved_cursor_pos: Point<ScreenCell>,

    top_line: usize,
    bottom_line: usize,
    left_column: usize,
    right_column: usize,
}
impl<'a, 'b> Term<'a, 'b> {
    pub fn new(render_context: &'a mut RenderContext<'b>, size: Size<usize>) -> Self {
        assert!(size.height > 0);
        let mut term = Term {
            renderer: Renderer::new(render_context, size),

            screen_size: size,
            screen_begin: 0,
            cursor: Point::new(0, 0),
            saved_cursor_pos: Point::new(0, 0),

            top_line: 0,
            bottom_line: size.height - 1,
            left_column: 0,
            right_column: size.width - 1,
        };
        term.renderer.clear();
        term
    }

    pub fn render(&mut self) -> Result<(), String> {
        self.renderer.render(Some(&self.cursor))
    }

    pub fn reset(&mut self) {
        self.screen_begin = 0;
        self.saved_cursor_pos = Point::new(0, 0);

        self.top_line = 0;
        self.bottom_line = self.screen_size.height - 1;
        self.left_column = 0;
        self.right_column = self.screen_size.width - 1;

        self.renderer.clear();
    }

    pub fn move_cursor(&mut self, m: CursorMove) -> bool {
        use CursorMove::*;
        match m {
            Up => {
                if self.cursor.y > self.top_line {
                    self.cursor.y -= 1;
                    true
                } else {
                    false
                }
            }
            Down => {
                if self.cursor.y < self.bottom_line {
                    self.cursor.y += 1;
                    true
                } else {
                    false
                }
            }
            Left => {
                if self.cursor.x > self.left_column {
                    self.cursor.x -= 1;
                    true
                } else {
                    false
                }
            }
            LeftMost => {
                self.cursor.x = self.left_column;
                true
            }
            RightMost => {
                self.cursor.x = self.right_column;
                true
            }
            Right => {
                if self.cursor.x < self.right_column {
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
                        self.renderer.scroll_up(self.top_line, self.bottom_line);
                    }
                }
                true
            }
            Prev => {
                if !self.move_cursor(Left) {
                    self.move_cursor(RightMost);
                    if !self.move_cursor(Up) {
                        self.renderer.scroll_down(self.top_line, self.bottom_line);
                    }
                }
                true
            }
        }
    }

    pub fn insert_char(&mut self, c: u8) {
        self.renderer.draw_char(char::from(c), self.cursor).unwrap();
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
                b'\x07' => {
                    // bell
                }
                b'\n' => {
                    #[cfg(debug_assertions)]
                    println!("[next line]");
                    if !self.move_cursor(CursorMove::Down) {
                        // next line
                        let x = self.cursor.x;
                        self.move_cursor(CursorMove::RightMost);
                        self.move_cursor(CursorMove::Next);
                        self.cursor.x = x;
                    }
                }
                b'\r' => {
                    #[cfg(debug_assertions)]
                    println!("[move left most]");
                    self.move_cursor(CursorMove::LeftMost);
                }
                b'\t' => {
                    #[cfg(debug_assertions)]
                    println!("[TAB]");
                    while self.cursor.x % 8 > 0 {
                        self.move_cursor(CursorMove::Right);
                    }
                }
                b'\x08' => {
                    #[cfg(debug_assertions)]
                    println!("[back]");
                    self.move_cursor(CursorMove::Left);
                }

                b'\x1B' => {
                    // begin of escape sequence
                    use ControlOp::*;
                    match parse_escape_sequence(&mut itr) {
                        (Some(op), _) => {
                            #[cfg(debug_assertions)]
                            println!("{:?}", op);
                            match op {
                                CursorHome(p) => {
                                    let x = wrap_range(p.x - 1, 0, self.screen_size.width - 1);
                                    let y = wrap_range(p.y - 1, 0, self.screen_size.height - 1);
                                    self.cursor = Point::new(x, y);
                                }
                                CursorUp(am) => {
                                    let am = std::cmp::min(am, self.cursor.y - self.top_line);
                                    for _ in 0..am {
                                        self.move_cursor(CursorMove::Up);
                                    }
                                }
                                CursorDown(am) => {
                                    let am = std::cmp::min(am, self.bottom_line - self.cursor.y);
                                    for _ in 0..am {
                                        self.move_cursor(CursorMove::Down);
                                    }
                                }
                                CursorForward(am) => {
                                    let am = std::cmp::min(am, self.right_column - self.cursor.x);
                                    for _ in 0..am {
                                        self.move_cursor(CursorMove::Right);
                                    }
                                }
                                CursorBackward(am) => {
                                    let am = std::cmp::min(am, self.cursor.x - self.left_column);
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
                                    self.renderer.clear_line(
                                        self.cursor.y,
                                        Some((self.cursor.x, self.screen_size.width)),
                                    )?;
                                }
                                EraseStartOfLine => {
                                    self.renderer
                                        .clear_line(self.cursor.y, Some((0, self.cursor.x + 1)))?;
                                }
                                EraseLine => {
                                    self.renderer.clear_line(self.cursor.y, None)?;
                                }
                                EraseDown => {
                                    // erase end of line
                                    self.renderer.clear_line(
                                        self.cursor.y,
                                        Some((self.cursor.x, self.screen_size.width)),
                                    )?;
                                    // erase down
                                    for row in self.cursor.y + 1..self.screen_size.height {
                                        self.renderer.clear_line(row, None)?;
                                    }
                                }
                                EraseUp => {
                                    // erase start of line
                                    self.renderer
                                        .clear_line(self.cursor.y, Some((0, self.cursor.x + 1)))?;
                                    // erase up
                                    for row in 0..self.cursor.y {
                                        self.renderer.clear_line(row, None)?;
                                    }
                                }
                                EraseScreen => {
                                    // erase entire screen
                                    for row in 0..self.screen_size.height {
                                        self.renderer.clear_line(row, None)?;
                                    }
                                }
                                Reset => {
                                    self.reset();
                                }
                                SetTopBottom(top, bottom) => {
                                    self.top_line = top - 1;
                                    self.bottom_line = bottom - 1;
                                    // set cursor to home position
                                    let x = wrap_range(0, 0, self.screen_size.width - 1);
                                    let y = wrap_range(0, 0, self.screen_size.height - 1);
                                    self.cursor = Point::new(x, y);
                                }
                                ChangeCellAttribute(attr) => {
                                    self.renderer.set_cell_attribute(attr);
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

                                SetCursorMode(_to_set) => {
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
