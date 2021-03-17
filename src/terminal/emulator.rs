use crate::basics::*;

use super::control;
use super::render::Renderer;
use super::{Cell, CellAttribute, CharWidth, ControlOp, Cursor, CursorMove, Style};

fn cell_top_left_corner(p: Point<ScreenCell>, cell_size: Size<Pixel>) -> Point<Pixel> {
    Point {
        x: p.x as PixelIdx * cell_size.width,
        y: p.y as PixelIdx * cell_size.height,
    }
}

pub struct Term<'ttf, 'texture> {
    renderer: Renderer<'ttf, 'texture>,

    screen_size: Size<ScreenCell>,
    scroll_range: Range2d<ScreenCell>,
    cursor: Cursor,
    saved_cursor: Option<Cursor>,
    screen_buf: Vec<Cell>,
    end_of_line: bool,
}
impl<'ttf, 'texture> Term<'ttf, 'texture> {
    pub fn new(renderer: Renderer<'ttf, 'texture>, size: Size<ScreenCell>) -> Self {
        assert!(size.width > 0 && size.height > 0);

        let mut term = Term {
            renderer,
            screen_size: size,
            cursor: Cursor::default(),
            saved_cursor: None,
            screen_buf: vec![Cell::default(); (size.width as usize) * (size.height as usize)],
            scroll_range: size.into(),
            end_of_line: false,
        };
        term.reset();
        term
    }

    pub fn reset(&mut self) {
        log::debug!("reset");
        self.cursor = Cursor::default();
        self.saved_cursor = None;
        self.scroll_range = self.screen_size.into();
        self.end_of_line = false;
        self.clear_screen_part(&Range2d::from(self.screen_size));
    }

    pub fn render(&mut self) {
        log::trace!("render");
        let cell_size = self.renderer.cell_size();
        let cells = Range2d::from(self.screen_size)
            .iter()
            .rev()
            .zip(self.screen_buf.iter().rev());
        for (p, cell) in cells {
            let mut cell = *cell;
            if self.cursor.visible && self.cursor.pos == p {
                cell.attr = CellAttribute::default();
                cell.attr.style = Style::Reverse;
            }
            self.renderer
                .draw_cell(cell, cell_top_left_corner(p, cell_size));
        }
        // TODO: draw sixel
        self.renderer.present();
    }

    fn move_cursor_nextline(&mut self, rep: usize) {
        log::trace!("move_cursor_nextline(rep={})", rep);
        for _ in 0..rep {
            match self
                .cursor
                .try_move(CursorMove::Down(1), &self.scroll_range)
            {
                Some(cursor) => self.cursor = cursor,
                None => {
                    self.scroll_up();
                }
            }
        }
    }

    fn move_cursor(&mut self, m: CursorMove) {
        self.end_of_line = false;
        log::trace!("self.end_of_line = {:?}", self.end_of_line);
        let cursor = self.cursor.try_saturating_move(m, &self.screen_size.into());
        self.cursor = cursor;
    }

    fn insert_char(&mut self, c: char) {
        let cw = CharWidth::from_char(c).columns();
        log::trace!("insert_char: \x1b[32;1m{:?}\x1b[m", c);
        log::trace!("(before insert) cursor = {:?}", self.cursor);
        if self.end_of_line {
            self.move_cursor(CursorMove::LeftMost);
            self.move_cursor_nextline(1);
            self.end_of_line = false;
            log::trace!("self.end_of_line = {:?}", self.end_of_line);
        }

        match self
            .cursor
            .try_move(CursorMove::Right(cw - 1), &self.scroll_range)
        {
            Some(_) => {} // we have enough space to draw the character
            None => {
                // TODO: consider line wrap
                self.move_cursor(CursorMove::LeftMost);
                self.move_cursor_nextline(1);
            }
        }

        let cell = Cell {
            c,
            attr: self.cursor.attr,
        };

        let cell_idx = (self.cursor.pos.y * self.screen_size.width + self.cursor.pos.x) as usize;
        self.screen_buf[cell_idx] = cell;

        match self
            .cursor
            .try_move(CursorMove::Right(cw), &self.scroll_range)
        {
            Some(cursor) => self.cursor = cursor,
            None => {
                self.end_of_line = true;
                log::trace!("self.end_of_line = {:?}", self.end_of_line);
            }
        }
        log::trace!("(after insert)  cursor = {:?}", self.cursor);
    }

    fn clear_screen_part(&mut self, range: &Range2d<ScreenCell>) {
        log::trace!("clear_screen_part(range={:?})", range);
        let w = self.screen_size.width;
        for y in range.v.clone() {
            for x in range.h.clone() {
                self.screen_buf[(y * w + x) as usize] = Cell::default();
            }
        }
    }

    fn scroll_up(&mut self) {
        log::trace!("scroll_up");
        let w = self.screen_size.width;

        let mut range = self.scroll_range.clone();
        range.v.start += 1;
        for Point { x, y } in range.iter() {
            self.screen_buf[((y - 1) * w + x) as usize] = self.screen_buf[(y * w + x) as usize];
        }

        let bottom = Range2d::<ScreenCell> {
            v: range.bottom()..(range.bottom() + 1),
            ..range
        };
        for Point { x, y } in bottom.iter() {
            self.screen_buf[(y * w + x) as usize] = Cell::default();
        }
    }
    fn scroll_down(&mut self) {
        log::trace!("scroll_down");
        let w = self.screen_size.width;

        let mut range = self.scroll_range.clone();
        range.v.end -= 1;
        for Point { x, y } in range.iter().rev() {
            self.screen_buf[((y + 1) * w + x) as usize] = self.screen_buf[(y * w + x) as usize];
        }

        let top = Range2d::<ScreenCell> {
            v: range.top()..(range.top() + 1),
            ..range
        };
        for Point { x, y } in top.iter() {
            self.screen_buf[(y * w + x) as usize] = Cell::default();
        }
    }

