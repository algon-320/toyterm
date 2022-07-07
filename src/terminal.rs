use std::cmp::min;
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::io::Result;
use std::sync::{Arc, Mutex};

use crate::control_function;
use crate::pipe_channel;
use crate::utils::fd::OwnedFd;
use crate::utils::utf8;

#[derive(Debug, Clone, Copy)]
pub struct TerminalSize {
    pub rows: usize,
    pub cols: usize,
}

fn set_term_window_size(pty_master: &OwnedFd, size: TerminalSize) -> Result<()> {
    let winsize = nix::pty::Winsize {
        ws_row: size.rows as u16,
        ws_col: size.cols as u16,
        // TODO
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    nix::ioctl_write_ptr_bad!(tiocswinsz, nix::libc::TIOCSWINSZ, nix::pty::Winsize);
    unsafe { tiocswinsz(pty_master.as_raw(), &winsize as *const nix::pty::Winsize) }?;

    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub width: u16,
    backlink: u16,
    pub attr: GraphicAttribute,
}

impl Cell {
    const VOID: Self = Cell {
        ch: '#',
        width: 0,
        backlink: u16::MAX,
        attr: GraphicAttribute::default(),
    };
    const SPACE: Self = Cell {
        ch: ' ',
        width: 1,
        backlink: 0,
        attr: GraphicAttribute::default(),
    };
}

#[derive(Debug, Clone, Copy)]
pub enum Color {
    Black,
    Red,
    Yellow,
    Green,
    Blue,
    Magenta,
    Cyan,
    White,
    Rgb { r: u8, g: u8, b: u8 },
    Special,
}

#[derive(Debug, Clone, Copy)]
pub struct GraphicAttribute {
    pub fg: Color,
    pub bg: Color,
    pub inversed: bool,
    pub blinking: u8,
    pub concealed: bool,
}

impl GraphicAttribute {
    const fn default() -> Self {
        GraphicAttribute {
            fg: Color::White,
            bg: Color::Black,
            inversed: false,
            blinking: 0,
            concealed: false,
        }
    }
}

use std::ops::RangeBounds;

/// A single line of terminal buffer
///
/// A `Line` consists of multiple `Cell`s, which may have different width.
/// The number of cells is the same as terminal columns.
/// If there are multi-width cells, the following invariants must be met.
///
/// ## Invariants
/// - If a cell has multiple width (let's call this "head" cell),
///   each of following cells that are covered by the "head" must be 0-width.
/// - Every cell must have a `backlink` field, which represents a distance from the "head" cell.
///
/// ## Example
/// If we have a cell
/// `Cell { ch: '\t', width: 4, backlink: 0 }`, then it should be followed by
/// `Cell { ch: '#',  width: 0, backlink: 1 }`,
/// `Cell { ch: '#',  width: 0, backlink: 2 }`, and
/// `Cell { ch: '#',  width: 0, backlink: 3 }`.
///
#[derive(Clone)]
pub struct Line(Vec<Cell>);

impl Line {
    fn new(len: usize) -> Self {
        Line(vec![Cell::SPACE; len])
    }

    fn copy_from(&mut self, src: &Self) {
        self.0.copy_from_slice(&src.0);
    }

    // [ret.0, ret.1)
    fn range<R: RangeBounds<usize>>(&self, range: R) -> (usize, usize) {
        let len = self.0.len();

        use std::ops::Bound;
        let start = match range.start_bound() {
            Bound::Included(&p) => p,
            Bound::Excluded(&p) => p + 1,
            Bound::Unbounded => 0,
        };
        let end = match range.end_bound() {
            Bound::Included(&p) => p + 1,
            Bound::Excluded(&p) => p,
            Bound::Unbounded => len,
        };

        let start = min(start, len);
        let end = min(end, len);
        debug_assert!(start <= len && end <= len && start <= end);

        (start, end)
    }

    fn copy_within<R: RangeBounds<usize> + Clone>(&mut self, src: R, dst: usize) {
        let (src_start, src_end) = self.range(src);
        let count = min(src_end - src_start, self.0.len() - dst);
        if count == 0 {
            return;
        }

        self.0.copy_within(src_start..src_start + count, dst);

        let (dst_start, dst_end) = (dst, dst + count);

        // Correct boundaries because the above `copy_within` may violates the invariant.
        {
            // correct ..dst_start)
            if dst_start > 0 {
                let head = self.get_head_pos(dst_start - 1);
                if head + self.0[head].width as usize > dst_start {
                    self.0[head..dst_start].fill(Cell::SPACE);
                }
            }

            // correct [dst_start..
            let mut i = dst_start;
            while i < dst_end && self.0[i].width == 0 {
                self.0[i] = Cell::SPACE;
                i += 1;
            }

            // correct ..dst_end)
            let head = self.get_head_pos(dst_end - 1);
            if head + self.0[head].width as usize > dst_end {
                self.0[head..dst_end].fill(Cell::SPACE);
            }

            // correct [dst_end..
            let mut i = dst + count;
            while i < self.0.len() && self.0[i].width == 0 {
                self.0[i] = Cell::SPACE;
                i += 1;
            }
        }
    }

    fn erase<R: RangeBounds<usize>>(&mut self, range: R) {
        let (start, end) = self.range(range);
        for i in start..end {
            self.erase_at(i);
        }
    }

    fn erase_all(&mut self) {
        self.0.fill(Cell::SPACE);
    }

    fn erase_at(&mut self, at: usize) {
        let head = self.get_head_pos(at);
        let width = self.0[head].width as usize;
        let end = min(head + width, self.0.len());

        #[cfg(debug_assertions)]
        for i in head + 1..end {
            debug_assert_eq!(self.0[i].width, 0);
            debug_assert_eq!(self.0[i].backlink as usize, i - head);
        }

        self.0[head..end].fill(Cell::SPACE);
    }

    fn get_head_pos(&self, at: usize) -> usize {
        at - self.0[at].backlink as usize
    }

    fn resize(&mut self, new_len: usize) {
        self.0.resize(new_len, Cell::SPACE);

        let head = self.get_head_pos(new_len - 1);
        let width = self.0[head].width as usize;
        if head + width > self.0.len() {
            self.erase_at(head);
        }
    }

    fn put(&mut self, at: usize, cell: Cell) {
        let width = cell.width as usize;

        debug_assert!(at + width <= self.0.len());

        self.erase(at..at + width);
        self.0[at] = cell;
        for d in 1..width {
            let mut cell = Cell::VOID;
            cell.backlink = d as u16;
            self.0[at + d] = cell;
        }
    }

    /// Returns a iterator that yeilds cells on the line (skip zero-width cells)
    pub fn iter(&self) -> CellIter {
        CellIter {
            slice: self.0.as_slice(),
            next: 0,
        }
    }
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "[\n")?;
        for c in self.0.iter() {
            write!(
                f,
                "\tch: {}, width: {}, backlink: {}\n",
                c.ch, c.width, c.backlink
            )?;
        }
        write!(f, "]\n")?;
        Ok(())
    }
}

