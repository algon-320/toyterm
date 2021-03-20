use super::{CellAttribute, Color, ControlOp, CursorMove, Style};
use crate::basics::*;

use std::iter::Peekable;
fn number<I>(itr: &mut Peekable<I>) -> Option<i64>
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

fn arguments<I>(itr: &mut I) -> Option<(bool, Vec<Option<i64>>, char)>
where
    I: Iterator<Item = char>,
{
    let mut itr = itr.peekable();
    let question = match itr.peek() {
        Some(&'?') => {
            itr.next().unwrap();
            true
        }
        _ => false,
    };
    let mut args = Vec::new();
    let fin = loop {
        args.push(number(&mut itr));
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
    Some((question, args, fin))
}

fn color(args: &[i64]) -> Option<Color> {
    match args[0] {
        30..=37 | 40..=47 => match args[0] % 10 {
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
        90..=97 | 100..=107 => match args[0] % 10 {
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
        38 | 48 => match &args[1..] {
            [5, idx] => {
                match idx {
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
            // 24-bit colors
            [2, red, green, blue] => Some(Color::RGB(*red as u8, *green as u8, *blue as u8)),
            _ => None,
        },
        _ => None,
    }
}

fn sgr(args: &[i64]) -> Option<(Option<Style>, Option<Color>, Option<Color>)> {
    let mut style = None;
    let mut fg = None;
    let mut bg = None;
    for (i, arg) in args.iter().enumerate() {
        match arg {
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
            _arg @ 30..=38 | _arg @ 90..=97 => {
                fg = Some(color(&args[i..])?);
                break;
            }
            _arg @ 40..=48 | _arg @ 100..=107 => {
                bg = Some(color(&args[i..])?);
                break;
            }
            _ => {}
        }
    }
    Some((style, fg, bg))
}

fn csi<I>(itr: &mut I) -> Option<ControlOp>
where
    I: Iterator<Item = char>,
{
    let (question, args, fin_char) = arguments(itr)?;
    log::trace!(
        "CSI({}{:?}, {:?})",
        if question { "? " } else { "" },
        args,
        fin_char
    );

    if question {
        match (args.as_slice(), fin_char) {
            ([Some(1)], 'h') => Some(ControlOp::SetCursorMode(true)),
            ([Some(1)], 'l') => Some(ControlOp::SetCursorMode(false)),
            ([Some(25)], 'h') => Some(ControlOp::ShowCursor),
            ([Some(25)], 'l') => Some(ControlOp::HideCursor),
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
    } else {
        match (args.as_slice(), fin_char) {
            // Cursor Home
            ([None], 'f') | ([None], 'H') => {
                Some(ControlOp::CursorMove(CursorMove::Exact(Point {
                    x: 0,
                    y: 0,
                })))
            }
            ([y, x], 'f') | ([y, x], 'H') => {
                Some(ControlOp::CursorMove(CursorMove::Exact(Point {
                    x: x.unwrap_or(1).checked_sub(1)? as ScreenCellIdx,
                    y: y.unwrap_or(1).checked_sub(1)? as ScreenCellIdx,
                })))
            }
            // Cursor Up
            ([amount], 'A') => Some(ControlOp::CursorMove(CursorMove::Up(
                amount.unwrap_or(1) as usize
            ))),
            // Cursor Down
            ([amount], 'B') => Some(ControlOp::CursorMove(CursorMove::Down(
                amount.unwrap_or(1) as usize
            ))),
            // Cursor Forward
            ([amount], 'C') => Some(ControlOp::CursorMove(CursorMove::Right(
                amount.unwrap_or(1) as usize,
            ))),
            // Cursor Backward
            ([amount], 'D') => Some(ControlOp::CursorMove(CursorMove::Left(
                amount.unwrap_or(1) as usize
            ))),

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
                Some(ControlOp::SetScrollRange((top)..(bot + 1)))
            }

            // SGR (Select Graphic Rendition)
            (args, 'm') => {
                let default_zero: Vec<_> = args.iter().map(|op| op.unwrap_or(0)).collect();
                let (style, fg, bg) = sgr(&default_zero)?;
                Some(ControlOp::ChangeCellAttribute(style, fg, bg))
            }

            _ => None,
        }
    }
}

enum State {
    NotStarted,
    EscapeSequence,
    Csi(Vec<char>),
    Sixel(Vec<char>),
}
impl State {
    fn start(&mut self, input: char) -> Option<ControlOp> {
        *self = State::NotStarted;
        match input {
            '\x00' => Some(ControlOp::Ignore),
            '\x07' => Some(ControlOp::Bell),
            '\x08' => Some(ControlOp::CursorMove(CursorMove::Left(1))),
            '\x09' => Some(ControlOp::Tab),
            '\x0A' => Some(ControlOp::LineFeed),
            '\x0D' => Some(ControlOp::CarriageReturn),
            '\x1B' => {
                *self = State::EscapeSequence;
                None
            }
            x => Some(ControlOp::InsertChar(x)),
        }
    }

    fn escape_sequence(&mut self, input: char) -> Option<ControlOp> {
        *self = State::NotStarted;
        match input {
            '[' => {
                *self = State::Csi(Vec::new());
                None
            }
            'D' => Some(ControlOp::ScrollUp),
            'M' => Some(ControlOp::ScrollDown),
            'P' => {
                *self = State::Sixel(Vec::new());
                None
            }
            '7' => Some(ControlOp::SaveCursor),
            '8' => Some(ControlOp::RestoreCursor),
            '=' => Some(ControlOp::Ignore),
            '>' => Some(ControlOp::Ignore),
            'c' => Some(ControlOp::Reset),
            x => {
                log::warn!("Unkwon escape sequence: \\E {:?}", x);
                None
            }
        }
    }

    fn csi(&mut self, input: char) -> Option<ControlOp> {
        match input {
            '?' | '0'..='9' | ';' | ' ' => match self {
                State::Csi(buf) => {
                    buf.push(input);
                    None
                }
                _ => unreachable!(),
            },
            _ => match std::mem::replace(self, State::NotStarted) {
                State::Csi(mut buf) => {
                    buf.push(input);
                    let mut iter = buf.iter().copied();
                    match csi(&mut iter) {
                        None => {
                            log::warn!("Unknown CSI sequnce: {:?}", buf);
                            None
                        }
                        Some(op) => Some(op),
                    }
                }
                _ => unreachable!(),
            },
        }
    }

    fn sixel(&mut self, input: char) -> Option<ControlOp> {
        let last = match self {
            State::Sixel(buf) => buf.last() == Some(&'\x1b') && input == '\\',
            _ => unreachable!(),
        };
        if last {
            match std::mem::replace(self, State::NotStarted) {
                State::Sixel(mut buf) => {
                    buf.push(input);
                    *self = State::NotStarted;
                    let mut itr = buf.into_iter();
                    let img = sixel::decode(&mut itr, [3, 2, 1, 0], 0, None);
                    Some(ControlOp::Sixel(img))
                }
                _ => unreachable!(),
            }
        } else {
            match self {
                State::Sixel(buf) => {
                    if !(buf.is_empty() && input == 'q') {
                        buf.push(input);
                    }
                    None
                }
                _ => unreachable!(),
            }
        }
    }

    pub fn transfer(&mut self, input: char) -> Option<ControlOp> {
        match self {
            State::NotStarted => self.start(input),
            State::EscapeSequence => self.escape_sequence(input),
            State::Csi(_) => self.csi(input),
            State::Sixel(_) => self.sixel(input),
        }
    }
}

use std::collections::VecDeque;
pub struct Parser {
    op_buf: VecDeque<ControlOp>,
    state: State,
}
impl Parser {
    pub fn new() -> Self {
        Self {
            op_buf: VecDeque::new(),
            state: State::NotStarted,
        }
    }
    pub fn feed(&mut self, input: &str) -> bool {
        for c in input.chars() {
            if let Some(op) = self.state.transfer(c) {
                self.op_buf.push_back(op);
            }
        }
        !self.op_buf.is_empty()
    }
}
impl Iterator for Parser {
    type Item = ControlOp;
    fn next(&mut self) -> Option<ControlOp> {
        self.op_buf.pop_front()
    }
}
