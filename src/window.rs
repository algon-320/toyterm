use glium::{glutin, Display};
use glutin::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, ModifiersState, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
};

use crate::terminal::{Mode, Terminal, TerminalSize};
use crate::view::{TerminalView, Viewport};

pub struct TerminalWindow {
    display: Display,
    terminal: Terminal,
    clipboard: arboard::Clipboard,

    view: TerminalView,
    mode: Mode,
    history_head: isize,
    last_history_head: isize,
    focused: bool,
    modifiers: ModifiersState,
    mouse: MouseState,
}

struct MouseState {
    wheel_delta_x: f32,
    wheel_delta_y: f32,
    cursor_pos: (f64, f64),
    pressed_pos: Option<(f64, f64)>,
    released_pos: Option<(f64, f64)>,
    click_count: usize,
    last_clicked: std::time::Instant,
}

impl TerminalWindow {
    #[allow(unused)]
    pub fn new(display: Display, cwd: Option<&std::path::Path>) -> Self {
        let size = display.gl_window().window().inner_size();
        let full = Viewport {
            x: 0,
            y: 0,
            w: size.width,
            h: size.height,
        };
        Self::with_viewport(display, full, cwd)
    }

    pub fn with_viewport(
        display: Display,
        viewport: Viewport,
        cwd: Option<&std::path::Path>,
    ) -> Self {
        let font_size = crate::TOYTERM_CONFIG.font_size;
        let view = TerminalView::with_viewport(
            display.clone(),
            viewport,
            font_size,
            Some((0, viewport.h)),
        );

        let terminal = {
            let cell_size = view.cell_size();
            let scroll_bar_width = crate::TOYTERM_CONFIG.scroll_bar_width;
            let size = TerminalSize {
                rows: (viewport.h / cell_size.h) as usize,
                cols: ((viewport.w - scroll_bar_width) / cell_size.w) as usize,
            };
            let parent_cwd = std::env::current_dir().expect("cwd");
            let child_cwd = cwd.unwrap_or(&parent_cwd);
            Terminal::new(size, cell_size, child_cwd)
        };

        // Use I-beam mouse cursor
        display
            .gl_window()
            .window()
            .set_cursor_icon(glutin::window::CursorIcon::Text);

        TerminalWindow {
            display,
            terminal,
            clipboard: arboard::Clipboard::new().expect("clipboard"),

            view,
            mode: Mode::default(),
            history_head: 0,
            last_history_head: 0,
            focused: true,
            modifiers: ModifiersState::empty(),
            mouse: MouseState {
                wheel_delta_x: 0.0,
                wheel_delta_y: 0.0,
                cursor_pos: (0.0, 0.0),
                pressed_pos: None,
                released_pos: None,
                click_count: 0,
                last_clicked: std::time::Instant::now() - std::time::Duration::from_secs(10),
            },
        }
    }

    // Change cursor icon according to the current mouse_track mode
    pub fn refresh_cursor_icon(&mut self) {
        let icon = if self.mode.mouse_track {
            glutin::window::CursorIcon::Arrow
        } else {
            glutin::window::CursorIcon::Text
        };
        self.display.gl_window().window().set_cursor_icon(icon);
    }

