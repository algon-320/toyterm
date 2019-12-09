#![allow(dead_code)]

mod parser;

type SixelSeq = Vec<u8>;

const SIX: usize = 6;
const EACH_PIXEL: usize = 4;

#[derive(Debug)]
pub struct Image {
    pub height: usize,
    pub width: usize,
    pub buf: Vec<u8>,
}
impl Image {
    pub fn new() -> Self {
        Image {
            height: 0,
            width: 0,
            buf: Vec::new(),
        }
    }
    fn resize(&mut self) {
        self.buf
            .resize_with(self.height * self.width * EACH_PIXEL, Default::default);
    }
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum Op {
    Sixel { bits: u8, rep: u64 },
    RasterAttributes(u64, u64, u64, u64),
    CarriageReturn,
    NextLine,
    UseColor(u8),
    SetColor(u8, Color),
    Finish,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) struct Color {
    a: u8,
    r: u8,
    g: u8,
    b: u8,
}
impl Color {
    fn new(r: u8, g: u8, b: u8) -> Self {
        Color { a: 255, r, g, b }
    }
}
impl Default for Color {
    fn default() -> Self {
        Color {
            a: 255,
            r: 0,
            g: 0,
            b: 0,
        }
    }
}

// decode sixel sequence to bitmap image
pub fn decode<I>(
    seq: &mut I,
    argb_ord: [usize; EACH_PIXEL],
    image_width: usize,
    image_height: Option<usize>,
) -> Image
where
    I: Iterator<Item = char>,
{
    let mut img = Image::new();
    img.buf = vec![];
    img.width = image_width;
    img.height = if let Some(h) = image_height {
        (h + SIX - 1) / SIX * SIX
    } else {
        SIX
    };
    #[cfg(debug_assertions)]
    println!("w={}, h={}", img.width, img.height);

    img.resize(); // allocate buffer

    let mut itr = seq.peekable();
    let mut y = 0;
    let mut x = 0;
    let mut color = Color::default();
    let mut palette: Vec<Color> = Vec::new();
    palette.resize_with(256, Default::default);

    let mut pixel_w = 1;
    let mut pixel_h = 1;

    while let Some(op) = parser::parse(&mut itr) {
        match op {
            Op::RasterAttributes(pan, pad, ph, pv) => {
                pixel_h = pan as usize;
                pixel_w = pad as usize;
                img.height = (pixel_h * pv as usize + SIX - 1) / SIX * SIX;
                img.width = pixel_w * ph as usize;

                #[cfg(debug_assertions)]
                println!("buffer size changed: w={}, h={}", img.width, img.height);
                img.resize();
            }
            Op::CarriageReturn => {
                x = 0;
            }
            Op::NextLine => {
                y += SIX * pixel_h;
                x = 0;
            }
            Op::Finish => {
                break;
            }
            Op::SetColor(reg, c) => {
                palette[reg as usize] = c;
            }
            Op::UseColor(reg) => {
                color = palette[reg as usize];
            }
            Op::Sixel { bits: b, rep: r } => {
                let required_buf = (y + SIX * pixel_h) * img.width * EACH_PIXEL;
                if img.buf.len() < required_buf {
                    let each_line = img.width * EACH_PIXEL;
                    img.height += (required_buf - img.buf.len() + each_line - 1) / each_line;
                    #[cfg(debug_assertions)]
                    println!("buffer size changed: h={}", img.height);
                    img.resize();
                }
                for _ in 0..r as usize * pixel_w {
                    for i in 0..SIX {
                        if ((b >> i) & 1) > 0 {
                            for k in 0..pixel_h {
                                let pos = (y + i * pixel_h + k) * img.width + x;
                                img.buf[pos * EACH_PIXEL + argb_ord[0]] = color.a; // TODO
                                img.buf[pos * EACH_PIXEL + argb_ord[1]] += color.r;
                                img.buf[pos * EACH_PIXEL + argb_ord[2]] += color.g;
                                img.buf[pos * EACH_PIXEL + argb_ord[3]] += color.b;
                            }
                        }
                    }
                    x += 1;
                }
            }
            _ => panic!("unsupported"),
        }
    }
    img
}

#[test]
fn test_decode() {
    let b = "\"1;1;6;6#0;2;100;0;0#1;2;0;100;0#2;2;0;0;100#0~~!4?$#1??!2~??$#2????~~\x1b\\";
    let mut itr = b.chars();
    let image = decode(&mut itr, [0, 1, 2, 3], 6, None);
    assert_eq!(image.width, 6);
    assert_eq!(image.height, 6);
    assert_eq!(
        image.buf,
        vec![
            /* row 0 */ 255, 255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
            0, 0, 255, 255, 0, 0, 255, /* row 1 */ 255, 255, 0, 0, 255, 255, 0, 0, 255, 0,
            255, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 255, /* row 2 */ 255, 255, 0,
            0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 255,
            /* row 3 */ 255, 255, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
            0, 0, 255, 255, 0, 0, 255, /* row 4 */ 255, 255, 0, 0, 255, 255, 0, 0, 255, 0,
            255, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 255, /* row 5 */ 255, 255, 0,
            0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 255,
        ]
    );

    let b = "\"1;1;10;10\x1b\\";
    let mut itr = b.chars();
    let image = decode(&mut itr, [0, 1, 2, 3], 6, None);
    assert_eq!(image.width, 10);
    assert_eq!(image.height, 12);

    let b = "~~~~~~-~~~~~~\x1b\\";
    let mut itr = b.chars();
    let image = decode(&mut itr, [0, 1, 2, 3], 6, None);
    assert_eq!(image.width, 6);
    assert_eq!(image.height, 12);

    let b = "\"1;1;6;6~~~~~~-~~~~~~-???-!6~\x1b\\";
    let mut itr = b.chars();
    let image = decode(&mut itr, [0, 1, 2, 3], 6, None);
    assert_eq!(image.width, 6);
    assert_eq!(image.height, 24);

    let b = "\"2;2;10;10\x1b\\";
    let mut itr = b.chars();
    let image = decode(&mut itr, [0, 1, 2, 3], 6, None);
    assert_eq!(image.width, 20);
    assert_eq!(image.height, 24);

    let b = "\"2;3;6;6~~~~~~-~~~~~~-???-!6~\x1b\\";
    let mut itr = b.chars();
    let image = decode(&mut itr, [0, 1, 2, 3], 6, None);
    assert_eq!(image.width, 18);
    assert_eq!(image.height, 48);
}
