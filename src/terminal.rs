use std::cmp::min;
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::io::Result;
use std::sync::{Arc, Mutex};

use crate::control_function;
use crate::pipe_channel;
use crate::utils::fd::OwnedFd;
use crate::utils::utf8;

fn set_term_window_size(pty_master: &OwnedFd, lines: u16, columns: u16) -> Result<()> {
    let winsize = nix::pty::Winsize {
        ws_row: lines,
        ws_col: columns,
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
    pub attr: GraphicAttribute,
}

impl Cell {
    const VOID: Self = Cell {
        ch: ' ',
        width: 0,
        attr: GraphicAttribute::default(),
    };
    const SPACE: Self = Cell {
        ch: ' ',
        width: 1,
        attr: GraphicAttribute::default(),
    };
}

#[derive(Debug, Clone, Copy)]
pub struct GraphicAttribute {
    pub fg: u8,
    pub bg: u8,
    pub inversed: bool,
}

impl GraphicAttribute {
    const fn default() -> Self {
        GraphicAttribute {
            fg: 7,
            bg: 0,
            inversed: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Buffer {
    pub lines: VecDeque<Vec<Cell>>,
    pub cursor: (usize, usize),
    cols: usize,
    capacity: usize,
}

impl Buffer {
    pub fn new(capacity: usize, cols: usize) -> Self {
        assert!(capacity > 0);
        Self {
            lines: VecDeque::new(),
            cols,
            capacity,
            cursor: (0, 0),
        }
    }

    fn allocate_line(&mut self) {
        if self.lines.len() == self.capacity {
            self.lines.rotate_left(1);
            self.lines[self.capacity - 1].fill(Cell::SPACE);
        } else {
            let fresh_line = vec![Cell::SPACE; self.cols];
            self.lines.push_back(fresh_line);
        }
    }

    fn erase_line(&mut self, row: usize) {
        self.lines[row].fill(Cell::SPACE);
    }

    fn put(&mut self, row: usize, col: usize, cell: Cell) {
        let mut c = col;
        while c > 0 && self.lines[row][c].width == 0 {
            c -= 1;
        }

        if self.lines[row][c].width > 1 {
            let w = self.lines[row][c].width as usize;
            for d in 0..w {
                self.lines[row][c + d] = Cell::SPACE;
            }
        }

        self.lines[row][col] = cell;

        if cell.width > 1 {
            let w = cell.width as usize;
            for d in 1..w {
                self.lines[row][col + d] = Cell::VOID;
            }
        }
    }
}

#[derive(Debug)]
enum Command {
    Resize { lines: usize, columns: usize },
}

#[derive(Debug)]
pub struct Terminal {
    pty: OwnedFd,
    control_req: pipe_channel::Sender<Command>,
    control_res: pipe_channel::Receiver<i32>,
    pub buffer: Arc<Mutex<Buffer>>,
    rows: usize,
    cols: usize,
}

impl Terminal {
    pub fn new(lines: usize, columns: usize) -> Self {
        let (pty, _child_pid) = init_pty().unwrap();

        let (control_req_tx, control_req_rx) = pipe_channel::channel();
        let (control_res_tx, control_res_rx) = pipe_channel::channel();

        let engine = Engine::new(
            pty.dup().expect("dup"),
            control_req_rx,
            control_res_tx,
            lines,
            columns,
        );
        let buffer = engine.buffer();
        std::thread::spawn(move || engine.start());

        Terminal {
            pty,
            control_req: control_req_tx,
            control_res: control_res_rx,
            buffer,
            rows: lines,
            cols: columns,
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    /// Writes the given data on PTY master
    pub fn pty_write(&mut self, data: &[u8]) {
        use std::io::Write as _;
        self.pty.write_all(data).unwrap();
    }

    #[allow(unused)]
    pub fn writer(&self) -> impl std::io::Write {
        let new_fd = self.pty.dup().expect("dup");
        new_fd.into_file()
    }

    pub fn request_resize(&mut self, lines: usize, columns: usize) {
        let size_changed = self.rows != lines || self.cols != columns;

        if size_changed {
            log::debug!("request_resize: {}x{} (cell)", lines, columns);
            self.control_req.send(Command::Resize { lines, columns });
            self.control_res.recv();

            self.rows = lines;
            self.cols = columns;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Cursor {
    drows: usize,
    dcols: usize,
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
            self.dcols - self.col
        }
    }

    fn exact(mut self, row: usize, col: usize) -> Self {
        self.row = min(self.drows - 1, row);
        self.col = min(self.dcols - 1, col);
        self.end = false;
        self
    }

    fn first_col(mut self) -> Self {
        self.end = false;
        self.col = 0;
        self
    }

    fn next_col(mut self) -> Self {
        if self.col + 1 < self.dcols {
            self.col += 1;
        } else {
            self.end = true;
        }
        self
    }

    fn prev_col(mut self) -> Self {
        if self.end {
            debug_assert_eq!(self.col, self.dcols - 1);
            self.end = false;
        } else if 0 < self.col {
            self.col -= 1;
        }
        self
    }

    fn next_row(mut self) -> Self {
        self.end = false;
        if self.row + 1 < self.drows {
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
    prows: usize,
    pcols: usize,
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
        lines: usize,
        columns: usize,
    ) -> Self {
        set_term_window_size(&pty, lines as u16, columns as u16).unwrap();

        let prows = lines;
        let pcols = columns;

        let capacity = 10000;
        let drows = capacity;
        let dcols = pcols;

        // Initialize tabulation stops
        let mut tabstops = Vec::new();
        for i in 0..pcols {
            if i % 8 == 0 {
                tabstops.push(i);
            }
        }

        let buffer = {
            let mut buf = Buffer::new(drows, dcols);
            (0..prows).for_each(|_| buf.allocate_line());
            Arc::new(Mutex::new(buf))
        };
        let cursor = Cursor {
            drows,
            dcols,
            row: 0,
            col: 0,
            end: false,
        };

        Self {
            pty,
            control_req,
            control_res,
            prows,
            pcols,
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

    fn resize(&mut self, lines: usize, columns: usize) {
        log::debug!("resize to {}x{} (cell)", lines, columns);

        set_term_window_size(&self.pty, lines as u16, columns as u16).unwrap();

        self.prows = lines;
        self.pcols = columns;

        self.tabstops.clear();
        for i in 0..self.pcols {
            if i % 8 == 0 {
                self.tabstops.push(i);
            }
        }

        let mut buf = self.buffer.lock().unwrap();
        buf.cols = columns;
        for line in buf.lines.iter_mut() {
            line.resize(columns, Cell::SPACE);
        }
        while buf.lines.len() < lines {
            buf.allocate_line();
        }

        self.cursor.dcols = columns;
        if self.cursor.col >= columns {
            self.cursor.col = columns - 1;
        }
        self.cursor.row = buf.lines.len() - lines;

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
                        Command::Resize { lines, columns } => {
                            self.resize(lines, columns);
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
                    self.cursor = self.cursor.next_row();
                    buffer_scroll_up_if_needed(&mut buf, self.cursor);
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
                    if col == self.pcols - 1 {
                        return;
                    }

                    // Move cursor to the next tabstop
                    let next = match self.tabstops.binary_search(&(col + 1)) {
                        Ok(i) => self.tabstops[i],
                        Err(i) if i < self.tabstops.len() => self.tabstops[i],
                        _ => self.pcols - 1,
                    };
                    let advance = next - col;
                    debug_assert!(advance > 0);

                    let tab = Cell {
                        ch: ' ',
                        width: advance as u16,
                        attr: self.attr,
                    };
                    buf.put(row, col, tab);

                    for _ in 0..advance {
                        self.cursor = self.cursor.next_col();
                    }
                }

                CUU(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let drow = self.cursor.row;
                    let prow = drow_to_prow(drow, self.prows, buf.lines.len());
                    let up = min(prow, pn);
                    for _ in 0..up {
                        self.cursor = self.cursor.prev_row();
                    }
                }

                CUD(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let drow = self.cursor.row;
                    let prow = drow_to_prow(drow, self.prows, buf.lines.len());
                    let down = min(self.prows - prow - 1, pn);
                    for _ in 0..down {
                        self.cursor = self.cursor.next_row();
                    }
                }

                CUF(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let col = self.cursor.col;
                    let right = min(self.pcols - 1 - col, pn);
                    for _ in 0..right {
                        self.cursor = self.cursor.next_col();
                    }
                }

                CUB(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1
                    }

                    let col = self.cursor.col;
                    let left = min(col, pn);
                    for _ in 0..left {
                        self.cursor = self.cursor.prev_col();
                    }
                }

                CUP(pn1, pn2) => {
                    let mut pn1 = pn1 as usize;
                    let mut pn2 = pn2 as usize;

                    if pn1 > 0 {
                        pn1 -= 1;
                    }
                    if pn2 > 0 {
                        pn2 -= 1;
                    }

                    let row = prow_to_drow(pn1, self.prows, buf.lines.len());
                    let col = pn2;
                    self.cursor = self.cursor.exact(row, col);
                }

                ECH(pn) => {
                    let mut pn = pn as usize;
                    if pn == 0 {
                        pn = 1;
                    }

                    let (row, col) = self.cursor.pos();
                    for d in 0..pn {
                        if col + d >= buf.cols {
                            break;
                        }
                        buf.put(row, col + d, Cell::SPACE);
                    }
                }

                ED(ps) => match ps {
                    0 => {
                        // clear from the the cursor position to the end (inclusive)
                        let (row, col) = self.cursor.pos();
                        let prow = drow_to_prow(row, self.prows, buf.lines.len());
                        for pr in (prow + 1)..self.prows {
                            let dr = prow_to_drow(pr, self.prows, buf.lines.len());
                            buf.erase_line(dr);
                        }
                        for c in col..buf.cols {
                            buf.put(row, c, Cell::SPACE);
                        }
                    }
                    1 => {
                        // clear from the beginning to the cursor position (inclusive)
                        let (row, col) = self.cursor.pos();
                        let prow = drow_to_prow(row, self.prows, buf.lines.len());
                        for pr in 0..prow {
                            let dr = prow_to_drow(pr, self.prows, buf.lines.len());
                            buf.erase_line(dr);
                        }
                        for c in 0..=col {
                            buf.put(row, c, Cell::SPACE);
                        }
                    }
                    2 => {
                        // clear all positions
                        for pr in 0..self.prows {
                            let dr = prow_to_drow(pr, self.prows, buf.lines.len());
                            buf.erase_line(dr);
                        }
                    }
                    _ => unreachable!(),
                },

                EL(ps) => match ps {
                    0 => {
                        // clear from the cursor position to the line end (inclusive)
                        let (row, col) = self.cursor.pos();
                        for c in col..buf.cols {
                            buf.put(row, c, Cell::SPACE);
                        }
                    }
                    1 => {
                        // clear from the line beginning to the cursor position (inclusive)
                        let (row, col) = self.cursor.pos();
                        for c in 0..=col {
                            buf.put(row, c, Cell::SPACE);
                        }
                    }
                    2 => {
                        // clear line
                        let row = self.cursor.row;
                        buf.erase_line(row);
                    }
                    _ => unreachable!(),
                },

                SGR(ps) => {
                    for &p in ps {
                        match p {
                            0 => self.attr = GraphicAttribute::default(),
                            7 => self.attr.inversed = true,
                            27 => self.attr.inversed = false,
                            30..=37 => self.attr.fg = p as u8 - 30,
                            40..=47 => self.attr.bg = p as u8 - 40,

                            // gaming effect (just for fun!)
                            70 => self.attr.fg = 0xFF,
                            80 => self.attr.bg = 0xFF,
                            _ => {}
                        }
                    }
                }

                GraphicChar(ch) => {
                    use unicode_width::UnicodeWidthChar as _;
                    if let Some(width) = ch.width() {
                        // If there is no space for new character, move cursor to the next line.
                        if self.cursor.right_space() < width {
                            self.cursor = self.cursor.next_row().first_col();
                            buffer_scroll_up_if_needed(&mut buf, self.cursor);
                        }

                        let (row, col) = self.cursor.pos();
                        let cell = Cell {
                            ch,
                            width: width as u16,
                            attr: self.attr,
                        };
                        buf.put(row, col, cell);

                        for _ in 0..width {
                            self.cursor = self.cursor.next_col();
                        }
                    }
                }

                ESC => {
                    log::warn!("the parser should not produce ESC function.");
                    continue;
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

                ICH => ignore!(),
                CNL => ignore!(),
                CPL => ignore!(),
                CHA => ignore!(),
                CHT => ignore!(),
                IL => ignore!(),
                DL => ignore!(),
                EF => ignore!(),
                EA => ignore!(),
                DCH => ignore!(),
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
                VPA => ignore!(),
                VPR => ignore!(),
                HVP => ignore!(),
                TBC => ignore!(),
                SM => ignore!(),
                MC => ignore!(),
                HPB => ignore!(),
                VPB => ignore!(),
                RM => ignore!(),
                DSR => ignore!(),
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

        buf.cursor = (self.cursor.row, self.cursor.col);
    }
}

fn buffer_scroll_up_if_needed(buf: &mut Buffer, cursor: Cursor) {
    if cursor.row + 1 == buf.capacity {
        buf.allocate_line();
    } else {
        while cursor.row >= buf.lines.len() {
            buf.allocate_line();
        }
    }
}

fn drow_to_prow(drow: usize, prows: usize, lines: usize) -> usize {
    debug_assert!(lines >= prows);
    let top_line = lines - prows;
    drow - top_line
}
fn prow_to_drow(prow: usize, prows: usize, lines: usize) -> usize {
    debug_assert!(lines >= prows);
    let top_line = lines - prows;
    top_line + prow
}

// Open PTY device and spawn a shell
// Returns a pair (PTY master, PID of shell)
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

// Execute the shell
fn exec_shell() -> Result<()> {
    // Restore the default handler for SIGPIPE (terminate)
    use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};
    let sigdfl = SigAction::new(SigHandler::SigDfl, SaFlags::empty(), SigSet::empty());
    unsafe { sigaction(Signal::SIGPIPE, &sigdfl).expect("sigaction") };

    let shell = CStr::from_bytes_with_nul(b"/bin/sh\0").unwrap();
    let args: [&CStr; 1] = [shell];

    let mut vars: std::collections::HashMap<String, String> = std::env::vars().collect();

    // FIXME
    vars.remove("TERM");

    let envs: Vec<CString> = vars
        .into_iter()
        .map(|(key, val)| {
            let keyval_bytes = format!("{}={}\0", key, val).into_bytes();
            CString::from_vec_with_nul(keyval_bytes).unwrap()
        })
        .collect();

    nix::unistd::execve(shell, &args, &envs)?;
    unreachable!();
}
