use glium::{glutin, Display};
use glutin::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
};
use std::borrow::Cow;

use crate::terminal::{Cell, Color, Line};
use crate::window::{TerminalView, TerminalWindow};

const PREFIX_KEY: char = '\x01'; // Ctrl + A
const VSPLIT: char = '"';
const HSPLIT: char = '%';

type Event = glutin::event::Event<'static, ()>;
type CursorPosition = PhysicalPosition<f64>;
type Viewport = glium::Rect;

enum Layout {
    Single(Box<TerminalWindow>),
    Binary(BinLayout),
}

struct BinLayout {
    split: Split,
    viewport: Viewport,
    focus_x: bool,
    x: Option<Box<Layout>>,
    y: Option<Box<Layout>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Split {
    Horizontal,
    Vertical,
}

impl BinLayout {
    fn x_mut(&mut self) -> &mut Layout {
        self.x.as_mut().unwrap()
    }
    fn y_mut(&mut self) -> &mut Layout {
        self.y.as_mut().unwrap()
    }
    fn focused_mut(&mut self) -> &mut Layout {
        if self.focus_x {
            self.x_mut()
        } else {
            self.y_mut()
        }
    }

    fn split_viewport(&self) -> (Viewport, Viewport) {
        let viewport = self.viewport;

        match self.split {
            Split::Vertical => {
                let u_height = viewport.height / 2;
                let d_height = viewport.height - u_height;

                let up = Viewport {
                    left: viewport.left,
                    bottom: viewport.bottom + d_height,
                    width: viewport.width,
                    height: u_height,
                };
                let down = Viewport {
                    left: viewport.left,
                    bottom: viewport.bottom,
                    width: viewport.width,
                    height: d_height - 1,
                };

                (up, down)
            }

            Split::Horizontal => {
                let l_width = viewport.width / 2;
                let r_width = viewport.width - l_width;

                let left = Viewport {
                    left: viewport.left,
                    bottom: viewport.bottom,
                    width: l_width - 1,
                    height: viewport.height,
                };
                let right = Viewport {
                    left: viewport.left + l_width,
                    bottom: viewport.bottom,
                    width: r_width,
                    height: viewport.height,
                };

                (left, right)
            }
        }
    }

    fn split_event<'e>(
        &mut self,
        event: &'e Event,
    ) -> (Option<Cow<'e, Event>>, Option<Cow<'e, Event>>) {
        match event {
            Event::WindowEvent {
                event: wev,
                window_id,
            } => match wev {
                WindowEvent::ModifiersChanged(..) => {
                    return (Some(Cow::Borrowed(event)), Some(Cow::Borrowed(event)));
                }

                #[allow(deprecated)]
                &WindowEvent::CursorMoved {
                    device_id,
                    position,
                    modifiers,
                } => {
                    let (vp_x, vp_y) = self.split_viewport();
                    let (mut ev_x, mut ev_y) = (None, None);

                    if self.contains(vp_x, position) {
                        ev_x = Some(Cow::Borrowed(event));
                    }

                    if self.contains(vp_y, position) {
                        let mut modified_pos = position;
                        match self.split {
                            Split::Vertical => {
                                modified_pos.y -= vp_x.height as f64;
                            }
                            Split::Horizontal => {
                                modified_pos.x -= vp_x.width as f64;
                            }
                        }

                        let modified = Event::WindowEvent {
                            event: WindowEvent::CursorMoved {
                                device_id,
                                position: modified_pos,
                                modifiers,
                            },
                            window_id: *window_id,
                        };

                        ev_y = Some(Cow::Owned(modified));
                    }

                    return (ev_x, ev_y);
                }

                _ => {}
            },

            Event::MainEventsCleared => {
                return (Some(Cow::Borrowed(event)), Some(Cow::Borrowed(event)));
            }

            _ => {}
        }

        if self.focus_x {
            (Some(Cow::Borrowed(event)), None)
        } else {
            (None, Some(Cow::Borrowed(event)))
        }
    }

