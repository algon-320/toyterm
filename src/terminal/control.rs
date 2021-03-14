use super::render::{CellAttribute, Color, Style};
use crate::basics::*;

#[derive(Debug)]
pub enum ControlOp {
    CursorHome(Point<ScreenCell>), // 0-origin
    CursorUp(usize),
    CursorDown(usize),
    CursorForward(usize),
    CursorBackward(usize),
    SaveCursor,
    RestoreCursor,
    ScrollDown,
    ScrollUp,
    EraseEndOfLine,
    EraseStartOfLine,
    EraseLine,
    EraseDown,
    EraseUp,
    EraseScreen,
    SetTopBottom(std::ops::Range<ScreenCellIdx>), // 0-origin
    Reset,
    ChangeCellAttribute(Option<Style>, Option<Color>, Option<Color>),
    SetCursorMode(bool),
    Sixel(sixel::Image),
    Ignore,
}

use std::iter::Peekable;
fn parse_numeric<I>(itr: &mut Peekable<I>) -> Option<i64>
where
    I: Iterator<Item = char>,
{
    let mut tmp = None;
    while let Some(d) = itr.peek().and_then(|c| c.to_digit(10)) {
        itr.next().unwrap();
        tmp = Some(tmp.unwrap_or(0i64) * 10 + d as i64);
    }
    tmp
}

fn parse_args<I>(itr: &mut I) -> Option<(Vec<Option<i64>>, char)>
where
    I: Iterator<Item = char>,
{
    let mut itr = itr.peekable();
    let mut args = Vec::new();
    let fin = loop {
        args.push(parse_numeric(&mut itr));
        match itr.peek() {
            Some(&';') => {
                itr.next().unwrap();
                continue;
            }
            Some(&fin) => {
                itr.next().unwrap();
                break fin;
            }
            None => {
                log::warn!("unexpected");
                return None;
            }
        }
    };
    Some((args, fin))
}

fn parse_color<I>(ty: i64, args: &mut I) -> Option<Color>
where
    I: Iterator<Item = Option<i64>>,
{
    match ty {
        30..=37 | 40..=47 => match ty % 10 {
            0 => Some(Color::Black),
            1 => Some(Color::Red),
            2 => Some(Color::Green),
            3 => Some(Color::Yellow),
            4 => Some(Color::Blue),
            5 => Some(Color::Magenta),
            6 => Some(Color::Cyan),
            7 => Some(Color::White),
            _ => unreachable!(),
        },
        90..=97 | 100..=107 => match ty % 10 {
            0 => Some(Color::Gray),
            1 => Some(Color::LightRed),
            2 => Some(Color::LightGreen),
            3 => Some(Color::LightYellow),
            4 => Some(Color::LightBlue),
            5 => Some(Color::LightMagenta),
            6 => Some(Color::LightCyan),
            7 => Some(Color::LightWhite),
            _ => unreachable!(),
        },
        38 | 48 => match args.next()?? {
            5 => {
                match args.next()?? {
                    0 => Some(Color::Black),
                    1 => Some(Color::Red),
                    2 => Some(Color::Yellow),
                    3 => Some(Color::Green),
                    4 => Some(Color::Blue),
                    5 => Some(Color::Magenta),
                    6 => Some(Color::Cyan),
                    7 => Some(Color::White),
                    8 => Some(Color::Gray),
                    9 => Some(Color::LightRed),
                    10 => Some(Color::LightYellow),
                    11 => Some(Color::LightGreen),
                    12 => Some(Color::LightBlue),
                    13 => Some(Color::LightMagenta),
                    14 => Some(Color::LightCyan),
                    15 => Some(Color::LightWhite),
                    x @ 16..=231 => {
                        // indexed colors
                        let x = x - 16;
                        let red = (x / 36) as u8;
                        let green = ((x % 36) / 6) as u8;
                        let blue = (x % 6) as u8;
                        Some(Color::RGB(red * 51, green * 51, blue * 51))
                    }
                    x @ 232..=255 => {
                        // grayscale colors
                        let x = x - 232;
                        let v = (x * 11) as u8;
                        Some(Color::RGB(v, v, v))
                    }
                    _ => None,
                }
            }
            2 => {
                // 24-bit colors
                let red = args.next()?.unwrap_or(255);
                let green = args.next()?.unwrap_or(255);
                let blue = args.next()?.unwrap_or(255);
                Some(Color::RGB(red as u8, green as u8, blue as u8))
            }
            _ => None,
        },
        _ => None,
    }
}

