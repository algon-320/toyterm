use glium::{glutin, index, texture, uniform, uniforms, Display};
use glutin::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, ModifiersState, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
};
use std::rc::Rc;

use crate::cache::GlyphCache;
use crate::clipboard::X11Clipboard;
use crate::font::{Font, FontSet, Style};
use crate::terminal::{CellSize, Color, CursorStyle, Line, Terminal, TerminalSize};

fn sort_points(a: (f64, f64), b: (f64, f64), cell_sz: CellSize) -> ((f64, f64), (f64, f64)) {
    let (ax, ay) = a;
    let (bx, by) = b;

    let a_row = ay.round() as u32 / cell_sz.h;
    let b_row = by.round() as u32 / cell_sz.h;

    if a_row < b_row {
        (a, b)
    } else if a_row > b_row {
        (b, a)
    } else if ax < bx {
        (a, b)
    } else {
        (b, a)
    }
}

fn build_font_set() -> FontSet {
    let config = &crate::TOYTERM_CONFIG;

    let mut fonts = FontSet::empty();

    for p in config.fonts_regular.iter() {
        // FIXME
        if p.as_os_str().is_empty() {
            continue;
        }

        if !p.exists() {
            log::warn!("font file {:?} doesn't exist, ignored", p.display());
            continue;
        }

        log::debug!("add regular font: {:?}", p.display());
        let data = std::fs::read(p).expect("cannot open font");
        let font = Font::new(&data);
        fonts.add(Style::Regular, font);
    }

    for p in config.fonts_bold.iter() {
        // FIXME
        if p.as_os_str().is_empty() {
            continue;
        }

        if !p.exists() {
            log::warn!("font file {:?} doesn't exist, ignored", p.display());
            continue;
        }

        log::debug!("add bold font: {:?}", p.display());
        let data = std::fs::read(p).expect("cannot open font");
        let font = Font::new(&data);
        fonts.add(Style::Bold, font);
    }

    for p in config.fonts_faint.iter() {
        // FIXME
        if p.as_os_str().is_empty() {
            continue;
        }

        if !p.exists() {
            log::warn!("font file {:?} doesn't exist, ignored", p.display());
            continue;
        }

        log::debug!("add faint font: {:?}", p.display());
        let data = std::fs::read(p).expect("cannot open font");
        let font = Font::new(&data);
        fonts.add(Style::Faint, font);
    }

    let regular_font = Font::new(include_bytes!("../fonts/Mplus1Code-Regular.ttf"));
    fonts.add(Style::Regular, regular_font);

    let bold_font = Font::new(include_bytes!("../fonts/Mplus1Code-SemiBold.ttf"));
    fonts.add(Style::Bold, bold_font);

    let faint_font = Font::new(include_bytes!("../fonts/Mplus1Code-Thin.ttf"));
    fonts.add(Style::Faint, faint_font);

    fonts
}

fn calculate_cell_size(fonts: &FontSet) -> (CellSize, i32) {
    use std::cmp::max;

    let mut max_advance_x: i32 = 0;
    let mut max_over: i32 = 0;
    let mut max_under: i32 = 0;

    let ascii_visible = ' '..='~';
    for ch in ascii_visible {
        for style in [Style::Regular, Style::Bold, Style::Faint] {
            let metrics = fonts.metrics(ch, style).expect("undefined glyph");

            let advance_x = (metrics.horiAdvance >> 6) as i32;
            max_advance_x = max(max_advance_x, advance_x);

            let over = (metrics.horiBearingY >> 6) as i32;
            max_over = max(max_over, over);

            let under = ((metrics.height - metrics.horiBearingY) >> 6) as i32;
            max_under = max(max_under, under);
        }
    }

    let cell_w = max_advance_x as u32;
    let cell_h = (max_over + max_under) as u32;

    log::debug!("cell size: {}x{} (px)", cell_w, cell_h);

    (
        CellSize {
            w: cell_w,
            h: cell_h,
        },
        max_over,
    )
}

type WindowSize = PhysicalSize<u32>;

struct DrawQuery<V: glium::vertex::Vertex> {
    vertices: glium::VertexBuffer<V>,
    texture: Rc<texture::Texture2d>,
}