pub struct CellIter<'b> {
    slice: &'b [Cell],
    next: usize,
}

impl<'b> Iterator for CellIter<'b> {
    type Item = Cell;
    fn next(&mut self) -> Option<Cell> {
        if self.next == self.slice.len() {
            None
        } else {
            let cell = self.slice[self.next];

            #[cfg(debug_assertions)]
            {
                let w = cell.width as usize;
                debug_assert!(w > 0);
                for i in 1..w {
                    debug_assert_eq!(self.slice[self.next + i].width, 0);
                    debug_assert_eq!(self.slice[self.next + i].backlink as usize, i);
                }
            }

            self.next += cell.width as usize;
            Some(cell)
        }
    }
}

#[derive(Debug, Clone)]
pub struct Buffer {
    pub lines: VecDeque<Line>,
    pub cursor: (usize, usize),
    sz: TerminalSize,
}

impl Buffer {
    pub fn new(sz: TerminalSize) -> Self {
        assert!(sz.rows > 0 && sz.cols > 0);

        let lines: VecDeque<_> = std::iter::repeat_with(|| Line::new(sz.cols))
            .take(sz.rows)
            .collect();

        Self {
            lines,
            cursor: (0, 0),
            sz,
        }
    }

    fn resize(&mut self, sz: TerminalSize) {
        self.sz = sz;

        self.lines.resize_with(sz.rows, || Line::new(sz.cols));

        for line in self.lines.iter_mut() {
            line.resize(sz.cols);
        }
    }

