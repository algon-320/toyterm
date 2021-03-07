mod basics;
mod input;
mod terminal;

use std::collections::HashMap;
use std::os::unix::io::RawFd;
use std::sync::mpsc;

use anyhow::Result;
use nix::ioctl_write_ptr_bad;
use nix::sys::{signal, wait};
use nix::unistd;
use sdl2::event::Event;

use basics::*;
use terminal::Term;

#[macro_export]
macro_rules! config_get {
    ($config:expr, $key:expr, $result:ty) => {{
        $config
            .get($key)
            .cloned()
            .and_then(|v| v.try_into::<$result>().ok())
    }};
}

fn tiocswinsz(pty_master: RawFd, winsz: &nix::pty::Winsize) -> Result<()> {
    ioctl_write_ptr_bad!(tiocswinsz, nix::libc::TIOCSWINSZ, nix::pty::Winsize);
    unsafe { tiocswinsz(pty_master, winsz as *const nix::pty::Winsize) }?;
    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();

    let general_config = {
        config::Config::default()
            .merge(config::File::with_name("settings.toml"))
            .and_then(|c| c.get_table("general"))
            .unwrap_or_else(|_| HashMap::new())
    };

    let rows = config_get!(general_config, "rows", usize).unwrap_or(24);
    let cols = config_get!(general_config, "columns", usize).unwrap_or(80);

    let pty = nix::pty::forkpty(None, None).expect("forkpty");
    match pty.fork_result {
        unistd::ForkResult::Parent { child, .. } => {
            // set screen size
            let winsz = nix::pty::Winsize {
                ws_row: rows as u16,
                ws_col: cols as u16,
                ws_xpixel: 0, // unused
                ws_ypixel: 0, // unused
            };
            tiocswinsz(pty.master, &winsz)?;

            let sdl_context = sdl2::init().expect("sdl2 init");
            let ttf_context = sdl2::ttf::init().expect("sdl2 ttf init");
            let mut render_context = terminal::render::RenderContext::new(
                "toyterm",
                &sdl_context,
                &ttf_context,
                Size {
                    width: cols,
                    height: rows,
                },
            );
            let mut term = Term::new(
                &mut render_context,
                Size {
                    width: cols,
                    height: rows,
                },
            );

            let mut event_pump = sdl_context.event_pump().expect("misuse of event_pump");
            let event_subsys = sdl_context.event().unwrap();
            let event_sender = event_subsys.event_sender();

            let master_readable_event_id = unsafe {
                event_subsys
                    .register_event()
                    .expect("too many custom events")
            };

            let (send, recv) = mpsc::sync_channel(1);
            {
                // spawn a thread which reads bytes from the slave
                // and forwards them to the main thread
                let mut buf = vec![0; 4 * 1024];
                std::thread::spawn(move || loop {
                    match unistd::read(pty.master, &mut buf) {
                        Ok(nb) => {
                            let bytes = buf[..nb].to_vec();
                            send.send(bytes).unwrap();

                            // notify
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
                        }
                        Err(_) => {
                            event_sender
                                .push_event(Event::Quit { timestamp: 0 })
                                .unwrap();
                            break;
                        }
                    }
                });
            }

            for event in event_pump.wait_iter() {
                match event {
                    Event::Quit { .. } => break,
                    Event::TextInput { .. } | Event::TextEditing { .. } | Event::KeyDown { .. } => {
                        match input::keyevent_to_bytes(&event) {
                            None => continue,
                            Some(bytes) => {
                                log::trace!("<---(user): {:?}", String::from_utf8_lossy(&bytes));
                                nix::unistd::write(pty.master, bytes)?;
                            }
                        }
                    }
                    Event::User {
                        type_: user_event_id,
                        ..
                    } if user_event_id == master_readable_event_id => {
                        let bytes: Vec<u8> = recv.recv().unwrap();
                        log::trace!("(shell)-->: {:?}", String::from_utf8_lossy(&bytes));

                        term.write(&bytes);
                        term.render();
                    }
                    _ => {}
                }
            }

            signal::kill(child, signal::Signal::SIGHUP)?;
            wait::waitpid(child, None)?;
            Ok(())
        }
        unistd::ForkResult::Child => {
            use std::env;
            use std::ffi::CString;

            let shell_fallback = || env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
            let shell = config_get!(general_config, "shell", String).unwrap_or_else(shell_fallback);
            let shell = CString::new(shell).expect("null-char");

            let mut args: Vec<CString> = vec![shell.clone()];
            args.extend(
                config_get!(general_config, "shell_args", Vec<String>)
                    .unwrap_or_else(Vec::new)
                    .into_iter()
                    .map(|arg| CString::new(arg).expect("null-char")),
            );

            env::set_var("TERM", "toyterm-256color");
            env::set_var("COLORTERM", "truecolor");
            env::set_var("COLUMNS", &cols.to_string());
            env::set_var("LINES", &rows.to_string());

            unistd::execv(&shell, &args).expect("failed to spawn a shell");
            unreachable!();
        }
    }
}