    // Returns true if the PTY is closed, false otherwise
    fn check_update(&mut self) -> bool {
        let cell_size = self.view.cell_size();

        let contents_updated: bool;
        let mouse_track_mode_changed: bool;
        let terminal_size: TerminalSize;
        {
            // hold the lock while copying states
            let mut state = self.terminal.state.lock().unwrap();

            if state.closed {
                return true;
            }

            mouse_track_mode_changed = self.mode.mouse_track != state.mode.mouse_track;
            self.mode = state.mode;

            contents_updated = state.updated || self.last_history_head != self.history_head;
            self.last_history_head = self.history_head;

            terminal_size = state.size;

            if contents_updated {
                // update scroll bar
                let scroll_bar_position = {
                    let hist_rows = state.history_size;
                    let rows = state.size.rows;
                    let viewport_height = self.viewport().h;

                    let total = hist_rows + rows;
                    let r = (hist_rows as isize + self.history_head) as f64 / total as f64;
                    let origin = (viewport_height as f64 * r) as u32;
                    let length = ((viewport_height as f64) * rows as f64 / total as f64) as u32;
                    Some((origin, length))
                };

                let mut lines = Vec::new();
                self.view
                    .update_contents(|view| std::mem::swap(&mut view.lines, &mut lines));

                {
                    let top = self.history_head;
                    let bot = top + terminal_size.rows as isize;

                    if lines.len() == terminal_size.rows {
                        // Copy lines w/o heap allocation
                        for (src, dst) in state.range(top, bot).zip(lines.iter_mut()) {
                            dst.copy_from(src);
                        }
                    } else {
                        // Copy lines w/ heap allocation
                        lines.clear();
                        lines.extend(state.range(top, bot).cloned());
                    }
                }

                let images = state
                    .images()
                    .cloned()
                    .map(|mut img| {
                        img.row -= self.history_head;
                        img
                    })
                    .collect();

                let cursor = if self.history_head >= 0 && state.mode.cursor_visible {
                    let (row, col, style) = state.cursor();

                    self.display
                        .gl_window()
                        .window()
                        .set_ime_position(PhysicalPosition {
                            x: col as u32 * cell_size.w,
                            y: (row + 1) as u32 * cell_size.h,
                        });

                    Some((row, col, style))
                } else {
                    None
                };

                self.view.update_contents(|view| {
                    view.lines = lines;
                    view.images = images;
                    view.cursor = cursor;
                    view.scroll_bar = scroll_bar_position;
                    view.view_focused = self.focused;
                });
            }

            state.updated = false;
        }

        if mouse_track_mode_changed {
            self.refresh_cursor_icon();
        }

        // Update text selection
        if let Some((sx, sy)) = self.mouse.pressed_pos {
            let (ex, ey) = self.mouse.released_pos.unwrap_or(self.mouse.cursor_pos);

            let lines = &self.view.lines;

            let x_max = cell_size.w as f64 * terminal_size.cols as f64;
            let y_max = cell_size.h as f64 * terminal_size.rows as f64;
            let sx = sx.clamp(0.0, x_max - 0.1);
            let sy = sy.clamp(0.0, y_max - 0.1);
            let ex = ex.clamp(0.0, x_max - 0.1);
            let ey = ey.clamp(0.0, y_max - 0.1);

            let mut s_row = (sy / cell_size.h as f64).floor() as usize;
            let mut s_col = (sx / cell_size.w as f64).round() as usize;
            let mut e_row = (ey / cell_size.h as f64).floor() as usize;
            let mut e_col = (ex / cell_size.w as f64).round() as usize;

            if (e_row, e_col) < (s_row, s_col) {
                std::mem::swap(&mut s_row, &mut e_row);
                std::mem::swap(&mut s_col, &mut e_col);
            }

            // NOTE: selecton is closed range [s, e]
            e_col = e_col.saturating_sub(1);

            match self.mouse.click_count {
                // single click: character selection
                1 => {
                    // nothing to do
                }

                // double click: word selection
                2 => {
                    fn delimiter(ch: char) -> bool {
                        ch.is_ascii_punctuation() || ch.is_ascii_whitespace()
                    }
                    fn on_different_word(a: char, b: char) -> bool {
                        delimiter(a) || delimiter(b)
                    }

                    while 0 < s_col && s_col < terminal_size.cols {
                        let prev = lines[s_row].get(s_col - 1).unwrap().ch;
                        let curr = lines[s_row].get(s_col).unwrap().ch;
                        if on_different_word(prev, curr) {
                            break;
                        }
                        s_col -= 1;
                    }
                    while e_col < terminal_size.cols - 1 {
                        let prev = lines[e_row].get(e_col).unwrap().ch;
                        let curr = lines[e_row].get(e_col + 1).unwrap().ch;
                        if on_different_word(prev, curr) {
                            break;
                        }
                        e_col += 1;
                    }
                }

                // tripple click (or more): line selection
                _ => {
                    s_col = 0;
                    e_col = terminal_size.cols - 1;
                }
            }

            let l = s_row * terminal_size.cols + s_col;
            let r = e_row * terminal_size.cols + e_col;
            let new_selection_range = if l <= r { Some((l, r)) } else { None };

            if self.view.selection_range != new_selection_range {
                self.view.update_contents(|view| {
                    view.selection_range = new_selection_range;
                });
            }
        } else if self.view.selection_range.is_some() {
            self.view.update_contents(|view| {
                view.selection_range = None;
            });
        }

        false
    }

    pub fn draw(&mut self, surface: &mut glium::Frame) {
        self.view.draw(surface);
    }

    pub fn viewport(&self) -> Viewport {
        self.view.viewport()
    }

    pub fn set_viewport(&mut self, new_viewport: Viewport) {
        log::debug!("viewport changed: {:?}", new_viewport);
        self.view.set_viewport(new_viewport);
        self.resize_buffer();
    }

    fn increase_font_size(&mut self, size_diff: i32) {
        self.view.increase_font_size(size_diff);
        self.resize_buffer();
    }

