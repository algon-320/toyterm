use super::Color;
use super::Op;
use std::iter::Peekable;

fn numeric<I>(seq: &mut Peekable<I>) -> u64
where
    I: Iterator<Item = char>,
{
    let mut num = 0u64;
    while let Some(c) = seq.peek() {
        if ('0'..='9').contains(c) {
            let c = seq.next().unwrap();
            num = num.saturating_mul(10);
            num = num.saturating_add(c.to_digit(10).unwrap() as u64);
        } else {
            log::trace!("c={}", c);
            break;
        }
    }
    num
}

fn parameters<I>(seq: &mut Peekable<I>) -> Vec<u64>
where
    I: Iterator<Item = char>,
{
    let mut ps: Vec<u64> = vec![];
    while let Some(&c) = seq.peek() {
        log::trace!("c={}", c);
        let tmp = numeric(seq);
        ps.push(tmp);
        if seq.peek() != Some(&';') {
            break;
        }
        seq.next();
    }
    ps
}

pub(crate) fn parse<I>(seq: &mut Peekable<I>) -> Option<Op>
where
    I: Iterator<Item = char>,
{
    match seq.peek() {
        Some(x) => match *x {
            '"' => {
                let _ = seq.next().unwrap();
                let ps = parameters(seq);
                log::debug!("parameters = {:?}", ps);
                Some(Op::RasterAttributes(ps[0], ps[1], ps[2], ps[3]))
            }
            '$' => {
                seq.next();
                Some(Op::CarriageReturn)
            }
            '-' => {
                seq.next();
                Some(Op::NextLine)
            }
            '#' => {
                seq.next();
                let ps = parameters(seq);
                match ps.len() {
                    1 => Some(Op::UseColor(ps[0] as u8)),
                    5 => {
                        let reg = ps[0] as u8;
                        let typ = ps[1];
                        match typ {
                            1 => {
                                // HLS
                                unimplemented!();
                            }
                            2 => {
                                // RGB
                                let r = (ps[2] * 255 / 100) as u8;
                                let g = (ps[3] * 255 / 100) as u8;
                                let b = (ps[4] * 255 / 100) as u8;
                                Some(Op::SetColor(reg, Color::new(r, g, b)))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            '\x1b' => {
                seq.next();
                if seq.peek() == Some(&&'\\') {
                    seq.next();
                    Some(Op::Finish)
                } else {
                    None
                }
            }
            '!' => {
                seq.next();
                let rep = numeric(seq);
                if seq.peek().is_none() {
                    None
                } else {
                    assert!(&'?' <= seq.peek().unwrap() && seq.peek().unwrap() <= &'~');
                    let x = seq.next().unwrap();
                    Some(Op::Sixel {
                        bits: ((x as u32) - ('?' as u32)) as u8,
                        rep,
                    })
                }
            }
            x if ('?'..='~').contains(&x) => {
                seq.next();
                Some(Op::Sixel {
                    bits: ((x as u32) - ('?' as u32)) as u8,
                    rep: 1,
                })
            }
            _ => None,
        },
        None => None,
    }
}

#[cfg(test)]
use std::str::Chars;

#[test]
fn test_parse_numeric() {
    let b = "012345";
    let mut itr = b.chars().peekable();
    let x = numeric::<Chars>(&mut itr);
    assert_eq!(x, 12345);
    let b = "0000000";
    let mut itr = b.chars().peekable();
    let x = numeric::<Chars>(&mut itr);
    assert_eq!(x, 0);
    let b = "9876543210";
    let mut itr = b.chars().peekable();
    let x = numeric::<Chars>(&mut itr);
    assert_eq!(x, 9876543210);
    let b = "10000000000000000000000000000000000000000";
    let mut itr = b.chars().peekable();
    let x = numeric::<Chars>(&mut itr);
    assert_eq!(x, u64::max_value());

    let b = "123ABC99999999999999999999999X456";
    let mut itr = b.chars().peekable();
    let x = numeric::<Chars>(&mut itr);
    assert_eq!(x, 123);
    assert_eq!(itr.next(), Some('A'));
    assert_eq!(itr.next(), Some('B'));
    let x = numeric::<Chars>(&mut itr);
    assert_eq!(x, 0);
    assert_eq!(itr.next(), Some('C'));
    let x = numeric::<Chars>(&mut itr);
    assert_eq!(x, u64::max_value());
    assert_eq!(itr.next(), Some('X'));
    let x = numeric::<Chars>(&mut itr);
    assert_eq!(x, 456);
    assert_eq!(itr.next(), None);
}

#[test]
fn test_parameters() {
    let b = "1;2;3";
    let itr = b.chars();
    let mut itr = itr.peekable();
    let ps = parameters::<Chars>(&mut itr);
    assert_eq!(ps, vec![1, 2, 3]);

    let b = "1";
    let mut itr = b.chars().peekable();
    let ps = parameters::<Chars>(&mut itr);
    assert_eq!(ps, vec![1]);

    let b = "";
    let mut itr = b.chars().peekable();
    let ps = parameters::<Chars>(&mut itr);
    assert_eq!(ps, vec![]);
}

#[test]
fn test_parse_sixel() {
    let b = "?";
    let mut itr = b.chars().peekable();
    assert_eq!(
        parse::<Chars>(&mut itr),
        Some(Op::Sixel { bits: 0, rep: 1 })
    );
    let b = "@";
    let mut itr = b.chars().peekable();
    assert_eq!(
        parse::<Chars>(&mut itr),
        Some(Op::Sixel { bits: 1, rep: 1 })
    );
    let b = "A";
    let mut itr = b.chars().peekable();
    assert_eq!(
        parse::<Chars>(&mut itr),
        Some(Op::Sixel { bits: 2, rep: 1 })
    );
    let b = "~";
    let mut itr = b.chars().peekable();
    assert_eq!(
        parse::<Chars>(&mut itr),
        Some(Op::Sixel { bits: 63, rep: 1 })
    );
    let b = "!123~";
    let mut itr = b.chars().peekable();
    assert_eq!(
        parse::<Chars>(&mut itr),
        Some(Op::Sixel { bits: 63, rep: 123 })
    );
}
#[test]
fn test_parse_raster_attributes() {
    let b = "\"1;2;3;4";
    let mut itr = b.chars().peekable();
    assert_eq!(
        parse::<Chars>(&mut itr),
        Some(Op::RasterAttributes(1, 2, 3, 4))
    );
}
#[test]
fn test_parse_cr() {
    let b = "$";
    let mut itr = b.chars().peekable();
    assert_eq!(parse::<Chars>(&mut itr), Some(Op::CarriageReturn));
}
#[test]
fn test_parse_nl() {
    let b = "-";
    let mut itr = b.chars().peekable();
    assert_eq!(parse::<Chars>(&mut itr), Some(Op::NextLine));
}
#[test]
fn test_parse_use_color() {
    let b = "#0";
    let mut itr = b.chars().peekable();
    assert_eq!(parse::<Chars>(&mut itr), Some(Op::UseColor(0)));
    let b = "#1";
    let mut itr = b.chars().peekable();
    assert_eq!(parse::<Chars>(&mut itr), Some(Op::UseColor(1)));
    let b = "#123";
    let mut itr = b.chars().peekable();
    assert_eq!(parse::<Chars>(&mut itr), Some(Op::UseColor(123)));
}
#[test]
fn test_parse_set_color() {
    let b = "#0;2;0;0;0";
    let mut itr = b.chars().peekable();
    assert_eq!(
        parse::<Chars>(&mut itr),
        Some(Op::SetColor(0, Color::new(0, 0, 0)))
    );
    let b = "#1;2;100;100;100";
    let mut itr = b.chars().peekable();
    assert_eq!(
        parse::<Chars>(&mut itr),
        Some(Op::SetColor(1, Color::new(255, 255, 255)))
    );
}
#[test]
fn test_parse() {
    let b = "\x1b\\";
    let mut itr = b.chars().peekable();
    assert_eq!(parse(&mut itr), Some(Op::Finish));

    let b = "\"1;1;10;10#0;2;100;0;0#1;2;0;100;0#2;2;0;0;100#0~~-#1~~-#2~~\x1b\\xyz";
    let mut itr = b.chars().peekable();
    let mut ops: Vec<Op> = vec![];
    while let Some(op) = parse(&mut itr) {
        ops.push(op);
        if op == Op::Finish {
            break;
        }
    }
    assert_eq!(
        ops,
        vec![
            Op::RasterAttributes(1, 1, 10, 10),
            Op::SetColor(0, Color::new(255, 0, 0)),
            Op::SetColor(1, Color::new(0, 255, 0)),
            Op::SetColor(2, Color::new(0, 0, 255)),
            Op::UseColor(0),
            Op::Sixel { bits: 63, rep: 1 },
            Op::Sixel { bits: 63, rep: 1 },
            Op::NextLine,
            Op::UseColor(1),
            Op::Sixel { bits: 63, rep: 1 },
            Op::Sixel { bits: 63, rep: 1 },
            Op::NextLine,
            Op::UseColor(2),
            Op::Sixel { bits: 63, rep: 1 },
            Op::Sixel { bits: 63, rep: 1 },
            Op::Finish
        ]
    );
    assert_eq!(itr.next(), Some('x'));
    assert_eq!(itr.next(), Some('y'));
    assert_eq!(itr.next(), Some('z'));
    assert_eq!(itr.next(), None);
}
