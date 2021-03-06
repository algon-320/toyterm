macro_rules! extract_config {
    ($config:expr, $key:expr, $result:ty) => {{
        $config
            .get($key)
            .cloned()
            .and_then(|v| v.try_into::<$result>().ok())
    }};
}

mod basics;
mod input;
mod terminal;
mod utils;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use nix::ioctl_write_ptr_bad;
use nix::sys::select;
use nix::sys::signal;
use nix::unistd;
use sdl2::event::Event;

use basics::*;
use terminal::Term;

const BUFFER_SIZE: usize = 1024 * 10;

fn main() -> Result<()> {
    let general_config: HashMap<String, _> = {
        config::Config::default()
            .merge(config::File::with_name("settings.toml"))
            .and_then(|c| c.get_table("general"))
            .unwrap_or_else(|_| HashMap::new())
    };

    let rows = extract_config!(general_config, "rows", usize).unwrap_or(24);
    let columns = extract_config!(general_config, "columns", usize).unwrap_or(80);

    let pty = nix::pty::forkpty(None, None).expect("forkpty");
    match pty.fork_result {
        unistd::ForkResult::Parent { child, .. } => {
            // set screen size
            const TIOCSWINSZ: usize = 0x5414;
            ioctl_write_ptr_bad!(tiocswinsz, TIOCSWINSZ, nix::pty::Winsize);
            let winsz = nix::pty::Winsize {
                ws_row: rows as u16,
                ws_col: columns as u16,
                ws_xpixel: 0, // unused
                ws_ypixel: 0, // unused
            };
            unsafe { tiocswinsz(pty.master, &winsz as *const nix::pty::Winsize) }?;

            let sdl_context = sdl2::init().unwrap();
            let ttf_context = sdl2::ttf::init().unwrap();
            let mut render_context = terminal::render::RenderContext::new(
                "toyterm",
                &sdl_context,
                &ttf_context,
                Size {
                    width: columns,
                    height: rows,
                },
            );
            let mut term = Term::new(
                &mut render_context,
                Size {
                    width: columns,
                    height: rows,
                },
            );

            let mut event_pump = sdl_context.event_pump().map_err(|e| anyhow!("{}", e))?;
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
                                data1: std::ptr::null_mut::<core::ffi::c_void>(),
                                data2: std::ptr::null_mut::<core::ffi::c_void>(),
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
                                nix::unistd::write(pty.master, bytes.as_slice())?;
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

                        term.write(&buf[..bytes]);
                        term.render();

                        enqueued_flag.store(false, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            signal::kill(child, signal::Signal::SIGHUP)?;
            nix::sys::wait::waitpid(child, None)?;
            Ok(())
        }
        unistd::ForkResult::Child => {
            use std::ffi::CString;

            let shell = extract_config!(general_config, "shell", String)
                .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned()));
            let shell = CString::new(shell).expect("null-char");

            let args: Vec<_> = {
                let mut ret = Vec::new();
                ret.push(shell.clone());
                ret.extend(
                    extract_config!(general_config, "shell_args", Vec<String>)
                        .map(|args| {
                            args.into_iter()
                                .map(|arg| CString::new(arg).unwrap())
                                .collect()
                        })
                        .unwrap_or_else(Vec::new),
                );
                ret
            };

            std::env::set_var("TERM", "toyterm-256color");
            std::env::set_var("COLORTERM", "truecolor");
            std::env::set_var("COLUMNS", &columns.to_string());
            std::env::set_var("LINES", &rows.to_string());

            nix::unistd::execv(&shell, &args).expect("failed to spawn a shell");
            unreachable!()
        }
    }
}
