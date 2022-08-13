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

#[derive(Debug, Clone, Copy, PartialEq)]
enum Command {
    FocusUp,
    FocusDown,
    FocusLeft,
    FocusRight,
    FocusNextTab,
    FocusPrevTab,
    SplitVertical,
    SplitHorizontal,
    AddNewTab,
}

enum Layout {
    Single(SingleLayout),
    Binary(BinaryLayout),
    Tabbed(TabbedLayout),
}

struct SingleLayout {
    display: Display,
    window: Option<Box<TerminalWindow>>,
}

impl SingleLayout {
    fn get_mut(&mut self) -> &mut TerminalWindow {
        self.window.as_mut().unwrap()
    }
}

struct BinaryLayout {
    split: Split,
    viewport: Viewport,
    ratio: f64,
    focus_x: bool,
    x: Option<Box<Layout>>,
    y: Option<Box<Layout>>,
    mouse_cursor_pos: CursorPosition,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Split {
    Horizontal,
    Vertical,
}

impl BinaryLayout {
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

    fn on_event(&mut self, event: &Event, control_flow: &mut ControlFlow) {
        if let Event::WindowEvent { event: wev, .. } = event {
            match wev {
                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse_cursor_pos = *position;
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => {
                    let (vp_x, vp_y) = self.split_viewport();
                    if !self.focus_x && vp_x.contains(self.mouse_cursor_pos) {
                        self.focused_mut().focused_window_mut().focus_changed(false);
                        self.focus_x = true;
                        self.focused_mut().focused_window_mut().focus_changed(true);
                    }
                    if self.focus_x && vp_y.contains(self.mouse_cursor_pos) {
                        self.focused_mut().focused_window_mut().focus_changed(false);
                        self.focus_x = false;
                        self.focused_mut().focused_window_mut().focus_changed(true);
                    }
                }
                _ => {}
            }
        }

        let (ev_x, ev_y) = self.split_event(event);

        if let Some(event) = ev_x {
            let mut cf = ControlFlow::default();
            self.x_mut().on_event(event, &mut cf);
            if cf == ControlFlow::Exit {
                *control_flow = ControlFlow::Exit;
            }
        }

        if let Some(event) = ev_y {
            let mut cf = ControlFlow::default();
            self.y_mut().on_event(event, &mut cf);
            if cf == ControlFlow::Exit {
                *control_flow = ControlFlow::Exit;
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

    fn process_command(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::FocusUp | Command::FocusDown | Command::FocusLeft | Command::FocusRight => {
                let split = self.split;
                let (x_focused, y_focused) = (self.focus_x, !self.focus_x);

                let changeable = match cmd {
                    Command::FocusDown => split == Split::Vertical && x_focused,
                    Command::FocusUp => split == Split::Vertical && y_focused,
                    Command::FocusRight => split == Split::Horizontal && x_focused,
                    Command::FocusLeft => split == Split::Horizontal && y_focused,
                    _ => unreachable!(),
                };

                let mut consumed = self.focused_mut().process_command(cmd);
                if !consumed && changeable {
                    self.focused_mut().focused_window_mut().focus_changed(false);
                    self.focus_x ^= true;
                    self.focused_mut().focused_window_mut().focus_changed(true);
                    consumed = true;
                }
                consumed
            }
            _ => self.focused_mut().process_command(cmd),
        }
    }
}

struct TabbedLayout {
    display: Display,
    viewport: Viewport,
    focus: usize,
    tabs: Vec<Option<Box<Layout>>>,
}

impl TabbedLayout {
    fn focused_mut(&mut self) -> &mut Layout {
        self.tabs[self.focus].as_mut().unwrap()
    }

    fn on_event(&mut self, event: &Event, control_flow: &mut ControlFlow) {
        self.focused_mut().on_event(event, control_flow);
    }

    fn process_command(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::AddNewTab => {
                self.focused_mut().focused_window_mut().focus_changed(false);

                let window = TerminalWindow::with_viewport(self.display.clone(), self.viewport);
                let single = Layout::new_single(self.display.clone(), window.into());

                self.tabs.push(Some(single.into()));
                self.focus = self.tabs.len() - 1;
                self.focused_mut().focused_window_mut().focus_changed(true);
                true
            }
            Command::FocusNextTab => {
                self.focused_mut().focused_window_mut().focus_changed(false);
                self.focus += 1;
                self.focus %= self.tabs.len();
                self.focused_mut().focused_window_mut().focus_changed(true);
                true
            }
            Command::FocusPrevTab => {
                self.focused_mut().focused_window_mut().focus_changed(false);
                self.focus = self.tabs.len() + self.focus - 1;
                self.focus %= self.tabs.len();
                self.focused_mut().focused_window_mut().focus_changed(true);
                true
            }

            _ => self.focused_mut().process_command(cmd),
        }
    }
}

impl Layout {
    fn new_single(display: Display, win: Box<TerminalWindow>) -> Self {
        Self::Single(SingleLayout {
            display,
            window: Some(win),
        })
    }

    fn new_binary(split: Split, viewport: Viewport, x: Box<Layout>, y: Box<Layout>) -> Self {
        let mut layout = Self::Binary(BinaryLayout {
            split,
            viewport,
            ratio: 0.50,
            focus_x: false,
            x: Some(x),
            y: Some(y),
            mouse_cursor_pos: CursorPosition::default(),
        });
        layout.set_viewport(viewport);
        layout
    }