    fn contains(&self, vp: Viewport, point: CursorPosition) -> bool {
        let base = self.viewport;

        let l = vp.left as f64 - base.left as f64;
        let r = (vp.left + vp.width) as f64 - base.left as f64;
        let t = (base.bottom + base.height) as f64 - (vp.bottom + vp.height) as f64;
        let b = (base.bottom + base.height) as f64 - vp.bottom as f64;

        l <= point.x && point.x < r && t <= point.y && point.y < b
    }
}

impl Layout {
    fn new_single(win: Box<TerminalWindow>) -> Self {
        Self::Single(win)
    }

    fn new_binary(split: Split, viewport: Viewport, x: Box<Layout>, y: Box<Layout>) -> Self {
        let mut layout = Self::Binary(BinLayout {
            split,
            viewport,
            focus_x: false,
            x: Some(x),
            y: Some(y),
        });
        layout.set_viewport(viewport);
        layout
    }

    fn is_single(&self) -> bool {
        matches!(self, Layout::Single(_))
    }

    fn draw(&mut self, surface: &mut glium::Frame) {
        match self {
            Self::Single(win) => win.draw(surface),
            Self::Binary(layout) => {
                layout.x_mut().draw(surface);
                layout.y_mut().draw(surface);
            }
        }
    }

    fn viewport(&self) -> Viewport {
        match self {
            Self::Single(win) => win.viewport(),
            Self::Binary(layout) => layout.viewport,
        }
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        match self {
            Self::Single(win) => {
                win.set_viewport(viewport);
                win.resize_window(PhysicalSize {
                    width: viewport.width,
                    height: viewport.height,
                });
            }
            Self::Binary(layout) => {
                layout.viewport = viewport;
                let (vp_x, vp_y) = layout.split_viewport();
                layout.x_mut().set_viewport(vp_x);
                layout.y_mut().set_viewport(vp_y);
            }
        }
    }