    fn resize_buffer(&mut self) {
        self.mouse.pressed_pos = None;
        self.mouse.released_pos = None;

        let viewport = self.view.viewport();

        let scroll_bar_width = crate::TOYTERM_CONFIG.scroll_bar_width;
        let width = viewport.w.saturating_sub(scroll_bar_width);

        let cell_size = self.view.cell_size();
        let rows = (viewport.h / cell_size.h) as usize;
        let cols = (width / cell_size.w) as usize;
        let buff_size = TerminalSize {
            rows: rows.max(1),
            cols: cols.max(1),
        };
        self.terminal.request_resize(buff_size, cell_size);
    }

    pub fn focus_changed(&mut self, gain: bool) {
        self.focused = gain;

        // Update cursor
        self.view.update_contents(|view| {
            view.view_focused = self.focused;
        });

        if gain {
            self.refresh_cursor_icon();
        }
    }

    pub fn on_event(&mut self, event: &Event<()>, control_flow: &mut ControlFlow) {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }

                &WindowEvent::Focused(gain) => self.focus_changed(gain),

                &WindowEvent::Resized(new_size) => {
                    let mut viewport = self.viewport();
                    viewport.w = new_size.width;
                    viewport.h = new_size.height;
                    self.set_viewport(viewport);
                }

                &WindowEvent::ModifiersChanged(new_states) => {
                    self.modifiers = new_states;
                }

                &WindowEvent::ReceivedCharacter(ch) => {
                    // Handle these characters on WindowEvent::KeyboardInput event
                    if ch == '-'
                        || ch == '='
                        || ch == '\x7F'
                        || ch == '\x03'
                        || ch == '\x08'
                        || ch == '\x0C'
                        || ch == '\x16'
                        || ch == '\x1B'
                    {
                        return;
                    }

                    if ch.is_control() {
                        log::debug!("input: {:?}", ch);
                    }

                    let mut buf = [0_u8; 4];
                    let utf8 = ch.encode_utf8(&mut buf).as_bytes();
                    self.terminal.pty_write(utf8);
                }

                WindowEvent::KeyboardInput { input, .. }
                    if input.state == ElementState::Pressed =>
                {
                    if let Some(key) = input.virtual_keycode {
                        self.on_key_press(key);
                    }
                }

                WindowEvent::CursorMoved { position, .. } => {
                    let viewport = self.viewport();
                    let x = position.x - viewport.x as f64;
                    let y = position.y - viewport.y as f64;
                    self.mouse.cursor_pos = (x, y);
                }

                WindowEvent::MouseInput { state, button, .. } => {
                    let is_inner = {
                        let viewport = self.viewport();
                        let (w, h) = (viewport.w as f64, viewport.h as f64);
                        let (x, y) = self.mouse.cursor_pos;
                        0.0 <= x && x < w && 0.0 <= y && y < h
                    };

                    if !is_inner {
                        self.mouse.pressed_pos = None;
                        self.mouse.released_pos = None;
                        return;
                    }

                    if self.mode.mouse_track {
                        let button = match state {
                            ElementState::Released if !self.mode.sgr_ext_mouse_track => 3,
                            _ => match button {
                                MouseButton::Left => 0,
                                MouseButton::Middle => 1,
                                MouseButton::Right => 2,
                                MouseButton::Other(button_id) => {
                                    // FIXME : Support multi button mouse?
                                    log::warn!("unkown mouse button : {}", button_id);
                                    0
                                }
                            },
                        };

                        #[rustfmt::skip]
                        let mods =
                            if self.modifiers.shift() { 0b00000100 } else { 0 }
                        |   if self.modifiers.alt()   { 0b00001000 } else { 0 }
                        |   if self.modifiers.ctrl()  { 0b00010000 } else { 0 };

                        let (x, y) = self.mouse.cursor_pos;
                        let cell_size = self.view.cell_size();
                        let col = x.round() as u32 / cell_size.w + 1;
                        let row = y.round() as u32 / cell_size.h + 1;

                        if self.mode.sgr_ext_mouse_track {
                            self.sgr_ext_mouse_report(button + mods, col, row, state);
                        } else {
                            self.normal_mouse_report(button + mods, col, row);
                        }
                    } else {
                        match state {
                            ElementState::Pressed => {
                                const CLICK_INTERVAL: std::time::Duration =
                                    std::time::Duration::from_millis(400);
                                if self.mouse.last_clicked.elapsed() > CLICK_INTERVAL {
                                    self.mouse.click_count = 0;
                                }

                                self.mouse.click_count += 1;
                                self.mouse.last_clicked = std::time::Instant::now();
                                log::debug!("clicked {} times", self.mouse.click_count);

                                self.mouse.pressed_pos = Some(self.mouse.cursor_pos);
                                self.mouse.released_pos = None;
                            }
                            ElementState::Released => {
                                self.mouse.released_pos = Some(self.mouse.cursor_pos);
                            }
                        }
                    }
                }