    fn new_tabbed(display: Display, viewport: Viewport, first_tab: Box<Layout>) -> Self {
        let mut layout = Self::Tabbed(TabbedLayout {
            display,
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
            Self::Single(layout) => layout.get_mut().draw(surface),
            Self::Binary(layout) => {
                layout.x_mut().draw(surface);
                layout.y_mut().draw(surface);
            }
            Self::Tabbed(layout) => {
                layout.focused_mut().draw(surface);
            }
        }
    }

    fn set_viewport(&mut self, viewport: Viewport) {
        match self {
            Self::Single(layout) => {
                let win = layout.get_mut();
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
            Self::Single(layout) => layout.get_mut().on_event(event, control_flow),
            Self::Binary(layout) => layout.on_event(event, control_flow),
            Self::Tabbed(layout) => layout.on_event(event, control_flow),
        }
    }

    fn close(&mut self) -> Option<Box<Layout>> {
        match self {
            Self::Single(_) => unreachable!(),
            Self::Binary(layout) => {
                if layout.focus_x {
                    let x = layout.x_mut();
                    if x.is_single() {
                        layout.y_mut().focused_window_mut().focus_changed(true);
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
                        layout.x_mut().focused_window_mut().focus_changed(true);
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
                    if !layout.tabs.is_empty() {
                        layout
                            .focused_mut()
                            .focused_window_mut()
                            .focus_changed(true);
                    }
                } else if let Some(new) = focused.close() {
                    layout.tabs[layout.focus] = Some(new);
                }

                None
            }
        }
    }

    fn focused_window_mut(&mut self) -> &mut TerminalWindow {
        match self {
            Self::Single(layout) => layout.get_mut(),
            Self::Binary(layout) => layout.focused_mut().focused_window_mut(),
            Self::Tabbed(layout) => layout.focused_mut().focused_window_mut(),
        }
    }

    fn process_command(&mut self, cmd: Command) -> bool {
        match self {
            Self::Single(old) => {
                let split = match cmd {
                    Command::SplitVertical => Split::Vertical,
                    Command::SplitHorizontal => Split::Horizontal,
                    _ => return false,
                };

                let display = old.display.clone();
                let old_window = old.window.take().unwrap();
                let new_window = Box::new(TerminalWindow::new(display.clone()));

                let viewport = old_window.viewport();

                let mut x = Layout::new_single(display.clone(), old_window);
                let mut y = Layout::new_single(display, new_window);

                x.focused_window_mut().focus_changed(false);
                y.focused_window_mut().focus_changed(true);

                *self = Layout::new_binary(split, viewport, x.into(), y.into());
                true
            }
            Self::Binary(layout) => layout.process_command(cmd),
            Self::Tabbed(layout) => layout.process_command(cmd),
        }
    }
}

pub struct Multiplexer {
    display: Display,
    viewport: Viewport,
    status_view: TerminalView,
    main_layout: Layout,
    consume: bool,
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
            let window = TerminalWindow::new(display.clone());
            let single = Layout::new_single(display.clone(), Box::new(window));
            Layout::new_tabbed(display.clone(), viewport, single.into())
        };

        let mut mux = Multiplexer {
            display,
            viewport,
            status_view,
            main_layout,
            consume: false,
        };

        mux.refresh_layout();
        mux.update_status_bar();
        mux
    }

    fn tab_layout(&mut self) -> &mut TabbedLayout {
        match &mut self.main_layout {
            Layout::Tabbed(layout) => layout,
            _ => unreachable!(),
        }
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
                    self.main_layout.process_command(Command::AddNewTab);
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }

                // Next
                WindowEvent::ReceivedCharacter('n') if self.consume => {
                    log::debug!("next window");
                    let cmd = Command::FocusNextTab;
                    self.main_layout.process_command(cmd);
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }
                // Prev
                WindowEvent::ReceivedCharacter('p') if self.consume => {
                    log::debug!("prev window");
                    let cmd = Command::FocusPrevTab;
                    self.main_layout.process_command(cmd);
                    self.update_status_bar();

                    self.consume = false;
                    return;
                }

                &WindowEvent::ReceivedCharacter(split_char @ (VSPLIT | HSPLIT)) if self.consume => {
                    let split_cmd = match split_char {
                        VSPLIT => {
                            log::debug!("vertical split");
                            Command::SplitVertical
                        }
                        HSPLIT => {
                            log::debug!("horizontal split");
                            Command::SplitHorizontal
                        }
                        _ => unreachable!(),
                    };

                    self.main_layout.process_command(split_cmd);

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
                                self.main_layout.process_command(Command::FocusUp);
                            }
                            VirtualKeyCode::Down => {
                                self.main_layout.process_command(Command::FocusDown);
                            }
                            VirtualKeyCode::Left => {
                                self.main_layout.process_command(Command::FocusLeft);
                            }
                            VirtualKeyCode::Right => {
                                self.main_layout.process_command(Command::FocusRight);
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

        let mut cf = ControlFlow::default();
        self.main_layout.on_event(event, &mut cf);

        if cf == ControlFlow::Exit {
            self.main_layout.close();
            if self.tab_layout().tabs.is_empty() {
                *control_flow = ControlFlow::Exit;
            } else {
                self.refresh_layout();
                self.update_status_bar();
            }
        }
    }
}