    fn on_event(&mut self, event: &Event, control_flow: &mut ControlFlow) {
        match self {
            Self::Single(win) => {
                win.on_event(event, control_flow);
            }
            Self::Binary(layout) => {
                let (ev_x, ev_y) = layout.split_event(event);

                if let Some(event) = &ev_x {
                    let mut cf = ControlFlow::default();
                    layout.x_mut().on_event(event, &mut cf);
                    if cf == ControlFlow::Exit {
                        *control_flow = ControlFlow::Exit;
                    }
                }

                if let Some(event) = &ev_y {
                    let mut cf = ControlFlow::default();
                    layout.y_mut().on_event(event, &mut cf);
                    if cf == ControlFlow::Exit {
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
        }
    }

    fn detach(&mut self) -> Box<Layout> {
        match self {
            Self::Single(_) => panic!(),
            Self::Binary(layout) => {
                if layout.focus_x {
                    match layout.x_mut() {
                        Self::Single(_) => layout.x.take().unwrap(),
                        Self::Binary(_) => layout.x_mut().detach(),
                    }
                } else {
                    match layout.y_mut() {
                        Self::Single(_) => layout.y.take().unwrap(),
                        Self::Binary(_) => layout.y_mut().detach(),
                    }
                }
            }
        }
    }

    fn attach(&mut self, l: Box<Layout>) {
        match self {
            Self::Single(_) => panic!(),
            Self::Binary(layout) => {
                if layout.focus_x {
                    match layout.x.as_mut() {
                        None => layout.x = Some(l),
                        Some(x) => x.attach(l),
                    }
                } else {
                    match layout.y.as_mut() {
                        None => layout.y = Some(l),
                        Some(y) => y.attach(l),
                    }
                }
            }
        }
    }

    pub fn close(&mut self) -> Option<Box<Layout>> {
        match self {
            Self::Single(_) => None,
            Self::Binary(layout) => {
                if layout.focus_x {
                    match layout.x_mut() {
                        Self::Single(_) => layout.y.take(),
                        Self::Binary(_) => {
                            if let Some(new_x) = layout.x_mut().close() {
                                layout.x = Some(new_x);
                            }
                            None
                        }
                    }
                } else {
                    match layout.y_mut() {
                        Self::Single(_) => layout.x.take(),
                        Self::Binary(_) => {
                            if let Some(new_y) = layout.y_mut().close() {
                                layout.y = Some(new_y);
                            }
                            None
                        }
                    }
                }
            }
        }
    }

    fn focused_window_mut(&mut self) -> &mut TerminalWindow {
        match self {
            Self::Single(win) => win,
            Self::Binary(layout) => layout.focused_mut().focused_window_mut(),
        }
    }

    fn focus_change(&mut self, focus: FocusDirection) -> bool {
        match self {
            Self::Single(_) => false,
            Self::Binary(layout) => {
                let split = layout.split;
                let (x_focused, y_focused) = (layout.focus_x, !layout.focus_x);

                let changeable = match focus {
                    FocusDirection::Down => split == Split::Vertical && x_focused,
                    FocusDirection::Up => split == Split::Vertical && y_focused,
                    FocusDirection::Right => split == Split::Horizontal && x_focused,
                    FocusDirection::Left => split == Split::Horizontal && y_focused,
                };

                if changeable {
                    if !layout.focused_mut().focus_change(focus) {
                        layout.focus_x ^= true;
                    }
                    true
                } else {
                    layout.focused_mut().focus_change(focus)
                }
            }
        }
    }

    fn focus_change_mouse(&mut self, p: CursorPosition) {
        match self {
            Self::Single(_) => {}
            Self::Binary(layout) => {
                let (vp_x, vp_y) = layout.split_viewport();
                if layout.contains(vp_x, p) {
                    layout.focus_x = true;
                    layout.x_mut().focus_change_mouse(p);
                }
                if layout.contains(vp_y, p) {
                    layout.focus_x = false;

                    let mut modified_pos = p;
                    match layout.split {
                        Split::Vertical => {
                            modified_pos.y -= vp_x.height as f64;
                        }
                        Split::Horizontal => {
                            modified_pos.x -= vp_x.width as f64;
                        }
                    }

                    layout.y_mut().focus_change_mouse(modified_pos);
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FocusDirection {
    Up,
    Down,
    Left,
    Right,
}

pub struct Multiplexer {
    display: Display,
    viewport: Viewport,
    select: usize,
    wins: Vec<Option<Layout>>,
    consume: bool,
    status_view: TerminalView,
    mouse_cursor_pos: CursorPosition,
}

impl Multiplexer {
    pub fn new(display: Display) -> Self {
        let size = display.gl_window().window().inner_size();

        let viewport = Viewport {
            left: 0,
            bottom: 0,
            width: size.width,
            height: size.height,
        };

        let status_view = TerminalView::with_viewport(display.clone(), viewport);

        let mut mux = Multiplexer {
            display,
            viewport,
            select: 0,
            wins: Vec::new(),
            consume: false,
            status_view,
            mouse_cursor_pos: CursorPosition::default(),
        };

        mux.select = mux.allocate_new_window();
        mux.update_status_bar();

        mux
    }

    pub fn allocate_new_window(&mut self) -> usize {
        let mut window_viewport = self.viewport;
        window_viewport.height -= self.status_bar_height();

        log::info!("new terminal window added");
        let new = TerminalWindow::with_viewport(self.display.clone(), window_viewport);

        let num = self.wins.len();
        self.wins.push(Some(Layout::new_single(Box::new(new))));

        num
    }

    // Recalculate viewport recursively for each window/pane
    fn refresh_layout(&mut self) {
        self.status_view.set_viewport(self.viewport);

        let mut window_viewport = self.viewport;
        window_viewport.height -= self.status_bar_height();

        for win in self.wins.iter_mut().flatten() {
            win.set_viewport(window_viewport);
        }
    }

    fn status_bar_height(&self) -> u32 {
        self.status_view.cell_size.h
    }

    fn update_status_bar(&mut self) {
        self.status_view.bg_color = Color::BrightGreen;

        let line: Line = (0..self.wins.len())
            .flat_map(|i| {
                let mut cells: Vec<Cell> = Vec::new();

                let num = format!("{} ", i);
                for ch in num.chars() {
                    let mut cell = Cell::new_ascii(ch);
                    cell.attr.fg = if i == self.select {
                        Color::Yellow
                    } else {
                        Color::BrightBlue
                    };
                    cell.attr.bg = Color::BrightGreen;
                    cells.push(cell);
                }

                cells
            })
            .collect();

        let contents = &mut self.status_view.contents;
        contents.lines = vec![line];
        contents.images = Vec::new();
        contents.cursor = None;
        contents.selection_range = None;

        self.status_view.updated = true;
    }

    fn current(&mut self) -> &mut Layout {
        self.wins[self.select].as_mut().unwrap()
    }

    fn notify_focus_gain(&mut self) {
        let window_id = self.display.gl_window().window().id();
        let event = Event::WindowEvent {
            window_id,
            event: WindowEvent::Focused(true),
        };
        let mut cf = ControlFlow::default();
        self.current()
            .focused_window_mut()
            .on_event(&event, &mut cf);
        // FIXME: check cf
    }

    fn notify_focus_lost(&mut self) {
        let window_id = self.display.gl_window().window().id();
        let event = Event::WindowEvent {
            window_id,
            event: WindowEvent::Focused(false),
        };
        let mut cf = ControlFlow::default();
        self.current()
            .focused_window_mut()
            .on_event(&event, &mut cf);
        // FIXME: check cf
    }

    pub fn on_event(&mut self, event: &Event, control_flow: &mut ControlFlow) {
        if self.wins.is_empty() {
            *control_flow = ControlFlow::Exit;
            return;
        }

        match &event {
            Event::WindowEvent {
                event: win_event,
                window_id,
            } => match win_event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                WindowEvent::ModifiersChanged(..) => {
                    for win in self.wins.iter_mut().flatten() {
                        let mut cf = ControlFlow::default();
                        win.on_event(event, &mut cf);
                        // FIXME: handle ControlFlow::Exit
                    }
                    return;
                }

                WindowEvent::Resized(new_size) => {
                    self.viewport = Viewport {
                        left: 0,
                        bottom: 0,
                        width: new_size.width,
                        height: new_size.height,
                    };
                    self.refresh_layout();
                    return;
                }

                #[allow(deprecated)]
                WindowEvent::CursorMoved {
                    position,
                    device_id,
                    modifiers,
                } => {
                    self.mouse_cursor_pos = *position;

                    let mut modified_pos = *position;
                    modified_pos.y -= self.status_bar_height() as f64;

                    let modified_event = Event::WindowEvent {
                        window_id: *window_id,
                        event: WindowEvent::CursorMoved {
                            position: modified_pos,
                            device_id: *device_id,
                            modifiers: *modifiers,
                        },
                    };

                    // Forward to the selected window
                    let mut cf = ControlFlow::default();
                    self.current().on_event(&modified_event, &mut cf);
                    // FIXME: handle ControlFlow::Exit
                    return;
                }

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => {
                    let mut p = self.mouse_cursor_pos;
                    p.y -= self.status_bar_height() as f64;

                    self.notify_focus_lost();
                    self.current().focus_change_mouse(p);
                    self.notify_focus_gain();
                }

                WindowEvent::ReceivedCharacter(PREFIX_KEY) if !self.consume => {
                    self.consume = true;
                    return;
                }

                WindowEvent::ReceivedCharacter(PREFIX_KEY) if self.consume => {
                    self.consume = false;
                }

                WindowEvent::ReceivedCharacter('\x1B') if self.consume => {
                    // Esc
                    self.consume = false;
                    return;
                }

                // Create a new window
                WindowEvent::ReceivedCharacter('c') if self.consume => {
                    log::debug!("create a new window");
                    self.select = self.allocate_new_window();
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }

                // Next
                WindowEvent::ReceivedCharacter('n') if self.consume => {
                    log::debug!("next window");
                    self.notify_focus_lost();
                    self.select += 1;
                    self.select %= self.wins.len();
                    self.notify_focus_gain();
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }
                // Prev
                WindowEvent::ReceivedCharacter('p') if self.consume => {
                    log::debug!("prev window");
                    self.notify_focus_lost();
                    self.select = self.wins.len() + self.select - 1;
                    self.select %= self.wins.len();
                    self.notify_focus_gain();
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }

                // Vertical Split
                &WindowEvent::ReceivedCharacter(split_char @ (VSPLIT | HSPLIT)) if self.consume => {
                    let split = match split_char {
                        VSPLIT => {
                            log::debug!("vertical split");
                            Split::Vertical
                        }
                        HSPLIT => {
                            log::debug!("horizontal split");
                            Split::Horizontal
                        }
                        _ => unreachable!(),
                    };

                    self.notify_focus_lost();

                    if self.current().is_single() {
                        let old_win = match self.wins[self.select].take() {
                            Some(single @ Layout::Single(_)) => Box::new(single),
                            _ => unreachable!(),
                        };
                        let viewport = old_win.viewport();

                        let new_win = TerminalWindow::new(self.display.clone());
                        let new_win = Box::new(Layout::new_single(Box::new(new_win)));

                        let layout = Layout::new_binary(split, viewport, old_win, new_win);

                        self.wins[self.select] = Some(layout);
                    } else {
                        let old_win = self.current().detach();
                        let viewport = old_win.viewport();

                        let new_win = TerminalWindow::new(self.display.clone());
                        let new_win = Box::new(Layout::new_single(Box::new(new_win)));

                        let layout = Layout::new_binary(split, viewport, old_win, new_win);

                        self.current().attach(Box::new(layout));
                    }

                    self.notify_focus_gain();

                    self.consume = false;
                    return;
                }

                // Just ignore other characters
                WindowEvent::ReceivedCharacter(_) if self.consume => {
                    self.consume = false;
                    return;
                }

                WindowEvent::KeyboardInput { input, .. }
                    if input.state == ElementState::Pressed && self.consume =>
                {
                    if let Some(key) = input.virtual_keycode {
                        match key {
                            VirtualKeyCode::Up => {
                                self.notify_focus_lost();
                                self.current().focus_change(FocusDirection::Up);
                                self.notify_focus_gain();
                            }
                            VirtualKeyCode::Down => {
                                self.notify_focus_lost();
                                self.current().focus_change(FocusDirection::Down);
                                self.notify_focus_gain();
                            }
                            VirtualKeyCode::Left => {
                                self.notify_focus_lost();
                                self.current().focus_change(FocusDirection::Left);
                                self.notify_focus_gain();
                            }
                            VirtualKeyCode::Right => {
                                self.notify_focus_lost();
                                self.current().focus_change(FocusDirection::Right);
                                self.notify_focus_gain();
                            }
                            _ => {}
                        }

                        match key {
                            VirtualKeyCode::LShift
                            | VirtualKeyCode::RShift
                            | VirtualKeyCode::LControl
                            | VirtualKeyCode::RControl
                            | VirtualKeyCode::LAlt
                            | VirtualKeyCode::RAlt => {}

                            VirtualKeyCode::A
                            | VirtualKeyCode::C
                            | VirtualKeyCode::N
                            | VirtualKeyCode::P => {
                                return;
                            }

                            _ => {
                                self.consume = false;
                                return;
                            }
                        }
                    }
                }
                _ => {}
            },

            Event::RedrawRequested(_) => {
                let mut surface = self.display.draw();

                self.status_view.draw(&mut surface);
                self.current().draw(&mut surface);

                surface.finish().expect("finish");
                return;
            }

            Event::MainEventsCleared => {
                self.display.gl_window().window().request_redraw();
            }

            _ => {}
        }

        // Forward to the selected window
        let mut cf = ControlFlow::default();
        self.current().on_event(event, &mut cf);

        if cf == ControlFlow::Exit {
            self.notify_focus_lost();

            // remove selected window
            if self.current().is_single() {
                self.wins.remove(self.select);
                if self.select == self.wins.len() {
                    self.select = 0;
                }

                if self.wins.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }

                self.update_status_bar();
            } else {
                if let Some(new_layout) = self.current().close() {
                    self.wins[self.select] = Some(*new_layout);
                }
            }

            self.notify_focus_gain();
            self.refresh_layout();
        }
    }
}
