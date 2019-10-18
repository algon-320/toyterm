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

pub trait PointType {
    type Type;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pixel;
impl PointType for Pixel {
    type Type = i32;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferCell;
impl PointType for BufferCell {
    type Type = usize;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenCell;
impl PointType for ScreenCell {
    type Type = isize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point<P: PointType> {
    pub x: P::Type,
    pub y: P::Type,
}
impl<P: PointType> Point<P> {
    pub fn new(x: P::Type, y: P::Type) -> Self {
        Point { x, y }
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

pub fn parse_int_from_ascii(bytes: &[u8]) -> Option<u32> {
    if bytes.len() == 0 {
        return None;
    }
    let mut ret = 0;
    for c in bytes.iter() {
        ret *= 10;
        if char::from(*c).is_digit(10) {
            ret += (*c - b'0') as u32
        } else {
            return None;
        }
    }
    Some(ret)
}