                WindowEvent::MouseWheel {
                    delta: glutin::event::MouseScrollDelta::LineDelta(dx, dy),
                    ..
                } => {
                    let mouse = &mut self.mouse;

                    mouse.wheel_delta_x += dx * 1.5;
                    mouse.wheel_delta_y += dy * 1.5;

                    let horizontal = mouse.wheel_delta_x.trunc() as isize;
                    let vertical = mouse.wheel_delta_y.trunc() as isize;

                    mouse.wheel_delta_x %= 1.0;
                    mouse.wheel_delta_y %= 1.0;

                    if self.modifiers.shift() {
                        // Scroll up history
                        let state = self.terminal.state.lock().unwrap();
                        let min = -(state.history_size as isize);
                        self.history_head = (self.history_head - vertical).clamp(min, 0);
                    } else {
                        // Send Up/Down key
                        if vertical > 0 {
                            for _ in 0..vertical.abs() {
                                self.terminal.pty_write(b"\x1b[\x41"); // Up
                            }
                        } else {
                            for _ in 0..vertical.abs() {
                                self.terminal.pty_write(b"\x1b[\x42"); // Down
                            }
                        }
                    }

                    if horizontal > 0 {
                        for _ in 0..horizontal.abs() {
                            self.terminal.pty_write(b"\x1b[\x43"); // Right
                        }
                    } else {
                        for _ in 0..horizontal.abs() {
                            self.terminal.pty_write(b"\x1b[\x44"); // Left
                        }
                    }
                }

