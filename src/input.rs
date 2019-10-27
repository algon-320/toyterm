use sdl2::event::Event;
use sdl2::keyboard::Mod;
use sdl2::keyboard::Scancode;

pub fn keyevent_to_bytes(event: &sdl2::event::Event) -> Option<Vec<u8>> {
    // println!("{:?}", event);
    match event {
        Event::TextInput { text: s, .. } => {
            println!("text input: {}", s);
            Some(s.clone().into_bytes().to_vec())
        }
        Event::TextEditing {
            text: s,
            start: st,
            length: len,
            ..
        } => {
            println!("text editing: s={}, start={}, length={}", s, st, len);
            None
        }
        Event::KeyDown {
            scancode,
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
            match scancode {
                Some(code) => match code {
                    Scancode::C => gen_match!([deco!(CTRL), b"\x03"]),
                    Scancode::D => gen_match!([deco!(CTRL), b"\x04"]),
                    Scancode::M => gen_match!([deco!(CTRL), b"\r"]),
                    Scancode::J => gen_match!([deco!(CTRL), b"\r"]),
                    Scancode::H => gen_match!([deco!(CTRL), b"\x7F"]),
                    Scancode::LShift | Scancode::RShift => None,
                    Scancode::Home => wrap(b"\x1b[1~"),
                    Scancode::End => wrap(b"\x1b[4~"),
                    Scancode::Backspace => wrap(b"\x7F"),
                    Scancode::Return => wrap(b"\n"),
                    Scancode::Escape => wrap(b"\x1b"),
                    Scancode::Tab => wrap(b"\t"),

                    //:k1=\EOP:k2=\EOQ:k3=\EOR:k4=\EOS:k5=\EOt:k6=\EOu:k7=\EOv:k8=\EOl:k9=\EOw:k;=\EOx:
                    Scancode::F1 => wrap(b"\x1bOP"),
                    Scancode::F2 => wrap(b"\x1bOQ"),
                    Scancode::F3 => wrap(b"\x1bOR"),
                    Scancode::F4 => wrap(b"\x1bOS"),
                    Scancode::F5 => wrap(b"\x1bOt"),
                    Scancode::F6 => wrap(b"\x1bOu"),
                    Scancode::F7 => wrap(b"\x1bOv"),
                    Scancode::F8 => wrap(b"\x1bOl"),
                    Scancode::F9 => wrap(b"\x1bOw"),
                    Scancode::F10 => wrap(b"\x1bOx"),

                    Scancode::Up => wrap(b"\x1bOA"),
                    Scancode::Down => wrap(b"\x1bOB"),
                    Scancode::Right => wrap(b"\x1bOC"),
                    Scancode::Left => wrap(b"\x1bOD"),
                    _ => None,
                },
                None => None,
            }
        }
        _ => panic!("must be key event"),
    }
}
