#[macro_use]
extern crate nix;
#[macro_use]
extern crate lazy_static;
extern crate config;
extern crate sdl2;
extern crate ucd;

mod basics;
mod input;
mod terminal;
#[allow(dead_code)]
mod utils;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nix::sys::select;
use nix::sys::signal;
use nix::unistd;
use sdl2::event::Event;

use basics::*;
use terminal::Term;
use utils::*;

const BUFFER_SIZE: usize = 1024 * 10;

fn main() -> Result<(), String> {
    let config = || -> Option<std::collections::HashMap<String, config::Value>> {
        let mut tmp = config::Config::default();
        tmp.merge(config::File::with_name("settings.toml")).ok()?;
        tmp.get_table("general").ok()
    }();
    macro_rules! find_config {
        ($key:expr, $func:path) => {
            config
                .as_ref()
                .and_then(|t| $func(t.get($key)?.clone()).ok())
        };
    }

    let rows = find_config!("rows", config::Value::into_int).unwrap_or(24) as usize;
    let columns = find_config!("columns", config::Value::into_int).unwrap_or(80) as usize;

    let pty = terminal::pty::PTY::open().unwrap();
    match unistd::fork() {
        Ok(unistd::ForkResult::Parent { child, .. }) => {
            err_str(unistd::close(pty.slave))?;

            // set screen size
            const TIOCSWINSZ: usize = 0x5414;
            ioctl_write_ptr_bad!(tiocswinsz, TIOCSWINSZ, nix::pty::Winsize);
            let winsz = nix::pty::Winsize {
                ws_row: rows as u16,
                ws_col: columns as u16,
                ws_xpixel: 0, // unused
                ws_ypixel: 0, // unused
            };
            err_str(unsafe { tiocswinsz(pty.master, &winsz as *const nix::pty::Winsize) })?;

            let sdl_context = sdl2::init().unwrap();
            let ttf_context = sdl2::ttf::init().unwrap();
            let mut render_context = terminal::render::RenderContext::new(
                "toyterm",
                &sdl_context,
                &ttf_context,
                Size::new(columns, rows),
            );
            let mut term = Term::new(&mut render_context, Size::new(columns, rows));

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
                                #[cfg(debug_assertions)]
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
                            Err(_) => {
                                break;
                            }
                            Ok(sz) => sz,
                        };

                        #[cfg(debug_assertions)]
                        println!("buf: {:?}", utils::pretty_format_ascii_bytes(&buf[..bytes]));

                        term.write(&buf[..bytes])?;
                        term.render()?;

                        enqueued_flag.store(false, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            err_str(signal::kill(child, signal::Signal::SIGHUP))?;
            err_str(nix::sys::wait::waitpid(child, None))?;
            Ok(())
        }
        Ok(unistd::ForkResult::Child) => {
            err_str(unistd::close(pty.master))?;

            use std::ffi::CString;
            let shell = {
                find_config!("shell", config::Value::into_str)
                    .and_then(|s| if s.is_empty() { None } else { Some(s) })
                    .unwrap_or(std::env::var("SHELL").unwrap_or("/bin/sh".to_string()))
            };
            let path = CString::new(shell.clone()).unwrap();
            let args: Vec<_> = {
                let mut ret = Vec::new();
                ret.push(path.clone());
                ret.append(
                    &mut find_config!("shell_args", config::Value::into_array)
                        .unwrap_or(Vec::new())
                        .into_iter()
                        .map(|v| CString::new(v.into_str().unwrap()).unwrap())
                        .collect::<Vec<_>>(),
                );
                ret
            };

            // create process group
            err_str(unistd::setsid())?;

            const TIOCSCTTY: usize = 0x540E;
            ioctl_write_int_bad!(tiocsctty, TIOCSCTTY);
            err_str(unsafe { tiocsctty(pty.slave, 0) })?;

            err_str(unistd::dup2(pty.slave, 0))?; // stdin
            err_str(unistd::dup2(pty.slave, 1))?; // stdout
            err_str(unistd::dup2(pty.slave, 2))?; // stderr
            err_str(unistd::close(pty.slave))?;

            std::env::set_var("TERM", "toyterm-256color");
            std::env::set_var("COLORTERM", "truecolor");
            std::env::set_var("COLUMNS", &columns.to_string());
            std::env::set_var("LINES", &rows.to_string());

            err_str(unistd::execv(&path, &args)).map(|_| ())
        }
        Err(e) => err_str(Err(e)),
    }
}
