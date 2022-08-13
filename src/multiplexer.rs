use glium::{glutin, Display};
use glutin::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
};

use crate::terminal::{Cell, Color, Line};
use crate::window::{TerminalView, TerminalWindow, Viewport};

const PREFIX_KEY: char = '\x01'; // Ctrl + A
const VSPLIT: char = '"';
const HSPLIT: char = '%';

type Event = glutin::event::Event<'static, ()>;
type CursorPosition = PhysicalPosition<f64>;

enum Layout {
    Single(Box<TerminalWindow>),
    Binary(BinLayout),
    Tabbed(TabLayout),
}

struct BinLayout {
    split: Split,
    viewport: Viewport,
    ratio: f64,
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
                let u_height = (viewport.h as f64 * self.ratio).round() as u32;
                let d_height = viewport.h - u_height;

                let mut up = viewport;
                up.h = u_height - 1;

                let mut down = viewport;
                down.y = viewport.y + u_height;
                down.h = d_height - 1;

                (up, down)
            }

            Split::Horizontal => {
                let l_width = (viewport.w as f64 * self.ratio).round() as u32;
                let r_width = viewport.w - l_width;

                let mut left = viewport;
                left.w = l_width - 1;

                let mut right = viewport;
                right.x = viewport.x + l_width;
                right.w = r_width;

                (left, right)
            }
        }
    }

    fn split_event<'e>(&mut self, event: &'e Event) -> (Option<&'e Event>, Option<&'e Event>) {
        match event {
            Event::WindowEvent { event: wev, .. } => match wev {
                WindowEvent::ModifiersChanged(..)
                | WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseInput { .. } => {
                    return (Some(event), Some(event));
                }
                _ => {}
            },

            Event::MainEventsCleared => {
                return (Some(event), Some(event));
            }

            _ => {}
        }

        if self.focus_x {
            (Some(event), None)
        } else {
            (None, Some(event))
        }
    }
}

struct TabLayout {
    viewport: Viewport,
    focus: usize,
    tabs: Vec<Option<Box<Layout>>>,
}

