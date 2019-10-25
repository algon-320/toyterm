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
            macro_rules! contains_match {
                ($(ANY [$x0:expr $(, $x:expr)*] => $y:expr),*) => {
                    match state {
                        $(tmp if tmp.contains($x0) $(|| tmp.contains($x))* => { $y },)*
                        _ => None,
                    }
                };
            }
            match scancode {
                Some(code) => match code {
                    Scancode::C => contains_match! {
                        ANY [Mod::LCTRLMOD, Mod::RCTRLMOD] => Some(b"\x03".to_vec())
                    },
                    Scancode::D => contains_match! {
                        ANY [Mod::LCTRLMOD, Mod::RCTRLMOD] => Some(b"\x04".to_vec())
                    },
                    Scancode::LShift | Scancode::RShift => None,
                    Scancode::Home => Some(b"\x1b[1~".to_vec()),
                    Scancode::End => Some(b"\x1b[4~".to_vec()),
                    Scancode::Backspace => Some(b"\x7F".to_vec()),
                    Scancode::Return => Some(b"\n".to_vec()),
                    Scancode::Escape => Some(b"\x1b".to_vec()),
                    Scancode::Tab => Some(b"\t".to_vec()),
                    _ => None,
                },
                None => None,
            }
        }
        _ => panic!("must be key event"),
    }
}
