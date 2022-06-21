mod cache;
mod control_function;
mod font;
mod terminal;
mod utils;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use glium::{glutin, index, texture, uniform, uniforms, Display};
use glutin::{dpi::PhysicalSize, event_loop::EventLoop, window::WindowBuilder, ContextBuilder};

fn main() {
    // Setup env_logger
    let our_logs = concat!(module_path!(), "=debug");
    let env = env_logger::Env::default().default_filter_or(our_logs);
    env_logger::Builder::from_env(env)
        .format_timestamp(None)
        .init();

    let terminal = terminal::Terminal::new();
    let mut pty_writer = terminal.writer();

    let font = font::Font::new();

    // Calculate cell size
    let (cell_w, cell_h, max_over) = {
        let mut max_advance_x: i32 = 0;
        let mut max_over: i32 = 0;
        let mut max_under: i32 = 0;

        let ascii_visible = ' '..='~';
        for ch in ascii_visible {
            let metrics = font.metrics(ch).expect("undefined glyph");

            let advance_x = (metrics.horiAdvance >> 6) as i32;
            max_advance_x = std::cmp::max(max_advance_x, advance_x);

            let over = (metrics.horiBearingY >> 6) as i32;
            max_over = std::cmp::max(max_over, over);

            let under = ((metrics.height - metrics.horiBearingY) >> 6) as i32;
            max_under = std::cmp::max(max_under, under);
        }

        let cell_w = max_advance_x as u32;
        let cell_h = (max_over + max_under) as u32;

        (cell_w, cell_h, max_over)
    };

    // Initialize OpenGL
    let width = cell_w * 80;
    let height = cell_h * 24;
    let win_builder = WindowBuilder::new()
        .with_title("toyterm")
        .with_inner_size(PhysicalSize::new(width, height))
        .with_resizable(false);
    let ctx_builder = ContextBuilder::new().with_vsync(true).with_srgb(true);
    let event_loop = EventLoop::<u8>::with_user_event();
    let display = Display::new(win_builder, ctx_builder, &event_loop).expect("display new");

    // Render ASCII characters and cache them as a texture
    let cache = cache::GlyphCache::build_ascii_visible(&display, &font, cell_w, cell_h);

    // Initialize shaders
    const VERT_SHADER: &str = include_str!("cell.vert");
    const FRAG_SHADER: &str = include_str!("cell.frag");
    const GEOM_SHADER: Option<&str> = None;
    let program = {
        use glium::program::{Program, ProgramCreationInput};
        let input = ProgramCreationInput::SourceCode {
            vertex_shader: VERT_SHADER,
            fragment_shader: FRAG_SHADER,
            geometry_shader: GEOM_SHADER,
            tessellation_control_shader: None,
            tessellation_evaluation_shader: None,
            transform_feedback_varyings: None,
            outputs_srgb: true,
            uses_point_size: false,
        };
        Program::new(&display, input).unwrap()
    };

    let (window_width, window_height): (Arc<AtomicU32>, Arc<AtomicU32>) = {
        let initial_size = display.gl_window().window().inner_size();
        (
            Arc::new(AtomicU32::new(initial_size.width)),
            Arc::new(AtomicU32::new(initial_size.height)),
        )
    };

    let mut draw = {
        let started_time = std::time::Instant::now();
        let window_width = window_width.clone();
        let window_height = window_height.clone();
        let display = display.clone();
        let mut vertices = Vec::new();

        move || {
            let elapsed = started_time.elapsed().as_millis() as f32;

            let window_width = window_width.load(Ordering::Relaxed);
            let window_height = window_height.load(Ordering::Relaxed);

            vertices.clear();

            use glium::Surface as _;
            let mut surface = display.draw();

            // FIXME
            surface.clear_color_srgb(0.1137, 0.1254, 0.1294, 1.0);

            let lines: Vec<Vec<terminal::Cell>>;
            let cursor: (usize, usize);
            {
                // hold the lock during copying states
                let buf = terminal.buffer.lock().unwrap();
                let top = std::cmp::max(buf.lines.len() as isize - 24, 0) as usize;
                lines = buf.lines.range(top..).cloned().collect();
                let (row, col) = buf.cursor;
                cursor = (row - top, col);
            };

            let mut baseline: u32 = max_over as u32;
            let mut i = 0_u32;
            for row in lines.iter() {
                let mut leftline = 0;
                let mut j = 0_u32;
                for cell in row.iter() {
                    if cell.width == 0 {
                        continue;
                    }

                    if let Some(region) = cache.get(cell.ch) {
                        // Background
                        {
                            let gl_x = x_to_gl((j * cell_w) as i32, window_width);
                            let gl_y = y_to_gl((i * cell_h) as i32, window_height);
                            let gl_w = w_to_gl(cell_w * cell.width as u32, window_width);
                            let gl_h = h_to_gl(cell_h, window_height);

                            let mut fg = cell.attr.fg;
                            let mut bg = cell.attr.bg;

                            if cell.attr.inversed {
                                std::mem::swap(&mut fg, &mut bg);
                            }

                            if i == cursor.0 as u32 && j == cursor.1 as u32 {
                                std::mem::swap(&mut fg, &mut bg);
                            }

                            let vs = background_cell(gl_x, gl_y, gl_w, gl_h, fg, bg);
                            vertices.extend_from_slice(&vs);
                        }

                        if !region.is_empty() {
                            let metrics = font.metrics(cell.ch).expect("ASCII character");
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

                            let vs = foreground_cell(
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
                            vertices.extend_from_slice(&vs);
                        }
                    } else if let Some((glyph_image, metrics)) = font.render(cell.ch) {
                        let mut vertices = Vec::with_capacity(12);

                        // Background
                        {
                            let gl_x = x_to_gl((j * cell_w) as i32, window_width);
                            let gl_y = y_to_gl((i * cell_h) as i32, window_height);
                            let gl_w = w_to_gl(cell_w * cell.width as u32, window_width);
                            let gl_h = h_to_gl(cell_h, window_height);

                            let mut fg = cell.attr.fg;
                            let mut bg = cell.attr.bg;

                            if cell.attr.inversed {
                                std::mem::swap(&mut fg, &mut bg);
                            }

                            if i == cursor.0 as u32 && j == cursor.1 as u32 {
                                std::mem::swap(&mut fg, &mut bg);
                            }

                            let vs = background_cell(gl_x, gl_y, gl_w, gl_h, fg, bg);
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

                            let vs =
                                foreground_cell(gl_x, gl_y, gl_w, gl_h, 0.0, 0.0, 1.0, 1.0, fg, bg);
                            vertices.extend_from_slice(&vs);
                        }

                        let vertex_buffer = glium::VertexBuffer::new(&display, &vertices).unwrap();
                        let indices = index::NoIndices(index::PrimitiveType::TrianglesList);

                        let single_glyph_texture = texture::Texture2d::with_mipmaps(
                            &display,
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
                                &program,
                                &uniforms,
                                &glium::DrawParameters::default(),
                            )
                            .expect("draw");
                    } else {
                        log::trace!("undefined glyph: {:?}", cell.ch);
                    }

                    leftline += cell_w * (cell.width as u32);
                    j += cell.width as u32;
                }
                baseline += cell_h;
                i += 1;
            }

            let vertex_buffer = glium::VertexBuffer::new(&display, &vertices).unwrap();
            // Vertices ordering: 3 vertices for single triangle polygon
            let indices = index::NoIndices(index::PrimitiveType::TrianglesList);

            // Generate a sample from the texture
            let sampler = cache
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
                    &program,
                    &uniforms,
                    &glium::DrawParameters::default(),
                )
                .expect("draw");

            surface.finish().expect("finish");
        }
    };

    event_loop.run(move |event, _, control_flow| {
        use glutin::{
            event::{ElementState, Event, VirtualKeyCode, WindowEvent},
            event_loop::ControlFlow,
        };

        let mut write_pty = |bytes: &[u8]| {
            use std::io::Write;
            pty_writer.write_all(bytes).unwrap();
            pty_writer.flush().unwrap();
        };

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }

                WindowEvent::Resized(new_size) => {
                    window_width.store(new_size.width, Ordering::Relaxed);
                    window_height.store(new_size.height, Ordering::Relaxed);
                }

                WindowEvent::ReceivedCharacter(ch) => {
                    if ch.is_control() {
                        log::debug!("input: {:?}", ch);
                    }
                    let mut buf = [0_u8; 4];
                    let utf8 = ch.encode_utf8(&mut buf).as_bytes();
                    write_pty(utf8);
                }

                WindowEvent::KeyboardInput { input, .. }
                    if input.state == ElementState::Pressed =>
                {
                    match input.virtual_keycode {
                        Some(VirtualKeyCode::Up) => {
                            write_pty(b"\x1b[\x41");
                        }
                        Some(VirtualKeyCode::Down) => {
                            write_pty(b"\x1b[\x42");
                        }
                        Some(VirtualKeyCode::Right) => {
                            write_pty(b"\x1b[\x43");
                        }
                        Some(VirtualKeyCode::Left) => {
                            write_pty(b"\x1b[\x44");
                        }
                        _ => {}
                    }
                }

                _ => {}
            },

            Event::MainEventsCleared => {
                draw();
            }

            _ => {}
        }

        *control_flow = ControlFlow::Poll;
    });
}

#[derive(Copy, Clone)]
struct Vertex {
    position: [f32; 2],
    tex_coords: [f32; 2],
    color_idx: [u32; 2],
    is_bg: u32,
}
glium::implement_vertex!(Vertex, position, tex_coords, color_idx, is_bg);

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

// Generate vertices for a single cell
fn foreground_cell(
    gl_x: f32,
    gl_y: f32,
    gl_w: f32,
    gl_h: f32,
    tx_x: f32,
    tx_y: f32,
    tx_w: f32,
    tx_h: f32,
    fg_color: u8,
    bg_color: u8,
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
        color_idx: [bg_color as u32, fg_color as u32],
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

fn background_cell(
    gl_x: f32,
    gl_y: f32,
    gl_w: f32,
    gl_h: f32,
    fg_color: u8,
    bg_color: u8,
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
        color_idx: [bg_color as u32, fg_color as u32],
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
