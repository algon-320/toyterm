// Reference: https://www.vt100.net/docs/vt3xx-gp/chapter14.html

use std::iter::Peekable;

const PIXEL_SIZE: usize = 3; // RGB

#[derive(Debug, Default)]
pub struct Image {
    pub width: u64,
    pub height: u64,
    pub data: Vec<u8>,
}

impl Image {
    fn new(width: u64, height: u64) -> Self {
        Image {
            width,

            // rounding up to a multiple of 6
            height: (height + 5) / 6 * 6,

            data: vec![0_u8; PIXEL_SIZE * (width * height) as usize],
        }
    }

    fn resize(&mut self, new_width: u64, new_height: u64) {
        self.width = new_width;
        self.height = (new_height + 5) / 6 * 6;
        let size = PIXEL_SIZE * (self.height * self.width) as usize;
        self.data.resize(size, 0_u8);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Function {
    Sixel { bits: u8, repeat: usize },
    RasterAttributes(u64, u64, u64, u64),
    CarriageReturn,
    NewLine,
    SelectColor(u8),
    DefineColor(u8, Color),
}

#[derive(Debug)]
pub struct Parser {
    colors: Vec<Color>,
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            colors: vec![Color::default(); 256],
        }
    }

    fn parse_numeric<I: Iterator<Item = char>>(&mut self, iter: &mut Peekable<I>) -> u64 {
        let mut num = 0u64;
        while let Some(digit @ '0'..='9') = iter.peek().copied() {
            iter.next();
            let digit = digit.to_digit(10).unwrap() as u64;
            num = num.saturating_mul(10).saturating_add(digit);
        }
        num
    }

    fn parse_parameters<I: Iterator<Item = char>>(&mut self, iter: &mut Peekable<I>) -> Vec<u64> {
        let mut ps: Vec<u64> = vec![];
        while iter.peek().is_some() {
            let param = self.parse_numeric(iter);
            ps.push(param);
            match iter.peek() {
                Some(&';') => {
                    iter.next();
                }
                _ => {
                    break;
                }
            }
        }
        ps
    }

    fn parse<I: Iterator<Item = char>>(&mut self, iter: &mut Peekable<I>) -> Option<Function> {
        let next = iter.peek()?;
        match *next {
            // Raster Attributes
            '"' => {
                iter.next();
                let ps = self.parse_parameters(iter);
                log::debug!("parameters = {:?}", ps);
                if ps.len() == 4 {
                    Some(Function::RasterAttributes(ps[0], ps[1], ps[2], ps[3]))
                } else {
                    None
                }
            }

            // Graphics Carriage Return
            '$' => {
                iter.next();
                Some(Function::CarriageReturn)
            }

            // Graphics New Line
            '-' => {
                iter.next();
                Some(Function::NewLine)
            }

            // Color selection
            '#' => {
                iter.next();
                let ps = self.parse_parameters(iter);
                match ps.as_slice() {
                    &[pc] => Some(Function::SelectColor(pc as u8)),
                    &[pc, pu @ (1 | 2), px, py, pz] => {
                        let reg = pc as u8;
                        match pu {
                            1 => {
                                // HLS
                                todo!();
                            }
                            2 => {
                                // RGB
                                let r = (px * 255 / 100) as u8;
                                let g = (py * 255 / 100) as u8;
                                let b = (pz * 255 / 100) as u8;
                                let color = Color { r, g, b };
                                Some(Function::DefineColor(reg, color))
                            }
                            _ => unreachable!(),
                        }
                    }
                    _ => {
                        // invalid
                        None
                    }
                }
            }

            // Graphics Repeat Introducer
            '!' => {
                iter.next();
                let repeat = self.parse_numeric(iter) as usize;
                match iter.peek() {
                    Some(&x @ '?'..='~') => {
                        iter.next();
                        let bits = ((x as u32) - ('?' as u32)) as u8;
                        Some(Function::Sixel { bits, repeat })
                    }
                    _ => None,
                }
            }

            // Single Sixel
            x @ '?'..='~' => {
                iter.next();
                let bits = ((x as u32) - ('?' as u32)) as u8;
                Some(Function::Sixel { bits, repeat: 1 })
            }

            x => {
                log::warn!("unknown function: {:?}", x);
                None
            }
        }
    }