                _ => {}
            },

            Event::MainEventsCleared => {
                if self.check_update() {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                self.display.gl_window().window().request_redraw();
            }

            Event::RedrawRequested(_) => {
                let mut surface = self.display.draw();
                self.draw(&mut surface);
                surface.finish().expect("finish");
            }

            _ => {}
        }
    }

    fn on_key_press(&mut self, keycode: VirtualKeyCode) {
        use ModifiersState as Mod;
        const EMPTY: u32 = Mod::empty().bits();
        const CTRL: u32 = Mod::CTRL.bits();
        const CTRL_SHIFT: u32 = Mod::CTRL.bits() | Mod::SHIFT.bits();

        // normally text selection is cleared when user types something,
        // but there are some exceptions. history_head is cleared too.
        let mut clear = true;

        match (self.modifiers.bits(), keycode) {
            (EMPTY, VirtualKeyCode::Escape) => {
                self.history_head = 0;
                self.mouse.pressed_pos = None;
                self.mouse.released_pos = None;
                self.terminal.pty_write(b"\x1B");
            }

            (CTRL, VirtualKeyCode::Minus) => {
                // font size -
                self.increase_font_size(-1);
            }
            (CTRL, VirtualKeyCode::Equals) => {
                // font size +
                self.increase_font_size(1);
            }

            // Backspace
            (EMPTY, VirtualKeyCode::Back) => {
                // Note: send DEL instead of BS
                self.terminal.pty_write(b"\x7f");
            }

            (EMPTY, VirtualKeyCode::Delete) => {
                self.terminal.pty_write(b"\x1b[3~");
            }

            (EMPTY, VirtualKeyCode::Up) => {
                self.terminal.pty_write(b"\x1b[\x41");
            }
            (EMPTY, VirtualKeyCode::Down) => {
                self.terminal.pty_write(b"\x1b[\x42");
            }
            (EMPTY, VirtualKeyCode::Right) => {
                self.terminal.pty_write(b"\x1b[\x43");
            }
            (EMPTY, VirtualKeyCode::Left) => {
                self.terminal.pty_write(b"\x1b[\x44");
            }

            (EMPTY, VirtualKeyCode::PageUp) => {
                self.terminal.pty_write(b"\x1b[5~");
            }
            (EMPTY, VirtualKeyCode::PageDown) => {
                self.terminal.pty_write(b"\x1b[6~");
            }

            (EMPTY, VirtualKeyCode::Minus) => {
                self.terminal.pty_write(b"-");
            }
            (EMPTY, VirtualKeyCode::Equals) => {
                self.terminal.pty_write(b"=");
            }

            (CTRL, VirtualKeyCode::C) => {
                self.terminal.pty_write(b"\x03");
            }

            (CTRL_SHIFT, VirtualKeyCode::C) => {
                clear = false;
                self.copy_clipboard();
            }

            (CTRL, VirtualKeyCode::V) => {
                self.terminal.pty_write(b"\x16");
            }

            (CTRL_SHIFT, VirtualKeyCode::V) => {
                self.paste_clipboard();
            }

            (CTRL, VirtualKeyCode::L) => {
                self.terminal.pty_write(b"\x0c");
            }

            (CTRL_SHIFT, VirtualKeyCode::L) => {
                self.history_head = 0;
                let mut state = self.terminal.state.lock().unwrap();
                state.clear_history();
            }

            (_, keycode) => {
                log::trace!("key pressed: ({:?}) {:?}", self.modifiers, keycode);

                use VirtualKeyCode::*;
                if let LControl | RControl | LShift | RShift = keycode {
                    clear = false;
                }
            }
        }

        if clear {
            self.view.update_contents(|view| {
                view.selection_range = None;
            });

            self.history_head = 0;
            self.mouse.pressed_pos = None;
            self.mouse.released_pos = None;
        }
    }

    fn copy_clipboard(&mut self) {
        let mut text = String::new();

        let selection_range = self.view.selection_range;

        'row: for (i, row) in self.view.lines.iter().enumerate() {
            let cols = row.columns();

            for (j, cell) in row.iter().enumerate() {
                if cell.width == 0 {
                    continue;
                }

                let is_selected = match selection_range {
                    Some((left, right)) => {
                        let offset = i * cols + j;
                        let center = offset + (cell.width / 2) as usize;
                        left <= center && center <= right
                    }
                    None => false,
                };

                if is_selected {
                    text.push(cell.ch);
                }

                if cell.ch == '\n' {
                    continue 'row;
                }
            }

            if !row.linewrap() {
                let is_selected = match selection_range {
                    Some((left, right)) => {
                        let offset = (i + 1) * cols;
                        left < offset && offset <= right
                    }
                    None => false,
                };
                if is_selected {
                    text.push('\n');
                }
            }
        }

        log::info!("copy: {:?}", text);
        let _ = self.clipboard.set_text(text);
    }

    fn paste_clipboard(&mut self) {
        match self.clipboard.get_text() {
            Ok(text) => {
                log::debug!("paste: {:?}", text);
                if self.mode.bracketed_paste {
                    self.terminal.pty_write(b"\x1b[200~");
                    self.terminal.pty_write(text.as_bytes());
                    self.terminal.pty_write(b"\x1b[201~");
                } else {
                    self.terminal.pty_write(text.as_bytes());
                }
            }
            Err(_) => {
                log::error!("Failed to paste something from clipboard");
            }
        }
    }

    fn normal_mouse_report(&mut self, button: u8, col: u32, row: u32) {
        let col = if 0 < col && col < 224 { col + 32 } else { 0 } as u8;
        let row = if 0 < row && row < 224 { row + 32 } else { 0 } as u8;

        let msg = [b'\x1b', b'[', b'M', 32 + button, col, row];

        self.terminal.pty_write(&msg);
    }

    fn sgr_ext_mouse_report(&mut self, button: u8, col: u32, row: u32, state: &ElementState) {
        let m = match state {
            ElementState::Pressed => 'M',
            ElementState::Released => 'm',
        };

        self.terminal
            .pty_write(format!("\x1b[<{button};{col};{row}{m}").as_bytes());
    }
}

#[cfg(feature = "multiplex")]
impl TerminalWindow {
    pub fn get_foreground_process_name(&self) -> String {
        let pgid = self.terminal.get_pgid();
        match std::fs::read(format!("/proc/{pgid}/cmdline")) {
            Ok(cmdline) => {
                let argv0 = cmdline.split(|b| *b == b'\0').next().unwrap();
                String::from_utf8_lossy(argv0).into()
            }
            Err(err) => {
                // A process group doesn't need to have a leader (PID=PGID).
                log::debug!("Failed to read /proc/{pgid}/cmdline: {}", err);
                "(unknown)".to_owned()
            }
        }
    }

    pub fn get_foreground_process_cwd(&self) -> std::path::PathBuf {
        let pgid = self.terminal.get_pgid();
        match std::fs::read_link(format!("/proc/{pgid}/cwd")) {
            Ok(cwd) => cwd,
            Err(err) => {
                // A process group doesn't need to have a leader (PID=PGID).
                log::debug!("Failed to read_link /proc/{pgid}/cwd: {}", err);

                // FIXME
                std::env::current_dir().unwrap()
            }
        }
    }
}