    /// Scroll up the buffer by 1 line
    fn rotate_line(&mut self) {
        self.lines.rotate_left(1);
        let last = self.lines.back_mut().unwrap();
        last.erase_all();
    }

    /// Copy lines[src.0..=src.1] to lines[dst..]
    fn copy_lines(&mut self, src: (usize, usize), dst_first: usize) {
        // FIXME: avoid unnecessary heap allocations

        let (src_first, src_last) = src;
        let src_count = src_last - src_first + 1;
        let room = self.sz.rows - dst_first;
        let copies = min(src_count, room);

        let mut first_to_last = 0..copies;
        let mut last_to_first = (0..copies).rev();

        let iter = if dst_first < src_first {
            &mut first_to_last as &mut dyn Iterator<Item = usize>
        } else {
            &mut last_to_first as &mut dyn Iterator<Item = usize>
        };

        for i in iter {
            use crate::utils::extension::GetMutPair as _;
            let (src, dst) = self.lines.get_mut_pair(src_first + i, dst_first + i);
            dst.copy_from(src);
        }
    }
}

#[derive(Debug)]
enum Command {
    Resize(TerminalSize),
}

#[derive(Debug)]
pub struct Terminal {
    pty: OwnedFd,
    control_req: pipe_channel::Sender<Command>,
    control_res: pipe_channel::Receiver<i32>,
    pub buffer: Arc<Mutex<Buffer>>,
}

impl Terminal {
    pub fn new(size: TerminalSize) -> Self {
        let (pty, _child_pid) = init_pty().unwrap();

        let (control_req_tx, control_req_rx) = pipe_channel::channel();
        let (control_res_tx, control_res_rx) = pipe_channel::channel();

        let engine = Engine::new(
            pty.dup().expect("dup"),
            control_req_rx,
            control_res_tx,
            size,
        );
        let buffer = engine.buffer();
        std::thread::spawn(move || engine.start());

        Terminal {
            pty,
            control_req: control_req_tx,
            control_res: control_res_rx,
            buffer,
        }
    }

    /// Writes the given data on PTY master
    pub fn pty_write(&mut self, data: &[u8]) {
        log::trace!("pty_write: {:x?}", data);
        use std::io::Write as _;
        self.pty.write_all(data).unwrap();
    }

    pub fn request_resize(&mut self, size: TerminalSize) {
        log::debug!("request_resize: {}x{} (cell)", size.rows, size.cols);
        self.control_req.send(Command::Resize(size));
        self.control_res.recv();
    }
}

#[derive(Debug, Clone, Copy)]
struct Cursor {
    sz: TerminalSize,
    row: usize,
    col: usize,
    end: bool,
}

