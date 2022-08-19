use glium::{glutin, index, texture, uniform, uniforms, Display};
use glutin::dpi::{PhysicalPosition, PhysicalSize};
use serde::{Deserialize, Serialize};
use std::cmp::max;
use std::rc::Rc;

use crate::cache::{GlyphCache, GlyphRegion};
use crate::font::{Font, FontSet, FontStyle};
use crate::terminal::{CellSize, Color, Cursor, CursorStyle, Line, PositionedImage};

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

pub struct TerminalView {
    fonts: FontSet,
    cache: GlyphCache,
    viewport: Viewport,
    cell_size: CellSize,
    cell_max_over: i32,

    pub lines: Vec<Line>,
    pub images: Vec<PositionedImage>,
    pub cursor: Option<Cursor>,
    pub selection_range: Option<(usize, usize)>,
    pub scroll_bar: Option<(u32, u32)>,
    pub bg_color: Color,
    pub view_focused: bool,
    updated: bool,

    display: Display,
    draw_params: glium::DrawParameters<'static>,
    program_cell: glium::Program,
    program_img: glium::Program,
    vertices_fg: Vec<CellVertex>,
    vertices_bg: Vec<CellVertex>,
    draw_queries_fg: Vec<DrawQuery<CellVertex>>,
    draw_queries_bg: Vec<DrawQuery<CellVertex>>,
    draw_queries_img: Vec<DrawQuery<ImageVertex>>,
    clock: std::time::Instant,
}

struct DrawQuery<V: glium::vertex::Vertex> {
    vertices: glium::VertexBuffer<V>,
    texture: Rc<texture::Texture2d>,
}

impl TerminalView {
    pub fn with_viewport(
        display: Display,
        viewport: Viewport,
        font_size: u32,
        scroll_bar: Option<(u32, u32)>,
    ) -> Self {
        let fonts = build_font_set(font_size);

        let (cell_size, cell_max_over) = calculate_cell_size(&fonts);

        // Rasterize ASCII characters and cache them as a texture
        let cache = GlyphCache::build_ascii_visible(&display, &fonts, cell_size);

        let draw_params = glium::DrawParameters {
            blend: glium::Blend::alpha_blending(),
            viewport: {
                let inner_size = display.gl_window().window().inner_size();
                Some(viewport.to_glium_rect(inner_size))
            },
            ..glium::DrawParameters::default()
        };

        fn new_program(display: &Display, vert: &str, frag: &str) -> glium::Program {
            use glium::program::{Program, ProgramCreationInput};
            Program::new(
                display,
                ProgramCreationInput::SourceCode {
                    vertex_shader: vert,
                    fragment_shader: frag,
                    geometry_shader: None,
                    tessellation_control_shader: None,
                    tessellation_evaluation_shader: None,
                    transform_feedback_varyings: None,
                    outputs_srgb: true,
                    uses_point_size: false,
                },
            )
            .unwrap()
        }

        let program_cell = new_program(
            &display,
            include_str!("shaders/cell.vert"),
            include_str!("shaders/cell.frag"),
        );

        let program_img = new_program(
            &display,
            include_str!("shaders/image.vert"),
            include_str!("shaders/image.frag"),
        );

        TerminalView {
            fonts,
            cache,

            viewport,
            cell_size,
            cell_max_over,

            lines: Vec::new(),
            images: Vec::new(),
            cursor: None,
            selection_range: None,
            scroll_bar,
            bg_color: Color::Black,
            view_focused: false,
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
            clock: std::time::Instant::now(),
        }
    }

