#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size<T> {
    pub width: T,
    pub height: T,
}
impl<T: std::ops::Mul> Size<T> {
    pub fn area(self) -> T::Output {
        self.width * self.height
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
    type Type = usize;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point<P: PointType> {
    pub x: P::Type,
    pub y: P::Type,
}