impl TabLayout {
    fn focused_mut(&mut self) -> &mut Layout {
        self.tabs[self.focus].as_mut().unwrap()
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
            ratio: 0.50,
            focus_x: false,
            x: Some(x),
            y: Some(y),
        });
        layout.set_viewport(viewport);
        layout
    }

    fn new_tabbed(viewport: Viewport, first_tab: Box<Layout>) -> Self {
        let mut layout = Self::Tabbed(TabLayout {
            viewport,
            focus: 0,
            tabs: vec![Some(first_tab)],
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
            Self::Tabbed(layout) => {
                layout.focused_mut().draw(surface);
            }
        }
    }

    fn viewport(&self) -> Viewport {
        match self {
            Self::Single(win) => win.viewport(),
            Self::Binary(layout) => layout.viewport,
            Self::Tabbed(layout) => layout.viewport,
        }
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        match self {
            Self::Single(win) => {
                win.set_viewport(viewport);
                win.resize_window(PhysicalSize {
                    width: viewport.w,
                    height: viewport.h,
                });
            }
            Self::Binary(layout) => {
                layout.viewport = viewport;
                let (vp_x, vp_y) = layout.split_viewport();
                layout.x_mut().set_viewport(vp_x);
                layout.y_mut().set_viewport(vp_y);
            }
            Self::Tabbed(layout) => {
                layout.viewport = viewport;
                for t in layout.tabs.iter_mut().flatten() {
                    t.set_viewport(viewport);
                }
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

                if let Some(event) = ev_x {
                    let mut cf = ControlFlow::default();
                    layout.x_mut().on_event(event, &mut cf);
                    if cf == ControlFlow::Exit {
                        *control_flow = ControlFlow::Exit;
                    }
                }

                if let Some(event) = ev_y {
                    let mut cf = ControlFlow::default();
                    layout.y_mut().on_event(event, &mut cf);
                    if cf == ControlFlow::Exit {
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            Self::Tabbed(layout) => {
                layout.focused_mut().on_event(event, control_flow);
            }
        }
    }

    fn detach(&mut self) -> Box<Layout> {
        match self {
            Self::Single(_) => unreachable!(),
            Self::Binary(layout) => {
                let focused = layout.focused_mut();
                if focused.is_single() {
                    if layout.focus_x {
                        layout.x.take().unwrap()
                    } else {
                        layout.y.take().unwrap()
                    }
                } else {
                    focused.detach()
                }
            }
            Self::Tabbed(layout) => {
                let focused = layout.focused_mut();
                if focused.is_single() {
                    layout.tabs[layout.focus].take().unwrap()
                } else {
                    focused.detach()
                }
            }
        }
    }

    fn attach(&mut self, l: Box<Layout>) {
        match self {
            Self::Single(_) => unreachable!(),
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
            Self::Tabbed(layout) => {
                if let Some(focused) = layout.tabs[layout.focus].as_mut() {
                    focused.attach(l);
                } else {
                    layout.tabs[layout.focus] = Some(l);
                }
            }
        }
    }

    pub fn close(&mut self) -> Option<Box<Layout>> {
        match self {
            Self::Single(_) => unreachable!(),
            Self::Binary(layout) => {
                if layout.focus_x {
                    let x = layout.x_mut();
                    if x.is_single() {
                        layout.y.take()
                    } else {
                        if let Some(new_x) = x.close() {
                            layout.x = Some(new_x);
                        }
                        None
                    }
                } else {
                    let y = layout.y_mut();
                    if y.is_single() {
                        layout.x.take()
                    } else {
                        if let Some(new_y) = y.close() {
                            layout.y = Some(new_y);
                        }
                        None
                    }
                }
            }
            Self::Tabbed(layout) => {
                let focused = layout.focused_mut();
                if focused.is_single() {
                    layout.tabs.remove(layout.focus);
                    if layout.focus >= layout.tabs.len() {
                        layout.focus = 0;
                    }
                } else {
                    if let Some(new) = focused.close() {
                        layout.tabs[layout.focus] = Some(new);
                    }
                }
                None
            }
        }
    }

    fn focused_window_mut(&mut self) -> &mut TerminalWindow {
        match self {
            Self::Single(win) => win,
            Self::Binary(layout) => layout.focused_mut().focused_window_mut(),
            Self::Tabbed(layout) => layout.focused_mut().focused_window_mut(),
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
                    _ => false,
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
            Self::Tabbed(layout) => match focus {
                FocusDirection::TabNext => {
                    layout.focus += 1;
                    layout.focus %= layout.tabs.len();
                    true
                }
                FocusDirection::TabPrev => {
                    layout.focus = layout.tabs.len() + layout.focus - 1;
                    layout.focus %= layout.tabs.len();
                    true
                }
                _ => layout.focused_mut().focus_change(focus),
            },
        }
    }

    fn focus_change_mouse(&mut self, p: CursorPosition) {
        match self {
            Self::Single(_) => {}
            Self::Binary(layout) => {
                let (vp_x, vp_y) = layout.split_viewport();
                if vp_x.contains(p) {
                    layout.focus_x = true;
                    layout.x_mut().focus_change_mouse(p);
                }
                if vp_y.contains(p) {
                    layout.focus_x = false;
                    layout.y_mut().focus_change_mouse(p);
                }
            }
            Self::Tabbed(layout) => {
                // TODO: mouse support
                layout.focused_mut().focus_change_mouse(p)
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
    TabNext,
    TabPrev,
}

pub struct Multiplexer {
    display: Display,
    viewport: Viewport,
    status_view: TerminalView,
    main_layout: Layout,
    consume: bool,
    mouse_cursor_pos: CursorPosition,
}

impl Multiplexer {
    pub fn new(display: Display) -> Self {
        let size = display.gl_window().window().inner_size();

        let viewport = Viewport {
            x: 0,
            y: 0,
            w: size.width,
            h: size.height,
        };

        let status_view = TerminalView::with_viewport(display.clone(), viewport);

        let main_layout = {
            let mut window_viewport = viewport;
            window_viewport.y += status_view.cell_size.h;
            window_viewport.h -= status_view.cell_size.h;

            let window = TerminalWindow::with_viewport(display.clone(), window_viewport);
            let single = Layout::new_single(Box::new(window));
            Layout::new_tabbed(viewport, single.into())
        };

        let mut mux = Multiplexer {
            display,
            viewport,
            status_view,
            main_layout,
            consume: false,
            mouse_cursor_pos: CursorPosition::default(),
        };
        mux.update_status_bar();
        mux
    }

    fn tab_layout(&mut self) -> &mut TabLayout {
        match &mut self.main_layout {
            Layout::Tabbed(layout) => layout,
            _ => unreachable!(),
        }
    }

    fn allocate_new_window(&mut self) {
        let mut window_viewport = self.viewport;
        window_viewport.y += self.status_bar_height();
        window_viewport.h -= self.status_bar_height();

        log::info!("adding new terminal window");
        let window = TerminalWindow::with_viewport(self.display.clone(), window_viewport);
        let single = Layout::new_single(Box::new(window));

        let layout = self.tab_layout();
        layout.tabs.push(Some(Box::new(single)));
        layout.focus = layout.tabs.len() - 1;
    }

    // Recalculate viewport recursively for each window/pane
    fn refresh_layout(&mut self) {
        self.status_view.set_viewport(self.viewport);

        let mut window_viewport = self.viewport;
        window_viewport.y += self.status_bar_height();
        window_viewport.h -= self.status_bar_height();

        self.main_layout.set_viewport(window_viewport);
    }

    fn status_bar_height(&self) -> u32 {
        self.status_view.cell_size.h
    }

    fn update_status_bar(&mut self) {
        self.status_view.bg_color = Color::BrightGreen;

        let tab_layout = self.tab_layout();
        let num_tabs = tab_layout.tabs.len();
        let focus = tab_layout.focus;

        let line: Line = (0..num_tabs)
            .flat_map(|i| {
                let mut cells: Vec<Cell> = Vec::new();

                let num = format!("{} ", i);
                for ch in num.chars() {
                    let mut cell = Cell::new_ascii(ch);
                    cell.attr.fg = if i == focus {
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

    fn notify_focus_gain(&mut self) {
        let window_id = self.display.gl_window().window().id();
        let event = Event::WindowEvent {
            window_id,
            event: WindowEvent::Focused(true),
        };
        let mut cf = ControlFlow::default();
        self.main_layout
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
        self.main_layout
            .focused_window_mut()
            .on_event(&event, &mut cf);
        // FIXME: check cf
    }

    pub fn on_event(&mut self, event: &Event, control_flow: &mut ControlFlow) {
        if self.tab_layout().tabs.is_empty() {
            *control_flow = ControlFlow::Exit;
            return;
        }

        match &event {
            Event::WindowEvent { event: wev, .. } => match wev {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    return;
                }

                WindowEvent::Resized(new_size) => {
                    self.viewport = Viewport {
                        x: 0,
                        y: 0,
                        w: new_size.width,
                        h: new_size.height,
                    };
                    self.refresh_layout();
                    return;
                }

                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse_cursor_pos = *position;
                }

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => {
                    self.notify_focus_lost();
                    self.main_layout.focus_change_mouse(self.mouse_cursor_pos);
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
                    self.allocate_new_window();
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }

                // Next
                WindowEvent::ReceivedCharacter('n') if self.consume => {
                    log::debug!("next window");
                    self.notify_focus_lost();
                    self.main_layout.focus_change(FocusDirection::TabNext);
                    self.notify_focus_gain();
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }
                // Prev
                WindowEvent::ReceivedCharacter('p') if self.consume => {
                    log::debug!("prev window");
                    self.notify_focus_lost();
                    self.main_layout.focus_change(FocusDirection::TabPrev);
                    self.notify_focus_gain();
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }

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
                    {
                        let old_win = self.main_layout.detach();
                        let viewport = old_win.viewport();

                        let new_win = TerminalWindow::new(self.display.clone());
                        let single = Layout::new_single(Box::new(new_win));

                        let bin = Layout::new_binary(split, viewport, old_win, single.into());
                        self.main_layout.attach(bin.into());
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
                                self.main_layout.focus_change(FocusDirection::Up);
                                self.notify_focus_gain();
                            }
                            VirtualKeyCode::Down => {
                                self.notify_focus_lost();
                                self.main_layout.focus_change(FocusDirection::Down);
                                self.notify_focus_gain();
                            }
                            VirtualKeyCode::Left => {
                                self.notify_focus_lost();
                                self.main_layout.focus_change(FocusDirection::Left);
                                self.notify_focus_gain();
                            }
                            VirtualKeyCode::Right => {
                                self.notify_focus_lost();
                                self.main_layout.focus_change(FocusDirection::Right);
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
                self.main_layout.draw(&mut surface);
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
        self.main_layout.on_event(event, &mut cf);

        if cf == ControlFlow::Exit {
            self.notify_focus_lost();
            self.main_layout.close();
            if self.tab_layout().tabs.is_empty() {
                *control_flow = ControlFlow::Exit;
            } else {
                self.notify_focus_gain();
                self.update_status_bar();
                self.refresh_layout();
            }
        }
    }
}
