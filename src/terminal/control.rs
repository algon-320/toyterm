use super::render::{CellAttribute, Color, Style};
use crate::basics::*;

#[derive(Debug)]
pub enum ControlOp {
    CursorHome(Point<ScreenCell>),
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
    SetTopBottom(usize, usize),
    Reset,
    ChangeCellAttribute(Option<Style>, Option<Color>, Option<Color>),
    SetCursorMode(bool),
    Sixel(sixel::Image),
    Ignore,
}

pub fn parse_escape_sequence<'a, I>(itr: &mut I) -> (Option<ControlOp>, usize)
where
    I: Iterator<Item = char> + Clone,
{
    let backup = itr.clone();
    match itr.next() {
        Some(c) => {
            let mut read_bytes = 1;
            let op = match c {
                // escape sequences
                '[' => {
                    let (args, fin_char) = {
                        let mut args = Vec::new();
                        let mut fin_char = None;
                        let mut tmp = None;
                        while let Some(c) = itr.next() {
                            read_bytes += 1;
                            match c {
                                x if '0' <= x && x <= '9' => {
                                    if tmp.is_none() {
                                        tmp = Some(0);
                                    } else {
                                        tmp = Some(tmp.unwrap() * 10);
                                    }
                                    tmp = Some(tmp.unwrap() + x.to_digit(10).unwrap());
                                }
                                ';' => {
                                    args.push(tmp);
                                    tmp = None;
                                }
                                x => {
                                    fin_char = Some(x);
                                    break;
                                }
                            }
                        }
                        if tmp.is_some() {
                            args.push(tmp);
                        }
                        (args, fin_char)
                    };
                    #[cfg(debug_assertions)]
                    println!("args:{:?}", args);

                    match fin_char {
                        // Cursor Home
                        Some('f') | Some('H') => match args.len() {
                            0 => Some(ControlOp::CursorHome(Point::new(1, 1))),
                            2 => Some(ControlOp::CursorHome(Point::new(
                                args[1].unwrap_or(1) as usize,
                                args[0].unwrap_or(1) as usize,
                            ))),
                            _ => None,
                        },
                        // Cursor Up
                        Some('A') => match args.len() {
                            0 => Some(ControlOp::CursorUp(1)),
                            1 => Some(ControlOp::CursorUp(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Down
                        Some('B') => match args.len() {
                            0 => Some(ControlOp::CursorDown(1)),
                            1 => Some(ControlOp::CursorDown(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Forward
                        Some('C') => match args.len() {
                            0 => Some(ControlOp::CursorForward(1)),
                            1 => Some(ControlOp::CursorForward(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Backward
                        Some('D') => match args.len() {
                            0 => Some(ControlOp::CursorBackward(1)),
                            1 => Some(ControlOp::CursorBackward(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },

                        // Save cursor position
                        Some('s') => match args.len() {
                            0 => Some(ControlOp::SaveCursor),
                            _ => None,
                        },
                        // Restore cursor position
                        Some('u') => match args.len() {
                            0 => Some(ControlOp::RestoreCursor),
                            _ => None,
                        },

                        // Erase end of line
                        Some('K') => match args.len() {
                            0 => Some(ControlOp::EraseEndOfLine),
                            1 => match args[0] {
                                Some(0) => Some(ControlOp::EraseEndOfLine),
                                Some(1) => Some(ControlOp::EraseStartOfLine),
                                Some(2) => Some(ControlOp::EraseLine),
                                _ => None,
                            },
                            _ => None,
                        },
                        Some('J') => match args.len() {
                            0 => Some(ControlOp::EraseDown),
                            1 => match args[0] {
                                Some(0) => Some(ControlOp::EraseDown),
                                Some(1) => Some(ControlOp::EraseUp),
                                Some(2) => Some(ControlOp::EraseScreen),
                                _ => None,
                            },
                            _ => None,
                        },

                        Some('r') => match args.len() {
                            2 => match (args[0], args[1]) {
                                (Some(x), Some(y)) => {
                                    Some(ControlOp::SetTopBottom(x as usize, y as usize))
                                }
                                _ => None,
                            },
                            _ => None,
                        },

                        Some('m') => {
                            let mut style = None;
                            let mut fg = None;
                            let mut bg = None;

                            // reset
                            if args.is_empty() {
                                let def = CellAttribute::default();
                                style = Some(def.style);
                                fg = Some(def.fg);
                                bg = Some(def.bg);
                            }

                            let mut itr = args.into_iter();
                            while let Some(a) = itr.next() {
                                match a {
                                    Some(0) => {
                                        // reset
                                        let def = CellAttribute::default();
                                        style = Some(def.style);
                                        fg = Some(def.fg);
                                        bg = Some(def.bg);
                                    }
                                    Some(1) => {
                                        style = Some(Style::Bold);
                                    }
                                    Some(4) => {
                                        style = Some(Style::UnderLine);
                                    }
                                    Some(5) => {
                                        style = Some(Style::Blink);
                                    }
                                    Some(7) => {
                                        style = Some(Style::Reverse);
                                    }
                                    Some(x) if x == 38 || x == 48 => {
                                        let color = match (itr.next(), itr.next()) {
                                            (Some(Some(5)), Some(Some(x))) if (x <= 15) => {
                                                Color::from_index(x as u8)
                                            }
                                            (Some(Some(5)), Some(Some(x))) if (232 <= x) => {
                                                let x = x - 232;
                                                let v = (x * 11) as u8;
                                                Color::RGB(v, v, v)
                                            }
                                            (Some(Some(5)), Some(Some(x))) if (x <= 255) => {
                                                let x = x - 16;
                                                let r = (x / 36) as u8;
                                                let g = ((x % 36) / 6) as u8;
                                                let b = (x % 6) as u8;
                                                Color::RGB(r * 51, g * 51, b * 51)
                                            }
                                            (Some(Some(2)), Some(Some(r))) => {
                                                use std::convert::identity as e;
                                                let g = itr.next().and_then(e).unwrap_or(255);
                                                let b = itr.next().and_then(e).unwrap_or(255);
                                                read_bytes += 2;
                                                Color::RGB(r as u8, g as u8, b as u8)
                                            }
                                            _ => Color::White,
                                        };
                                        if x == 38 {
                                            fg = Some(color);
                                        } else if x == 48 {
                                            bg = Some(color);
                                        }
                                        read_bytes += 2;
                                    }
                                    // foreground color
                                    Some(x) if (31 <= x && x <= 39) => {
                                        let c = x % 10;
                                        fg = Some(match c {
                                            1 => Color::Red,
                                            2 => Color::Green,
                                            3 => Color::Yellow,
                                            4 => Color::Blue,
                                            5 => Color::Magenta,
                                            6 => Color::Cyan,
                                            _ => Color::White,
                                        });
                                    }
                                    // background color
                                    Some(x) if (41 <= x && x <= 49) => {
                                        let c = x % 10;
                                        bg = Some(match c {
                                            1 => Color::Red,
                                            2 => Color::Green,
                                            3 => Color::Yellow,
                                            4 => Color::Blue,
                                            5 => Color::Magenta,
                                            6 => Color::Cyan,
                                            _ => Color::White,
                                        });
                                    }
                                    _ => {}
                                }
                            }
                            Some(ControlOp::ChangeCellAttribute(style, fg, bg))
                        }

                        Some('?') => {
                            read_bytes += 1;
                            let p =
                                || -> Option<(char, char)> { Some((itr.next()?, itr.next()?)) }();
                            match p {
                                Some(('1', 'h')) => Some(ControlOp::SetCursorMode(true)),
                                Some(('1', 'l')) => Some(ControlOp::SetCursorMode(false)),
                                _ => None,
                            }
                        }

                        Some(x) => {
                            // #[cfg(debug_assertions)]
                            println!("unsupported: \\E[{}", char::from(x));
                            None
                        }
                        None => None,
                    }
                }
                'D' => Some(ControlOp::ScrollDown),
                'M' => Some(ControlOp::ScrollUp),
                'P' => {
                    while let Some(x) = itr.next() {
                        if x == 'q' {
                            break;
                        }
                        read_bytes += 1;
                    }
                    let img = sixel::decode(itr, [3, 2, 1, 0], 0, None);
                    Some(ControlOp::Sixel(img))
                }
                '=' => Some(ControlOp::Ignore),
                '>' => Some(ControlOp::Ignore),
                'c' => Some(ControlOp::Reset),
                x => {
                    // #[cfg(debug_assertions)]
                    println!("unsupported: \\E{}", x);
                    None
                }
            };
            // revert iterator if it is followed by a invalid sequence
            if op.is_none() {
                *itr = backup;
            }
            (op, read_bytes)
        }
        None => (None, 0),
    }
}
