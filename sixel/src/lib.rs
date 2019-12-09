#![allow(dead_code)]

type SixelSeq = Vec<u8>;

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
}

mod parser;

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
    argb_ord: [usize; 4],
    image_width: usize,
    image_height: Option<usize>,
) -> Image
where
    I: Iterator<Item = char>,
{
    let mut img = Image::new();
    img.buf = vec![];
    img.width = image_width;

    // allocate buffer
    img.height = if let Some(h) = image_height {
        (h + 5) / 6 * 6
    } else {
        6
    };
    img.buf
        .resize_with(img.width * 4 * img.height, Default::default);
    let mut itr = seq.peekable();
    let mut y = 0;
    let mut x = 0;
    let mut color = Color::default();
    let mut palette: Vec<Color> = Vec::new();
    palette.resize_with(256, Default::default);

    while let Some(op) = parser::parse(&mut itr) {
        println!("{:?}", op);
        match op {
            Op::RasterAttributes(pan, pad, ph, pv) => {
                img.height = (pan * pv + 5) as usize / 6 * 6;
                img.width = (pad * ph) as usize;
                println!("w={}, h={}", img.width, img.height);
                img.buf
                    .resize_with(img.width * img.height * 4, Default::default);
            }
            Op::CarriageReturn => {
                x = 0;
            }
            Op::NextLine => {
                y += 6;
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
                if img.buf.len() <= img.height * img.width {
                    img.height +=
                        (img.height * img.width - img.buf.len() + img.width - 1) / img.width;
                    img.buf
                        .resize_with(img.width * img.height * 4, Default::default);
                }
                for _ in 0..r {
                    for i in 0..6 {
                        if ((b >> i) & 1) > 0 {
                            img.buf[((y + i) * img.width + x) * 4 + argb_ord[0]] = color.a;
                            img.buf[((y + i) * img.width + x) * 4 + argb_ord[1]] += color.r;
                            img.buf[((y + i) * img.width + x) * 4 + argb_ord[2]] += color.g;
                            img.buf[((y + i) * img.width + x) * 4 + argb_ord[3]] += color.b;
                        }
                    }
                    x += 1;
                }
            }
            _ => panic!("unsupported"),
        }
    }
    assert_eq!(img.buf.len(), 4 * img.width * img.height);
    img
}

#[test]
fn test_decode() {
    let b = "\"1;1;6;6#0;2;255;0;0#1;2;0;255;0#2;2;0;0;255#0~~!4?$#1??!2~??$#2????~~\x1b\\";
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

    let b = "\"1;1;10;10#0;2;6;25;44#1;2;9;22;41#2;2;16;9;31#3;2;22;6;25#4;2;16;19;31#5;2;9;35;41#6;2;22;16;25#7;2;16;31;31#8;2;31;13;16#9;2;22;28;25#10;2;16;44;31#11;2;31;25;16#12;2;22;41;25#13;2;38;22;9#14;2;31;38;16#15;2;25;50;25#16;2;38;35;9#0BB#1FB@#2@B@$#5[K#4?CME#11__$#10__#9?_oo#3?AFF$#7?OwW#6?G[K$#8!7?Oww-#15KKG#14KME#13??FN$#10B@#9??@#16GKKG$#12?AFB#11?@BB\x1b\\";
    let mut itr = b.chars();
    let image = decode(&mut itr, [0, 1, 2, 3], 6, None);
    assert_eq!(image.width, 10);
    assert_eq!(image.height, 12);
}
