use glium::{glutin, Display};
use glutin::{
    dpi::PhysicalPosition,
    event::{ElementState, ModifiersState, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    window::CursorIcon,
};

use crate::terminal::{Cell, Color, Line};
use crate::window::{TerminalView, TerminalWindow, Viewport};

type Event = glutin::event::Event<'static, ()>;
type CursorPosition = PhysicalPosition<f64>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Command {
    Nop,
    FocusUp,
    FocusDown,
    FocusLeft,
    FocusRight,
    FocusNextTab,
    FocusPrevTab,
    SplitVertical,
    SplitHorizontal,
    AddNewTab,
    SetMaximze,
    ResetMaximze,
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
    display: Display,
    partition: Partition,
    viewport: Viewport,
    ratio: f64,
    focus_x: bool,
    x: Option<Box<Layout>>,
    y: Option<Box<Layout>>,
    mouse_cursor_pos: CursorPosition,
    grabbing: bool,
    maximized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Partition {
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

    const GAP: u32 = 2;

    fn split_viewport(&self) -> (Viewport, Viewport) {
        let viewport = self.viewport;

        if self.maximized {
            return if self.focus_x {
                (viewport, Viewport::default())
            } else {
                (Viewport::default(), viewport)
            };
        }

        match self.partition {
            Partition::Horizontal => {
                let mid = (viewport.h as f64 * self.ratio).round() as u32;

                let mut up = viewport;
                up.y = viewport.y;
                up.h = mid - Self::GAP;

                let mut down = viewport;
                down.y = viewport.y + mid + Self::GAP;
                down.h = viewport.h - mid - Self::GAP;

                // +------+ <-- viewport.y
                // |  up  |
                // +======+ <-- viewport.y + mid - GAP
                // |------| <-- viewport.y + mid
                // +======+ <-- viewport.y + mid + GAP
                // | down |
                // +------+ <-- viewport.y + viewport.h

                (up, down)
            }

            Partition::Vertical => {
                let mid = (viewport.w as f64 * self.ratio).round() as u32;

                let mut left = viewport;
                left.x = viewport.x;
                left.w = mid - Self::GAP;

                let mut right = viewport;
                right.x = viewport.x + mid + Self::GAP;
                right.w = viewport.w - mid - Self::GAP;

                // +-------------------- viewport.x
                // |      +------------- viewport.x + mid - GAP
                // |      |+------------ viewport.x + mid
                // |      ||+----------- viewport.x + mid + GAP
                // |      |||       +--- viewport.x + viewport.w
                // v      vvv       v
                // +------+-+-------+
                // | left ||| right |
                // +------+-+-------+

                (left, right)
            }
        }
    }

    fn cursor_on_partition(&self) -> bool {
        let x = self.mouse_cursor_pos.x.round() as i32;
        let y = self.mouse_cursor_pos.y.round() as i32;
        let viewport = self.viewport;

        let gap = Self::GAP as i32;

        match self.partition {
            Partition::Horizontal => {
                let mid = viewport.y as i32 + (viewport.h as f64 * self.ratio).round() as i32;
                let hit_y = mid - gap <= y && y < mid + gap;

                let left = viewport.x as i32;
                let right = (viewport.x + viewport.w) as i32;
                let hit_x = left - gap * 2 <= x && x < right + gap * 2;

                hit_x && hit_y
            }

            Partition::Vertical => {
                let mid = viewport.x as i32 + (viewport.w as f64 * self.ratio).round() as i32;
                let hit_x = mid - gap <= x && x < mid + gap;

                let top = viewport.y as i32;
                let bottom = (viewport.y + viewport.h) as i32;
                let hit_y = top - gap * 2 <= y && y < bottom + gap * 2;

                hit_x && hit_y
            }
        }
    }

    fn update_ratio(&mut self) {
        debug_assert!(self.grabbing);
        let CursorPosition { x, y } = self.mouse_cursor_pos;
        let viewport = self.viewport;

        let min_r = (Self::GAP * 2) as f64 / (viewport.h as f64);

        match self.partition {
            Partition::Horizontal => {
                let mid = y - viewport.y as f64;
                let r = mid / (viewport.h as f64);
                self.ratio = r.clamp(min_r, 1.0 - min_r);
            }

            Partition::Vertical => {
                let mid = x - viewport.x as f64;
                let r = mid / (viewport.w as f64);
                self.ratio = r.clamp(min_r, 1.0 - min_r);
            }
        }

        let (vp_x, vp_y) = self.split_viewport();
        self.x_mut().set_viewport(vp_x);
        self.y_mut().set_viewport(vp_y);
    }

    fn on_event(&mut self, event: &Event, control_flow: &mut ControlFlow) {
        if let Event::WindowEvent { event: wev, .. } = event {
            match wev {
                WindowEvent::CursorMoved { position, .. } => {
                    let on_partition_before = self.cursor_on_partition();
                    self.mouse_cursor_pos = *position;
                    let on_partition_after = self.cursor_on_partition();

                    if !self.grabbing && on_partition_before != on_partition_after {
                        if on_partition_after {
                            let icon = match self.partition {
                                Partition::Vertical => CursorIcon::EwResize,
                                Partition::Horizontal => CursorIcon::NsResize,
                            };
                            self.display.gl_window().window().set_cursor_icon(icon);
                        } else {
                            // Reload normal icon
                            self.focused_mut()
                                .focused_window_mut()
                                .refresh_cursor_icon();
                        }
                    }

                    if self.grabbing {
                        self.update_ratio();
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    ..
                } => {
                    if self.cursor_on_partition() {
                        self.grabbing = true;
                        self.display
                            .gl_window()
                            .window()
                            .set_cursor_icon(CursorIcon::Grabbing);
                    } else {
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
                }
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    ..
                } => {
                    if self.grabbing {
                        self.grabbing = false;

                        // Reload normal icon
                        self.focused_mut()
                            .focused_window_mut()
                            .refresh_cursor_icon();
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
        if self.maximized {
            return if self.focus_x {
                (Some(event), None)
            } else {
                (None, Some(event))
            };
        }

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
                let (x_focused, y_focused) = (self.focus_x, !self.focus_x);

                let changeable = match cmd {
                    Command::FocusDown => self.partition == Partition::Horizontal && x_focused,
                    Command::FocusUp => self.partition == Partition::Horizontal && y_focused,
                    Command::FocusRight => self.partition == Partition::Vertical && x_focused,
                    Command::FocusLeft => self.partition == Partition::Vertical && y_focused,
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
            Command::SetMaximze => {
                self.focused_mut().process_command(cmd);
                self.maximized = true;
                true
            }
            Command::ResetMaximze => {
                self.focused_mut().process_command(cmd);
                self.maximized = false;
                true
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

    fn new_binary(
        display: Display,
        partition: Partition,
        viewport: Viewport,
        x: Box<Layout>,
        y: Box<Layout>,
    ) -> Self {
        let mut layout = Self::Binary(BinaryLayout {
            display,
            partition,
            viewport,
            ratio: 0.50,
            focus_x: false,
            x: Some(x),
            y: Some(y),
            mouse_cursor_pos: CursorPosition::default(),
            grabbing: false,
            maximized: false,
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
                if layout.maximized {
                    layout.focused_mut().draw(surface);
                } else {
                    layout.x_mut().draw(surface);
                    layout.y_mut().draw(surface);
                }
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
            }
            Self::Binary(layout) => {
                layout.viewport = viewport;
                if layout.maximized {
                    layout.focused_mut().set_viewport(viewport);
                } else {
                    let (vp_x, vp_y) = layout.split_viewport();
                    layout.x_mut().set_viewport(vp_x);
                    layout.y_mut().set_viewport(vp_y);
                }
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
                let partition = match cmd {
                    Command::SplitVertical => Partition::Vertical,
                    Command::SplitHorizontal => Partition::Horizontal,
                    _ => return false,
                };

                let display = old.display.clone();
                let old_window = old.window.take().unwrap();
                let new_window = Box::new(TerminalWindow::new(display.clone()));

                let viewport = old_window.viewport();

                let mut x = Layout::new_single(display.clone(), old_window);
                let mut y = Layout::new_single(display.clone(), new_window);

                x.focused_window_mut().focus_changed(false);
                y.focused_window_mut().focus_changed(true);

                *self = Layout::new_binary(display, partition, viewport, x.into(), y.into());
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
    controller: Controller,
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
            controller: Controller::default(),
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

        if let Some(cmd) = self.controller.on_event(event) {
            log::debug!("command: {:?}", cmd);
            self.main_layout.process_command(cmd);
            self.update_status_bar();

            if let Command::SetMaximze | Command::ResetMaximze = cmd {
                self.refresh_layout();
            }
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

#[derive(Default)]
struct Controller {
    modifiers: ModifiersState,
    consume: bool,
    maximized: bool,
}

impl Controller {
    fn on_event(&mut self, event: &Event) -> Option<Command> {
        if let Event::WindowEvent { event: wev, .. } = event {
            match wev {
                &WindowEvent::ModifiersChanged(new_states) => {
                    self.modifiers = new_states;
                }

                &WindowEvent::ReceivedCharacter(ch) => {
                    return self.on_character(ch);
                }

                WindowEvent::KeyboardInput { input, .. }
                    if input.state == ElementState::Pressed =>
                {
                    if let Some(key) = input.virtual_keycode {
                        return self.on_key_press(key);
                    }
                }
                _ => {}
            }
        }

        None
    }

    fn on_character(&mut self, ch: char) -> Option<Command> {
        if !self.consume {
            if ch == '\x01' {
                self.consume = true;
                Some(Command::Nop)
            } else {
                None
            }
        } else {
            self.consume = false;
            match ch {
                '\x01' => None,
                '\x1b' => Some(Command::Nop),
                'c' => Some(Command::AddNewTab),
                'n' => Some(Command::FocusNextTab),
                'p' => Some(Command::FocusPrevTab),
                '%' => Some(Command::SplitVertical),
                '"' => Some(Command::SplitHorizontal),
                'z' => {
                    self.maximized ^= true;
                    if self.maximized {
                        Some(Command::SetMaximze)
                    } else {
                        Some(Command::ResetMaximze)
                    }
                }
                _ => Some(Command::Nop),
            }
        }
    }

    fn on_key_press(&mut self, keycode: VirtualKeyCode) -> Option<Command> {
        use ModifiersState as Mod;
        const EMPTY: u32 = Mod::empty().bits();

        if self.consume {
            let cmd = match (self.modifiers.bits(), keycode) {
                (EMPTY, VirtualKeyCode::Up) => Command::FocusUp,
                (EMPTY, VirtualKeyCode::Down) => Command::FocusDown,
                (EMPTY, VirtualKeyCode::Left) => Command::FocusLeft,
                (EMPTY, VirtualKeyCode::Right) => Command::FocusRight,
                _ => return None,
            };

            self.consume = false;
            Some(cmd)
        } else {
            None
        }
    }
}
