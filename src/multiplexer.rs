use glium::{glutin, Display};
use glutin::{
    dpi::PhysicalPosition,
    event::{ElementState, ModifiersState, VirtualKeyCode, WindowEvent},
    event_loop::ControlFlow,
    window::CursorIcon,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::terminal::{Cell, Color};
use crate::view::{TerminalView, Viewport};
use crate::window::TerminalWindow;

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
    SetMaximize,
    ResetMaximize,

    SaveLayout,
    RestoreLayout,
}

#[derive(Serialize, Deserialize)]
enum Layout {
    Single(SingleLayout),
    Binary(BinaryLayout),
    Tabbed(TabbedLayout),
}

#[derive(Serialize, Deserialize)]
struct SingleLayout {
    #[serde(skip)]
    window: Option<Box<TerminalWindow>>,
    cwd: PathBuf,
}

impl SingleLayout {
    fn get_mut(&mut self) -> &mut TerminalWindow {
        self.window.as_mut().unwrap()
    }

    fn update_cwd(&mut self) {
        let cwd = self.get_mut().get_foreground_process_cwd();
        self.cwd = cwd;
    }
}

#[derive(Serialize, Deserialize)]
struct BinaryLayout {
    partition: Partition,
    viewport: Viewport,
    ratio: f64,
    focus_x: bool,
    x: Option<Box<Layout>>,
    y: Option<Box<Layout>>,

    #[serde(skip)]
    maximized: bool,
    #[serde(skip)]
    mouse_cursor_pos: CursorPosition,
    #[serde(skip)]
    grabbing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

        match self.partition {
            Partition::Horizontal => {
                let min_r = (Self::GAP * 2) as f64 / (viewport.h as f64);
                let mid = y - viewport.y as f64;
                let r = mid / (viewport.h as f64);
                self.ratio = r.clamp(min_r, 1.0 - min_r);
            }

            Partition::Vertical => {
                let min_r = (Self::GAP * 2) as f64 / (viewport.w as f64);
                let mid = x - viewport.x as f64;
                let r = mid / (viewport.w as f64);
                self.ratio = r.clamp(min_r, 1.0 - min_r);
            }
        }

