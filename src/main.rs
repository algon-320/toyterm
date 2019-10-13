#[macro_use]
extern crate nix;

extern crate sdl2;

use nix::fcntl::{open, OFlag};
use nix::pty::{grantpt, posix_openpt, ptsname, unlockpt};
use nix::sys::stat::Mode;
use nix::unistd;

use std::collections::LinkedList;
use std::os::unix::io::RawFd;
use std::path::Path;

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::ttf;

struct PTY {
    pub master: RawFd,
    pub slave: RawFd,
}

fn conv_err<T, E: ToString>(e: Result<T, E>) -> std::result::Result<T, String> {
    e.map_err(|e| e.to_string())
}

fn openpty() -> Result<PTY, String> {
    // Open a new PTY master
    let master_fd = conv_err(posix_openpt(OFlag::O_RDWR))?;

    // Allow a slave to be generated for it
    conv_err(grantpt(&master_fd))?;
    conv_err(unlockpt(&master_fd))?;

    // Get the name of the slave
    let slave_name = conv_err(unsafe { ptsname(&master_fd) })?;

    // Try to open the slave
    let slave_fd = conv_err(open(Path::new(&slave_name), OFlag::O_RDWR, Mode::empty()))?;

    use std::os::unix::io::IntoRawFd;
    Ok(PTY {
        master: master_fd.into_raw_fd(),
        slave: slave_fd.into(),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Size<T> {
    width: T,
    height: T,
}
impl<T> Size<T> {
    pub fn new(width: T, height: T) -> Self {
        Size { width, height }
    }
}

use std::marker::PhantomData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Pixel;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BufferCell;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenCell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Position<U> {
    x: usize,
    y: usize,
    _phantom: PhantomData<U>,
}
impl<U> Position<U> {
    pub fn new(x: usize, y: usize) -> Self {
        Position {
            x,
            y,
            _phantom: PhantomData,
        }
    }
}

struct Screen {
    size: Size<usize>,
    cursor: Position<BufferCell>,
    buffer: Vec<Vec<char>>,
    offset_row: usize, // start index of showing rows
}
impl Screen {
    pub fn new(screen_size: Size<usize>) -> Self {
        Screen {
            size: screen_size,
            cursor: Position::new(0, 0),
            buffer: Vec::new(),
            offset_row: 0,
        }
    }

    // put a character on the cursor position
    fn put_char(&mut self, c: char) {
        while self.buffer.len() <= self.cursor.y {
            self.new_line();
        }
        assert!(self.cursor.y < self.buffer.len());

        let line = &mut self.buffer[self.cursor.y];
        while line.len() <= self.cursor.x {
            line.push(' ');
        }
        assert!(self.cursor.x < line.len());

        line[self.cursor.x] = c;

        self.cursor.x += 1;
        if self.cursor.x == self.size.width {
            self.cursor.x = 0;
            self.cursor.y += 1;
            self.new_line();
        }
    }

    fn new_line(&mut self) {
        self.buffer.push(vec![' '; self.size.width]);
    }

    fn get_screen_pos(&self, pos: Position<BufferCell>) -> Position<ScreenCell> {
        Position::new(pos.x, pos.y - self.offset_row)
    }
}

struct Console<'ttf> {
    ttf_context: &'ttf sdl2::ttf::Sdl2TtfContext,
    canvas: sdl2::render::Canvas<sdl2::video::Window>,
    font: sdl2::ttf::Font<'ttf, 'static>,
    screen: Screen,
    char_size: Size<usize>,
}
impl<'ttf> Console<'ttf> {
    fn new<P: AsRef<Path>>(
        sdl_context: &sdl2::Sdl,
        ttf_context: &'ttf sdl2::ttf::Sdl2TtfContext,
        size: Size<usize>,
        font_path: P,
        font_size: u16,
    ) -> Self {
        let font = ttf_context.load_font(font_path, font_size).unwrap();
        let char_size = font.size_of_char('#').unwrap();
        let char_size = Size::new(char_size.0 as usize, char_size.1 as usize);
        println!("[debug] font char size: {:?}", char_size);

        let window = {
            let video = sdl_context.video().unwrap();
            video
                .window(
                    "toyterm",
                    (char_size.width * size.width) as u32,
                    (char_size.height * size.height) as u32,
                )
                .position_centered()
                .build()
                .unwrap()
        };
        let canvas = window.into_canvas().build().unwrap();

        let mut cons = Console {
            ttf_context,
            canvas,
            font,
            screen: Screen::new(size),
            char_size,
        };
        cons.clear();
        cons
    }
    fn clear(&mut self) {
        self.canvas.set_draw_color(Color::RGB(0, 0, 0));
        self.canvas.clear();
    }

    fn draw_char(&mut self, c: char, p: Position<ScreenCell>) -> Result<(), String> {
        let surface = conv_err(
            self.font
                .render(&c.to_string())
                .blended(Color::RGB(255, 255, 255)),
        )?;

        {
            let tc = self.canvas.texture_creator();
            let texture = conv_err(tc.create_texture_from_surface(surface))?;
            let rect = Rect::new(
                (p.x * self.char_size.width) as i32,
                (p.y * self.char_size.height) as i32,
                texture.query().width,
                texture.query().height,
            );
            conv_err(self.canvas.copy(&texture, None, rect))?;
        }

        Ok(())
    }

    fn render(&mut self) -> Result<(), String> {
        self.clear();

        'loop_row: for r in 0..self.screen.size.height {
            for c in 0..self.screen.size.width {
                if self.screen.buffer.len() <= r + self.screen.offset_row {
                    break 'loop_row;
                }
                self.draw_char(
                    self.screen.buffer[r + self.screen.offset_row][c],
                    Position::new(c, r),
                )?;
            }
        }

        self.canvas.set_draw_color(Color::RGB(200, 200, 200));
        let cursor = self.screen.get_screen_pos(self.screen.cursor);
        // draw cursor
        self.canvas.draw_rect(Rect::new(
            (cursor.x * self.char_size.width) as i32,
            (cursor.y * self.char_size.height) as i32,
            self.char_size.width as u32,
            self.char_size.height as u32,
        ))?;

        self.canvas.present();
        Ok(())
    }
}

fn start(pty: &PTY) -> Result<(), String> {
    match unistd::fork() {
        Ok(unistd::ForkResult::Parent { child, .. }) => {
            conv_err(unistd::close(pty.slave))?;

            let sdl_context = sdl2::init().unwrap();
            let ttf_context = sdl2::ttf::init().unwrap();
            let mut console = Console::new(
                &sdl_context,
                &ttf_context,
                Size::new(80, 24),
                "UbuntuMono-R.ttf",
                30,
            );
            let mut event_pump = sdl_context.event_pump().unwrap();

            'main_loop: loop {
                let mut readable = nix::sys::select::FdSet::new();
                readable.insert(pty.master);

                println!("wait...");

                use nix::sys::time::TimeValLike;
                conv_err(nix::sys::select::select(
                    None,
                    Some(&mut readable),                        // read
                    None,                                       // write
                    None,                                       // error
                    Some(&mut nix::sys::time::TimeVal::zero()), // polling
                ))?;

                if readable.contains(pty.master) {
                    let mut buf = [0];
                    if let Err(e) = nix::unistd::read(pty.master, &mut buf) {
                        eprintln!("Nothing to read from child: {}", e);
                        break;
                    }
                    println!("buf: {:?}", buf);
                    console.screen.put_char(char::from(buf[0]));
                    console.render().unwrap();
                }

                // read input
                for event in event_pump.poll_iter() {
                    match event {
                        Event::Quit { .. } => break 'main_loop,
                        Event::KeyDown { keycode: code, .. } => {
                            let ch = match code {
                                Some(Keycode::A) => b'a',
                                Some(Keycode::B) => b'b',
                                Some(Keycode::C) => b'c',
                                Some(Keycode::D) => b'd',
                                Some(Keycode::E) => b'e',
                                Some(Keycode::F) => b'f',
                                Some(Keycode::G) => b'g',
                                Some(Keycode::H) => b'h',
                                Some(Keycode::I) => b'i',
                                Some(Keycode::J) => b'j',
                                Some(Keycode::K) => b'k',
                                Some(Keycode::L) => b'l',
                                Some(Keycode::M) => b'm',
                                Some(Keycode::N) => b'n',
                                Some(Keycode::O) => b'o',
                                Some(Keycode::P) => b'p',
                                Some(Keycode::Q) => b'q',
                                Some(Keycode::R) => b'r',
                                Some(Keycode::S) => b's',
                                Some(Keycode::T) => b't',
                                Some(Keycode::U) => b'u',
                                Some(Keycode::V) => b'v',
                                Some(Keycode::W) => b'w',
                                Some(Keycode::X) => b'x',
                                Some(Keycode::Y) => b'y',
                                Some(Keycode::Z) => b'z',
                                Some(Keycode::Return) => b'\n',
                                _ => b'?',
                            };
                            conv_err(nix::unistd::write(pty.master, &mut [ch; 1]))?;
                        }
                        _ => {}
                    }
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
            conv_err(unistd::execve(&path, &[], &[]))?;
        }
        Err(e) => return Err(e.to_string()),
    }
    Ok(())
}

fn main() {
    let pty = openpty().unwrap();
    start(&pty);
}
