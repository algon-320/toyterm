use glium::{glutin, index, texture, uniform, uniforms, Display};
use glutin::{
    dpi::PhysicalSize,
    event::{ElementState, Event, ModifiersState, VirtualKeyCode, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    ContextBuilder,
};

use crate::cache::GlyphCache;
use crate::font::Font;
use crate::terminal::{Cell, Color, Terminal};

#[derive(Debug, Clone, Copy)]
struct CellSize {
    w: u32,
    h: u32,
    max_over: i32,
}

fn calculate_cell_size(font: &Font) -> CellSize {
    use std::cmp::max;

    let mut max_advance_x: i32 = 0;
    let mut max_over: i32 = 0;
    let mut max_under: i32 = 0;

    let ascii_visible = ' '..='~';
    for ch in ascii_visible {
        let metrics = font.metrics(ch).expect("undefined glyph");

        let advance_x = (metrics.horiAdvance >> 6) as i32;
        max_advance_x = max(max_advance_x, advance_x);

        let over = (metrics.horiBearingY >> 6) as i32;
        max_over = max(max_over, over);

        let under = ((metrics.height - metrics.horiBearingY) >> 6) as i32;
        max_under = max(max_under, under);
    }

    let cell_w = max_advance_x as u32;
    let cell_h = (max_over + max_under) as u32;

    log::debug!("cell size: {}x{} (px)", cell_w, cell_h);

    CellSize {
        w: cell_w,
        h: cell_h,
        max_over,
    }
}

pub struct TerminalWindow {
    display: Display,
    program: glium::Program,
    vertices: Vec<Vertex>,
    modifiers: ModifiersState,

    terminal: Terminal,
    font: Font,
    cache: GlyphCache,

    window_width: u32,
    window_height: u32,
    cell_size: CellSize,
    started_time: std::time::Instant,
}

impl TerminalWindow {
    pub fn new(event_loop: &EventLoop<()>, lines: usize, columns: usize) -> Self {
        let terminal = Terminal::new(lines, columns);

        let font = Font::new();
        let cell_size = calculate_cell_size(&font);

        let width = columns as u32 * cell_size.w;
        let height = lines as u32 * cell_size.h;

        let win_builder = WindowBuilder::new()
            .with_title("toyterm")
            .with_inner_size(PhysicalSize::new(width, height))
            .with_resizable(true);
        let ctx_builder = ContextBuilder::new().with_vsync(true).with_srgb(true);
        let display = Display::new(win_builder, ctx_builder, event_loop).expect("display new");

        // Initialize shaders
        let program = {
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

        // Rasterize ASCII characters and cache them as a texture
        let cache = GlyphCache::build_ascii_visible(&display, &font, cell_size.w, cell_size.h);

        let initial_size = display.gl_window().window().inner_size();

        TerminalWindow {
            display,
            program,
            vertices: Vec::new(),
            modifiers: ModifiersState::empty(),

            terminal,
            font,
            cache,

            window_width: initial_size.width,
            window_height: initial_size.height,
            cell_size,
            started_time: std::time::Instant::now(),
        }
    }

    pub fn draw(&mut self) {
        let elapsed = self.started_time.elapsed().as_millis() as f32;
        let window_width = self.window_width;
        let window_height = self.window_height;
        let cell_size = self.cell_size;

        self.vertices.clear();

        use glium::Surface as _;
        let mut surface = self.display.draw();

        surface.clear_color_srgb(0.0, 0.0, 0.0, 1.0); // black

        let lines: Vec<Vec<Cell>>;
        let cursor: (usize, usize);
        {
            // hold the lock during copying states
            let buf = self.terminal.buffer.lock().unwrap();

            lines = buf.lines.iter().cloned().collect();
            cursor = buf.cursor;
        };

        let mut baseline: u32 = cell_size.max_over as u32;
        let mut i: u32 = 0;
        for row in lines.iter() {
            let mut leftline: u32 = 0;
            let mut j: u32 = 0;
            for cell in row.iter() {
                if cell.width == 0 {
                    continue;
                }

                if let Some(region) = self.cache.get(cell.ch) {
                    // Background
                    {
                        let gl_x = x_to_gl((j * cell_size.w) as i32, window_width);
                        let gl_y = y_to_gl((i * cell_size.h) as i32, window_height);
                        let gl_w = w_to_gl(cell_size.w * cell.width as u32, window_width);
                        let gl_h = h_to_gl(cell_size.h, window_height);

                        let mut fg = cell.attr.fg;
                        let mut bg = cell.attr.bg;

                        if cell.attr.inversed {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        if i == cursor.0 as u32 && j == cursor.1 as u32 {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        let vs = cell_vertices(gl_x, gl_y, gl_w, gl_h, fg, bg);
                        self.vertices.extend_from_slice(&vs);
                    }

                    if !region.is_empty() {
                        let metrics = self.font.metrics(cell.ch).expect("ASCII character");
                        let bearing_x = (metrics.horiBearingX >> 6) as u32;
                        let bearing_y = (metrics.horiBearingY >> 6) as u32;

                        let x = leftline as i32 + bearing_x as i32;
                        let y = baseline as i32 - bearing_y as i32;
                        let gl_x = x_to_gl(x, window_width);
                        let gl_y = y_to_gl(y, window_height);
                        let gl_w = w_to_gl(region.px_w, window_width);
                        let gl_h = h_to_gl(region.px_h, window_height);

                        let mut fg = cell.attr.fg;
                        let mut bg = cell.attr.bg;

                        if cell.attr.inversed {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        if i == cursor.0 as u32 && j == cursor.1 as u32 {
                            std::mem::swap(&mut fg, &mut bg);
                        }

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
                        );
                        self.vertices.extend_from_slice(&vs);
                    }
                } else if let Some((glyph_image, metrics)) = self.font.render(cell.ch) {
                    // FIXME
                    let mut vertices = Vec::with_capacity(12);

                    // Background
                    {
                        let gl_x = x_to_gl((j * cell_size.w) as i32, window_width);
                        let gl_y = y_to_gl((i * cell_size.h) as i32, window_height);
                        let gl_w = w_to_gl(cell_size.w * cell.width as u32, window_width);
                        let gl_h = h_to_gl(cell_size.h, window_height);

                        let mut fg = cell.attr.fg;
                        let mut bg = cell.attr.bg;

                        if cell.attr.inversed {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        if i == cursor.0 as u32 && j == cursor.1 as u32 {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        let vs = cell_vertices(gl_x, gl_y, gl_w, gl_h, fg, bg);
                        vertices.extend_from_slice(&vs);
                    }

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

                        let mut fg = cell.attr.fg;
                        let mut bg = cell.attr.bg;

                        if cell.attr.inversed {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        if i == cursor.0 as u32 && j == cursor.1 as u32 {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        let vs = glyph_vertices(gl_x, gl_y, gl_w, gl_h, 0.0, 0.0, 1.0, 1.0, fg, bg);
                        vertices.extend_from_slice(&vs);
                    }

                    let vertex_buffer = glium::VertexBuffer::new(&self.display, &vertices).unwrap();
                    let indices = index::NoIndices(index::PrimitiveType::TrianglesList);

                    let single_glyph_texture = texture::Texture2d::with_mipmaps(
                        &self.display,
                        glyph_image,
                        texture::MipmapsOption::NoMipmap,
                    )
                    .expect("Failed to create texture");

                    let sampler = single_glyph_texture
                        .sampled()
                        .magnify_filter(uniforms::MagnifySamplerFilter::Linear)
                        .minify_filter(uniforms::MinifySamplerFilter::Linear);
                    let uniforms = uniform! { tex: sampler, timestamp: elapsed };

                    surface
                        .draw(
                            &vertex_buffer,
                            indices,
                            &self.program,
                            &uniforms,
                            &glium::DrawParameters::default(),
                        )
                        .expect("draw");
                } else {
                    log::trace!("undefined glyph: {:?}", cell.ch);

                    // FIXME
                    let mut vertices = Vec::with_capacity(6);

                    // Background
                    {
                        let gl_x = x_to_gl((j * cell_size.w) as i32, window_width);
                        let gl_y = y_to_gl((i * cell_size.h) as i32, window_height);
                        let gl_w = w_to_gl(cell_size.w * cell.width as u32, window_width);
                        let gl_h = h_to_gl(cell_size.h, window_height);

                        let mut fg = cell.attr.fg;
                        let mut bg = cell.attr.bg;

                        if cell.attr.inversed {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        if i == cursor.0 as u32 && j == cursor.1 as u32 {
                            std::mem::swap(&mut fg, &mut bg);
                        }

                        let vs = cell_vertices(gl_x, gl_y, gl_w, gl_h, fg, bg);
                        vertices.extend_from_slice(&vs);
                    }

                    let vertex_buffer = glium::VertexBuffer::new(&self.display, &vertices).unwrap();
                    let indices = index::NoIndices(index::PrimitiveType::TrianglesList);
                    let uniforms = uniform! { timestamp: elapsed };
                    surface
                        .draw(
                            &vertex_buffer,
                            indices,
                            &self.program,
                            &uniforms,
                            &glium::DrawParameters::default(),
                        )
                        .expect("draw");
                }

                leftline += cell_size.w * (cell.width as u32);
                j += cell.width as u32;
            }
            baseline += cell_size.h;
            i += 1;
        }

        let vertex_buffer = glium::VertexBuffer::new(&self.display, &self.vertices).unwrap();
        // Vertices ordering: 3 vertices for single triangle polygon
        let indices = index::NoIndices(index::PrimitiveType::TrianglesList);

        // Generate a sampler from the texture
        let sampler = self
            .cache
            .texture()
            .sampled()
            .magnify_filter(uniforms::MagnifySamplerFilter::Linear)
            .minify_filter(uniforms::MinifySamplerFilter::Linear);
        let uniforms = uniform! { tex: sampler, timestamp: elapsed };

        // Perform drawing
        surface
            .draw(
                &vertex_buffer,
                indices,
                &self.program,
                &uniforms,
                &glium::DrawParameters::default(),
            )
            .expect("draw");

        surface.finish().expect("finish");
    }

    pub fn resize(&mut self, new_width: u32, new_height: u32) {
        log::debug!("window resized: {}x{} (px)", new_width, new_height);
        self.window_width = new_width;
        self.window_height = new_height;

        let lines = (new_height / self.cell_size.h) as usize;
        let columns = (new_width / self.cell_size.w) as usize;
        self.terminal.request_resize(lines, columns);
    }

    pub fn on_event(&mut self, event: Event<()>, control_flow: &mut ControlFlow) {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }

                WindowEvent::Resized(new_size) => {
                    self.resize(new_size.width, new_size.height);
                }

                WindowEvent::ModifiersChanged(new_states) => {
                    self.modifiers = new_states;
                }

                WindowEvent::ReceivedCharacter(ch) => {
                    // Handle these characters on WindowEvent::KeyboardInput event
                    if ch == '-' || ch == '=' {
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
                    match input.virtual_keycode {
                        Some(VirtualKeyCode::Minus) if self.modifiers.ctrl() => {
                            // font size -

                            self.font.decrease_size(1);
                            self.cell_size = calculate_cell_size(&self.font);
                            self.cache = GlyphCache::build_ascii_visible(
                                &self.display,
                                &self.font,
                                self.cell_size.w,
                                self.cell_size.h,
                            );

                            let lines = (self.window_height / self.cell_size.h) as usize;
                            let columns = (self.window_width / self.cell_size.w) as usize;
                            self.terminal.request_resize(lines, columns);
                        }
                        Some(VirtualKeyCode::Equals) if self.modifiers.ctrl() => {
                            // font size +

                            self.font.increase_size(1);
                            self.cell_size = calculate_cell_size(&self.font);
                            self.cache = GlyphCache::build_ascii_visible(
                                &self.display,
                                &self.font,
                                self.cell_size.w,
                                self.cell_size.h,
                            );

                            let lines = (self.window_height / self.cell_size.h) as usize;
                            let columns = (self.window_width / self.cell_size.w) as usize;
                            self.terminal.request_resize(lines, columns);
                        }

                        Some(VirtualKeyCode::Up) => {
                            self.terminal.pty_write(b"\x1b[\x41");
                        }
                        Some(VirtualKeyCode::Down) => {
                            self.terminal.pty_write(b"\x1b[\x42");
                        }
                        Some(VirtualKeyCode::Right) => {
                            self.terminal.pty_write(b"\x1b[\x43");
                        }
                        Some(VirtualKeyCode::Left) => {
                            self.terminal.pty_write(b"\x1b[\x44");
                        }

                        Some(VirtualKeyCode::Minus) => {
                            self.terminal.pty_write(b"-");
                        }
                        Some(VirtualKeyCode::Equals) => {
                            self.terminal.pty_write(b"=");
                        }

                        _ => {}
                    }
                }

                _ => {}
            },

            Event::MainEventsCleared => {
                self.draw();
            }

            _ => {}
        }
    }
}

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
    color: [u32; 2],
    is_bg: u32,
}
glium::implement_vertex!(Vertex, position, tex_coords, color, is_bg);

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
    match color {
        // Base16 Gruvbox dark hard
        // Dawid Kurek (dawikur@gmail.com), morhetz (https://github.com/morhetz/gruvbox)
        Color::Black => 0x1d2021ff,
        Color::Red => 0xfb4934ff,
        Color::Yellow => 0xb8bb26ff,
        Color::Green => 0xfabd2fff,
        Color::Blue => 0x83a598ff,
        Color::Magenta => 0xd3869bff,
        Color::Cyan => 0x8ec07cff,
        Color::White => 0xd5c4a1ff,

        Color::Rgb { r, g, b } => {
            let r = (r as u32) << 24;
            let g = (g as u32) << 16;
            let b = (b as u32) << 8;
            let a = 0xFF;
            r | g | b | a
        }
        Color::Special => 0xFFFFFF00,
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