impl Cursor {
    fn pos(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    fn right_space(&self) -> usize {
        if self.end {
            0
        } else {
            self.sz.cols - self.col
        }
    }

    fn exact(mut self, row: usize, col: usize) -> Self {
        self.row = min(row, self.sz.rows - 1);
        self.col = min(col, self.sz.cols - 1);
        self.end = false;
        self
    }

    fn first_col(mut self) -> Self {
        self.end = false;
        self.col = 0;
        self
    }

    fn next_col(mut self) -> Self {
        if self.col + 1 < self.sz.cols {
            self.col += 1;
        } else {
            self.end = true;
        }
        self
    }

    fn prev_col(mut self) -> Self {
        if self.end {
            debug_assert_eq!(self.col, self.sz.cols - 1);
            self.end = false;
        } else if 0 < self.col {
            self.col -= 1;
        }
        self
    }

    fn next_row(mut self) -> Self {
        self.end = false;
        if self.row + 1 < self.sz.rows {
            self.row += 1;
        }
        self
    }

    fn prev_row(mut self) -> Self {
        self.end = false;
        if self.row > 0 {
            self.row -= 1;
        }
        self
    }
}

struct Engine {
    pty: OwnedFd,
    control_req: pipe_channel::Receiver<Command>,
    control_res: pipe_channel::Sender<i32>,
    sz: TerminalSize,
    buffer: Arc<Mutex<Buffer>>,
    cursor: Cursor,
    parser: control_function::Parser,
    tabstops: Vec<usize>,
    attr: GraphicAttribute,
}

impl Engine {
    fn new(
        pty: OwnedFd,
        control_req: pipe_channel::Receiver<Command>,
        control_res: pipe_channel::Sender<i32>,
        sz: TerminalSize,
    ) -> Self {
        set_term_window_size(&pty, sz).unwrap();

        // Initialize tabulation stops
        let mut tabstops = Vec::new();
        for i in 0..sz.cols {
            if i % 8 == 0 {
                tabstops.push(i);
            }
        }

        let buffer = Arc::new(Mutex::new(Buffer::new(sz)));

        let cursor = Cursor {
            sz,
            row: 0,
            col: 0,
            end: false,
        };

        Self {
            pty,
            control_req,
            control_res,
            sz,
            buffer,
            cursor,
            parser: control_function::Parser::default(),
            tabstops,
            attr: GraphicAttribute::default(),
        }
    }

    fn buffer(&self) -> Arc<Mutex<Buffer>> {
        self.buffer.clone()
    }

    fn resize(&mut self, sz: TerminalSize) {
        log::debug!("resize to {}x{} (cell)", sz.rows, sz.cols);

        set_term_window_size(&self.pty, sz).unwrap();

        self.sz = sz;

        // Update tabulation stops
        self.tabstops.clear();
        for i in 0..self.sz.cols {
            if i % 8 == 0 {
                self.tabstops.push(i);
            }
        }

        let (row, col) = self.cursor.pos();
        self.cursor.sz = sz;
        self.cursor = self.cursor.exact(row, col);

        let mut buf = self.buffer.lock().unwrap();
        buf.resize(sz);
        buf.cursor = self.cursor.pos();
    }

    fn start(mut self) {
        let pty_fd = self.pty.as_raw();
        let ctl_fd = self.control_req.get_fd();

        let mut buf = vec![0_u8; 0x1000];
        let mut begin = 0;

        use nix::poll::{poll, PollFd, PollFlags};
        let mut fds = [
            PollFd::new(pty_fd, PollFlags::POLLIN),
            PollFd::new(ctl_fd, PollFlags::POLLIN),
        ];

        loop {
            log::trace!("polling");
            let ready_count = poll(&mut fds, -1).expect("poll");

            if ready_count == 0 {
                continue;
            }

            let pty_revents = fds[0].revents();
            let ctl_revents = fds[1].revents();

            if let Some(flags) = ctl_revents {
                if flags.contains(PollFlags::POLLIN) {
                    match self.control_req.recv() {
                        Command::Resize(new_size) => {
                            self.resize(new_size);
                            self.control_res.send(0);
                        }
                    }
                } else if flags.contains(PollFlags::POLLERR) || flags.contains(PollFlags::POLLHUP) {
                    break;
                }
            }

            if let Some(flags) = pty_revents {
                if flags.contains(PollFlags::POLLIN) {
                    let nb = match nix::unistd::read(pty_fd, &mut buf[begin..]) {
                        Ok(0) => break,
                        Ok(nb) => nb,
                        Err(err) => {
                            log::error!("PTY read: {}", err);
                            continue;
                        }
                    };

                    let end = begin + nb;
                    let bytes = &buf[0..end];

                    let rem = utf8::process_utf8(bytes, |res| match res {
                        Ok(s) => self.process(s),

                        // Process invalid sequence as U+FFFD (REPLACEMENT CHARACTER)
                        Err(invalid) => {
                            log::debug!("invalid UTF-8 sequence: {:?}", invalid);
                            self.process("\u{FFFD}");
                        }
                    });
                    let rem_len = rem.len();

                    // Move remaining bytes to the begining
                    // (these bytes will be parsed in the next process_utf8 call)
                    buf.copy_within((end - rem_len)..end, 0);
                    begin = rem_len;
                } else if flags.contains(PollFlags::POLLERR) || flags.contains(PollFlags::POLLHUP) {
                    break;
                }
            }
        }

        // FIXME: graceful shutdown
        std::process::exit(0);
    }

