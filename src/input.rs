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
                    Scancode::LShift | Scancode::RShift => None,
                    Scancode::Home => wrap(b"\x1b[1~"),
                    Scancode::End => wrap(b"\x1b[4~"),
                    Scancode::Backspace => wrap(b"\x7F"),
                    Scancode::Return => wrap(b"\n"),
                    Scancode::Escape => wrap(b"\x1b"),
                    Scancode::Tab => wrap(b"\t"),
                    _ => None,
                },
                None => None,
            }
        }
        _ => panic!("must be key event"),
    }
}
