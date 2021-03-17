pub trait PointType {
    type Type;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pixel {}
impl PointType for Pixel {
    type Type = i32;
}
pub type PixelIdx = <Pixel as PointType>::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenCell {}
impl PointType for ScreenCell {
    type Type = i32;
}
pub type ScreenCellIdx = <ScreenCell as PointType>::Type;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Point<P: PointType> {
    pub x: P::Type,
    pub y: P::Type,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size<P: PointType> {
    pub width: P::Type,
    pub height: P::Type,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range2d<P: PointType> {
    pub h: std::ops::Range<P::Type>,
    pub v: std::ops::Range<P::Type>,
}
impl<P: PointType> Range2d<P>
where
    P::Type: Copy + std::ops::Add<Output = P::Type>,
{
    pub fn new(p: Point<P>, sz: Size<P>) -> Self {
        Self {
            h: (p.x)..(p.x + sz.width),
            v: (p.y)..(p.y + sz.height),
        }
    }
}
impl<P: PointType> Range2d<P>
where
    P::Type: Copy,
{
    pub fn left(&self) -> P::Type {
        self.h.start
    }
    pub fn top(&self) -> P::Type {
        self.v.start
    }
}
impl<P: PointType> Range2d<P>
where
    P::Type: std::iter::Step + Copy,
{
    pub fn iter(&self) -> impl Iterator<Item = Point<P>> + DoubleEndedIterator {
        let v = self.v.clone();
        let h = self.h.clone();
        v.flat_map(move |y| h.clone().map(move |x| Point { x, y }))
    }
}
impl<P: PointType> Range2d<P>
where
    P::Type: std::ops::Sub<Output = P::Type> + num::One + Copy,
{
    pub fn right(&self) -> P::Type {
        self.h.end - <P::Type as num::One>::one()
    }
    pub fn bottom(&self) -> P::Type {
        self.v.end - <P::Type as num::One>::one()
    }
    pub fn width(&self) -> P::Type {
        self.h.end - self.h.start
    }
    pub fn height(&self) -> P::Type {
        self.v.end - self.v.start
    }
    pub fn decompose(&self) -> (Point<P>, Size<P>) {
        (
            Point {
                x: self.left(),
                y: self.top(),
            },
            Size {
                width: self.width(),
                height: self.height(),
            },
        )
    }
}
impl<P: PointType> Range2d<P>
where
    P::Type: std::cmp::Ord,
{
    pub fn contains(&self, p: &Point<P>) -> bool {
        self.h.contains(&p.x) && self.v.contains(&p.y)
    }
}
impl<P: PointType> Range2d<P>
where
    P::Type: std::ops::Add<Output = P::Type>
        + std::ops::Sub<Output = P::Type>
        + num::One
        + std::cmp::Ord
        + Copy,
{
    pub fn intersection(&self, other: &Self) -> Self {
        use std::cmp::{max, min};
        let top = max(self.top(), other.top());
        let left = max(self.left(), other.left());
        let bottom = min(self.bottom(), other.bottom());
        let right = min(self.right(), other.right());
        Range2d {
            v: top..(bottom + <P::Type as num::One>::one()),
            h: left..(right + <P::Type as num::One>::one()),
        }
    }
}

impl<P: PointType> From<Size<P>> for Range2d<P>
where
    P::Type: num::Zero,
{
    fn from(sz: Size<P>) -> Self {
        Self {
            h: <P::Type as num::Zero>::zero()..sz.width,
            v: <P::Type as num::Zero>::zero()..sz.height,
        }
    }
}