    pub fn update_contents<F>(&mut self, callback: F)
    where
        F: FnOnce(&mut Self),
    {
        callback(self);
        self.updated = true;
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn set_viewport(&mut self, new_viewport: Viewport) {
        log::debug!("viewport changed: {:?}", new_viewport);
        self.viewport = new_viewport;

        let inner_size = self.display.gl_window().window().inner_size();
        self.draw_params.viewport = Some(self.viewport.to_glium_rect(inner_size));

        self.updated = true;
    }

    pub fn cell_size(&self) -> CellSize {
        self.cell_size
    }

    pub fn increase_font_size(&mut self, size_diff: i32) {
        log::debug!("increase font size: {} (diff)", size_diff);

        {
            let size = self.fonts.fontsize();
            let new_size = (size as i32 + size_diff).max(1) as u32;
            self.fonts.set_fontsize(new_size);
        }

        let (new_cell_size, new_cell_max_over) = calculate_cell_size(&self.fonts);
        self.cell_size = new_cell_size;
        self.cell_max_over = new_cell_max_over;

        self.cache = GlyphCache::build_ascii_visible(&self.display, &self.fonts, self.cell_size);

        self.updated = true;
    }

    fn rebuild_draw_queries(&mut self) {
        let viewport = self.viewport;
        let cell_size = self.cell_size;

        self.draw_queries_img.clear();
        for img in self.images.iter() {
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
            let vs = rect_vertices(rect, fg, bg);
            self.vertices_bg.extend_from_slice(&vs);
        }

        // scroll bar
        if let Some((sb_origin, sb_length)) = self.scroll_bar {
            let config = &crate::TOYTERM_CONFIG;
            if config.scroll_bar_width > 0 {
                let sb_width = config.scroll_bar_width;

                let mut rect = PixelRect {
                    x: viewport.w.saturating_sub(sb_width) as i32,
                    y: 0,
                    w: sb_width,
                    h: viewport.h,
                };
                let fg = Color::White;
                let bg = Color::Rgb {
                    rgba: config.scroll_bar_bg_color,
                };
                let vs = rect_vertices(rect.to_gl(viewport), fg, bg);
                self.vertices_bg.extend_from_slice(&vs);

                rect.y = sb_origin as i32;
                rect.h = sb_length;
                let fg = Color::White;
                let bg = Color::Rgb {
                    rgba: config.scroll_bar_fg_color,
                };
                let vs = rect_vertices(rect.to_gl(viewport), fg, bg);
                self.vertices_bg.extend_from_slice(&vs);
            }
        }

        let mut baseline: u32 = self.cell_max_over as u32;
        for (i, row) in self.lines.iter().enumerate() {
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

                    let on_cursor = if let Some(cursor) = self.cursor {
                        self.view_focused
                            && cursor.style == CursorStyle::Block
                            && i == cursor.row
                            && j == cursor.col
                    } else {
                        false
                    };

                    let is_selected = match self.selection_range {
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

                    let vs = rect_vertices(rect.to_gl(viewport), fg, bg);
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

        if let Some(cursor) = self.cursor {
            if self.view_focused
                && matches!(cursor.style, CursorStyle::Underline | CursorStyle::Bar)
            {
                let rect = if cursor.style == CursorStyle::Underline {
                    PixelRect {
                        x: cursor.col as i32 * cell_size.w as i32,
                        y: (cursor.row + 1) as i32 * cell_size.h as i32 - 4,
                        w: cell_size.w,
                        h: 4,
                    }
                } else {
                    PixelRect {
                        x: cursor.col as i32 * cell_size.w as i32,
                        y: cursor.row as i32 * cell_size.h as i32,
                        w: 4,
                        h: cell_size.h,
                    }
                };

                let fg = Color::Black;
                let bg = Color::White;
                let vs = rect_vertices(rect.to_gl(viewport), fg, bg);
                self.vertices_fg.extend_from_slice(&vs);
            }
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
            self.rebuild_draw_queries();
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

fn build_font_set(font_size: u32) -> FontSet {
    let config = &crate::TOYTERM_CONFIG;

    let mut fonts = FontSet::new(font_size);

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
                let font = Font::new(&data, face_idx);
                fonts.add(style, font);
            }

            Err(e) => {
                log::warn!("ignore {:?} (reason: {:?})", path.display(), e);
            }
        }
    }

    // Add embedded fonts
    {
        let regular_font = Font::new(include_bytes!("../fonts/Mplus1Code-Regular.ttf"), 0);
        fonts.add(FontStyle::Regular, regular_font);

        let bold_font = Font::new(include_bytes!("../fonts/Mplus1Code-SemiBold.ttf"), 0);
        fonts.add(FontStyle::Bold, bold_font);

        let faint_font = Font::new(include_bytes!("../fonts/Mplus1Code-Thin.ttf"), 0);
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

/// Generate vertices for a rectangle
fn rect_vertices(gl_rect: GlRect, fg_color: Color, bg_color: Color) -> [CellVertex; 6] {
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