    fn process(&mut self, input: &str) {
        log::trace!("process: {:?}", input);
        let mut buf = self.buffer.lock().unwrap();

        for ch in input.chars() {
            let func = match self.parser.feed(ch) {
                Some(f) => f,
                None => continue,
            };

            macro_rules! ignore {
                () => {{
                    log::warn!("Function {:?} is not implemented", func);
                    continue;
                }};
            }

            use control_function::Function::*;
            match func {
                Unsupported => {
                    log::debug!("unsupported sequence");
                }
                Invalid => {
                    log::debug!("invalid sequence");
                }

                LF | VT | FF => {
                    buffer_scroll_up_if_needed(&mut buf, self.cursor);
                    self.cursor = self.cursor.next_row();
                }

                CR => {
                    self.cursor = self.cursor.first_col();
                }

                BS => {
                    self.cursor = self.cursor.prev_col();
                }

                HT => {
                    let (row, col) = self.cursor.pos();

                    // If the cursor is already at the end, do nothing
                    if col == self.sz.cols - 1 {
                        return;
                    }

                    // Move cursor to the next tabstop
                    let next = match self.tabstops.binary_search(&(col + 1)) {
                        Ok(i) => self.tabstops[i],
                        Err(i) if i < self.tabstops.len() => self.tabstops[i],
                        _ => self.sz.cols - 1,
                    };
                    let advance = next - col;
                    debug_assert!(advance > 0);

                    let tab = Cell {
                        ch: ' ',
                        width: advance as u16,
                        backlink: 0,
                        attr: self.attr,
                    };
                    buf.lines[row].put(col, tab);

                    for _ in 0..advance {
                        self.cursor = self.cursor.next_col();
                    }
                }

                CUU(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let (row, _) = self.cursor.pos();
                    let up = min(pn, row);
                    for _ in 0..up {
                        self.cursor = self.cursor.prev_row();
                    }
                }

                CUD(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let (row, _) = self.cursor.pos();
                    let down = min(pn, self.sz.rows - 1 - row);
                    for _ in 0..down {
                        self.cursor = self.cursor.next_row();
                    }
                }

                CUF(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let (_, col) = self.cursor.pos();
                    let right = min(pn, self.sz.cols - 1 - col);
                    for _ in 0..right {
                        self.cursor = self.cursor.next_col();
                    }
                }

                CUB(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let (_, col) = self.cursor.pos();
                    let left = min(pn, col);
                    for _ in 0..left {
                        self.cursor = self.cursor.prev_col();
                    }
                }

                CUP(pn1, pn2) => {
                    let mut pn1 = pn1 as usize;
                    if pn1 > 0 {
                        pn1 -= 1;
                    }

                    let mut pn2 = pn2 as usize;
                    if pn2 > 0 {
                        pn2 -= 1;
                    }

                    self.cursor = self.cursor.exact(pn1, pn2);
                }

                CHA(pn) => {
                    let mut pn = pn as usize;
                    if pn > 0 {
                        pn -= 1;
                    }

                    let (row, _) = self.cursor.pos();
                    self.cursor = self.cursor.exact(row, pn);
                }

                VPA(pn) => {
                    let mut pn = pn as usize;
                    if pn > 0 {
                        pn -= 1;
                    }

                    let (_, col) = self.cursor.pos();
                    let row = min(pn, self.sz.rows - 1);
                    self.cursor = self.cursor.exact(row, col);
                }

                ECH(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1;
                    }

                    let (row, col) = self.cursor.pos();
                    buf.lines[row].erase(col..col + pn);
                }

                ED(ps) => match ps {
                    0 => {
                        // clear from the the cursor position to the end (inclusive)
                        let (row, col) = self.cursor.pos();
                        buf.lines[row].erase(col..);
                        for line in buf.lines.range_mut(row + 1..) {
                            line.erase_all();
                        }
                    }
                    1 => {
                        // clear from the beginning to the cursor position (inclusive)
                        let (row, col) = self.cursor.pos();
                        for line in buf.lines.range_mut(0..row) {
                            line.erase_all();
                        }
                        buf.lines[row].erase(0..=col);
                    }
                    2 => {
                        // clear all positions
                        for line in buf.lines.iter_mut() {
                            line.erase_all();
                        }
                    }
                    _ => unreachable!(),
                },

                EL(ps) => match ps {
                    0 => {
                        // clear from the cursor position to the line end (inclusive)
                        let (row, col) = self.cursor.pos();
                        buf.lines[row].erase(col..);
                    }
                    1 => {
                        // clear from the line beginning to the cursor position (inclusive)
                        let (row, col) = self.cursor.pos();
                        buf.lines[row].erase(0..=col);
                    }
                    2 => {
                        // clear line
                        let row = self.cursor.row;
                        buf.lines[row].erase_all();
                    }
                    _ => unreachable!(),
                },

                DSR(ps) => match ps {
                    5 => {
                        // ready, no malfunction detected
                        use std::io::Write as _;
                        self.pty.write_all(b"\x1b[0\x6E").unwrap();
                    }
                    6 => {
                        let (row, col) = self.cursor.pos();

                        // a report of the active position
                        use std::io::Write as _;
                        self.pty
                            .write_fmt(format_args!("\x1b[{};{}\x52", row + 1, col + 1))
                            .unwrap();
                    }
                    _ => unreachable!(),
                },

                ICH(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1;
                    }

                    let (row, col) = self.cursor.pos();
                    let line = &mut buf.lines[row];

                    let src = col;
                    let dst = min(src + pn, self.sz.cols);
                    let count = self.sz.cols - dst;

                    line.copy_within(src..src + count, dst);
                    line.erase(src..dst);
                }

                DCH(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1;
                    }

                    let (row, col) = self.cursor.pos();
                    let line = &mut buf.lines[row];

                    let src = min(col + pn, self.sz.cols);
                    let dst = col;
                    let count = self.sz.cols - src;

                    line.copy_within(src..src + count, dst);
                    line.erase(dst + count..);
                }

