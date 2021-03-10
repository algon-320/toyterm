use crate::basics::*;

use super::control::parse_escape_sequence;
use super::control::ControlOp;
use super::render::*;

fn wrap_range<T: Ord>(v: T, l: T, h: T) -> T {
    if v < l {
        l
    } else if h < v {
        h
    } else {
        v
    }
}

#[derive(Debug, Clone, Copy, Hash)]
pub enum CursorMove {
    Up,
    Down,
    Left,
    Right,
    Next,
    Prev,
    LeftMost,
    RightMost,
    NewLine,
}

pub struct Term<'ttf, 'texture> {
    renderer: Renderer<'ttf, 'texture>,

    screen_size: Size<usize>,
    cursor: Point<ScreenCell>,
    saved_cursor_pos: Point<ScreenCell>,

    top_line: usize,
    bottom_line: usize,
    left_column: usize,
    right_column: usize,
}
impl<'ttf, 'texture> Term<'ttf, 'texture> {
    pub fn new(renderer: Renderer<'ttf, 'texture>, size: Size<usize>) -> Self {
        assert!(size.width > 0 && size.height > 0);
        let mut term = Term {
            renderer,

            screen_size: size,
            cursor: Point { x: 0, y: 0 },
            saved_cursor_pos: Point { x: 0, y: 0 },

            top_line: 0,
            bottom_line: size.height - 1,
            left_column: 0,
            right_column: size.width - 1,
        };
        term.renderer.clear_entire_screen();
        term
    }

    pub fn render(&mut self) {
        self.renderer.render(Some(&self.cursor));
    }

    pub fn reset(&mut self) {
        self.saved_cursor_pos = Point { x: 0, y: 0 };
        self.cursor = Point { x: 0, y: 0 };

        self.top_line = 0;
        self.bottom_line = self.screen_size.height - 1;
        self.left_column = 0;
        self.right_column = self.screen_size.width - 1;

        self.renderer.clear_entire_screen();
    }

    pub fn move_cursor_repeat(&mut self, m: CursorMove, repeat: usize) -> bool {
        let mut last = false;
        for _ in 0..repeat {
            last = self.move_cursor(m);
        }
        last
    }
    pub fn move_cursor(&mut self, m: CursorMove) -> bool {
        log::trace!("{:?}", m);
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
            NewLine => {
                if !self.move_cursor(CursorMove::Down) {
                    // scroll
                    let x = self.cursor.x;
                    self.move_cursor(CursorMove::RightMost);
                    self.move_cursor(CursorMove::Next);
                    self.cursor.x = x;
                }
                true
            }
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if self.cursor.x + CharWidth::from_char(c).columns() > self.right_column + 1 {
            self.move_cursor(CursorMove::NewLine);
            self.move_cursor(CursorMove::LeftMost);
        }

        let cols = self.renderer.draw_char(c, self.cursor);
        for _ in 0..cols {
            self.move_cursor(CursorMove::Next);
        }
    }

    /// clear cells on (x, y) where:
    ///   x in [left_top.x, right_down.x)
    ///   y in [left_top.y, right_down.y)
    fn clear_screen_part(&mut self, left_top: Point<ScreenCell>, right_down: Point<ScreenCell>) {
        let Size {
            width: cw,
            height: ch,
        } = self.renderer.cell_size();
        let top_left_in_pixel = Point {
            x: (left_top.x as i32) * (cw as i32),
            y: (left_top.y as i32) * (ch as i32),
        };
        let size_in_pixel = Size {
            width: ((right_down.x - left_top.x) * cw) as u32,
            height: ((right_down.y - left_top.y) * ch) as u32,
        };
        self.renderer.clear_area(top_left_in_pixel, size_in_pixel);
    }
    /// clear cells on the current line in [left, right)
    fn clear_line(&mut self, left: Option<usize>, right: Option<usize>) {
        let left = left.unwrap_or(0);
        let right = right.unwrap_or(self.screen_size.width);
        let left_top = Point {
            x: left,
            y: self.cursor.y,
        };
        let right_down = Point {
            x: right,
            y: self.cursor.y + 1,
        };
        self.clear_screen_part(left_top, right_down);
    }

