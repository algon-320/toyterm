use crate::basics::*;

use super::control::parse_escape_sequence;
use super::control::ControlOp;
use super::render::*;

fn cell_to_pixel(p: Point<ScreenCell>, cell_size: Size<Pixel>) -> Point<Pixel> {
    Point {
        x: p.x as PixelIdx * cell_size.width,
        y: p.y as PixelIdx * cell_size.height,
    }
}

fn scale_range(range: &Range2d<ScreenCell>, cell_size: Size<Pixel>) -> Range2d<Pixel> {
    let Size {
        width: cw,
        height: ch,
    } = cell_size;
    Range2d {
        h: (range.h.start as PixelIdx * cw)..(range.h.end as PixelIdx * cw),
        v: (range.v.start as PixelIdx * ch)..(range.v.end as PixelIdx * ch),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CursorMove {
    Exact(Point<ScreenCell>),
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
    LeftMost,
    RightMost,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cursor {
    pub pos: Point<ScreenCell>,
    pub attr: CellAttribute,
}
impl Default for Cursor {
    fn default() -> Self {
        Self {
            pos: Point { x: 0, y: 0 },
            attr: CellAttribute::default(),
        }
    }
}

impl Cursor {
    pub fn try_move_once(&self, m: CursorMove, range: &Range2d<ScreenCell>) -> Option<Cursor> {
        use CursorMove::*;
        let mut new_pos = self.pos;
        match m {
            Exact(p) => new_pos = p,
            Up => new_pos.y = self.pos.y.checked_sub(1)?,
            Down => new_pos.y = self.pos.y.checked_add(1)?,
            Left => new_pos.x = self.pos.x.checked_sub(1)?,
            Right => new_pos.x = self.pos.x.checked_add(1)?,
            LeftMost => new_pos.x = range.left(),
            RightMost => new_pos.x = range.right(),
            Top => new_pos.y = range.top(),
            Bottom => new_pos.y = range.bottom(),
        }
        range.contains(&new_pos).then(|| Cursor {
            pos: new_pos,
            attr: self.attr,
        })
    }

    pub fn try_move(
        &self,
        m: CursorMove,
        range: &Range2d<ScreenCell>,
        rep: usize,
    ) -> (Cursor, usize) {
        let mut cursor = self.clone();
        for i in 0..rep {
            match cursor.try_move_once(m, range) {
                Some(new_cursor) => cursor = new_cursor,
                None => return (cursor, rep - i),
            }
        }
        (cursor, 0)
    }
}

pub struct Term<'ttf, 'texture> {
    renderer: Renderer<'ttf, 'texture>,

    screen_size: Size<ScreenCell>,
    scroll_range: Range2d<ScreenCell>,
    cursor: Cursor,
    saved_cursor: Option<Cursor>,
    screen_buf: Vec<char>,
}
impl<'ttf, 'texture> Term<'ttf, 'texture> {
    pub fn new(renderer: Renderer<'ttf, 'texture>, size: Size<ScreenCell>) -> Self {
        assert!(size.width > 0 && size.height > 0);

        let mut term = Term {
            renderer,
            screen_size: size,
            cursor: Cursor::default(),
            saved_cursor: None,
            screen_buf: vec![' '; (size.width as usize) * (size.height as usize)],
            scroll_range: size.into(),
        };
        term.reset();
        term
    }

    pub fn reset(&mut self) {
        log::debug!("reset");
        self.cursor = Cursor::default();
        self.saved_cursor = None;
        self.scroll_range = self.screen_size.into();
        self.clear_screen_part(&Range2d::from(self.screen_size));
    }

    pub fn render(&mut self) {
        log::trace!("render");
        self.renderer.render();
    }

    pub fn move_cursor_nextline(&mut self, rep: usize) {
        log::trace!("move_cursor_nextline(rep={})", rep);
        for _ in 0..rep {
            match self
                .cursor
                .try_move_once(CursorMove::Down, &self.scroll_range)
            {
                Some(cursor) => self.cursor = cursor,
                None => {
                    self.scroll_up();
                }
            }
        }
    }

    pub fn move_cursor(&mut self, m: CursorMove, rep: usize) {
        let (cursor, _) = self.cursor.try_move(m, &self.screen_size.into(), rep);
        self.cursor = cursor;
    }

    pub fn insert_char(&mut self, c: char) {
        let cw = CharWidth::from_char(c).columns();
        match self
            .cursor
            .try_move(CursorMove::Right, &self.screen_size.into(), cw - 1)
        {
            (_, 0) => {} // we have enough space to draw the character
            (_, _) => {
                // TODO: consider line wrap
                self.move_cursor_nextline(1);
                self.move_cursor(CursorMove::LeftMost, 1);
            }
        }

        let cell_idx = (self.cursor.pos.y * self.screen_size.height + self.cursor.pos.x) as usize;
        self.screen_buf[cell_idx] = c;

        let cell = Cell::new(c, self.cursor.attr);
        self.renderer.draw_cell(
            cell,
            cell_to_pixel(self.cursor.pos, self.renderer.cell_size()),
        );

        match self
            .cursor
            .try_move(CursorMove::Right, &self.screen_size.into(), cw)
        {
            (cursor, 0) => self.cursor = cursor,
            (_, _) => {
                // TODO: consider line wrap
                self.move_cursor(CursorMove::LeftMost, 1);
                self.move_cursor_nextline(1);
            }
        }
    }

    fn clear_screen_part(&mut self, range: &Range2d<ScreenCell>) {
        log::trace!("clear_screen_part(range={:?})", range);
        let pixel_range = scale_range(range, self.renderer.cell_size());
        self.renderer
            .clear_range(&self.cursor.attr.bg, &pixel_range);
    }

    pub fn scroll_up(&mut self) {
        log::trace!("scroll_up");
        let cell_size = self.renderer.cell_size();
        let mut src = self.scroll_range.clone();
        src.v.start += 1;
        let (dst, _) = self.scroll_range.decompose();
        self.renderer.shift_texture(
            &self.cursor.attr.bg,
            &scale_range(&src, cell_size),
            cell_to_pixel(dst, cell_size),
        );
    }
    pub fn scroll_down(&mut self) {
        log::trace!("scroll_down");
        let cell_size = self.renderer.cell_size();
        let mut src = self.scroll_range.clone();
        src.v.end -= 1;
        let (mut dst, _) = self.scroll_range.decompose();
        dst.y += 1;
        self.renderer.shift_texture(
            &self.cursor.attr.bg,
            &scale_range(&src, cell_size),
            cell_to_pixel(dst, cell_size),
        );
    }

    pub fn process(&mut self, op: ControlOp) {
        log::trace!("op: {:?}", op);

        use ControlOp::*;
        match op {
            CursorHome(p) => {
                self.move_cursor(CursorMove::Exact(p), 1);
            }
            CursorUp(am) => {
                self.move_cursor(CursorMove::Up, am);
            }
            CursorDown(am) => {
                self.move_cursor(CursorMove::Down, am);
            }
            CursorForward(am) => {
                self.move_cursor(CursorMove::Right, am);
            }
            CursorBackward(am) => {
                self.move_cursor(CursorMove::Left, am);
            }
            SaveCursor => {
                log::debug!("cursor saved");
                self.saved_cursor = Some(self.cursor.clone());
            }
            RestoreCursor => {
                log::debug!("cursor restored");
                self.cursor = self.saved_cursor.clone().unwrap_or_else(|| {
                    log::info!("no saved cursor");
                    Cursor::default()
                });
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
            SetTopBottom(range) => {
                self.scroll_range.v = range;
                // self.cursor.pos = Point { x: 0, y: 0 };
            }
            ChangeCellAttribute(style, fg, bg) => {
                if let Some(s) = style {
                    self.cursor.attr.style = s;
                }
                if let Some(f) = fg {
                    self.cursor.attr.fg = f;
                }
                if let Some(b) = bg {
                    self.cursor.attr.bg = b;
                }
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
                let cell_size = self.renderer.cell_size();
                let ch = cell_size.height as usize;
                let corresponding_lines = ((img.height + ch - 1) / ch) as ScreenCellIdx;
                self.move_cursor_nextline(corresponding_lines as usize);
                let pos = cell_to_pixel(
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
        }
    }
}

impl<'ttf, 'texture> std::io::Write for Term<'ttf, 'texture> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut itr = std::str::from_utf8(bytes).expect("UTF-8").chars();
        while let Some(c) = itr.next() {
            match c {
                '\x00' => break,
                '\x07' => {
                    // bell
                    log::debug!("[Bell]");
                }
                '\x08' => {
                    log::trace!("[Backspace]");
                    self.move_cursor(CursorMove::Left, 1);
                }
                '\x09' => {
                    // FIXME: tabwidth=8
                    let rep = (8 - self.cursor.pos.x as usize % 8) % 8;
                    log::trace!("[TAB] CursorMove::Right * {}", rep);
                    self.move_cursor(CursorMove::Right, rep);
                }
                '\x0A' => {
                    log::trace!("[LF]");
                    self.move_cursor_nextline(1);
                }
                '\x0D' => {
                    log::trace!("[CR]");
                    self.move_cursor(CursorMove::LeftMost, 1);
                }

                '\x1B' => {
                    match parse_escape_sequence(&mut itr) {
                        Some(op) => self.process(op),
                        None => {
                            // print sequence as string followed by '^['
                            // to indicate it is unknown escape sequence
                            self.insert_char('^');
                            self.insert_char('[');
                            log::warn!("unknown escape sequence: \\E {:?}", itr.as_str());
                        }
                    }
                }
                x => {
                    log::trace!("insert_char: {}", x);
                    self.insert_char(x);
                }
            }
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.render();
        Ok(())
    }
}
