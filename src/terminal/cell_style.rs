#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Black,
    White,
    Red,
    Green,
    Blue,
    Cyan,
    Magenta,
    Yellow,
    Gray,
    RGB(u8, u8, u8),
}
impl Color {
    pub fn to_sdl_color(self) -> sdl2::pixels::Color {
        match self {
            Color::Black => sdl2::pixels::Color::RGB(0, 0, 0),
            Color::White => sdl2::pixels::Color::RGB(255, 255, 255),
            Color::Red => sdl2::pixels::Color::RGB(255, 0, 0),
            Color::Green => sdl2::pixels::Color::RGB(0, 255, 0),
            Color::Blue => sdl2::pixels::Color::RGB(0, 0, 255),
            Color::Cyan => sdl2::pixels::Color::RGB(0, 255, 255),
            Color::Magenta => sdl2::pixels::Color::RGB(255, 0, 255),
            Color::Yellow => sdl2::pixels::Color::RGB(255, 255, 0),
            Color::Gray => sdl2::pixels::Color::RGB(120, 120, 120),
            Color::RGB(r, g, b) => sdl2::pixels::Color::RGB(r, g, b),
        }
    }
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
pub struct CellStyle {
    pub(crate) style: Style,
    pub(crate) fg: Color,
    pub(crate) bg: Color,
}
impl Default for CellStyle {
    fn default() -> Self {
        CellStyle {
            style: Style::Normal,
            fg: Color::White,
            bg: Color::Black,
        }
    }
}
