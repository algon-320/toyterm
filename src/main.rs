#[macro_use]
extern crate nix;

#[macro_use]
extern crate lazy_static;

extern crate sdl2;

extern crate regex;

use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};
use nix::sys::stat::Mode;
use nix::unistd;

use std::os::unix::io::RawFd;
use std::path::Path;

use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::ttf;

mod basics;
mod input;
mod terminal;

use crate::basics::*;
use terminal::Term;
use terminal::PTY;

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

fn start(pty: &PTY) -> Result<(), String> {
    match unistd::fork() {
        Ok(unistd::ForkResult::Parent { child, .. }) => {
            conv_err(unistd::close(pty.slave))?;

            let sdl_context = sdl2::init().unwrap();
            let ttf_context = sdl2::ttf::init().unwrap();
            let mut term = Term::new(
                &sdl_context,
                &ttf_context,
                Size::new(80, 24),
                "./fonts/UbuntuMono-R.ttf",
                // "./fonts/dos_font.ttf",
                16,
            );
            let mut event_pump = sdl_context.event_pump()?;
            let event_subsys = sdl_context.event().unwrap();
            let event_sender = event_subsys.event_sender();
            let master_readable_event_id = unsafe { event_subsys.register_event().unwrap() };

            use std::sync::{
                atomic::{AtomicBool, Ordering},
                Arc,
            };
            let enqueued_flag = Arc::new(AtomicBool::new(false));

            // check whether the master FD is readable
            {
                let enqueued = enqueued_flag.clone();
                let master_fd = pty.master;
                std::thread::spawn(move || loop {
                    if enqueued.load(Ordering::Relaxed) {
                        continue;
                    }

                    let mut readable = nix::sys::select::FdSet::new();
                    readable.insert(master_fd);

                    unsafe {
                        static mut CNT: i32 = 0;
                        println!("wait... {}", CNT);
                        CNT += 1;
                    }

                    nix::sys::select::select(
                        None,
                        Some(&mut readable), // read
                        None,                // write
                        None,                // error
                        None,
                    )
                    .unwrap();

                    if readable.contains(master_fd) {
                        event_sender
                            .push_event(sdl2::event::Event::User {
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

            for event in event_pump.wait_iter() {
                match event {
                    Event::Quit { .. } => break,
                    Event::TextInput { .. } | Event::TextEditing { .. } | Event::KeyDown { .. } => {
                        match input::keycode_to_bytes(&event) {
                            None => continue,
                            Some(bytes) => {
                                println!("keydown: bytes: {:?}", bytes);
                                conv_err(nix::unistd::write(pty.master, bytes.as_slice()))?;
                            }
                        }
                    }
                    Event::User {
                        type_: user_event_id,
                        ..
                    } if user_event_id == master_readable_event_id => {
                        // read from master FD
                        let mut buf = vec![0; 1024 * 10];
                        let bytes = match nix::unistd::read(pty.master, &mut buf) {
                            Err(e) => {
                                eprintln!("Nothing to read from child: {}", e);
                                break;
                            }
                            Ok(sz) => sz,
                        };

                        #[cfg(debug_assertions)]
                        println!(
                            "buf: {:?}",
                            basics::pretty_format_ascii_bytes(&buf[..bytes])
                        );

                        term.write(&buf[..bytes]);
                        term.render_all()?;

                        enqueued_flag.store(false, std::sync::atomic::Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            // conv_err(nix::sys::wait::waitpid(child, None))?;
        }
        Ok(unistd::ForkResult::Child) => {
            conv_err(unistd::close(pty.master))?;

            // create process group
            conv_err(unistd::setsid())?;

            const TIOCSCTTY: usize = 0x540E;
            ioctl_write_int_bad!(tiocsctty, TIOCSCTTY);
            conv_err(unsafe { tiocsctty(pty.slave, 0) })?;

            conv_err(unistd::dup2(pty.slave, 0))?; // stdin
            conv_err(unistd::dup2(pty.slave, 1))?; // stdout
            conv_err(unistd::dup2(pty.slave, 2))?; // stderr
            conv_err(unistd::close(pty.slave))?;

            use std::ffi::CString;
            let path = CString::new("/bin/sh").unwrap();
            conv_err(unistd::execve(
                &path,
                &[],
                &[
                    CString::new("TERM=vt100").unwrap(),
                    CString::new("DISPLAY=:0").unwrap(),
                ],
            ))?;
        }
        Err(e) => return Err(e.to_string()),
    }
    Ok(())
}

fn main() {
    let pty = terminal::PTY::open().unwrap();
    start(&pty);
}
