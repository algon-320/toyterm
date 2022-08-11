use glium::{glutin, Display};
use glutin::{
    event::{Event, WindowEvent},
    event_loop::ControlFlow,
};

use crate::terminal::{Cell, Color, Line};
use crate::window::{TerminalView, TerminalWindow};

const PREFIX_KEY: char = '\x01'; // Ctrl + A

pub struct Multiplexer {
    display: Display,
    select: usize,
    wins: Vec<Option<TerminalWindow>>,
    consume: bool,
    status_view: TerminalView,
}

impl Multiplexer {
    pub fn new(display: Display) -> Self {
        let size = display.gl_window().window().inner_size();

        let mut viewport = glium::Rect {
            left: 0,
            bottom: 0,
            width: size.width,
            height: 32,
        };
        let mut status_view = TerminalView::with_viewport(display.clone(), viewport);
        let cell_height = status_view.cell_size.h;

        viewport.bottom = size.height - cell_height;
        viewport.height = cell_height;
        status_view.change_viewport(viewport);

        Multiplexer {
            display,
            select: 0,
            wins: Vec::new(),
            consume: false,
            status_view,
        }
    }

    pub fn allocate_new_window(&mut self) -> usize {
        let size = self.display.gl_window().window().inner_size();
        let viewport = glium::Rect {
            left: 0,
            bottom: 0,
            width: size.width,
            height: size.height - self.status_bar_height(),
        };

        log::info!("new terminal window added");
        let new = TerminalWindow::with_viewport(self.display.clone(), viewport);
        let num = self.wins.len();
        self.wins.push(Some(new));
        num
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

    pub fn on_event(&mut self, event: &Event<()>, control_flow: &mut ControlFlow) {
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
                    let mut modified_size = *new_size;
                    modified_size.height -= self.status_bar_height();

                    self.status_view.change_viewport(glium::Rect {
                        left: 0,
                        bottom: modified_size.height,
                        width: modified_size.width,
                        height: self.status_bar_height(),
                    });

                    let modified_event = Event::WindowEvent {
                        window_id: *window_id,
                        event: WindowEvent::Resized(modified_size),
                    };

                    for win in self.wins.iter_mut().flatten() {
                        let mut cf = ControlFlow::default();
                        win.change_viewport(glium::Rect {
                            left: 0,
                            bottom: 0,
                            width: modified_size.width,
                            height: modified_size.height,
                        });
                        win.on_event(&modified_event, &mut cf);
                        // FIXME: handle ControlFlow::Exit
                    }
                    return;
                }

                #[allow(deprecated)]
                WindowEvent::CursorMoved {
                    position,
                    device_id,
                    modifiers,
                } => {
                    let mut modified_position = *position;
                    modified_position.y -= self.status_bar_height() as f64;

                    let modified_event = Event::WindowEvent {
                        window_id: *window_id,
                        event: WindowEvent::CursorMoved {
                            position: modified_position,
                            device_id: *device_id,
                            modifiers: *modifiers,
                        },
                    };

                    // Forward to the selected window
                    let mut cf = ControlFlow::default();
                    self.wins[self.select]
                        .as_mut()
                        .unwrap()
                        .on_event(&modified_event, &mut cf);
                    // FIXME: handle ControlFlow::Exit
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
                    self.select = self.allocate_new_window();
                    self.consume = false;
                    return;
                }

                // Next
                WindowEvent::ReceivedCharacter('n') if self.consume => {
                    log::debug!("next window");
                    self.select += 1;
                    self.select %= self.wins.len();
                    self.consume = false;
                    return;
                }
                // Prev
                WindowEvent::ReceivedCharacter('p') if self.consume => {
                    log::debug!("prev window");
                    self.select = self.wins.len() + self.select - 1;
                    self.select %= self.wins.len();
                    self.consume = false;
                    return;
                }

                // Just ignore other characters
                WindowEvent::ReceivedCharacter(_) if self.consume => {
                    self.consume = false;
                    return;
                }

                _ => {}
            },

            Event::MainEventsCleared => {
                self.update_status_bar();
                self.display.gl_window().window().request_redraw();
            }

            Event::RedrawRequested(_) => {
                let mut surface = self.display.draw();

                self.status_view.draw(&mut surface);
                self.wins[self.select].as_mut().unwrap().draw(&mut surface);

                surface.finish().expect("finish");
                return;
            }

            _ => {}
        }

        // Forward to the selected window
        let mut cf = ControlFlow::default();

        self.wins[self.select]
            .as_mut()
            .unwrap()
            .on_event(event, &mut cf);

        if cf == ControlFlow::Exit {
            // remove selected window
            self.wins.remove(self.select);

            if self.select == self.wins.len() {
                self.select = 0;
            }

            if self.wins.is_empty() {
                *control_flow = ControlFlow::Exit;
            }
        }
    }
}
