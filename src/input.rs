use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::keyboard::Mod;

pub fn keyevent_to_bytes(event: &sdl2::event::Event) -> Option<Vec<u8>> {
    // println!("{:?}", event);
    match event {
        Event::TextInput { text: s, .. } => {
            #[cfg(debug_assertions)]
            println!("text input: {}", s);
            Some(s.clone().into_bytes().to_vec())
        }
        Event::TextEditing {
            text: s,
            start: st,
            length: len,
            ..
        } => {
            #[cfg(debug_assertions)]
            println!("text editing: s={}, start={}, length={}", s, st, len);
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
            }
            fn wrap(bytes: &[u8]) -> Option<Vec<u8>> {
                Some(bytes.to_vec())
            }
            macro_rules! gen_match {
                ($([$p:pat, $e:expr]),*) => {
                    match (ctrl, shift, alt) {
                        $($p => wrap($e),)*
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

                    Keycode::Home => wrap(b"\x1b[1~"),
                    Keycode::End => wrap(b"\x1b[4~"),
                    Keycode::Backspace => wrap(b"\x7F"),
                    Keycode::Delete => wrap(b"\x1bOC\x7F"),
                    Keycode::Return => wrap(b"\r"),
                    Keycode::Escape => wrap(b"\x1b"),
                    Keycode::Tab => wrap(b"\t"),

                    //:k1=\EOP:k2=\EOQ:k3=\EOR:k4=\EOS:k5=\EOt:k6=\EOu:k7=\EOv:k8=\EOl:k9=\EOw:k;=\EOx:
                    Keycode::F1 => wrap(b"\x1bOP"),
                    Keycode::F2 => wrap(b"\x1bOQ"),
                    Keycode::F3 => wrap(b"\x1bOR"),
                    Keycode::F4 => wrap(b"\x1bOS"),
                    Keycode::F5 => wrap(b"\x1bOt"),
                    Keycode::F6 => wrap(b"\x1bOu"),
                    Keycode::F7 => wrap(b"\x1bOv"),
                    Keycode::F8 => wrap(b"\x1bOl"),
                    Keycode::F9 => wrap(b"\x1bOw"),
                    Keycode::F10 => wrap(b"\x1bOx"),

                    Keycode::Up => wrap(b"\x1bOA"),
                    Keycode::Down => wrap(b"\x1bOB"),
                    Keycode::Right => wrap(b"\x1bOC"),
                    Keycode::Left => wrap(b"\x1bOD"),
                    _ => None,
                },
                None => None,
            }
        }
        _ => panic!("must be key event"),
    }
}
