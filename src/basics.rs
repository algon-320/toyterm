#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}
impl<T> Size<T> {
    pub fn new(width: T, height: T) -> Self {
        Size { width, height }
    }
}

use std::marker::PhantomData;

pub mod PositionType {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Pixel;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BufferCell;
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ScreenCell;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position<U> {
    pub x: usize,
    pub y: usize,
    _phantom: PhantomData<U>,
}
impl<U> Position<U> {
    pub fn new(x: usize, y: usize) -> Self {
        Position {
            x,
            y,
            _phantom: PhantomData,
        }
    }
}

pub fn conv_err<T, E: ToString>(e: Result<T, E>) -> std::result::Result<T, String> {
    e.map_err(|e| e.to_string())
}

pub fn pretty_format_ascii_bytes(bytes: &[u8]) -> Vec<String> {
    const TABLE: [&str; 33] = [
        "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR",
        "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB",
        "ESC", "FS", "GS", "RS", "US", "` `",
    ];
    bytes
        .iter()
        .map(|c| {
            TABLE
                .get(*c as usize)
                .map(|s| format!("{}(x{:02X})", s, c))
                .unwrap_or_else(|| char::from(*c).to_string())
        })
        .collect()
}
