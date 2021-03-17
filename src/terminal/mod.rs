mod emulator;
mod parser;
pub mod render;

pub use emulator::*;

use crate::basics::*;

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
    pub c: char,
    pub attr: CellAttribute,
}
impl Default for Cell {
    fn default() -> Self {
        Cell {
            c: ' ',
            attr: CellAttribute::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug)]
pub enum ControlOp {
    InsertChar(char),
    Bell,
    Tab,
    LineFeed,
    CarriageReturn,
    CursorMove(CursorMove),
    SaveCursor,
    RestoreCursor,
    HideCursor,
    ShowCursor,
    ScrollDown,
    ScrollUp,
    EraseEndOfLine,
    EraseStartOfLine,
    EraseLine,
    EraseDown,
    EraseUp,
    EraseScreen,
    SetScrollRange(std::ops::Range<ScreenCellIdx>), // 0-origin
    Reset,
    ChangeCellAttribute(Option<Style>, Option<Color>, Option<Color>),
    SetCursorMode(bool),
    Sixel(sixel::Image),
    Unknown(Vec<char>),
    Ignore,
}

#[derive(Debug, Clone, Copy)]
pub enum CursorMove {
    Exact(Point<ScreenCell>), // 0-origin
    Up(usize),
    Down(usize),
    Left(usize),
    Right(usize),
    Top,
    Bottom,
    LeftMost,
    RightMost,
}

#[derive(Debug, Clone, PartialEq)]
struct Cursor {
    pub pos: Point<ScreenCell>,
    pub attr: CellAttribute,
    pub visible: bool,
}
impl Default for Cursor {
    fn default() -> Self {
        Self {
            pos: Point { x: 0, y: 0 },
            attr: CellAttribute::default(),
            visible: true,
        }
    }
}

impl Cursor {
    pub fn try_move(&self, m: CursorMove, range: &Range2d<ScreenCell>) -> Option<Cursor> {
        use CursorMove::*;
        let mut new_pos = self.pos;
        match m {
            Exact(p) => new_pos = p,
            Up(a) => new_pos.y = self.pos.y.checked_sub(a as ScreenCellIdx)?,
            Down(a) => new_pos.y = self.pos.y.checked_add(a as ScreenCellIdx)?,
            Left(a) => new_pos.x = self.pos.x.checked_sub(a as ScreenCellIdx)?,
            Right(a) => new_pos.x = self.pos.x.checked_add(a as ScreenCellIdx)?,
            LeftMost => new_pos.x = range.left(),
            RightMost => new_pos.x = range.right(),
            Top => new_pos.y = range.top(),
            Bottom => new_pos.y = range.bottom(),
        }
        range.contains(&new_pos).then(|| Cursor {
            pos: new_pos,
            ..*self
        })
    }

    pub fn try_saturating_move(&self, m: CursorMove, range: &Range2d<ScreenCell>) -> Cursor {
        use CursorMove::*;
        let mut new_pos = self.pos;
        match m {
            LeftMost | RightMost | Top | Bottom => {
                return self
                    .try_move(m, range)
                    .expect("these movements should always success")
            }
            Exact(p) => new_pos = p,
            Up(a) => new_pos.y = self.pos.y.saturating_sub(a as ScreenCellIdx),
            Down(a) => new_pos.y = self.pos.y.saturating_add(a as ScreenCellIdx),
            Left(a) => new_pos.x = self.pos.x.saturating_sub(a as ScreenCellIdx),
            Right(a) => new_pos.x = self.pos.x.saturating_add(a as ScreenCellIdx),
        }
        use std::cmp::{max, min};
        Cursor {
            pos: Point {
                x: min(max(range.left(), new_pos.x), range.right()),
                y: min(max(range.top(), new_pos.y), range.bottom()),
            },
            ..*self
        }
    }
}
