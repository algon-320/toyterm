use glium::{glutin, index, texture, uniform, uniforms, Display};
use glutin::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, ModifiersState, MouseButton, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
};
use serde::{Deserialize, Serialize};
use std::cmp::{max, min};
use std::rc::Rc;

use crate::cache::{GlyphCache, GlyphRegion};
use crate::font::{Font, FontSet, FontStyle};
use crate::terminal::{
    CellSize, Color, CursorStyle, Line, Mode, PositionedImage, Terminal, TerminalSize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Viewport {
    #[allow(unused)]
    pub fn contains(&self, p: PhysicalPosition<f64>) -> bool {
        let l = self.x as f64;
        let r = (self.x + self.w) as f64;
        let t = self.y as f64;
        let b = (self.y + self.h) as f64;
        l <= p.x && p.x < r && t <= p.y && p.y < b
    }

    fn to_glium_rect(self, inner_size: PhysicalSize<u32>) -> glium::Rect {
        let bottom = inner_size.height as i64 - (self.y + self.h) as i64;
        glium::Rect {
            left: self.x,
            bottom: max(bottom, 0) as u32,
            width: self.w,
            height: self.h,
        }
    }
}

#[derive(Default)]
pub struct Contents {
    pub lines: Vec<Line>,
    pub images: Vec<PositionedImage>,
    pub cursor: Option<(usize, usize, CursorStyle, bool)>,
    pub selection_range: Option<(usize, usize)>,
}

pub struct TerminalView {
    viewport: Viewport,
    fonts: FontSet,
    cache: GlyphCache,
    pub cell_size: CellSize,
    cell_max_over: i32,
    clock: std::time::Instant,
    pub bg_color: Color,
    pub contents: Contents,
    pub updated: bool,

    display: Display,
    draw_params: glium::DrawParameters<'static>,
    program_cell: glium::Program,
    program_img: glium::Program,
    vertices_fg: Vec<CellVertex>,
    vertices_bg: Vec<CellVertex>,
    draw_queries_fg: Vec<DrawQuery<CellVertex>>,
    draw_queries_bg: Vec<DrawQuery<CellVertex>>,
    draw_queries_img: Vec<DrawQuery<ImageVertex>>,
}

struct DrawQuery<V: glium::vertex::Vertex> {
    vertices: glium::VertexBuffer<V>,
    texture: Rc<texture::Texture2d>,
}

impl TerminalView {
    pub fn with_viewport(display: Display, viewport: Viewport) -> Self {
        let fonts = build_font_set();

        let (cell_size, cell_max_over) = calculate_cell_size(&fonts);

        // Rasterize ASCII characters and cache them as a texture
        let cache = GlyphCache::build_ascii_visible(&display, &fonts, cell_size);

        let inner_size = display.gl_window().window().inner_size();

        let draw_params = glium::DrawParameters {
            blend: glium::Blend::alpha_blending(),
            viewport: Some(viewport.to_glium_rect(inner_size)),
            ..glium::DrawParameters::default()
        };

        // Initialize shaders
        let program_cell = {
            use glium::program::{Program, ProgramCreationInput};
            let input = ProgramCreationInput::SourceCode {
                vertex_shader: include_str!("shaders/cell.vert"),
                fragment_shader: include_str!("shaders/cell.frag"),
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
                vertex_shader: include_str!("shaders/image.vert"),
                fragment_shader: include_str!("shaders/image.frag"),
                geometry_shader: None,
                tessellation_control_shader: None,
                tessellation_evaluation_shader: None,
                transform_feedback_varyings: None,
                outputs_srgb: true,
                uses_point_size: false,
            };
            Program::new(&display, input).unwrap()
        };

        TerminalView {
            viewport,
            fonts,
            cache,
            cell_size,
            cell_max_over,
            bg_color: Color::Black,
            clock: std::time::Instant::now(),
            contents: Contents::default(),
            updated: false,

            display,
            draw_params,
            program_cell,
            program_img,
            vertices_fg: Vec::new(),
            vertices_bg: Vec::new(),
            draw_queries_fg: Vec::new(),
            draw_queries_bg: Vec::new(),
            draw_queries_img: Vec::new(),
        }
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn set_viewport(&mut self, new_viewport: Viewport) {
        log::debug!("viewport changed: {:?}", new_viewport);
        self.viewport = new_viewport;

        let inner_size = self.display.gl_window().window().inner_size();
        self.draw_params.viewport = Some(new_viewport.to_glium_rect(inner_size));

        self.updated = true;
    }

    pub fn increase_font_size(&mut self, size_diff: i32) {
        log::debug!("increase font size: {} (diff)", size_diff);
        self.fonts.increase_size(size_diff);

        let (new_cell_size, new_cell_max_over) = calculate_cell_size(&self.fonts);
        self.cell_size = new_cell_size;
        self.cell_max_over = new_cell_max_over;

        self.cache = GlyphCache::build_ascii_visible(&self.display, &self.fonts, self.cell_size);

        self.updated = true;
    }

    fn update(&mut self) {
        let viewport = self.viewport();
        let cell_size = self.cell_size;

        self.draw_queries_img.clear();
        for img in self.contents.images.iter() {
            let col = img.col;
            let row = img.row;

            let image_rect = PixelRect {
                x: col as i32 * cell_size.w as i32,
                y: row as i32 * cell_size.h as i32,
                w: img.width as u32,
                h: img.height as u32,
            };
            let vs = image_vertices(image_rect.to_gl(viewport));

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

        self.vertices_fg.clear();
        self.vertices_bg.clear();
        self.draw_queries_fg.clear();
        self.draw_queries_bg.clear();

        // clear entire screen
        {
            let rect = GlRect {
                x: -1.0,
                y: 1.0,
                w: 2.0,
                h: 2.0,
            };
            let fg = Color::White;
            let bg = self.bg_color;
            let vs = cell_vertices(rect, fg, bg);
            self.vertices_bg.extend_from_slice(&vs);
        }

        let mut baseline: u32 = self.cell_max_over as u32;
        for (i, row) in self.contents.lines.iter().enumerate() {
            let cols = row.columns();
            let mut leftline: u32 = 0;
            for (j, cell) in row.iter().enumerate() {
                if cell.width == 0 {
                    continue;
                }

                let cell_width_px = cell_size.w * cell.width as u32;

                let style = if cell.attr.bold == -1 {
                    FontStyle::Faint
                } else if cell.attr.bold == 0 {
                    FontStyle::Regular
                } else {
                    FontStyle::Bold
                };

                let (fg, bg) = {
                    let is_inversed = cell.attr.inversed;

                    let on_cursor =
                        if let Some((row, col, CursorStyle::Block, true)) = self.contents.cursor {
                            i == row && j == col
                        } else {
                            false
                        };

                    let is_selected = match self.contents.selection_range {
                        Some((left, right)) => {
                            let offset = i * cols + j;
                            let center = offset + (cell.width / 2) as usize;
                            left <= center && center <= right
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

                let blinking = cell.attr.blinking;

                // Background
                {
                    let rect = PixelRect {
                        x: (j as u32 * cell_size.w) as i32,
                        y: (i as u32 * cell_size.h) as i32,
                        w: cell_width_px,
                        h: cell_size.h,
                    };

                    let vs = cell_vertices(rect.to_gl(viewport), fg, bg);
                    self.vertices_bg.extend_from_slice(&vs);
                }

                if let Some((region, metrics)) = self.cache.get(cell.ch, style) {
                    if !region.is_empty() {
                        let bearing_x = (metrics.horiBearingX >> 6) as u32;
                        let bearing_y = (metrics.horiBearingY >> 6) as u32;

                        let rect = PixelRect {
                            x: leftline as i32 + bearing_x as i32,
                            y: baseline as i32 - bearing_y as i32,
                            w: region.px_w,
                            h: region.px_h,
                        };

                        let vs = glyph_vertices(rect.to_gl(viewport), region, fg, bg, blinking);
                        self.vertices_fg.extend_from_slice(&vs);
                    }
                } else if let Some((glyph_image, metrics)) = self.fonts.render(cell.ch, style) {
                    // for non-ASCII characters
                    if !glyph_image.data.is_empty() {
                        let bearing_x = (metrics.horiBearingX >> 6) as u32;
                        let bearing_y = (metrics.horiBearingY >> 6) as u32;

                        let rect = PixelRect {
                            x: leftline as i32 + bearing_x as i32,
                            y: baseline as i32 - bearing_y as i32,
                            w: glyph_image.width,
                            h: glyph_image.height,
                        };

                        let region = GlyphRegion {
                            px_w: glyph_image.width,
                            px_h: glyph_image.height,
                            tx_x: 0.0,
                            tx_y: 0.0,
                            tx_w: 1.0,
                            tx_h: 1.0,
                        };

                        let vs = glyph_vertices(rect.to_gl(viewport), region, fg, bg, blinking);

                        let vertex_buffer = glium::VertexBuffer::new(&self.display, &vs).unwrap();

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

        if let Some((row, col, CursorStyle::Bar, true)) = self.contents.cursor {
            let rect = PixelRect {
                x: ((col as u32) * cell_size.w) as i32,
                y: ((row as u32) * cell_size.h) as i32,
                w: 4,
                h: cell_size.h,
            };

            let fg = Color::Black;
            let bg = Color::White;

            let vs = cell_vertices(rect.to_gl(viewport), fg, bg);
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

        self.updated = false;
    }

    pub fn draw(&mut self, surface: &mut glium::Frame) {
        if self.updated {
            self.update();
        }

        let elapsed = self.clock.elapsed().as_millis() as f32;

        const TRIANGLES: index::NoIndices = index::NoIndices(index::PrimitiveType::TrianglesList);

        let iter_fg = self.draw_queries_fg.iter();
        let iter_bg = self.draw_queries_bg.iter();
        let iter_img = self.draw_queries_img.iter();

        use glium::Surface as _;

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
                    TRIANGLES,
                    &self.program_cell,
                    &uniforms,
                    &self.draw_params,
                )
                .expect("draw cells");
        }

        for query in iter_img {
            let sampler = query
                .texture
                .sampled()
                .magnify_filter(uniforms::MagnifySamplerFilter::Linear)
                .minify_filter(uniforms::MinifySamplerFilter::Linear);
            let uniforms = uniform! { tex: sampler };

            surface
                .draw(
                    &query.vertices,
                    TRIANGLES,
                    &self.program_img,
                    &uniforms,
                    &self.draw_params,
                )
                .expect("draw image");
        }
    }
}

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
        let view = TerminalView::with_viewport(display.clone(), viewport);

        let terminal = {
            let size = TerminalSize {
                rows: (viewport.h / view.cell_size.h) as usize,
                cols: (viewport.w / view.cell_size.w) as usize,
            };
            let cell_size = view.cell_size;
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
        let cell_size = self.view.cell_size;

        let contents_updated: bool;
        let mouse_track_mode_changed: bool;
        let terminal_size: TerminalSize;
        {
            // hold the lock while copying buffer states
            let mut buf = self.terminal.buffer.lock().unwrap();

            if buf.closed {
                return true;
            }

            mouse_track_mode_changed = self.mode.mouse_track != buf.mode.mouse_track;
            self.mode = buf.mode;

            if self.history_head < -(buf.history_size as isize) {
                self.history_head = -(buf.history_size as isize);
            }

            contents_updated = buf.updated || self.last_history_head != self.history_head;
            self.last_history_head = self.history_head;

            terminal_size = buf.size;

            if contents_updated {
                let contents = &mut self.view.contents;

                let mut lines = std::mem::take(&mut contents.lines);
                {
                    let top = self.history_head;
                    let bot = top + terminal_size.rows as isize;

                    if lines.len() == terminal_size.rows {
                        // Copy lines w/o heap allocation
                        for (src, dst) in buf.range(top, bot).zip(lines.iter_mut()) {
                            dst.copy_from(src);
                        }
                    } else {
                        // Copy lines w/ heap allocation
                        lines.clear();
                        lines.extend(buf.range(top, bot).cloned());
                    }
                }
                std::mem::swap(&mut contents.lines, &mut lines);

                contents.images = buf
                    .images
                    .iter()
                    .cloned()
                    .map(|mut img| {
                        img.row -= self.history_head;
                        img
                    })
                    .collect();

                if self.history_head >= 0 && buf.mode.cursor_visible {
                    let (row, col) = buf.cursor;
                    contents.cursor = Some((row, col, buf.cursor_style, self.focused));

                    self.display
                        .gl_window()
                        .window()
                        .set_ime_position(PhysicalPosition {
                            x: col as u32 * cell_size.w,
                            y: (row + 1) as u32 * cell_size.h,
                        });
                } else {
                    contents.cursor = None;
                }

                self.view.updated = true;
            }

            buf.updated = false;
        }

        if mouse_track_mode_changed {
            self.refresh_cursor_icon();
        }

        if let Some((sx, sy)) = self.mouse.pressed_pos {
            let (ex, ey) = self.mouse.released_pos.unwrap_or(self.mouse.cursor_pos);

            let lines = &self.view.contents.lines;

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
            e_col = max(e_col, 1) - 1;

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

            if self.view.contents.selection_range != new_selection_range {
                self.view.contents.selection_range = new_selection_range;
                self.view.updated = true;
            }
        } else if self.view.contents.selection_range.is_some() {
            self.view.contents.selection_range = None;
            self.view.updated = true;
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
        let rows = (viewport.h / self.view.cell_size.h) as usize;
        let cols = (viewport.w / self.view.cell_size.w) as usize;
        let buff_size = TerminalSize {
            rows: rows.max(1),
            cols: cols.max(1),
        };
        self.terminal.request_resize(buff_size, self.view.cell_size);
    }

    pub fn focus_changed(&mut self, gain: bool) {
        self.focused = gain;

        // Update cursor
        if let Some((_, _, _, focused)) = self.view.contents.cursor.as_mut() {
            *focused = self.focused;
            self.view.updated = true;
        }

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
                        let col = x.round() as u32 / self.view.cell_size.w + 1;
                        let row = y.round() as u32 / self.view.cell_size.h + 1;

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
                        self.history_head = min(self.history_head - vertical, 0);
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
                #[cfg(feature = "multiplex")]
                {
                    unreachable!();
                }

                #[cfg(not(feature = "multiplex"))]
                {
                    let mut surface = self.display.draw();
                    self.draw(&mut surface);
                    surface.finish().expect("finish");
                }
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
            self.view.contents.selection_range = None;
            self.view.updated = true;

            self.mouse.pressed_pos = None;
            self.mouse.released_pos = None;
            self.history_head = 0;
        }
    }

    fn copy_clipboard(&mut self) {
        let mut text = String::new();

        let selection_range = self.view.contents.selection_range;

        'row: for (i, row) in self.view.contents.lines.iter().enumerate() {
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

    #[cfg(feature = "multiplex")]
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

    #[cfg(feature = "multiplex")]
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

fn build_font_set() -> FontSet {
    let config = &crate::TOYTERM_CONFIG;

    let mut fonts = FontSet::empty();

    use std::iter::repeat;
    let regular_iter = repeat(FontStyle::Regular).zip(config.fonts_regular.iter());
    let bold_iter = repeat(FontStyle::Bold).zip(config.fonts_bold.iter());
    let faint_iter = repeat(FontStyle::Faint).zip(config.fonts_faint.iter());

    for (style, path) in regular_iter.chain(bold_iter).chain(faint_iter) {
        // FIXME
        if path.as_os_str().is_empty() {
            continue;
        }

        log::debug!("add {:?} font: {:?}", style, path.display());

        match std::fs::read(path) {
            Ok(data) => {
                // TODO: add config
                let face_idx = 0;
                let font = Font::new(&data, face_idx, config.font_size);
                fonts.add(style, font);
            }

            Err(e) => {
                log::warn!("ignore {:?} (reason: {:?})", path.display(), e);
            }
        }
    }

    // Add embedded fonts
    {
        let regular_font = Font::new(
            include_bytes!("../fonts/Mplus1Code-Regular.ttf"),
            0,
            config.font_size,
        );
        fonts.add(FontStyle::Regular, regular_font);

        let bold_font = Font::new(
            include_bytes!("../fonts/Mplus1Code-SemiBold.ttf"),
            0,
            config.font_size,
        );
        fonts.add(FontStyle::Bold, bold_font);

        let faint_font = Font::new(
            include_bytes!("../fonts/Mplus1Code-Thin.ttf"),
            0,
            config.font_size,
        );
        fonts.add(FontStyle::Faint, faint_font);
    }

    fonts
}

fn calculate_cell_size(fonts: &FontSet) -> (CellSize, i32) {
    let mut max_advance_x: i32 = 0;
    let mut max_over: i32 = 0;
    let mut max_under: i32 = 0;

    let ascii_visible = ' '..='~';
    for ch in ascii_visible {
        for style in FontStyle::all() {
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

#[derive(Clone, Copy)]
struct PixelRect {
    x: i32,
    y: i32,
    w: u32,
    h: u32,
}

#[derive(Clone, Copy)]
struct GlRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl PixelRect {
    fn to_gl(self, vp: Viewport) -> GlRect {
        GlRect {
            x: (self.x as f32 / vp.w as f32) * 2.0 - 1.0,
            y: -(self.y as f32 / vp.h as f32) * 2.0 + 1.0,
            w: (self.w as f32 / vp.w as f32) * 2.0,
            h: (self.h as f32 / vp.h as f32) * 2.0,
        }
    }
}

#[derive(Copy, Clone)]
struct CellVertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
    color: [u32; 2],
    is_bg: u32,
    blinking: u32,
}
glium::implement_vertex!(CellVertex, position, tex_coords, color, is_bg, blinking);

/// Generate vertices for a single glyph image
fn glyph_vertices(
    gl_rect: GlRect,
    region: GlyphRegion,
    fg_color: Color,
    bg_color: Color,
    blinking: u8,
) -> [CellVertex; 6] {
    // top-left, bottom-left, bottom-right, top-right
    let gl_ps = [
        [gl_rect.x, gl_rect.y],
        [gl_rect.x, gl_rect.y - gl_rect.h],
        [gl_rect.x + gl_rect.w, gl_rect.y - gl_rect.h],
        [gl_rect.x + gl_rect.w, gl_rect.y],
    ];
    let tx_ps = [
        [region.tx_x, region.tx_y],
        [region.tx_x, region.tx_y + region.tx_h],
        [region.tx_x + region.tx_w, region.tx_y + region.tx_h],
        [region.tx_x + region.tx_w, region.tx_y],
    ];

    let v = |idx| CellVertex {
        position: gl_ps[idx],
        tex_coords: tx_ps[idx],
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

    [/* A */ v(0), v(1), v(2), /* B */ v(2), v(3), v(0)]
}

/// Generate vertices for a single cell (background)
fn cell_vertices(gl_rect: GlRect, fg_color: Color, bg_color: Color) -> [CellVertex; 6] {
    let GlRect { x, y, w, h } = gl_rect;

    // top-left, bottom-left, bottom-right, top-right
    let gl_ps = [[x, y], [x, y - h], [x + w, y - h], [x + w, y]];

    let v = |idx| CellVertex {
        position: gl_ps[idx],
        tex_coords: [0.0, 0.0],
        color: [color_to_rgba(bg_color), color_to_rgba(fg_color)],
        is_bg: 1,
        blinking: 0,
    };

    [v(0), v(1), v(2), v(2), v(3), v(0)]
}

#[derive(Clone, Copy)]
struct ImageVertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
}
glium::implement_vertex!(ImageVertex, position, tex_coords);

/// Generate vertices for a single sixel image
fn image_vertices(gl_rect: GlRect) -> [ImageVertex; 6] {
    let GlRect { x, y, w, h } = gl_rect;

    // top-left, bottom-left, bottom-right, top-right
    let gl_ps = [[x, y], [x, y - h], [x + w, y - h], [x + w, y]];
    let tx_ps = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];

    let v = |idx| ImageVertex {
        position: gl_ps[idx],
        tex_coords: tx_ps[idx],
    };

    [v(0), v(1), v(2), v(2), v(3), v(0)]
}