fn parse_sgr<I>(args: &mut I) -> Option<(Option<Style>, Option<Color>, Option<Color>)>
where
    I: Iterator<Item = Option<i64>>,
{
    let mut style = None;
    let mut fg = None;
    let mut bg = None;
    while let Some(arg) = args.next() {
        match arg.unwrap_or(0) {
            0 => {
                // reset
                let def = CellAttribute::default();
                style = Some(def.style);
                fg = Some(def.fg);
                bg = Some(def.bg);
            }
            1 => style = Some(Style::Bold),
            4 => style = Some(Style::UnderLine),
            5 => style = Some(Style::Blink),
            7 => style = Some(Style::Reverse),
            arg @ 30..=38 | arg @ 90..=97 => fg = Some(parse_color(arg, args)?),
            arg @ 40..=48 | arg @ 100..=107 => bg = Some(parse_color(arg, args)?),
            _ => {}
        }
    }
    Some((style, fg, bg))
}

fn csi<I>(itr: &mut I) -> Option<ControlOp>
where
    I: Iterator<Item = char>,
{
    let (args, fin_char) = parse_args(itr)?;
    log::trace!("CSI({:?}, {:?})", args, fin_char);

    match (args.as_slice(), fin_char) {
        // Cursor Home
        (args, 'f') | (args, 'H') => match args {
            [None] => Some(ControlOp::CursorHome(Point { x: 0, y: 0 })),
            [y, x] => Some(ControlOp::CursorHome(Point {
                x: x.unwrap_or(1).checked_sub(1)? as ScreenCellIdx,
                y: y.unwrap_or(1).checked_sub(1)? as ScreenCellIdx,
            })),
            _ => None,
        },
        // Cursor Up
        ([amount], 'A') => Some(ControlOp::CursorUp(amount.unwrap_or(1) as usize)),
        // Cursor Down
        ([amount], 'B') => Some(ControlOp::CursorDown(amount.unwrap_or(1) as usize)),
        // Cursor Forward
        ([amount], 'C') => Some(ControlOp::CursorForward(amount.unwrap_or(1) as usize)),
        // Cursor Backward
        ([amount], 'D') => Some(ControlOp::CursorBackward(amount.unwrap_or(1) as usize)),

        // Save cursor position
        ([None], 's') => Some(ControlOp::SaveCursor),
        // Restore cursor position
        ([None], 'u') => Some(ControlOp::RestoreCursor),

        // Erase line
        ([None], 'K') | ([Some(0)], 'K') => Some(ControlOp::EraseEndOfLine),
        ([Some(1)], 'K') => Some(ControlOp::EraseStartOfLine),
        ([Some(2)], 'K') => Some(ControlOp::EraseLine),

        // Erase screen
        ([None], 'J') | ([Some(0)], 'J') => Some(ControlOp::EraseDown),
        ([Some(1)], 'J') => Some(ControlOp::EraseUp),
        ([Some(2)], 'J') => Some(ControlOp::EraseScreen),

        // Scroll Region
        ([Some(top), Some(bot)], 'r') => {
            let top = (*top as ScreenCellIdx).checked_sub(1)?;
            let bot = (*bot as ScreenCellIdx).checked_sub(1)?;
            Some(ControlOp::SetTopBottom((top)..(bot + 1)))
        }

        // SGR (Select Graphic Rendition)
        (args, 'm') => {
            let (style, fg, bg) = parse_sgr(&mut args.iter().copied())?;
            Some(ControlOp::ChangeCellAttribute(style, fg, bg))
        }

        ([None], '?') => {
            let (arg, fin_char) = parse_args(itr)?;
            match (arg.as_slice(), fin_char) {
                ([Some(1)], 'h') => Some(ControlOp::SetCursorMode(true)),
                ([Some(1)], 'l') => Some(ControlOp::SetCursorMode(false)),
                ([Some(2004)], 'h') => {
                    // TODO
                    Some(ControlOp::Ignore)
                }
                ([Some(2004)], 'l') => {
                    // TODO
                    Some(ControlOp::Ignore)
                }
                _ => None,
            }
        }

        _ => None,
    }
}

pub fn parse_escape_sequence<I>(itr: &mut I) -> Option<ControlOp>
where
    I: Iterator<Item = char> + Clone,
{
    let backup = itr.clone();
    match itr.next() {
        Some(c) => {
            let op = match c {
                '[' => csi(itr),
                'D' => Some(ControlOp::ScrollUp),
                'M' => Some(ControlOp::ScrollDown),
                'P' => {
                    while let Some(c) = itr.next() {
                        if c == 'q' {
                            break;
                        }
                    }
                    let img = sixel::decode(itr, [3, 2, 1, 0], 0, None);
                    Some(ControlOp::Sixel(img))
                }
                '=' => Some(ControlOp::Ignore),
                '>' => Some(ControlOp::Ignore),
                'c' => Some(ControlOp::Reset),
                _ => None,
            };
            // revert the iterator if it is followed by a invalid sequence
            if op.is_none() {
                *itr = backup;
            }
            op
        }
        None => None,
    }
}
