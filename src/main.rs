#[macro_use]
extern crate nix;
#[macro_use]
extern crate lazy_static;
extern crate sdl2;

mod basics;
mod input;
mod terminal;
mod utils;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nix::sys::select;
use nix::unistd;
use sdl2::event::Event;

use basics::*;
use terminal::Term;
use utils::*;

const BUFFER_SIZE: usize = 1024 * 10;

fn set_input_rect(pos: Point<Pixel>) {
    unsafe {
        let mut text_input_rect = sdl2::sys::SDL_Rect {
            x: pos.x as i32,
            y: pos.y as i32,
            w: 0,
            h: 0,
        };
        sdl2::sys::SDL_SetTextInputRect(&mut text_input_rect as *mut sdl2::sys::SDL_Rect);
    }
}

fn main() -> Result<(), String> {
    let pty = terminal::pty::PTY::open().unwrap();
    match unistd::fork() {
        Ok(unistd::ForkResult::Parent { child, .. }) => {
            err_str(unistd::close(pty.slave))?;

            // set screen size
            const TIOCSWINSZ: usize = 0x5414;
            ioctl_write_ptr_bad!(tiocswinsz, TIOCSWINSZ, nix::pty::Winsize);
            let winsz = nix::pty::Winsize {
                ws_row: 24,
                ws_col: 80,
                ws_xpixel: 0, // unused
                ws_ypixel: 0, // unused
            };
            err_str(unsafe { tiocswinsz(pty.master, &winsz as *const nix::pty::Winsize) })?;

            let sdl_context = sdl2::init().unwrap();
            let ttf_context = sdl2::ttf::init().unwrap();

            let mut font = ttf_context
                // .load_font("./fonts/dos_font.ttf", 16)
                .load_font("./fonts/UbuntuMono-R.ttf", 25)
                .unwrap();
            let char_size = {
                let tmp = font.size_of_char('#').unwrap();
                Size::new(tmp.0 as usize, tmp.1 as usize)
            };
            let window = {
                let video = sdl_context.video().unwrap();
                video
                    .window(
                        "toyterm",
                        (char_size.width * 80) as u32,
                        (char_size.height * 24) as u32,
                    )
                    .position_centered()
                    .build()
                    .unwrap()
            };
            let mut canvas = window
                .into_canvas()
                .accelerated()
                .target_texture()
                .build()
                .unwrap();
            let mut texture_creator = canvas.texture_creator();

            let mut term = Term::new(
                &mut canvas,
                &mut texture_creator,
                &mut font,
                Size::new(80, 24),
            );
            let mut event_pump = sdl_context.event_pump()?;
            let event_subsys = sdl_context.event().unwrap();
            let event_sender = event_subsys.event_sender();
            let master_readable_event_id = unsafe { event_subsys.register_event().unwrap() };

            let enqueued_flag = Arc::new(AtomicBool::new(false));

            // check whether the master FD is readable
            {
                let enqueued = enqueued_flag.clone();
                let master_fd = pty.master;
                std::thread::spawn(move || loop {
                    if enqueued.load(Ordering::Relaxed) {
                        continue;
                    }

                    let mut readable = select::FdSet::new();
                    readable.insert(master_fd);

                    select::select(
                        None,
                        Some(&mut readable), // read
                        None,                // write
                        None,                // error
                        None,
                    )
                    .unwrap();

                    if readable.contains(master_fd) {
                        event_sender
                            .push_event(Event::User {
                                timestamp: 0,
                                window_id: 0,
                                type_: master_readable_event_id,
                                code: 0,
                                data1: 0 as *mut core::ffi::c_void,
                                data2: 0 as *mut core::ffi::c_void,
                            })
                            .unwrap();
                        enqueued.store(true, Ordering::Relaxed);
                    }
                });
            }

            let mut buf = vec![0; BUFFER_SIZE];
            for event in event_pump.wait_iter() {
                match event {
                    Event::Quit { .. } => break,
                    Event::TextInput { .. } | Event::TextEditing { .. } | Event::KeyDown { .. } => {
                        match input::keyevent_to_bytes(&event) {
                            None => continue,
                            Some(bytes) => {
                                println!("keydown: bytes: {:?}", bytes);
                                err_str(nix::unistd::write(pty.master, bytes.as_slice()))?;
                            }
                        }
                    }
                    Event::User {
                        type_: user_event_id,
                        ..
                    } if user_event_id == master_readable_event_id => {
                        // read from master FD
                        let bytes = match nix::unistd::read(pty.master, &mut buf) {
                            Err(e) => {
                                eprintln!("Nothing to read from child: {}", e);
                                break;
                            }
                            Ok(sz) => sz,
                        };

                        #[cfg(debug_assertions)]
                        println!("buf: {:?}", utils::pretty_format_ascii_bytes(&buf[..bytes]));

                        term.write(&buf[..bytes]);
                        term.render()?;

                        enqueued_flag.store(false, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            // err_str(nix::sys::wait::waitpid(child, None))?;
            Ok(())
        }
        Ok(unistd::ForkResult::Child) => {
            err_str(unistd::close(pty.master))?;

            // create process group
            err_str(unistd::setsid())?;

            const TIOCSCTTY: usize = 0x540E;
            ioctl_write_int_bad!(tiocsctty, TIOCSCTTY);
            err_str(unsafe { tiocsctty(pty.slave, 0) })?;

            err_str(unistd::dup2(pty.slave, 0))?; // stdin
            err_str(unistd::dup2(pty.slave, 1))?; // stdout
            err_str(unistd::dup2(pty.slave, 2))?; // stderr
            err_str(unistd::close(pty.slave))?;

            use std::ffi::CString;
            let path = CString::new("/bin/sh").unwrap();

            setenv("TERM", "vt100", true).unwrap();

            err_str(unistd::execv(&path, &[])).map(|_| ())
        }
        Err(e) => err_str(Err(e)),
    }
}