    pub fn process(&mut self, op: ControlOp) {
        log::trace!("op: {:?}", op);

        use super::CursorMove as Move;
        use ControlOp::*;
        match op {
            InsertChar(x) => {
                self.insert_char(x);
            }
            Bell => {
                log::debug!("[Bell]");
            }
            Tab => {
                // FIXME: tabwidth=8
                let rep = (8 - self.cursor.pos.x as usize % 8) % 8;
                log::trace!("[TAB] CursorMove::Right * {}", rep);
                self.move_cursor(Move::Right(rep));
            }
            LineFeed => {
                log::trace!("[LF]");
                self.move_cursor_nextline(1);
            }
            CarriageReturn => {
                log::trace!("[CR]");
                self.end_of_line = false;
                log::trace!("self.end_of_line = {:?}", self.end_of_line);
                self.move_cursor(Move::LeftMost);
            }
            CursorMove(mov) => {
                self.move_cursor(mov);
            }
            SaveCursor => {
                log::debug!("cursor saved: {:?}", self.cursor);
                self.saved_cursor = Some(self.cursor.clone());
            }
            RestoreCursor => {
                self.cursor = self.saved_cursor.clone().unwrap_or_else(|| {
                    log::info!("no saved cursor");
                    Cursor::default()
                });
                log::debug!("cursor restored: {:?}", self.cursor);
            }
            HideCursor => {
                self.cursor.visible = false;
                log::debug!("cursor invisible");
            }
            ShowCursor => {
                self.cursor.visible = true;
                log::debug!("cursor visible");
            }

            EraseEndOfLine => {
                let line = self.cursor.pos.y;
                let to_line_end = Range2d {
                    h: self.cursor.pos.x..self.screen_size.width,
                    v: line..(line + 1),
                };
                self.clear_screen_part(&to_line_end);
            }
            EraseStartOfLine => {
                let line = self.cursor.pos.y;
                let to_line_begin = Range2d {
                    h: 0..(self.cursor.pos.x + 1),
                    v: line..(line + 1),
                };
                self.clear_screen_part(&to_line_begin);
            }
            EraseLine => {
                let line = self.cursor.pos.y;
                let whole = Range2d {
                    h: 0..self.screen_size.width,
                    v: line..(line + 1),
                };
                self.clear_screen_part(&whole);
            }
            EraseDown => {
                self.process(EraseEndOfLine);
                let below = Range2d {
                    h: 0..self.screen_size.width,
                    v: (self.cursor.pos.y + 1)..self.screen_size.height,
                };
                self.clear_screen_part(&below);
            }
            EraseUp => {
                self.process(EraseStartOfLine);
                let above = Range2d {
                    h: 0..self.screen_size.width,
                    v: 0..self.cursor.pos.y,
                };
                self.clear_screen_part(&above);
            }
            EraseScreen => {
                // clear entire screen
                self.clear_screen_part(&self.screen_size.into());
            }
            Reset => {
                self.reset();
            }
            SetScrollRange(range) => {
                self.scroll_range.v = range;
                log::debug!("scroll_range changed --> {:?}", self.scroll_range);
            }
            ChangeCellAttribute(style, fg, bg) => {
                log::trace!(
                    "(before attribute change) cursor.attr = {:?}",
                    self.cursor.attr
                );
                if let Some(s) = style {
                    self.cursor.attr.style = s;
                }
                if let Some(f) = fg {
                    self.cursor.attr.fg = f;
                }
                if let Some(b) = bg {
                    self.cursor.attr.bg = b;
                }
                log::trace!(
                    "(after attribute change)  cursor.attr = {:?}",
                    self.cursor.attr
                );
            }
            Ignore => {}

            ScrollUp => {
                self.scroll_up();
            }
            ScrollDown => {
                self.scroll_down();
            }

            SetCursorMode(_to_set) => {
                // TODO
            }

            Sixel(img) => {
                // TODO: retain this sixel image and re-render it on each render() call.

                let cell_size = self.renderer.cell_size();
                let ch = cell_size.height as usize;
                let corresponding_lines = ((img.height + ch - 1) / ch) as ScreenCellIdx;
                self.move_cursor_nextline(corresponding_lines as usize);
                let pos = cell_top_left_corner(
                    {
                        let mut tmp = self.cursor.pos;
                        tmp.y -= corresponding_lines;
                        tmp
                    },
                    cell_size,
                );
                log::debug!(
                    "draw sixel: x={}, y={}, h={}, w={} (lines: {})",
                    pos.x,
                    pos.y,
                    img.height,
                    img.width,
                    corresponding_lines,
                );
                self.renderer.draw_sixel(&img, pos);
            }

            Unknown(seq) => {
                // print sequence as string followed by '^['
                // to indicate it is unknown escape sequence
                self.insert_char('^');
                self.insert_char('[');
                log::warn!("unknown escape sequence: {:?}", seq);
                for c in seq {
                    self.insert_char(c);
                }
            }
        }
    }
}

impl<'ttf, 'texture> std::io::Write for Term<'ttf, 'texture> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut itr = std::str::from_utf8(bytes).expect("UTF-8").chars();
        while let Some(op) = control::parse(&mut itr) {
            self.process(op);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.render();
        Ok(())
    }
}
