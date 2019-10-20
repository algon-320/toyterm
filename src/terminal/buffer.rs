use crate::basics::*;

use super::cell_style::CellStyle;

pub struct Buffer {
    pub(super) width: usize,
    pub(super) cursor: Point<BufferCell>,
    pub(super) data: Vec<Vec<(char, CellStyle)>>,
}
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
impl Buffer {
    pub fn new(width: usize) -> Self {
        let mut b = Buffer {
            width,
            cursor: Point::new(0, 0),
            data: Vec::new(),
        };
        b.add_newline();
        b
    }

    pub fn reset(&mut self) {
        self.data.clear();
        self.add_newline();
        self.cursor = Point::new(0, 0);
    }

    // put a character on the cursor
    pub fn put_char(&mut self, c: char, style: CellStyle) {
        assert!(self.cursor.y < self.data.len());
        let line = &mut self.data[self.cursor.y as usize];
        assert!(self.cursor.x < line.len());
        line[self.cursor.x] = (c, style);
    }

    pub fn clear_line(&mut self, row: usize, range: (usize, usize)) {
        if row >= self.data.len() {
            return;
        }
        let (l, r) = range;
        for i in l..r {
            self.data[row][i] = (' ', CellStyle::default());
        }
    }

    pub fn set_cursor_pos(&mut self, p: Point<BufferCell>) {
        self.cursor = p;
        self.add_newline();
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
                self.cursor.y += 1;
                self.add_newline();
                true
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
                self.cursor.x = self.width - 1;
                true
            }
            Right => {
                if self.cursor.x < self.width - 1 {
                    self.cursor.x += 1;
                    true
                } else {
                    false
                }
            }
            Next => {
                if !self.move_cursor(Right) {
                    self.move_cursor(LeftMost);
                    self.move_cursor(Down);
                }
                true
            }
            Prev => {
                if !self.move_cursor(Left) {
                    self.move_cursor(RightMost);
                    self.move_cursor(Up);
                }
                true
            }
        }
    }

    fn add_newline(&mut self) {
        while self.cursor.y >= self.data.len() {
            self.data
                .push(vec![(' ', CellStyle::default()); self.width]);
        }
    }
}