        let (vp_x, vp_y) = self.split_viewport();
        self.x_mut().set_viewport(vp_x);
        self.y_mut().set_viewport(vp_y);
    }

    fn on_event(&mut self, display: &Display, event: &Event, control_flow: &mut ControlFlow) {
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
                            display.gl_window().window().set_cursor_icon(icon);
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
                        display
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
            self.x_mut().on_event(display, event, &mut cf);
            if cf == ControlFlow::Exit {
                *control_flow = ControlFlow::Exit;
            }
        }

        if let Some(event) = ev_y {
            let mut cf = ControlFlow::default();
            self.y_mut().on_event(display, event, &mut cf);
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

    fn process_command(&mut self, display: &Display, cmd: Command) -> bool {
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

                let mut consumed = self.focused_mut().process_command(display, cmd);
                if !consumed && changeable {
                    self.focused_mut().focused_window_mut().focus_changed(false);
                    self.focus_x ^= true;
                    self.focused_mut().focused_window_mut().focus_changed(true);
                    consumed = true;
                }
                consumed
            }
            Command::SetMaximize => {
                self.focused_mut().process_command(display, cmd);
                self.maximized = true;
                true
            }
            Command::ResetMaximize => {
                self.focused_mut().process_command(display, cmd);
                self.maximized = false;
                true
            }

            Command::SaveLayout | Command::RestoreLayout => {
                self.x_mut().process_command(display, cmd);
                self.y_mut().process_command(display, cmd);
                true
            }

            _ => self.focused_mut().process_command(display, cmd),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct TabbedLayout {
    viewport: Viewport,
    focus: usize,
    tabs: Vec<Option<Box<Layout>>>,
}

impl TabbedLayout {
    fn focused_mut(&mut self) -> &mut Layout {
        self.tabs[self.focus].as_mut().unwrap()
    }

    fn on_event(&mut self, display: &Display, event: &Event, control_flow: &mut ControlFlow) {
        self.focused_mut().on_event(display, event, control_flow);
    }

    fn process_command(&mut self, display: &Display, cmd: Command) -> bool {
        match cmd {
            Command::AddNewTab => {
                self.focused_mut().focused_window_mut().focus_changed(false);

                let window = TerminalWindow::with_viewport(display.clone(), self.viewport, None);
                let single = Layout::new_single(window.into());

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

            Command::SaveLayout | Command::RestoreLayout => {
                for tab in self.tabs.iter_mut().flatten() {
                    tab.process_command(display, cmd);
                }
                true
            }

            _ => self.focused_mut().process_command(display, cmd),
        }
    }
}

impl Layout {
    fn new_single(win: Box<TerminalWindow>) -> Self {
        let cwd = win.get_foreground_process_cwd();
        Self::Single(SingleLayout {
            window: Some(win),
            cwd,
        })
    }

    fn new_binary(
        partition: Partition,
        viewport: Viewport,
        x: Box<Layout>,
        y: Box<Layout>,
    ) -> Self {
        let mut layout = Self::Binary(BinaryLayout {
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

    fn new_tabbed(viewport: Viewport, first_tab: Box<Layout>) -> Self {
        let mut layout = Self::Tabbed(TabbedLayout {
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

    fn update_focus(&mut self, focus: bool) {
        match self {
            Self::Single(layout) => {
                layout.get_mut().focus_changed(focus);
            }
            Self::Binary(layout) => {
                let focus_x = layout.focus_x;
                layout.x_mut().update_focus(focus && focus_x);
                layout.y_mut().update_focus(focus && !focus_x);
            }
            Self::Tabbed(layout) => {
                for (i, tab) in layout.tabs.iter_mut().enumerate() {
                    if let Some(tab) = tab {
                        tab.update_focus(focus && i == layout.focus);
                    }
                }
            }
        }
    }

    fn on_event(&mut self, display: &Display, event: &Event, control_flow: &mut ControlFlow) {
        match self {
            Self::Single(layout) => layout.get_mut().on_event(event, control_flow),
            Self::Binary(layout) => layout.on_event(display, event, control_flow),
            Self::Tabbed(layout) => layout.on_event(display, event, control_flow),
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

    fn process_command(&mut self, display: &Display, cmd: Command) -> bool {
        match self {
            Self::Single(layout) => match cmd {
                Command::SplitVertical | Command::SplitHorizontal => {
                    let partition = match cmd {
                        Command::SplitVertical => Partition::Vertical,
                        Command::SplitHorizontal => Partition::Horizontal,
                        _ => unreachable!(),
                    };

                    layout.update_cwd();
                    let old_cwd = layout.cwd.clone();
                    let old_window = layout.window.take().unwrap();

                    let new_window = {
                        let cwd = Some(old_cwd.as_ref()); // derive from current pane
                        Box::new(TerminalWindow::new(display.clone(), cwd))
                    };

                    let viewport = old_window.viewport();

                    let mut y = Layout::new_single(new_window);
                    let mut x = Layout::new_single(old_window);

                    x.focused_window_mut().focus_changed(false);
                    y.focused_window_mut().focus_changed(true);

                    *self = Layout::new_binary(partition, viewport, x.into(), y.into());
                    true
                }

                Command::SaveLayout => {
                    layout.update_cwd();
                    true
                }
                Command::RestoreLayout => {
                    debug_assert!(layout.window.is_none());
                    let new_window =
                        Box::new(TerminalWindow::new(display.clone(), Some(&layout.cwd)));
                    layout.window = Some(new_window);
                    true
                }

                _ => false,
            },

            Self::Binary(layout) => layout.process_command(display, cmd),
            Self::Tabbed(layout) => layout.process_command(display, cmd),
        }
    }
}

pub struct Multiplexer {
    display: Display,
    viewport: Viewport,
    status_view: TerminalView,
    last_updated: std::time::Instant,
    main_layout: Layout,
    controller: Controller,
    finished: bool,
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

        let font_size = crate::TOYTERM_CONFIG.status_bar_font_size;
        let status_view = TerminalView::with_viewport(display.clone(), viewport, font_size, None);

        let main_layout = {
            let window = TerminalWindow::new(display.clone(), None);
            let single = Layout::new_single(Box::new(window));
            Layout::new_tabbed(viewport, single.into())
        };

        let mut mux = Multiplexer {
            display,
            viewport,
            status_view,
            last_updated: std::time::Instant::now(),
            main_layout,
            controller: Controller::default(),
            finished: false,
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
        self.status_view.cell_size().h
    }

    fn update_status_bar(&mut self) {
        const FOCUSED_FG: Color = Color::Yellow;
        const NORMAL_FG: Color = Color::BrightBlue;
        const BG: Color = Color::BrightGreen;

        fn default_cell() -> Cell {
            let mut cell = Cell::new_ascii(' ');
            cell.attr.fg = NORMAL_FG;
            cell.attr.bg = BG;
            cell
        }

        struct Tab {
            i: usize,
            focus: bool,
            name: String,
        }

        impl Tab {
            fn display(&self) -> Vec<Cell> {
                let text = format!("{}:{} ", self.i, self.name);
                text.chars()
                    .map(|ch| {
                        let mut cell = default_cell();
                        cell.ch = ch;
                        if self.focus {
                            cell.attr.fg = FOCUSED_FG;
                        }
                        cell
                    })
                    .collect()
            }
        }

        let cols = (self.viewport.w / self.status_view.cell_size().w) as usize;
        let mut cells = Vec::new();

        let tab_layout = self.tab_layout();
        let focused_tab = tab_layout.focus;
        for (i, layout) in tab_layout.tabs.iter_mut().enumerate() {
            if let Some(layout) = layout {
                let win = layout.focused_window_mut();
                let name = win.get_foreground_process_name();
                let last_part = name.rsplit('/').next().unwrap().to_owned();

                let tab = Tab {
                    i,
                    focus: i == focused_tab,
                    name: last_part,
                };

                cells.extend(tab.display());
            }
        }

        cells.resize(cols, default_cell());

        // display date/time
        {
            use chrono::{DateTime, Local};
            let now: DateTime<Local> = Local::now();

            let text = format!("{}", now.format("%Y/%m/%d %H:%M"));
            let start = cols.saturating_sub(text.len());
            for (i, ch) in text.chars().enumerate() {
                if let Some(cell) = cells.get_mut(start + i) {
                    cell.ch = ch;
                    cell.attr.fg = NORMAL_FG;
                }
            }
        }

        self.status_view.update_contents(|view| {
            view.bg_color = BG;
            view.lines = vec![cells.into_iter().collect()];
            view.images = Vec::new();
            view.cursor = None;
            view.selection_range = None;
        });

        self.last_updated = std::time::Instant::now();
    }

    pub fn on_event(&mut self, event: &Event, control_flow: &mut ControlFlow) {
        if self.finished {
            *control_flow = ControlFlow::Exit;
            return;
        }

        if let Some(cmd) = self.controller.on_event(event) {
            self.process_command(cmd);
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
                    self.update_status_bar();
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
                if self.last_updated.elapsed().as_secs() >= 5 {
                    self.update_status_bar();
                }

                self.display.gl_window().window().request_redraw();
            }

            _ => {}
        }

        let mut cf = ControlFlow::default();
        self.main_layout.on_event(&self.display, event, &mut cf);

        if cf == ControlFlow::Exit {
            self.main_layout.close();
            if self.tab_layout().tabs.is_empty() {
                *control_flow = ControlFlow::Exit;
                self.finished = true;
            } else {
                // FIXME
                self.controller.maximized = false;
                self.main_layout
                    .process_command(&self.display, Command::ResetMaximize);

                self.refresh_layout();
                self.update_status_bar();
            }
        }
    }

    fn process_command(&mut self, cmd: Command) {
        log::debug!("command: {:?}", cmd);
        match cmd {
            Command::Nop => {}

            Command::SaveLayout => {
                self.main_layout
                    .process_command(&self.display, Command::SaveLayout);
                self.refresh_layout();

                let path = find_layout_file();
                let bytes = serde_json::to_vec(&self.main_layout).expect("serialize");
                match std::fs::write(&path, &bytes) {
                    Ok(_) => {
                        log::info!("layout saved in {}", path.display());
                    }
                    Err(err) => {
                        log::error!("Failed to save layout in {}: {err}", path.display());
                    }
                }
            }

            Command::RestoreLayout => {
                let path = find_layout_file();
                let restore_result = std::fs::read(&path).and_then(|bytes| {
                    serde_json::from_slice(&bytes).map_err(|err| {
                        use std::io::{Error, ErrorKind};
                        Error::new(ErrorKind::Other, format!("layout file corrupted: {err}"))
                    })
                });

                let saved_layout = match restore_result {
                    Ok(saved_layout) => saved_layout,
                    Err(err) => {
                        log::error!("Failed to restore layout from {}: {err}", path.display());
                        return;
                    }
                };

                self.main_layout = saved_layout;
                self.main_layout
                    .process_command(&self.display, Command::RestoreLayout);
                self.main_layout.update_focus(true);

                self.refresh_layout();
                self.update_status_bar();

                self.controller.maximized = false;

                log::info!("layout restored from {}", path.display());
            }

            Command::SetMaximize | Command::ResetMaximize => {
                self.main_layout.process_command(&self.display, cmd);
                self.refresh_layout();
            }

            Command::FocusUp
            | Command::FocusDown
            | Command::FocusLeft
            | Command::FocusRight
            | Command::SplitVertical
            | Command::SplitHorizontal => {
                if self.controller.maximized {
                    self.controller.maximized = false;
                    self.main_layout
                        .process_command(&self.display, Command::ResetMaximize);
                    self.refresh_layout();
                }

                self.main_layout.process_command(&self.display, cmd);
            }

            Command::FocusNextTab | Command::FocusPrevTab | Command::AddNewTab => {
                self.main_layout.process_command(&self.display, cmd);
                self.update_status_bar();
            }
        }
    }
}

fn find_layout_file() -> PathBuf {
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            // fallback to "$HOME/.config"
            let home = std::env::var_os("HOME")?;
            let mut p = PathBuf::from(home);
            p.push(".config");
            Some(p)
        })
        .unwrap_or_else(|| {
            // otherwise use "/tmp"
            std::env::temp_dir()
        });

    let mut layout_path = config_home;
    layout_path.push("toyterm");
    layout_path.push("layout.json");
    layout_path
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
                's' => Some(Command::SaveLayout),
                'r' => Some(Command::RestoreLayout),
                'z' => {
                    self.maximized ^= true;
                    if self.maximized {
                        Some(Command::SetMaximize)
                    } else {
                        Some(Command::ResetMaximize)
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