#[derive(Default)]
struct Contents {
    lines: Vec<Line>,
    cursor_row: usize,
    cursor_col: usize,
    cursor_style: CursorStyle,
    cursor_visible: bool,
    // FIXME: use cell index instead of pixel
    selection_range: Option<(f64, f64)>,
    history_head: isize,
    window_size: WindowSize,
    cell_size: CellSize,
}

impl Contents {
    fn eq_except_for_lines(&self, other: &Self) -> bool {
        self.cursor_row == other.cursor_row
            && self.cursor_col == other.cursor_col
            && self.cursor_style == other.cursor_style
            && self.cursor_visible == other.cursor_visible
            && self.selection_range == other.selection_range
            && self.history_head == other.history_head
            && self.window_size == other.window_size
            && self.cell_size == other.cell_size
    }
}

#[derive(Default)]
struct MouseState {
    wheel_delta_x: f32,
    wheel_delta_y: f32,
    cursor_pos: (f64, f64),
    pressed_pos: Option<(f64, f64)>,
    released_pos: Option<(f64, f64)>,
}

pub struct TerminalWindow {
    terminal: Terminal,
    display: Display,
    fonts: FontSet,
    cache: GlyphCache,
    clipboard: X11Clipboard,

    contents: Contents,
    history_head: isize,
    bracketed_paste_mode: bool,
    mouse_track_mode: bool,
    sgr_ext_mouse_track_mode: bool,
    window_size: WindowSize,
    cell_size: CellSize,
    cell_max_over: i32,
    modifiers: ModifiersState,
    mouse: MouseState,
    started_time: std::time::Instant,

    program_cell: glium::Program,
    program_img: glium::Program,
    vertices_fg: Vec<Vertex>,
    vertices_bg: Vec<Vertex>,
    draw_queries_fg: Vec<DrawQuery<Vertex>>,
    draw_queries_bg: Vec<DrawQuery<Vertex>>,
    draw_queries_img: Vec<DrawQuery<SimpleVertex>>,
}

impl TerminalWindow {
    pub fn new(display: Display) -> Self {
        let window_size = display.gl_window().window().inner_size();

        let fonts = build_font_set();

        let (cell_size, cell_max_over) = calculate_cell_size(&fonts);

        // Rasterize ASCII characters and cache them as a texture
        let cache = GlyphCache::build_ascii_visible(&display, &fonts, cell_size);

        let clipboard = X11Clipboard::new();

        let size = TerminalSize {
            rows: (window_size.height / cell_size.h) as usize,
            cols: (window_size.width / cell_size.w) as usize,
        };
        let terminal = Terminal::new(size, cell_size);

        // Initialize shaders
        let program_cell = {
            use glium::program::{Program, ProgramCreationInput};
            let input = ProgramCreationInput::SourceCode {
                vertex_shader: include_str!("cell.vert"),
                fragment_shader: include_str!("cell.frag"),
                geometry_shader: None,
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                transform_feedback_varyings: None,
                outputs_srgb: true,
                uses_point_size: false,
            };
            Program::new(&display, input).unwrap()
        };

        let program_img = {
            use glium::program::{Program, ProgramCreationInput};
            let input = ProgramCreationInput::SourceCode {
                vertex_shader: include_str!("image.vert"),
                fragment_shader: include_str!("image.frag"),
                geometry_shader: None,
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                transform_feedback_varyings: None,
                outputs_srgb: true,
                uses_point_size: false,
            };
            Program::new(&display, input).unwrap()
        };

        // Use I-beam mouse cursor
        display
            .gl_window()
            .window()
            .set_cursor_icon(glutin::window::CursorIcon::Text);

        TerminalWindow {
            terminal,
            display,
            fonts,
            cache,
            clipboard,

            contents: Contents::default(),
            history_head: 0,
            bracketed_paste_mode: false,
            mouse_track_mode: false,
            sgr_ext_mouse_track_mode: false,
            window_size,
            cell_size,
            cell_max_over,
            modifiers: ModifiersState::empty(),
            mouse: MouseState::default(),
            started_time: std::time::Instant::now(),

            program_cell,
            program_img,
            vertices_fg: Vec::new(),
            vertices_bg: Vec::new(),
            draw_queries_fg: Vec::new(),
            draw_queries_bg: Vec::new(),
            draw_queries_img: Vec::new(),
        }
    }