    /// Decodes sixel string
    pub fn decode<I>(&mut self, iter: &mut I) -> Image
    where
        I: Iterator<Item = char>,
    {
        let mut iter = iter.peekable();

        let mut img = Image::new(0, 6);
        let mut color = Color::default();
        let mut x: u64 = 0;
        let mut y: u64 = 0;
        let mut pixel_w: u64 = 1;
        let mut pixel_h: u64 = 1;

        while let Some(func) = self.parse(&mut iter) {
            match func {
                Function::RasterAttributes(pan, pad, ph, pv) => {
                    pixel_h = pan;
                    pixel_w = pad;
                    img.resize(pixel_w * ph, pixel_h * pv);
                    log::debug!("buffer size changed: w={}, h={}", img.width, img.height);
                }
                Function::CarriageReturn => {
                    x = 0;
                }
                Function::NewLine => {
                    y += pixel_h * 6;
                    x = 0;
                }
                Function::SelectColor(reg) => {
                    color = self.colors[reg as usize];
                }
                Function::DefineColor(reg, c) => {
                    self.colors[reg as usize] = c;
                }
                Function::Sixel { bits, repeat } => {
                    let total = PIXEL_SIZE * ((y + pixel_h * 6) * img.width) as usize;

                    if img.data.len() < total {
                        let each_line = PIXEL_SIZE * img.width as usize;
                        let new_height = ((total + each_line - 1) / each_line) as u64;
                        img.resize(img.width, new_height);
                        log::debug!("image height changed: h={}", new_height);
                    }

                    for _ in 0..(pixel_w as usize) * repeat {
                        // FIXME
                        if x >= img.width {
                            log::debug!("line overflow");
                            break;
                        }

                        for i in 0..6 {
                            if ((bits >> i) & 1) == 0 {
                                continue;
                            }

                            for k in 0..pixel_h {
                                let y = y + i * pixel_h + k;
                                let offset = PIXEL_SIZE * (y * img.width + x) as usize;
                                img.data[offset + 0] += color.r;
                                img.data[offset + 1] += color.g;
                                img.data[offset + 2] += color.b;
                            }
                        }

                        x += 1;
                    }
                }
            }
        }

        img
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode() {
        let b = "\"1;1;6;6#0;2;100;0;0#1;2;0;100;0#2;2;0;0;100#0~~!4?$#1??!2~??$#2????~~\x1b\\";
        let mut itr = b.chars();

        let mut parser = Parser::new();
        let image = parser.decode(&mut itr);

        assert_eq!(image.width, 6);
        assert_eq!(image.height, 6);
        assert_eq!(
            image.data,
            vec![
                255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, //
                255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, //
                255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, //
                255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, //
                255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, //
                255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255, 0, 0, 0, 255, 0, 0, 255,
            ]
        );

        let b = "\"1;1;10;10\x1b\\";
        let mut itr = b.chars();
        let image = parser.decode(&mut itr);
        assert_eq!(image.width, 10);
        assert_eq!(image.height, 12);

        let b = "~~~~~~-~~~~~~\x1b\\";
        let mut itr = b.chars();
        let image = parser.decode(&mut itr);
        assert_eq!(image.width, 6);
        assert_eq!(image.height, 12);

        let b = "\"1;1;6;6~~~~~~-~~~~~~-???-!6~\x1b\\";
        let mut itr = b.chars();
        let image = parser.decode(&mut itr);
        assert_eq!(image.width, 6);
        assert_eq!(image.height, 24);

        let b = "\"2;2;10;10\x1b\\";
        let mut itr = b.chars();
        let image = parser.decode(&mut itr);
        assert_eq!(image.width, 20);
        assert_eq!(image.height, 24);

        let b = "\"2;3;6;6~~~~~~-~~~~~~-???-!6~\x1b\\";
        let mut itr = b.chars();
        let image = parser.decode(&mut itr);
        assert_eq!(image.width, 18);
        assert_eq!(image.height, 48);
    }
}