                IL(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1;
                    }

                    let (row, _) = self.cursor.pos();

                    let src = row;
                    let dst = min(row + pn, self.sz.rows);
                    let count = self.sz.rows - dst;

                    if count > 0 {
                        buf.copy_lines((src, src + count - 1), dst);
                    }
                    for line in buf.lines.range_mut(src..dst) {
                        line.erase_all();
                    }
                }

                DL(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1;
                    }

                    let (row, _) = self.cursor.pos();

                    let src = min(row + pn, self.sz.rows);
                    let dst = row;
                    let count = self.sz.rows - src;

                    if count > 0 {
                        buf.copy_lines((src, src + count - 1), dst);
                    }
                    for line in buf.lines.range_mut(dst + count..) {
                        line.erase_all();
                    }
                }

                SGR(ps) => {
                    let mut ps = ps.iter().peekable();
                    while let Some(&p) = ps.next() {
                        match p {
                            0 => self.attr = GraphicAttribute::default(),

                            5 => self.attr.blinking = 1,
                            6 => self.attr.blinking = 2,
                            25 => self.attr.blinking = 0,

                            7 => self.attr.inversed = true,
                            27 => self.attr.inversed = false,

                            8 => self.attr.concealed = true,
                            28 => self.attr.concealed = false,

                            30 => self.attr.fg = Color::Black,
                            31 => self.attr.fg = Color::Red,
                            32 => self.attr.fg = Color::Yellow,
                            33 => self.attr.fg = Color::Green,
                            34 => self.attr.fg = Color::Blue,
                            35 => self.attr.fg = Color::Magenta,
                            36 => self.attr.fg = Color::Cyan,
                            37 => self.attr.fg = Color::White,
                            38 => {
                                let s = ps.next();
                                let (r, g, b) = (ps.next(), ps.next(), ps.next());
                                if let (Some(2), Some(&r), Some(&g), Some(&b)) = (s, r, g, b) {
                                    let (r, g, b) = (r as u8, g as u8, b as u8);
                                    self.attr.fg = Color::Rgb { r, g, b };
                                }
                            }
                            70 => self.attr.fg = Color::Special,
                            39 => self.attr.fg = GraphicAttribute::default().fg,

                            40 => self.attr.bg = Color::Black,
                            41 => self.attr.bg = Color::Red,
                            42 => self.attr.bg = Color::Yellow,
                            43 => self.attr.bg = Color::Green,
                            44 => self.attr.bg = Color::Blue,
                            45 => self.attr.bg = Color::Magenta,
                            46 => self.attr.bg = Color::Cyan,
                            47 => self.attr.bg = Color::White,
                            48 => {
                                let s = ps.next();
                                let (r, g, b) = (ps.next(), ps.next(), ps.next());
                                if let (Some(2), Some(&r), Some(&g), Some(&b)) = (s, r, g, b) {
                                    let (r, g, b) = (r as u8, g as u8, b as u8);
                                    self.attr.bg = Color::Rgb { r, g, b };
                                }
                            }
                            80 => self.attr.bg = Color::Special,
                            49 => self.attr.bg = GraphicAttribute::default().bg,

                            _ => {}
                        }
                    }
                }

                GraphicChar(ch) => {
                    use unicode_width::UnicodeWidthChar as _;
                    if let Some(width @ 1..) = ch.width() {
                        // If there is no space for new character, move cursor to the next line.
                        if self.cursor.right_space() < width {
                            let (row, col) = self.cursor.pos();
                            buf.lines[row].erase(col..);

                            buffer_scroll_up_if_needed(&mut buf, self.cursor);
                            self.cursor = self.cursor.next_row().first_col();
                        }

                        let (row, col) = self.cursor.pos();
                        let cell = Cell {
                            ch,
                            width: width as u16,
                            backlink: 0,
                            attr: self.attr,
                        };
                        buf.lines[row].put(col, cell);

                        for _ in 0..width {
                            self.cursor = self.cursor.next_col();
                        }
                    }
                }

                SixelImage(image) => {
                    log::debug!("image: {}x{}", image.width, image.height);
                    // TODO
                }

                ESC => {
                    unreachable!();
                }

                NUL => ignore!(),
                SOH => ignore!(),
                STX => ignore!(),
                EOT => ignore!(),
                ENQ => ignore!(),
                ACK => ignore!(),
                BEL => ignore!(),
                SO => ignore!(),
                SI => ignore!(),
                DLE => ignore!(),
                DC1 => ignore!(),
                DC2 => ignore!(),
                DC3 => ignore!(),
                DC4 => ignore!(),
                NAK => ignore!(),
                SYN => ignore!(),
                ETB => ignore!(),
                CAN => ignore!(),
                EM => ignore!(),
                SUB => ignore!(),
                IS4 => ignore!(),
                IS3 => ignore!(),
                IS2 => ignore!(),
                IS1 => ignore!(),

                BPH => ignore!(),
                NBH => ignore!(),
                NEL => ignore!(),
                SSA => ignore!(),
                ESA => ignore!(),
                HTS => ignore!(),
                HTJ => ignore!(),
                VTS => ignore!(),
                PLD => ignore!(),
                PLU => ignore!(),
                RI => ignore!(),
                SS2 => ignore!(),
                SS3 => ignore!(),
                DCS => ignore!(),
                PU1 => ignore!(),
                PU2 => ignore!(),
                STS => ignore!(),
                CCH => ignore!(),
                MW => ignore!(),
                SPA => ignore!(),
                EPA => ignore!(),
                SOS => ignore!(),
                SCI => ignore!(),
                ST => ignore!(),
                OSC => ignore!(),
                PM => ignore!(),
                APC => ignore!(),

                CNL => ignore!(),
                CPL => ignore!(),
                CHT => ignore!(),
                EF => ignore!(),
                EA => ignore!(),
                SSE => ignore!(),
                CPR => ignore!(),
                SU => ignore!(),
                SD => ignore!(),
                NP => ignore!(),
                PP => ignore!(),
                CTC => ignore!(),
                CVT => ignore!(),
                CBT => ignore!(),
                SRS => ignore!(),
                PTX => ignore!(),
                SDS => ignore!(),
                SIMD => ignore!(),
                HPA => ignore!(),
                HPR => ignore!(),
                REP => ignore!(),
                DA => ignore!(),
                VPR => ignore!(),
                HVP => ignore!(),
                TBC => ignore!(),
                SM => ignore!(),
                MC => ignore!(),
                HPB => ignore!(),
                VPB => ignore!(),
                RM => ignore!(),
                DAQ => ignore!(),

                SL => ignore!(),
                SR => ignore!(),
                GSM => ignore!(),
                GSS => ignore!(),
                FNT => ignore!(),
                TSS => ignore!(),
                JFY => ignore!(),
                SPI => ignore!(),
                QUAD => ignore!(),
                SSU => ignore!(),
                PFS => ignore!(),
                SHS => ignore!(),
                SVS => ignore!(),
                IGS => ignore!(),
                IDCS => ignore!(),
                PPA => ignore!(),
                PPR => ignore!(),
                PPB => ignore!(),
                SPD => ignore!(),
                DTA => ignore!(),
                SHL => ignore!(),
                SLL => ignore!(),
                FNK => ignore!(),
                SPQR => ignore!(),
                SEF => ignore!(),
                PEC => ignore!(),
                SSW => ignore!(),
                SACS => ignore!(),
                SAPV => ignore!(),
                STAB => ignore!(),
                GCC => ignore!(),
                TATE => ignore!(),
                TALE => ignore!(),
                TAC => ignore!(),
                TCC => ignore!(),
                TSR => ignore!(),
                SCO => ignore!(),
                SRCS => ignore!(),
                SCS => ignore!(),
                SLS => ignore!(),
                SCP => ignore!(),
            }
        }

        let (row, col) = self.cursor.pos();
        let col = buf.lines[row].get_head_pos(col);
        buf.cursor = (row, col);
    }
}