    // Returns true if the PTY is closed, false otherwise
    fn update(&mut self) -> bool {
        let window_width = self.window_size.width;
        let window_height = self.window_size.height;
        let cell_size = self.cell_size;

        let mut current = Contents::default();
        current.selection_range = self.mouse.pressed_pos.map(|start| {
            let end = self.mouse.released_pos.unwrap_or(self.mouse.cursor_pos);
            let ((sx, sy), (ex, ey)) = sort_points(start, end, cell_size);
            let s_row = sy.round() as u32 / cell_size.h;
            let e_row = ey.round() as u32 / cell_size.h;
            let l = (s_row as f64) * (window_width as f64) + sx;
            let r = (e_row as f64) * (window_width as f64) + ex;
            (l, r)
        });
        current.window_size = self.window_size;
        current.cell_size = self.cell_size;
        std::mem::swap(&mut current.lines, &mut self.contents.lines);

        let previous: &Contents = &self.contents;

        current.cursor_row = previous.cursor_row;
        current.cursor_col = previous.cursor_col;
        current.cursor_visible = previous.cursor_visible;
        current.cursor_style = previous.cursor_style;

        let view_changed: bool;
        {
            // hold the lock while copying buffer states
            let mut buf = self.terminal.buffer.lock().unwrap();

            if buf.closed {
                return true;
            }

            self.mouse_track_mode = buf.mouse_track_mode;
            self.sgr_ext_mouse_track_mode = buf.sgr_ext_mouse_track_mode;

            self.bracketed_paste_mode = buf.bracketed_paste_mode;

            if self.history_head < -(buf.history_size as isize) {
                self.history_head = -(buf.history_size as isize);
            }
            current.history_head = self.history_head;

            view_changed = buf.updated || previous.history_head != self.history_head;

            if view_changed {
                let top = self.history_head;
                let bot = top + buf.lines.len() as isize;

                if current.lines.len() != buf.lines.len() {
                    current.lines.clear();
                    current.lines.extend(buf.range(top, bot).cloned());
                } else {
                    for (src, dst) in buf.range(top, bot).zip(current.lines.iter_mut()) {
                        dst.copy_from(src);
                    }
                }

                self.draw_queries_img.clear();

                for img in buf.images.iter() {
                    let col = img.col;
                    let row = img.row - self.history_head;

                    let gl_x = x_to_gl(col as i32 * cell_size.w as i32, window_width);
                    let gl_y = y_to_gl(row as i32 * cell_size.h as i32, window_height);
                    let gl_w = w_to_gl(img.width as u32, window_width);
                    let gl_h = h_to_gl(img.height as u32, window_height);
                    let vs = image_vertices(gl_x, gl_y, gl_w, gl_h);

                    let vertices = glium::VertexBuffer::new(&self.display, &vs).unwrap();

                    let texture = texture::Texture2d::with_mipmaps(
                        &self.display,
                        glium::texture::RawImage2d {
                            data: img.data.clone().into(),
                            width: img.width as u32,
                            height: img.height as u32,
                            format: glium::texture::ClientFormat::U8U8U8,
                        },
                        texture::MipmapsOption::NoMipmap,
                    )
                    .expect("Failed to create texture");

                    self.draw_queries_img.push(DrawQuery {
                        vertices,
                        texture: Rc::new(texture),
                    });
                }

                current.cursor_row = buf.cursor.0;
                current.cursor_col = buf.cursor.1;
                current.cursor_visible = buf.cursor_visible_mode;
                current.cursor_style = buf.cursor_style;

                self.display
                    .gl_window()
                    .window()
                    .set_ime_position(PhysicalPosition {
                        x: current.cursor_col as u32 * cell_size.w,
                        y: (current.cursor_row + 1) as u32 * cell_size.h,
                    });
            }

            buf.updated = false;
        }

        if view_changed || !current.eq_except_for_lines(previous) {
            self.vertices_fg.clear();
            self.vertices_bg.clear();
            self.draw_queries_fg.clear();
            self.draw_queries_bg.clear();

            let cursor_visible = current.cursor_visible && current.history_head >= 0;
            if !cursor_visible {
                log::info!("current.cursor_visible: {}", current.cursor_visible);
                log::info!("current.history_head: {}", current.history_head);
            }

            // clear entire screen
            {
                let fg = Color::White;
                let bg = Color::Black;
                let vs = cell_vertices(-1.0, 1.0, 2.0, 2.0, fg, bg);
                self.vertices_bg.extend_from_slice(&vs);
            }

            let mut baseline: u32 = self.cell_max_over as u32;
            for (i, row) in current.lines.iter().enumerate() {
                let mut leftline: u32 = 0;
                for (j, cell) in row.iter().enumerate() {
                    if cell.width == 0 {
                        continue;
                    }

                    let cell_width_px = cell_size.w * cell.width as u32;

                    let style = if cell.attr.bold == -1 {
                        Style::Faint
                    } else if cell.attr.bold == 0 {
                        Style::Regular
                    } else {
                        Style::Bold
                    };

                    let (fg, bg) = {
                        let is_inversed = cell.attr.inversed;

                        let on_cursor = cursor_visible
                            && current.cursor_style == CursorStyle::Block
                            && i == current.cursor_row
                            && j == current.cursor_col;

                        let is_selected = match current.selection_range {
                            Some((left, right)) => {
                                let offset = (i as u32 * window_width + leftline) as f64;
                                let mid_point = offset + (cell_width_px as f64) / 2.0;
                                left <= mid_point && mid_point <= right
                            }
                            None => false,
                        };

                        let mut fg = cell.attr.fg;
                        let mut bg = cell.attr.bg;

                        if is_inversed ^ on_cursor ^ is_selected {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        if cell.attr.concealed {
                            fg = bg;
                        }

                        (fg, bg)
                    };

                    // Background
                    {
                        let gl_x = x_to_gl((j as u32 * cell_size.w) as i32, window_width);
                        let gl_y = y_to_gl((i as u32 * cell_size.h) as i32, window_height);
                        let gl_w = w_to_gl(cell_width_px, window_width);
                        let gl_h = h_to_gl(cell_size.h, window_height);
                        let vs = cell_vertices(gl_x, gl_y, gl_w, gl_h, fg, bg);
                        self.vertices_bg.extend_from_slice(&vs);
                    }

                    if let Some((region, metrics)) = self.cache.get(cell.ch, style) {
                        if !region.is_empty() {
                            let bearing_x = (metrics.horiBearingX >> 6) as u32;
                            let bearing_y = (metrics.horiBearingY >> 6) as u32;

                            let x = leftline as i32 + bearing_x as i32;
                            let y = baseline as i32 - bearing_y as i32;
                            let gl_x = x_to_gl(x, window_width);
                            let gl_y = y_to_gl(y, window_height);
                            let gl_w = w_to_gl(region.px_w, window_width);
                            let gl_h = h_to_gl(region.px_h, window_height);

                            let vs = glyph_vertices(
                                gl_x,
                                gl_y,
                                gl_w,
                                gl_h,
                                region.tx_x,
                                region.tx_y,
                                region.tx_w,
                                region.tx_h,
                                fg,
                                bg,
                                cell.attr.blinking,
                            );
                            self.vertices_fg.extend_from_slice(&vs);
                        }
                    } else if let Some((glyph_image, metrics)) = self.fonts.render(cell.ch, style) {
                        // for non-ASCII characters
                        if !glyph_image.data.is_empty() {
                            let bearing_x = (metrics.horiBearingX >> 6) as u32;
                            let bearing_y = (metrics.horiBearingY >> 6) as u32;

                            let glyph_width = glyph_image.width;
                            let glyph_height = glyph_image.height;

                            let gl_x = x_to_gl(leftline as i32 + bearing_x as i32, window_width);
                            let gl_y = y_to_gl(baseline as i32 - bearing_y as i32, window_height);
                            let gl_w = w_to_gl(glyph_width, window_width);
                            let gl_h = h_to_gl(glyph_height, window_height);

                            let vs = glyph_vertices(
                                gl_x,
                                gl_y,
                                gl_w,
                                gl_h,
                                0.0,
                                0.0,
                                1.0,
                                1.0,
                                fg,
                                bg,
                                cell.attr.blinking,
                            );

                            let vertex_buffer =
                                glium::VertexBuffer::new(&self.display, &vs).unwrap();

                            let single_glyph_texture = texture::Texture2d::with_mipmaps(
                                &self.display,
                                glyph_image,
                                texture::MipmapsOption::NoMipmap,
                            )
                            .expect("Failed to create texture");

                            self.draw_queries_fg.push(DrawQuery {
                                vertices: vertex_buffer,
                                texture: Rc::new(single_glyph_texture),
                            });
                        }
                    } else {
                        log::trace!("undefined glyph: {:?}", cell.ch);
                    }

                    leftline += cell_width_px;
                }
                baseline += cell_size.h;
            }

            if current.cursor_style == CursorStyle::Bar {
                let cursor_col = current.cursor_col as u32;
                let cursor_row = current.cursor_row as u32;

                let fg = Color::Black;
                let bg = Color::White;

                let gl_x = x_to_gl((cursor_col * cell_size.w) as i32, window_width);
                let gl_y = y_to_gl((cursor_row * cell_size.h) as i32, window_height);
                let gl_w = w_to_gl(4, window_width);
                let gl_h = h_to_gl(cell_size.h, window_height);
                let vs = cell_vertices(gl_x, gl_y, gl_w, gl_h, fg, bg);
                self.vertices_fg.extend_from_slice(&vs);
            }

            let vb_fg = glium::VertexBuffer::new(&self.display, &self.vertices_fg).unwrap();
            self.draw_queries_fg.push(DrawQuery {
                vertices: vb_fg,
                texture: self.cache.texture(),
            });

            let vb_bg = glium::VertexBuffer::new(&self.display, &self.vertices_bg).unwrap();
            self.draw_queries_bg.push(DrawQuery {
                vertices: vb_bg,
                texture: self.cache.texture(),
            });
        }

        self.contents = current;

        false
    }

    pub fn draw(&mut self) {
        let elapsed = self.started_time.elapsed().as_millis() as f32;

        use glium::Surface as _;
        let mut surface = self.display.draw();

        let indices = index::NoIndices(index::PrimitiveType::TrianglesList);

        let iter_fg = self.draw_queries_fg.iter();
        let iter_bg = self.draw_queries_bg.iter();
        for query in iter_bg.chain(iter_fg) {
            let sampler = query
                .texture
                .sampled()
                .magnify_filter(uniforms::MagnifySamplerFilter::Linear)
                .minify_filter(uniforms::MinifySamplerFilter::Linear);
            let uniforms = uniform! { tex: sampler, timestamp: elapsed };

            surface
                .draw(
                    &query.vertices,
                    indices,
                    &self.program_cell,
                    &uniforms,
                    &glium::DrawParameters {
                        blend: glium::Blend::alpha_blending(),
                        ..glium::DrawParameters::default()
                    },
                )
                .expect("draw cells");
        }

        for query in self.draw_queries_img.iter() {
            let sampler = query
                .texture
                .sampled()
                .magnify_filter(uniforms::MagnifySamplerFilter::Linear)
                .minify_filter(uniforms::MinifySamplerFilter::Linear);
            let uniforms = uniform! { tex: sampler };

            surface
                .draw(
                    &query.vertices,
                    indices,
                    &self.program_img,
                    &uniforms,
                    &glium::DrawParameters::default(),
                )
                .expect("draw image");
        }

        surface.finish().expect("finish");
    }

    fn resize_window(&mut self, new_size: WindowSize) {
        log::debug!(
            "window resized: {}x{} (px)",
            new_size.width,
            new_size.height
        );
        self.window_size = new_size;
        self.resize_buffer();
    }

    fn increase_font_size(&mut self, size_diff: i32) {
        log::debug!("increase font size: {} (diff)", size_diff);
        self.fonts.increase_size(size_diff);

        let (new_cell_size, new_cell_max_over) = calculate_cell_size(&self.fonts);
        self.cell_size = new_cell_size;
        self.cell_max_over = new_cell_max_over;

        self.cache = GlyphCache::build_ascii_visible(&self.display, &self.fonts, self.cell_size);

        self.resize_buffer();
    }

    fn resize_buffer(&mut self) {
        self.mouse.pressed_pos = None;
        self.mouse.released_pos = None;

        let rows = (self.window_size.height / self.cell_size.h) as usize;
        let cols = (self.window_size.width / self.cell_size.w) as usize;
        let buff_size = TerminalSize { rows, cols };
        self.terminal.request_resize(buff_size, self.cell_size);
    }

    fn copy_clipboard(&mut self) {
        let mut text = String::new();

        let window_width = self.window_size.width;
        let cell_size = self.cell_size;

        for (i, row) in self.contents.lines.iter().enumerate() {
            let mut x = 0;
            for cell in row.iter() {
                let cell_width_px = cell_size.w * cell.width as u32;

                let is_selected = match self.contents.selection_range {
                    Some((left, right)) => {
                        let offset = (i as u32 * window_width + x) as f64;
                        let mid_point = offset + (cell_width_px as f64) / 2.0;
                        left <= mid_point && mid_point <= right
                    }
                    None => false,
                };

                if is_selected {
                    text.push(cell.ch);
                }

                x += cell_width_px;
            }
        }

        log::info!("copy: {:?}", text);
        let _ = self.clipboard.store(&text);
    }

    fn paste_clipboard(&mut self) {
        match self.clipboard.load() {
            Ok(text) => {
                log::debug!("paste: {:?}", text);
                if self.bracketed_paste_mode {
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

    fn pty_write_char_utf8(&mut self, ch: char) {
        let mut buf = [0_u8; 4];
        let utf8 = ch.encode_utf8(&mut buf).as_bytes();
        self.terminal.pty_write(utf8);
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
                let mut buf = self.terminal.buffer.lock().unwrap();
                buf.clear_history();
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
            self.mouse.pressed_pos = None;
            self.mouse.released_pos = None;
            self.history_head = 0;
        }
    }

    pub fn on_event(&mut self, event: &Event<()>, control_flow: &mut ControlFlow) {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }

                &WindowEvent::Resized(new_size) => {
                    self.resize_window(new_size);
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

                    self.pty_write_char_utf8(ch);
                }

                WindowEvent::KeyboardInput { input, .. }
                    if input.state == ElementState::Pressed =>
                {
                    if let Some(key) = input.virtual_keycode {
                        self.on_key_press(key);
                    }
                }

                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse.cursor_pos = (position.x, position.y);
                }

                WindowEvent::MouseInput { state, button, .. } => {
                    if self.mouse_track_mode {
                        let button = match state {
                            ElementState::Released if !self.sgr_ext_mouse_track_mode => 3,
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

                        #[cfg_attr(rustfmt, rustfmt_skip)]
                        let mods =
                            if self.modifiers.shift() { 0b00000100 } else { 0 }
                        |   if self.modifiers.alt()   { 0b00001000 } else { 0 }
                        |   if self.modifiers.ctrl()  { 0b00010000 } else { 0 };

                        let pos = self.mouse.cursor_pos;
                        let col = pos.0.round() as u32 / self.cell_size.w + 1;
                        let row = pos.1.round() as u32 / self.cell_size.h + 1;

                        if self.sgr_ext_mouse_track_mode {
                            self.sgr_ext_mouse_report(button + mods, col, row, state);
                        } else {
                            self.normal_mouse_report(button + mods, col, row);
                        }
                    } else {
                        match state {
                            ElementState::Pressed => {
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
                        self.history_head = std::cmp::min(self.history_head - vertical, 0);
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
                if self.update() {
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                self.draw();
            }

            _ => {}
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

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
    color: [u32; 2],
    is_bg: u32,
    blinking: u32,
}
glium::implement_vertex!(Vertex, position, tex_coords, color, is_bg, blinking);

// Converts window coordinate to opengl coordinate
fn x_to_gl(x: i32, window_width: u32) -> f32 {
    (x as f32 / window_width as f32) * 2.0 - 1.0
}
fn y_to_gl(y: i32, window_height: u32) -> f32 {
    -(y as f32 / window_height as f32) * 2.0 + 1.0
}
fn w_to_gl(w: u32, window_width: u32) -> f32 {
    (w as f32 / window_width as f32) * 2.0
}
fn h_to_gl(h: u32, window_height: u32) -> f32 {
    (h as f32 / window_height as f32) * 2.0
}

fn color_to_rgba(color: Color) -> u32 {
    let config = &crate::TOYTERM_CONFIG;

    match color {
        Color::Rgb { rgba } => rgba,
        Color::Special => 0xFFFFFF00,

        Color::Black => config.color_black,
        Color::Red => config.color_red,
        Color::Green => config.color_green,
        Color::Yellow => config.color_yellow,
        Color::Blue => config.color_blue,
        Color::Magenta => config.color_magenta,
        Color::Cyan => config.color_cyan,
        Color::White => config.color_white,

        Color::BrightBlack => config.color_bright_black,
        Color::BrightRed => config.color_bright_red,
        Color::BrightGreen => config.color_bright_green,
        Color::BrightYellow => config.color_bright_yellow,
        Color::BrightBlue => config.color_bright_blue,
        Color::BrightMagenta => config.color_bright_magenta,
        Color::BrightCyan => config.color_bright_cyan,
        Color::BrightWhite => config.color_bright_white,
    }
}

/// Generate vertices for a single glyph image
fn glyph_vertices(
    gl_x: f32,
    gl_y: f32,
    gl_w: f32,
    gl_h: f32,
    tx_x: f32,
    tx_y: f32,
    tx_w: f32,
    tx_h: f32,
    fg_color: Color,
    bg_color: Color,
    blinking: u8,
) -> [Vertex; 6] {
    // top-left, bottom-left, bottom-right, top-right
    let gl_ps = [
        [gl_x, gl_y],
        [gl_x, gl_y - gl_h],
        [gl_x + gl_w, gl_y - gl_h],
        [gl_x + gl_w, gl_y],
    ];

    // top-left, bottom-left, bottom-right, top-right
    let tex_ps = [
        [tx_x, tx_y],
        [tx_x, tx_y + tx_h],
        [tx_x + tx_w, tx_y + tx_h],
        [tx_x + tx_w, tx_y],
    ];

    let v = |idx| Vertex {
        position: gl_ps[idx],
        tex_coords: tex_ps[idx],
        color: [color_to_rgba(bg_color), color_to_rgba(fg_color)],
        is_bg: 0,
        blinking: blinking as u32,
    };

    // 0    3
    // *----*
    // |\  B|
    // | \  |
    // |  \ |
    // |A  \|
    // *----*
    // 1    2

    [
        // A
        v(0),
        v(1),
        v(2),
        // B
        v(2),
        v(3),
        v(0),
    ]
}

/// Generate vertices for a single cell (background)
fn cell_vertices(
    gl_x: f32,
    gl_y: f32,
    gl_w: f32,
    gl_h: f32,
    fg_color: Color,
    bg_color: Color,
) -> [Vertex; 6] {
    // top-left, bottom-left, bottom-right, top-right
    let gl_ps = [
        [gl_x, gl_y],
        [gl_x, gl_y - gl_h],
        [gl_x + gl_w, gl_y - gl_h],
        [gl_x + gl_w, gl_y],
    ];

    let v = |idx| Vertex {
        position: gl_ps[idx],
        tex_coords: [0.0, 0.0],
        color: [color_to_rgba(bg_color), color_to_rgba(fg_color)],
        is_bg: 1,
        blinking: 0,
    };

    // 0    3
    // *----*
    // |\  B|
    // | \  |
    // |  \ |
    // |A  \|
    // *----*
    // 1    2

    [
        // A
        v(0),
        v(1),
        v(2),
        // B
        v(2),
        v(3),
        v(0),
    ]
}

#[derive(Clone, Copy)]
struct SimpleVertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}
glium::implement_vertex!(SimpleVertex, position, tex_coords);

/// Generate vertices for a single sixel image
fn image_vertices(gl_x: f32, gl_y: f32, gl_w: f32, gl_h: f32) -> [SimpleVertex; 6] {
    let gl_ps = [
        [gl_x, gl_y],
        [gl_x, gl_y - gl_h],
        [gl_x + gl_w, gl_y - gl_h],
        [gl_x + gl_w, gl_y],
    ];
    let tex_ps = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];

    let v = |idx| SimpleVertex {
        position: gl_ps[idx],
        tex_coords: tex_ps[idx],
    };

    [v(0), v(1), v(2), v(2), v(3), v(0)]
}
