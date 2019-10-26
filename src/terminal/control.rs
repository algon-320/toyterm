use super::render::{CellAttribute, Style};
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
    ChangeCellAttribute(CellAttribute),
    SetCursorMode(bool),
    Ignore,
}

pub fn parse_escape_sequence<'a>(itr: &mut std::slice::Iter<'a, u8>) -> (Option<ControlOp>, usize) {
    let backup = itr.clone();
    match itr.next() {
        Some(c) => {
            let mut read_bytes = 1;
            let op = match c {
                // escape sequences
                b'[' => {
                    let (args, fin_char) = {
                        let mut args = Vec::new();
                        let mut fin_char = None;
                        let mut tmp = None;
                        while let Some(c) = itr.next() {
                            read_bytes += 1;
                            match *c {
                                x if b'0' <= x && x <= b'9' => {
                                    if tmp.is_none() {
                                        tmp = Some(0);
                                    } else {
                                        tmp = Some(tmp.unwrap() * 10);
                                    }
                                    tmp = Some(tmp.unwrap() + (x - b'0') as u32);
                                }
                                b';' => {
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
                        Some(b'f') | Some(b'H') => match args.len() {
                            0 => Some(ControlOp::CursorHome(Point::new(1, 1))),
                            2 => Some(ControlOp::CursorHome(Point::new(
                                args[1].unwrap_or(1) as usize,
                                args[0].unwrap_or(1) as usize,
                            ))),
                            _ => None,
                        },
                        // Cursor Up
                        Some(b'A') => match args.len() {
                            0 => Some(ControlOp::CursorUp(1)),
                            1 => Some(ControlOp::CursorUp(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Down
                        Some(b'B') => match args.len() {
                            0 => Some(ControlOp::CursorDown(1)),
                            1 => Some(ControlOp::CursorDown(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Forward
                        Some(b'C') => match args.len() {
                            0 => Some(ControlOp::CursorForward(1)),
                            1 => Some(ControlOp::CursorForward(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },
                        // Cursor Backward
                        Some(b'D') => match args.len() {
                            0 => Some(ControlOp::CursorBackward(1)),
                            1 => Some(ControlOp::CursorBackward(args[0].unwrap_or(1) as usize)),
                            _ => None,
                        },

                        // Save cursor position
                        Some(b's') => match args.len() {
                            0 => Some(ControlOp::SaveCursor),
                            _ => None,
                        },
                        // Restore cursor position
                        Some(b'u') => match args.len() {
                            0 => Some(ControlOp::RestoreCursor),
                            _ => None,
                        },

                        // Erase end of line
                        Some(b'K') => match args.len() {
                            0 => Some(ControlOp::EraseEndOfLine),
                            1 => match args[0] {
                                Some(0) => Some(ControlOp::EraseEndOfLine),
                                Some(1) => Some(ControlOp::EraseStartOfLine),
                                Some(2) => Some(ControlOp::EraseLine),
                                _ => None,
                            },
                            _ => None,
                        },
                        Some(b'J') => match args.len() {
                            0 => Some(ControlOp::EraseDown),
                            1 => match args[0] {
                                Some(0) => Some(ControlOp::EraseDown),
                                Some(1) => Some(ControlOp::EraseUp),
                                Some(2) => Some(ControlOp::EraseScreen),
                                _ => None,
                            },
                            _ => None,
                        },

                        Some(b'r') => match args.len() {
                            2 => match (args[0], args[1]) {
                                (Some(x), Some(y)) => {
                                    Some(ControlOp::SetTopBottom(x as usize, y as usize))
                                }
                                _ => None,
                            },
                            _ => None,
                        },

                        Some(b'm') => {
                            let mut style = CellAttribute::default();
                            for a in args.iter() {
                                match a {
                                    Some(0) => {
                                        // reset
                                        style = CellAttribute::default();
                                    }
                                    Some(1) => {
                                        style.style = Style::Bold;
                                    }
                                    Some(4) => {
                                        style.style = Style::UnderLine;
                                    }
                                    Some(5) => {
                                        style.style = Style::Blink;
                                    }
                                    Some(7) => {
                                        style.style = Style::Reverse;
                                    }
                                    _ => {}
                                }
                            }
                            Some(ControlOp::ChangeCellAttribute(style))
                        }

                        Some(b'?') => {
                            read_bytes += 1;
                            let p = || -> Option<(u8, u8)> { Some((*itr.next()?, *itr.next()?)) }();
                            match p {
                                Some((b'1', b'h')) => Some(ControlOp::SetCursorMode(true)),
                                Some((b'1', b'l')) => Some(ControlOp::SetCursorMode(false)),
                                _ => None,
                            }
                        }

                        Some(x) => {
                            #[cfg(debug_assertions)]
                            println!("unsupported: \\E[{}", char::from(x));
                            None
                        }
                        None => None,
                    }
                }
                b'D' => Some(ControlOp::ScrollDown),
                b'M' => Some(ControlOp::ScrollUp),
                b'=' => Some(ControlOp::Ignore),
                b'>' => Some(ControlOp::Ignore),
                b'c' => Some(ControlOp::Reset),
                x => {
                    #[cfg(debug_assertions)]
                    println!("unsupported: \\E{}", char::from(*x));
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