fn buffer_scroll_up_if_needed(buf: &mut Buffer, cursor: Cursor) {
    if cursor.row + 1 == cursor.sz.rows {
        buf.rotate_line();
    }
}

/// Opens PTY device and spawn a shell
/// `init_pty` returns a pair (PTY master, PID of shell)
fn init_pty() -> Result<(OwnedFd, nix::unistd::Pid)> {
    use nix::unistd::ForkResult;

    // Safety: single threaded here
    let res = unsafe { nix::pty::forkpty(None, None)? };

    match res.fork_result {
        // Shell side
        ForkResult::Child => {
            exec_shell()?;
            unreachable!();
        }

        // Terminal side
        ForkResult::Parent { child: shell_pid } => {
            // Safety: res.master is not used in other places
            let fd = unsafe { OwnedFd::from_raw_fd(res.master) };
            Ok((fd, shell_pid))
        }
    }
}

/// Setup process states and execute shell
fn exec_shell() -> Result<()> {
    // Restore the default handler for SIGPIPE (terminate)
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
    let sigdfl = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
    unsafe { sigaction(Signal::SIGPIPE, &sigdfl).expect("sigaction") };

    let shell = {
        let mut bytes = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
        bytes.push('\0');
        CString::from_vec_with_nul(bytes.into_bytes()).unwrap()
    };

    let args: [&CStr; 1] = [&shell];

    let mut vars: std::collections::HashMap<String, String> = std::env::vars().collect();

    // FIXME
    vars.insert("TERM".to_owned(), "ansi".to_owned());

    let envs: Vec<CString> = vars
        .into_iter()
        .map(|(key, val)| {
            let keyval_bytes = format!("{}={}\0", key, val).into_bytes();
            CString::from_vec_with_nul(keyval_bytes).unwrap()
        })
        .collect();

    nix::unistd::execve(args[0], &args, &envs)?;
    unreachable!();
}