    pub fn write(&mut self, buf: &[u8]) {
        let buf: Vec<char> = std::str::from_utf8(buf).unwrap().chars().collect();
        let mut itr = buf.into_iter();
        while let Some(c) = itr.next() {
            match c {
                '\x00' => break,
                '\x07' => {
                    // bell
                    log::trace!("[Bell]");
                }
                '\x08' => {
                    log::trace!("[Backspace]");
                    self.move_cursor(CursorMove::Left);
                }
                '\x09' => {
                    // FIXME: tabwidth=8
                    let rep = (8 - self.cursor.x % 8) % 8;
                    log::trace!("[TAB] CursorMove::Right * {}", rep);
                    self.move_cursor_repeat(CursorMove::Right, rep);
                }
                '\x0A' => {
                    self.move_cursor(CursorMove::NewLine);
                }
                '\x0D' => {
                    self.move_cursor(CursorMove::LeftMost);
                }

                '\x1B' => {
                    // begin of escape sequence
                    use ControlOp::*;
                    match parse_escape_sequence(&mut itr) {
                        Some(op) => {
                            log::trace!("{:?}", op);
                            match op {
                                CursorHome(p) => {
                                    let x = wrap_range(p.x - 1, 0, self.screen_size.width - 1);
                                    let y = wrap_range(p.y - 1, 0, self.screen_size.height - 1);
                                    self.cursor = Point { x, y };
                                }
                                CursorUp(am) => {
                                    let am = std::cmp::min(am, self.cursor.y - self.top_line);
                                    self.move_cursor_repeat(CursorMove::Up, am);
                                }
                                CursorDown(am) => {
                                    let am = std::cmp::min(am, self.bottom_line - self.cursor.y);
                                    self.move_cursor_repeat(CursorMove::Down, am);
                                }
                                CursorForward(am) => {
                                    let am = std::cmp::min(am, self.right_column - self.cursor.x);
                                    self.move_cursor_repeat(CursorMove::Right, am);
                                }
                                CursorBackward(am) => {
                                    let am = std::cmp::min(am, self.cursor.x - self.left_column);
                                    self.move_cursor_repeat(CursorMove::Left, am);
                                }

                                SaveCursor => {
                                    self.saved_cursor_pos = self.cursor;
                                }
                                RestoreCursor => {
                                    self.cursor = self.saved_cursor_pos;
                                }

                                EraseEndOfLine => {
                                    self.clear_line(Some(self.cursor.x), None);
                                }
                                EraseStartOfLine => {
                                    self.clear_line(None, Some(self.cursor.x + 1));
                                }
                                EraseLine => {
                                    self.clear_line(None, None);
                                }
                                EraseDown => {
                                    // erase end of line
                                    self.clear_line(Some(self.cursor.x), None);

                                    // erase below
                                    let left_top = Point {
                                        x: 0,
                                        y: self.cursor.y + 1,
                                    };
                                    let right_down = Point {
                                        x: self.screen_size.width,
                                        y: self.screen_size.height,
                                    };
                                    self.clear_screen_part(left_top, right_down);
                                }
                                EraseUp => {
                                    // erase start of line
                                    self.clear_line(None, Some(self.cursor.x + 1));

                                    // erase above
                                    let left_top = Point { x: 0, y: 0 };
                                    let right_down = Point {
                                        x: self.screen_size.width,
                                        y: self.cursor.y,
                                    };
                                    self.clear_screen_part(left_top, right_down);
                                }
                                EraseScreen => {
                                    // erase entire screen
                                    self.renderer.clear_entire_screen();
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
                                    self.cursor = Point { x, y };
                                }
                                ChangeCellAttribute(style, fg, bg) => {
                                    let mut attr = self.renderer.get_cell_attribute();
                                    if let Some(s) = style {
                                        attr.style = s;
                                    }
                                    if let Some(f) = fg {
                                        attr.fg = f;
                                    }
                                    if let Some(b) = bg {
                                        attr.bg = b;
                                    }
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
                                    // TODO
                                }

                                Sixel(img) => {
                                    let cell_size = self.renderer.cell_size();
                                    let corresponding_lines =
                                        (img.height + cell_size.height - 1) / cell_size.height;
                                    for _ in 0..corresponding_lines {
                                        self.move_cursor(CursorMove::NewLine);
                                    }
                                    let left_top = Point {
                                        x: self.cursor.x as i32 * cell_size.width as i32,
                                        y: (self.cursor.y as i32 - corresponding_lines as i32)
                                            * cell_size.height as i32,
                                    };
                                    log::debug!(
                                        "draw sixel: x={}, y={}, h={}, w={}",
                                        left_top.x,
                                        left_top.y,
                                        img.height,
                                        img.width
                                    );
                                    self.renderer.draw_sixel(&img, left_top);
                                }
                            }
                        }
                        None => {
                            // print sequence as string followed by '^['
                            // to indicate it is unknown escape sequence
                            self.insert_char('^');
                            self.insert_char('[');
                            log::warn!(
                                "unknown escape sequence: \\E {:?}",
                                itr.as_slice().iter().collect::<String>()
                            );
                        }
                    }
                }
                x => self.insert_char(x),
            }
        }
    }
}
