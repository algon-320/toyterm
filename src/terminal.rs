use nix::errno::Errno;
use nix::unistd::Pid;
use std::cmp::{max, min};
use std::collections::VecDeque;
use std::io::Result;
use std::ops::{Range, RangeBounds};
use std::os::unix::io::{AsRawFd as _, FromRawFd as _, OwnedFd};
use std::sync::{Arc, Mutex};

use crate::control_function;
use crate::pipe_channel;
use crate::utils::io::FdIo;
use crate::utils::utf8;

#[derive(Debug, Clone)]
pub struct PositionedImage {
    pub row: isize,
    pub col: isize,
    pub height: u64,
    pub width: u64,
    pub data: Vec<u8>,
}

fn overwrap(outer: &PositionedImage, inner: &PositionedImage) -> bool {
    let a = outer;
    let b = inner;
    a.row <= b.row
        && a.col <= b.col
        && b.row + b.height as isize <= a.row + a.height as isize
        && b.col + b.width as isize <= a.col + a.width as isize
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TerminalSize {
    pub rows: usize,
    pub cols: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellSize {
    pub w: u32,
    pub h: u32,
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

    // A marker representing a termination of line
    const TERM: Self = Cell {
        ch: '\n',
        width: 1,
        backlink: 0,
        attr: GraphicAttribute::default(),
    };

    #[allow(unused)]
    pub fn new_ascii(ch: char) -> Cell {
        let mut cell = Self::SPACE;
        cell.ch = ch;
        cell
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Rgb { rgba: u32 },
    Special,
}

#[derive(Debug, Clone, Copy)]
pub struct GraphicAttribute {
    pub fg: Color,
    pub bg: Color,
    pub bold: i8,
    pub inversed: bool,
    pub blinking: u8,
    pub concealed: bool,
}

impl GraphicAttribute {
    const fn default() -> Self {
        GraphicAttribute {
            fg: Color::White,
            bg: Color::Black,
            bold: 0,
            inversed: false,
            blinking: 0,
            concealed: false,
        }
    }
}

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
pub struct Line {
    cells: Vec<Cell>,
    linewrap: bool,
}

impl std::iter::FromIterator<Cell> for Line {
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = Cell>,
    {
        Line {
            cells: iter.into_iter().collect(),
            linewrap: false,
        }
    }
}

impl Line {
    fn new(len: usize) -> Self {
        Line {
            cells: vec![Cell::TERM; len],
            linewrap: false,
        }
    }

    pub fn copy_from(&mut self, src: &Self) {
        if self.cells.len() == src.cells.len() {
            self.cells.copy_from_slice(&src.cells);
        } else {
            self.cells.clear();
            self.cells.extend_from_slice(&src.cells);
        }
        self.linewrap = src.linewrap;
    }

    fn saturating_range<R: RangeBounds<usize>>(&self, range: R) -> Range<usize> {
        let len = self.cells.len();

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

        Range { start, end }
    }

    fn copy_within<R: RangeBounds<usize> + Clone>(&mut self, src: R, dst: usize) {
        let src = self.saturating_range(src);
        let count = min(src.len(), self.cells.len() - dst);
        if count == 0 {
            return;
        }

        self.cells.copy_within(src.start..src.start + count, dst);

        let (dst_start, dst_end) = (dst, dst + count);

        // Correct boundaries because the above `copy_within` may violates the invariant.
        {
            // correct ..dst_start)
            if dst_start > 0 {
                let head = self.get_head_pos(dst_start - 1);
                if head + self.cells[head].width as usize > dst_start {
                    self.cells[head..dst_start].fill(Cell::SPACE);
                }
            }

            // correct [dst_start..
            let mut i = dst_start;
            while i < dst_end && self.cells[i].width == 0 {
                self.cells[i] = Cell::SPACE;
                i += 1;
            }

            // correct ..dst_end)
            let head = self.get_head_pos(dst_end - 1);
            if head + self.cells[head].width as usize > dst_end {
                self.cells[head..dst_end].fill(Cell::SPACE);
            }

            // correct [dst_end..
            let mut i = dst + count;
            while i < self.cells.len() && self.cells[i].width == 0 {
                self.cells[i] = Cell::SPACE;
                i += 1;
            }
        }
    }

    fn erase<R: RangeBounds<usize>>(&mut self, range: R) {
        for i in self.saturating_range(range) {
            self.erase_at(i);
        }
    }

    fn erase_all(&mut self) {
        self.cells.fill(Cell::TERM);
        self.linewrap = false;
    }

    fn erase_at(&mut self, at: usize) {
        let head = self.get_head_pos(at);
        let width = self.cells[head].width as usize;
        let end = min(head + width, self.cells.len());

        #[cfg(debug_assertions)]
        for i in head + 1..end {
            debug_assert_eq!(self.cells[i].width, 0);
            debug_assert_eq!(self.cells[i].backlink as usize, i - head);
        }

        self.cells[head..end].fill(Cell::SPACE);
    }

    fn get_head_pos(&self, at: usize) -> usize {
        at - self.cells[at].backlink as usize
    }

    fn resize(&mut self, new_len: usize) {
        self.cells.resize(new_len, Cell::TERM);

        let head = self.get_head_pos(new_len - 1);
        let width = self.cells[head].width as usize;
        if head + width > self.cells.len() {
            self.erase_at(head);
        }
    }

    pub fn columns(&self) -> usize {
        self.cells.len()
    }

    fn put(&mut self, at: usize, cell: Cell) {
        let width = cell.width as usize;

        debug_assert!(at + width <= self.cells.len());

        self.erase(at..at + width);
        self.cells[at] = cell;
        for d in 1..width {
            let mut cell = Cell::VOID;
            cell.backlink = d as u16;
            self.cells[at + d] = cell;
        }
    }

    pub fn get(&self, at: usize) -> Option<Cell> {
        if at < self.cells.len() {
            let head = self.get_head_pos(at);
            Some(self.cells[head])
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Cell> + '_ {
        self.cells.iter().copied()
    }

    pub fn linewrap(&self) -> bool {
        self.linewrap
    }
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        writeln!(f, "[")?;
        for c in self.cells.iter() {
            writeln!(
                f,
                "\tch: {:?}, width: {}, backlink: {}",
                c.ch, c.width, c.backlink
            )?;
        }
        writeln!(f, "]")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Mode {
    pub cursor_visible: bool,
    pub bracketed_paste: bool,
    pub mouse_track: bool,
    pub sgr_ext_mouse_track: bool,
    pub sixel_scrolling: bool,
}

impl Default for Mode {
    fn default() -> Self {
        Mode {
            cursor_visible: true,
            bracketed_paste: false,
            mouse_track: false,
            sgr_ext_mouse_track: false,
            sixel_scrolling: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct State {
    history: VecDeque<Line>,
    lines: VecDeque<Line>,
    alt_lines: VecDeque<Line>,
    images: Vec<PositionedImage>,
    alt_images: Vec<PositionedImage>,
    cursor: Cursor,

    pub size: TerminalSize,
    pub history_size: usize,
    pub mode: Mode,

    pub updated: bool,
    pub closed: bool,
}

impl State {
    const HISTORY_CAPACITY: usize = 10000;

    pub fn new(sz: TerminalSize) -> Self {
        assert!(sz.rows > 0 && sz.cols > 0);

        let history: VecDeque<_> = std::iter::repeat_with(|| Line::new(sz.cols))
            .take(Self::HISTORY_CAPACITY)
            .collect();

        let lines: VecDeque<_> = std::iter::repeat_with(|| Line::new(sz.cols))
            .take(sz.rows)
            .collect();

        let alt_lines = lines.clone();

        let cursor = Cursor {
            sz,
            ..Cursor::default()
        };

        Self {
            history,
            lines,
            alt_lines,
            images: Vec::new(),
            alt_images: Vec::new(),

            size: sz,
            history_size: 0,
            cursor,
            mode: Mode::default(),

            updated: true,
            closed: false,
        }
    }

    pub fn cursor(&self) -> (usize, usize, CursorStyle) {
        let (row, col) = self.cursor.pos();
        let col = self.lines[row].get_head_pos(col);
        (row, col, self.cursor.style)
    }

    pub fn clear_history(&mut self) {
        self.history_size = 0;
        for line in self.history.iter_mut() {
            line.erase_all();
        }
    }

    pub fn range(&self, top: isize, bot: isize) -> impl Iterator<Item = &Line> + '_ {
        let buff_len = self.lines.len() as isize;
        let hist_len = self.history.len() as isize;

        if top >= 0 {
            let top = top as usize;
            let bot = min(bot, buff_len) as usize;
            self.history.range(0..0).chain(self.lines.range(top..bot))
        } else if bot < 0 {
            let top = max(hist_len + top, 0) as usize;
            let bot = (hist_len + bot) as usize;
            self.history.range(top..bot).chain(self.lines.range(0..0))
        } else {
            let top = max(hist_len + top, 0) as usize;
            let bot = min(bot, buff_len) as usize;
            self.history.range(top..).chain(self.lines.range(..bot))
        }
    }

    pub fn images(&self) -> impl Iterator<Item = &PositionedImage> + '_ {
        self.images.iter()
    }

    fn resize(&mut self, sz: TerminalSize) {
        self.size = sz;

        let (row, col) = self.cursor.pos();
        self.cursor.sz = sz;
        self.cursor = self.cursor.exact(row, col);

        for line in self.history.iter_mut() {
            line.resize(sz.cols);
        }

        self.lines.resize_with(sz.rows, || Line::new(sz.cols));
        for line in self.lines.iter_mut() {
            line.resize(sz.cols);
        }

        self.alt_lines.resize_with(sz.rows, || Line::new(sz.cols));
        for line in self.alt_lines.iter_mut() {
            line.resize(sz.cols);
        }
    }

    /// Scroll up the buffer by 1 line
    fn scroll_up(&mut self) {
        let line = self.lines.pop_front().unwrap();
        self.history.push_back(line);
        self.history_size = min(self.history_size + 1, Self::HISTORY_CAPACITY);

        let mut line = self.history.pop_front().unwrap();
        line.erase_all();
        self.lines.push_back(line);
    }

    /// Copy lines[src.0..=src.1] to lines[dst..]
    fn copy_lines(&mut self, src: (usize, usize), dst_first: usize) {
        let (src_first, src_last) = src;
        let src_count = src_last - src_first + 1;
        let room = self.size.rows - dst_first;
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

    fn swap_screen_buffers(&mut self) {
        std::mem::swap(&mut self.lines, &mut self.alt_lines);
        std::mem::swap(&mut self.images, &mut self.alt_images);
    }
}

#[derive(Debug)]
enum Command {
    Resize {
        buff_sz: TerminalSize,
        cell_sz: CellSize,
    },
}

#[derive(Debug)]
pub struct Terminal {
    pty: OwnedFd,
    control_req: pipe_channel::Sender<Command>,
    control_res: pipe_channel::Receiver<i32>,
    pub state: Arc<Mutex<State>>,
}

impl Terminal {
    pub fn new(size: TerminalSize, cell_size: CellSize, cwd: &std::path::Path) -> Self {
        let (pty, child_pid) = init_pty(cwd).unwrap();

        let (control_req_tx, control_req_rx) = pipe_channel::channel();
        let (control_res_tx, control_res_rx) = pipe_channel::channel();

        let engine = Engine::new(
            child_pid,
            pty.try_clone().expect("dup"),
            control_req_rx,
            control_res_tx,
            size,
            cell_size,
        );
        let state = engine.state();
        std::thread::spawn(move || engine.start());

        Terminal {
            pty,
            control_req: control_req_tx,
            control_res: control_res_rx,
            state,
        }
    }

    /// Writes the given data on PTY master
    pub fn pty_write(&mut self, data: &[u8]) {
        log::trace!("pty_write: {:x?}", data);
        use std::io::Write as _;
        FdIo(&self.pty).write_all(data).unwrap();
    }

    pub fn request_resize(&mut self, buff_sz: TerminalSize, cell_sz: CellSize) {
        log::debug!("request_resize: {}x{} (cell)", buff_sz.rows, buff_sz.cols);
        self.control_req.send(Command::Resize { buff_sz, cell_sz });
        self.control_res.recv();
    }

    #[cfg(feature = "multiplex")]
    pub fn get_pgid(&self) -> Pid {
        let mut pgid_buf = Pid::from_raw(0);
        nix::ioctl_read_bad!(tiocgpgrp, nix::libc::TIOCGPGRP, Pid);
        unsafe { tiocgpgrp(self.pty.as_raw_fd(), &mut pgid_buf as *mut Pid).expect("TIOCGPGRP") };
        pgid_buf
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Cursor {
    sz: TerminalSize,
    row: usize,
    col: usize,
    end: bool,
    style: CursorStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorStyle {
    #[default]
    Block,
    Underline,
    Bar,
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
    pid: Pid,
    pty: OwnedFd,
    control_req: pipe_channel::Receiver<Command>,
    control_res: pipe_channel::Sender<i32>,
    sz: TerminalSize,
    cell_sz: CellSize,
    state: Arc<Mutex<State>>,
    parser: control_function::Parser,
    tabstops: Vec<usize>,
    attr: GraphicAttribute,
    saved_cursor: Cursor,
    saved_attr: GraphicAttribute,
}

impl Engine {
    fn set_term_window_size(pty_master: &OwnedFd, size: TerminalSize) -> Result<()> {
        let winsize = nix::pty::Winsize {
            ws_row: size.rows as u16,
            ws_col: size.cols as u16,
            // TODO
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        nix::ioctl_write_ptr_bad!(tiocswinsz, nix::libc::TIOCSWINSZ, nix::pty::Winsize);
        unsafe { tiocswinsz(pty_master.as_raw_fd(), &winsize as *const nix::pty::Winsize) }?;

        Ok(())
    }

    fn new(
        pid: Pid,
        pty: OwnedFd,
        control_req: pipe_channel::Receiver<Command>,
        control_res: pipe_channel::Sender<i32>,
        sz: TerminalSize,
        cell_sz: CellSize,
    ) -> Self {
        Self::set_term_window_size(&pty, sz).unwrap();

        let state = Arc::new(Mutex::new(State::new(sz)));

        // Initialize tabulation stops
        let mut tabstops = Vec::new();
        for i in 0..sz.cols {
            if i % 8 == 0 {
                tabstops.push(i);
            }
        }

        let saved_cursor = Cursor {
            sz,
            ..Cursor::default()
        };

        Self {
            pid,
            pty,
            control_req,
            control_res,
            sz,
            cell_sz,
            state,
            parser: control_function::Parser::default(),
            tabstops,
            attr: GraphicAttribute::default(),
            saved_cursor,
            saved_attr: GraphicAttribute::default(),
        }
    }

    fn state(&self) -> Arc<Mutex<State>> {
        self.state.clone()
    }

    fn resize(&mut self, sz: TerminalSize, cell_sz: CellSize) {
        log::debug!("resize to {}x{} (cell)", sz.rows, sz.cols);

        Self::set_term_window_size(&self.pty, sz).unwrap();

        self.sz = sz;
        self.cell_sz = cell_sz;

        // Update tabulation stops
        self.tabstops.clear();
        for i in 0..self.sz.cols {
            if i % 8 == 0 {
                self.tabstops.push(i);
            }
        }

        let (row, col) = self.saved_cursor.pos();
        self.saved_cursor.sz = sz;
        self.saved_cursor = self.saved_cursor.exact(row, col);

        let mut state = self.state.lock().unwrap();
        state.resize(sz);

        debug_assert_eq!(self.sz, state.size);
        debug_assert_eq!(self.sz, self.saved_cursor.sz);
    }

    fn start(mut self) {
        let pty_fd = self.pty.as_raw_fd();
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
            if let Err(err) = poll(&mut fds, -1) {
                if let Errno::EINTR | Errno::EAGAIN = err {
                    continue;
                }
                log::error!("poll failed: {err}");
                break;
            }

            let pty_revents = fds[0].revents();
            let ctl_revents = fds[1].revents();

            if let Some(flags) = ctl_revents {
                if flags.contains(PollFlags::POLLIN) {
                    match self.control_req.recv() {
                        Command::Resize { buff_sz, cell_sz } => {
                            self.resize(buff_sz, cell_sz);
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

        let mut state = self.state.lock().unwrap();
        state.closed = true;

        use nix::sys::signal::{kill, Signal};
        let _ = kill(self.pid, Signal::SIGHUP);
        let _ = nix::sys::wait::waitpid(self.pid, None);
    }

    fn process(&mut self, input: &str) {
        log::trace!("process: {:?}", input);
        let mut state = self.state.lock().unwrap();

        state.updated = true;

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
                    buffer_scroll_up_if_needed(&mut state, self.cell_sz);
                    state.cursor = state.cursor.next_row();
                }

                CR => {
                    state.cursor = state.cursor.first_col();
                }

                BS => {
                    state.cursor = state.cursor.prev_col();
                }

                HT => {
                    let (row, col) = state.cursor.pos();

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
                        ch: '\t',
                        width: advance as u16,
                        backlink: 0,
                        attr: self.attr,
                    };
                    state.lines[row].put(col, tab);

                    for _ in 0..advance {
                        state.cursor = state.cursor.next_col();
                    }
                }

                CUU(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let (row, _) = state.cursor.pos();
                    let up = min(pn, row);
                    for _ in 0..up {
                        state.cursor = state.cursor.prev_row();
                    }
                }

                CUD(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let (row, _) = state.cursor.pos();
                    let down = min(pn, self.sz.rows - 1 - row);
                    for _ in 0..down {
                        state.cursor = state.cursor.next_row();
                    }
                }

                CUF(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let (_, col) = state.cursor.pos();
                    let right = min(pn, self.sz.cols - 1 - col);
                    for _ in 0..right {
                        state.cursor = state.cursor.next_col();
                    }
                }

                CUB(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let (_, col) = state.cursor.pos();
                    let left = min(pn, col);
                    for _ in 0..left {
                        state.cursor = state.cursor.prev_col();
                    }
                }

                HVP(pn1, pn2) | CUP(pn1, pn2) => {
                    let mut pn1 = pn1 as usize;
                    if pn1 > 0 {
                        pn1 -= 1;
                    }

                    let mut pn2 = pn2 as usize;
                    if pn2 > 0 {
                        pn2 -= 1;
                    }

                    state.cursor = state.cursor.exact(pn1, pn2);
                }

                CHA(pn) => {
                    let mut pn = pn as usize;
                    if pn > 0 {
                        pn -= 1;
                    }

                    let (row, _) = state.cursor.pos();
                    state.cursor = state.cursor.exact(row, pn);
                }

                VPA(pn) => {
                    let mut pn = pn as usize;
                    if pn > 0 {
                        pn -= 1;
                    }

                    let (_, col) = state.cursor.pos();
                    let row = min(pn, self.sz.rows - 1);
                    state.cursor = state.cursor.exact(row, col);
                }

                ECH(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1;
                    }

                    let (row, col) = state.cursor.pos();
                    state.lines[row].erase(col..col + pn);
                }

                ED(ps) => match ps {
                    0 => {
                        // clear from the the cursor position to the end (inclusive)
                        let (row, col) = state.cursor.pos();
                        state.lines[row].erase(col..);
                        for line in state.lines.range_mut(row + 1..) {
                            line.erase_all();
                        }

                        // Remove sixel graphics
                        let cell_hpx = self.cell_sz.h;
                        state.images.retain(|img| {
                            let v_cells = ((img.height as u32 + cell_hpx - 1) / cell_hpx) as isize;
                            let bottom_row = img.row + v_cells;
                            bottom_row <= row as isize
                        });
                        log::debug!("{} images retained", state.images.len());
                    }
                    1 => {
                        // clear from the beginning to the cursor position (inclusive)
                        let (row, col) = state.cursor.pos();
                        for line in state.lines.range_mut(0..row) {
                            line.erase_all();
                        }
                        state.lines[row].erase(0..=col);

                        // Remove sixel graphics
                        state.images.retain(|img| img.row >= row as isize);
                        log::debug!("{} images retained", state.images.len());
                    }
                    2 => {
                        // clear all positions
                        for line in state.lines.iter_mut() {
                            line.erase_all();
                        }

                        // Remove sixel graphics
                        state.images.clear();
                    }
                    _ => unreachable!(),
                },

                EL(ps) => match ps {
                    0 => {
                        // clear from the cursor position to the line end (inclusive)
                        let (row, col) = state.cursor.pos();
                        state.lines[row].erase(col..);
                    }
                    1 => {
                        // clear from the line beginning to the cursor position (inclusive)
                        let (row, col) = state.cursor.pos();
                        state.lines[row].erase(0..=col);
                    }
                    2 => {
                        // clear line
                        let row = state.cursor.row;
                        state.lines[row].erase_all();
                    }
                    _ => unreachable!(),
                },

                DSR(ps) => match ps {
                    5 => {
                        // ready, no malfunction detected
                        use std::io::Write as _;
                        FdIo(&self.pty).write_all(b"\x1b[0\x6E").unwrap();
                    }
                    6 => {
                        let (row, col) = state.cursor.pos();

                        // a report of the active position
                        use std::io::Write as _;
                        FdIo(&self.pty)
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

                    let (row, col) = state.cursor.pos();
                    let line = &mut state.lines[row];

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

                    let (row, col) = state.cursor.pos();
                    let line = &mut state.lines[row];

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

                    let (row, _) = state.cursor.pos();

                    let src = row;
                    let dst = min(row + pn, self.sz.rows);
                    let count = self.sz.rows - dst;

                    if count > 0 {
                        state.copy_lines((src, src + count - 1), dst);
                    }
                    for line in state.lines.range_mut(src..dst) {
                        line.erase_all();
                    }
                }

                DL(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1;
                    }

                    let (row, _) = state.cursor.pos();

                    let src = min(row + pn, self.sz.rows);
                    let dst = row;
                    let count = self.sz.rows - src;

                    if count > 0 {
                        state.copy_lines((src, src + count - 1), dst);
                    }
                    for line in state.lines.range_mut(dst + count..) {
                        line.erase_all();
                    }
                }

                SGR(pss) => {
                    let mut iter = pss.iter().copied().peekable();
                    while let Some(ps) = iter.next() {
                        match ps {
                            0 => self.attr = GraphicAttribute::default(),

                            1 => self.attr.bold = 1,
                            2 => self.attr.bold = -1,
                            22 => self.attr.bold = 0,

                            5 => self.attr.blinking = 1,
                            6 => self.attr.blinking = 2,
                            25 => self.attr.blinking = 0,

                            7 => self.attr.inversed = true,
                            27 => self.attr.inversed = false,

                            8 => self.attr.concealed = true,
                            28 => self.attr.concealed = false,

                            x @ (30..=37 | 38 | 90..=97) => {
                                if let Some(color) = parse_color(x - 30, &mut iter) {
                                    self.attr.fg = color;
                                }
                            }
                            70 => self.attr.fg = Color::Special,
                            39 => self.attr.fg = GraphicAttribute::default().fg,

                            x @ (40..=47 | 48 | 100..=107) => {
                                if let Some(color) = parse_color(x - 40, &mut iter) {
                                    self.attr.bg = color;
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
                    let ch_width = if crate::TOYTERM_CONFIG.east_asian_width_ambiguous == 1 {
                        ch.width()
                    } else {
                        ch.width_cjk()
                    };

                    if let Some(width @ 1..) = ch_width {
                        // If there is no space for new character, move cursor to the next line.
                        if state.cursor.right_space() < width {
                            let (row, col) = state.cursor.pos();
                            if !state.cursor.end {
                                state.lines[row].erase(col..);
                            }
                            state.lines[row].linewrap = true;

                            buffer_scroll_up_if_needed(&mut state, self.cell_sz);
                            state.cursor = state.cursor.next_row().first_col();
                        }

                        let (row, col) = state.cursor.pos();
                        let cell = Cell {
                            ch,
                            width: width as u16,
                            backlink: 0,
                            attr: self.attr,
                        };
                        state.lines[row].put(col, cell);

                        for _ in 0..width {
                            state.cursor = state.cursor.next_col();
                        }
                    }
                }

                SixelImage(image) => {
                    log::debug!("image: {}x{}", image.width, image.height);
                    let (cursor_row, cursor_col) = state.cursor.pos();

                    let cell_w = self.cell_sz.w as u64;
                    let cell_h = self.cell_sz.h as u64;

                    let (row, col) = if state.mode.sixel_scrolling {
                        (cursor_row as isize, cursor_col as isize)
                    } else {
                        (0, 0)
                    };

                    let new_image = PositionedImage {
                        row,
                        col,
                        width: image.width,
                        height: image.height,
                        data: image.data,
                    };

                    state.images.retain(|img| !overwrap(&new_image, img));
                    state.images.push(new_image);

                    log::debug!("total {} images", state.images.len());

                    if state.mode.sixel_scrolling {
                        let advance_h = (image.width + cell_w - 1) / cell_w;
                        let advance_v = (image.height + cell_h - 1) / cell_h - 1;

                        for _ in 0..advance_h {
                            state.cursor = state.cursor.next_col();
                        }
                        for _ in 0..advance_v {
                            buffer_scroll_up_if_needed(&mut state, self.cell_sz);
                            state.cursor = state.cursor.next_row();
                        }
                    }
                }

                SelectCursorStyle(ps) => match ps {
                    2 => state.cursor.style = CursorStyle::Block,
                    4 => state.cursor.style = CursorStyle::Underline,
                    6 => state.cursor.style = CursorStyle::Bar,
                    _ => {
                        log::warn!("unknown cursor shape: {}", ps);
                    }
                },

                SM(b'?', ps) => {
                    log::trace!("SM - ps : {:?}", ps);

                    for p in ps {
                        match p {
                            25 => {
                                state.mode.cursor_visible = true;
                            }

                            80 => {
                                state.mode.sixel_scrolling = true;
                                log::debug!("Sixel Scrolling Mode Enabled");
                            }

                            // FIXME : I'm not sure that 1002 is equivalent to 1000 but it works
                            1000 | 1002 => {
                                state.mode.mouse_track = true;
                                log::debug!("Mouse Tracking Mode Enabled");
                            }

                            1006 => {
                                state.mode.sgr_ext_mouse_track = true;
                                log::debug!("SGR Extended Mode Mouse Tracking Enabled");
                            }

                            1049 => {
                                // save current cursor
                                self.saved_cursor = state.cursor;
                                self.saved_attr = self.attr;

                                // clear the alternative buffers
                                for line in state.alt_lines.iter_mut() {
                                    line.erase_all();
                                }
                                state.alt_images.clear();

                                state.swap_screen_buffers();
                            }

                            2004 => {
                                state.mode.bracketed_paste = true;
                                log::debug!("Bracketed Paste Mode Enabled");
                            }

                            _ => {
                                log::debug!("Set ? mode: {:?}", ps);
                            }
                        }
                    }
                }

                SM(..) => ignore!(),

                RM(b'?', ps) => {
                    log::trace!("RM - ps : {:?}", ps);
                    for p in ps {
                        match p {
                            25 => {
                                state.mode.cursor_visible = false;
                            }

                            80 => {
                                state.mode.sixel_scrolling = false;
                                log::debug!("Sixel Scrolling Mode Disabled");
                            }

                            // FIXME : I'm not sure that 1002 is equivalent to 1000 but it works
                            1000 | 1002 => {
                                state.mode.mouse_track = false;
                                log::debug!("Mouse Tracking Mode Disabled");
                            }

                            1006 => {
                                state.mode.sgr_ext_mouse_track = false;
                                log::debug!("SGR Extended Mode Mouse Tracking Disabled");
                            }

                            1049 => {
                                // restore cursor and switch back to the primary screen buffer
                                state.cursor = self.saved_cursor;
                                self.attr = self.saved_attr;
                                state.swap_screen_buffers();
                            }

                            2004 => {
                                state.mode.bracketed_paste = false;
                                log::debug!("Bracketed Paste Mode Disabled");
                            }

                            _ => {
                                log::debug!("Reset ? mode: {:?}", ps);
                            }
                        }
                    }
                }

                RM(..) => ignore!(),

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
                TBC => ignore!(),
                MC => ignore!(),
                HPB => ignore!(),
                VPB => ignore!(),
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
    }
}

fn parse_color(prefix: u16, ps: &mut impl Iterator<Item = u16>) -> Option<Color> {
    match prefix {
        0 => Some(Color::Black),
        1 => Some(Color::Red),
        2 => Some(Color::Green),
        3 => Some(Color::Yellow),
        4 => Some(Color::Blue),
        5 => Some(Color::Magenta),
        6 => Some(Color::Cyan),
        7 => Some(Color::White),

        60 => Some(Color::BrightBlack),
        61 => Some(Color::BrightRed),
        62 => Some(Color::BrightGreen),
        63 => Some(Color::BrightYellow),
        64 => Some(Color::BrightBlue),
        65 => Some(Color::BrightMagenta),
        66 => Some(Color::BrightCyan),
        67 => Some(Color::BrightWhite),

        8 => {
            match ps.next() {
                // direct color
                Some(2) => {
                    if let (Some(r), Some(g), Some(b)) = (ps.next(), ps.next(), ps.next()) {
                        let (r, g, b) = (r as u32, g as u32, b as u32);
                        Some(Color::Rgb {
                            rgba: (r << 24) | (g << 16) | (b << 8) | 0xFF,
                        })
                    } else {
                        None
                    }
                }

                // indexed color
                Some(5) => {
                    if let Some(idx @ 0..=255) = ps.next() {
                        match idx {
                            0 => Some(Color::Black),
                            1 => Some(Color::Red),
                            2 => Some(Color::Green),
                            3 => Some(Color::Yellow),
                            4 => Some(Color::Blue),
                            5 => Some(Color::Magenta),
                            6 => Some(Color::Cyan),
                            7 => Some(Color::White),

                            8 => Some(Color::BrightBlack),
                            9 => Some(Color::BrightRed),
                            10 => Some(Color::BrightGreen),
                            11 => Some(Color::BrightYellow),
                            12 => Some(Color::BrightBlue),
                            13 => Some(Color::BrightMagenta),
                            14 => Some(Color::BrightCyan),
                            15 => Some(Color::BrightWhite),

                            // 6x6x6 colors
                            16..=231 => {
                                let mut x = (idx - 16) as u32;

                                let b = (x % 6) * 51;
                                x /= 6;
                                let g = (x % 6) * 51;
                                x /= 6;
                                let r = (x % 6) * 51;

                                Some(Color::Rgb {
                                    rgba: (r << 24) | (g << 16) | (b << 8) | 0xFF,
                                })
                            }

                            // grayscale colors
                            232..=255 => {
                                let x = (idx - 232) as u32;
                                let v = x * 11;
                                Some(Color::Rgb {
                                    rgba: (v << 24) | (v << 16) | (v << 8) | 0xFF,
                                })
                            }

                            _ => unreachable!(),
                        }
                    } else {
                        None
                    }
                }

                // unknown color format
                _ => None,
            }
        }

        _ => unimplemented!(),
    }
}

fn buffer_scroll_up_if_needed(state: &mut State, cell_sz: CellSize) {
    if state.cursor.row + 1 == state.cursor.sz.rows {
        state.scroll_up();

        if !state.images.is_empty() {
            for img in state.images.iter_mut() {
                img.row -= 1;
            }
            state.images.retain(|img| {
                let v_cells = ((img.height as u32 + cell_sz.h - 1) / cell_sz.h) as isize;
                (-v_cells) < img.row
            });
            log::debug!("{} images retained", state.images.len());
        }
    }
}

/// Opens PTY device and spawn a shell
/// `init_pty` returns a pair (PTY master, PID of shell)
fn init_pty(cwd: &std::path::Path) -> Result<(OwnedFd, Pid)> {
    use nix::unistd::ForkResult;

    // Safety: single threaded here
    let res = unsafe { nix::pty::forkpty(None, None)? };

    match res.fork_result {
        // Shell side
        ForkResult::Child => {
            std::env::set_current_dir(cwd).expect("chdir");
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
    use std::ffi::{CStr, CString};

    // Restore the default handler for SIGPIPE (terminate)
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
    let sigdfl = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
    unsafe { sigaction(Signal::SIGPIPE, &sigdfl).expect("sigaction") };

    let shell = {
        let mut shell_string = crate::TOYTERM_CONFIG.shell[0].clone();
        shell_string.push('\0');
        CString::from_vec_with_nul(shell_string.into_bytes()).unwrap()
    };

    let args: [&CStr; 1] = [&shell];

    let mut vars: std::collections::HashMap<String, String> = std::env::vars().collect();

    vars.insert("TERM".to_owned(), "toyterm-256color".to_owned());

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
