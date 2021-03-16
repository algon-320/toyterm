use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::keyboard::Mod;

pub fn keyevent_to_bytes(event: &sdl2::event::Event) -> Option<&[u8]> {
    match event {
        Event::TextInput { text: s, .. } => Some(s.as_bytes()),
        Event::TextEditing { text: s, .. } => {
            log::trace!("text editing: {:?}", s);
            None
        }
        Event::KeyDown {
            keycode,
            keymod: state,
            ..
        } => {
            let ctrl = state.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD);
            let shift = state.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD);
            let alt = state.intersects(Mod::LALTMOD | Mod::RALTMOD);
            macro_rules! deco {
                (CTRL) => {
                    (true, false, false)
                };
                (SHIFT) => {
                    (false, true, false)
                };
                (ALT) => {
                    (false, false, true)
                };
                (()) => {
                    (false, false, false)
                };
            }
            macro_rules! gen_match {
                ($([$p:pat, $e:expr]),*) => {
                    match (ctrl, shift, alt) {
                        $($p => Some($e),)*
                        _ => None,
                    }
                };
            }
            match keycode {
                Some(code) => match code {
                    Keycode::At => gen_match!([deco!(CTRL), b"\x00"]),
                    Keycode::A => gen_match!([deco!(CTRL), b"\x01"]),
                    Keycode::B => gen_match!([deco!(CTRL), b"\x02"]),
                    Keycode::C => gen_match!([deco!(CTRL), b"\x03"]),
                    Keycode::D => gen_match!([deco!(CTRL), b"\x04"]),
                    Keycode::E => gen_match!([deco!(CTRL), b"\x05"]),
                    Keycode::F => gen_match!([deco!(CTRL), b"\x06"]),
                    Keycode::G => gen_match!([deco!(CTRL), b"\x07"]),
                    Keycode::H => gen_match!([deco!(CTRL), b"\x08"]),
                    Keycode::I => gen_match!([deco!(CTRL), b"\x09"]),
                    Keycode::J => gen_match!([deco!(CTRL), b"\x0a"]),
                    Keycode::K => gen_match!([deco!(CTRL), b"\x0b"]),
                    Keycode::L => gen_match!([deco!(CTRL), b"\x0c"]),
                    Keycode::M => gen_match!([deco!(CTRL), b"\x0d"]),
                    Keycode::N => gen_match!([deco!(CTRL), b"\x0e"]),
                    Keycode::O => gen_match!([deco!(CTRL), b"\x0f"]),
                    Keycode::P => gen_match!([deco!(CTRL), b"\x10"]),
                    Keycode::Q => gen_match!([deco!(CTRL), b"\x11"]),
                    Keycode::R => gen_match!([deco!(CTRL), b"\x12"]),
                    Keycode::S => gen_match!([deco!(CTRL), b"\x13"]),
                    Keycode::T => gen_match!([deco!(CTRL), b"\x14"]),
                    Keycode::U => gen_match!([deco!(CTRL), b"\x15"]),
                    Keycode::V => gen_match!([deco!(CTRL), b"\x16"]),
                    Keycode::W => gen_match!([deco!(CTRL), b"\x17"]),
                    Keycode::X => gen_match!([deco!(CTRL), b"\x18"]),
                    Keycode::Y => gen_match!([deco!(CTRL), b"\x19"]),
                    Keycode::Z => gen_match!([deco!(CTRL), b"\x1a"]),
                    Keycode::LeftBracket => gen_match!([deco!(CTRL), b"\x1B"]),
                    Keycode::Backslash => gen_match!([deco!(CTRL), b"\x1C"]),
                    Keycode::RightBracket => gen_match!([deco!(CTRL), b"\x1D"]),
                    Keycode::Caret => gen_match!([deco!(CTRL), b"\x1E"]),
                    Keycode::Underscore => gen_match!([deco!(CTRL), b"\x1F"]),
                    Keycode::Question => gen_match!([deco!(CTRL), b"\x7F"]),

                    Keycode::Home => {
                        gen_match!([deco!(CTRL), b"\x1b[1;5H"], [deco!(()), b"\x1b[H"])
                    }
                    Keycode::End => {
                        gen_match!([deco!(CTRL), b"\x1b[1;5F"], [deco!(()), b"\x1b[F"])
                    }

                    Keycode::Backspace => Some(b"\x7F"),
                    Keycode::Delete => Some(b"\x1bOC\x7F"),
                    Keycode::Return => Some(b"\r"),
                    Keycode::Escape => Some(b"\x1b"),
                    Keycode::Tab => Some(b"\t"),

                    //:k1=\EOP:k2=\EOQ:k3=\EOR:k4=\EOS:k5=\EOt:k6=\EOu:k7=\EOv:k8=\EOl:k9=\EOw:k;=\EOx:
                    Keycode::F1 => Some(b"\x1bOP"),
                    Keycode::F2 => Some(b"\x1bOQ"),
                    Keycode::F3 => Some(b"\x1bOR"),
                    Keycode::F4 => Some(b"\x1bOS"),
                    Keycode::F5 => Some(b"\x1bOt"),
                    Keycode::F6 => Some(b"\x1bOu"),
                    Keycode::F7 => Some(b"\x1bOv"),
                    Keycode::F8 => Some(b"\x1bOl"),
                    Keycode::F9 => Some(b"\x1bOw"),
                    Keycode::F10 => Some(b"\x1bOx"),

                    Keycode::Up => Some(b"\x1bOA"),
                    Keycode::Down => Some(b"\x1bOB"),
                    Keycode::Right => Some(b"\x1bOC"),
                    Keycode::Left => Some(b"\x1bOD"),

                    _ => None,
                },
                None => None,
            }
        }
        _ => panic!("must be key event"),
    }
}
