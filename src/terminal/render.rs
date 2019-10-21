use std::collections::HashMap;

use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::ttf::Font;
use sdl2::ttf::FontStyle;
use sdl2::video::{Window, WindowContext};

use crate::basics::*;
use crate::utils::*;

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
pub struct CellAttribute {
    pub(crate) style: Style,
    pub(crate) fg: Color,
    pub(crate) bg: Color,
}
impl Default for CellAttribute {
    fn default() -> Self {
        CellAttribute {
            style: Style::Normal,
            fg: Color::Green,
            bg: Color::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell {
    pub(crate) c: char,
    pub(crate) attribute: CellAttribute,
}
impl Cell {
    pub fn new(c: char, attr: CellAttribute) -> Self {
        Cell { c, attribute: attr }
    }
}
impl Default for Cell {
    fn default() -> Self {
        Cell {
            c: ' ',
            attribute: CellAttribute::default(),
        }
    }
}
