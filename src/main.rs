mod cache;
mod control_function;
mod font;
mod terminal;
mod utils;
mod window;

fn main() {
    // Setup env_logger
    let our_logs = concat!(module_path!(), "=debug");
    let env = env_logger::Env::default().default_filter_or(our_logs);
    env_logger::Builder::from_env(env)
        .format_timestamp(None)
        .init();

    use glium::glutin::event_loop::{ControlFlow, EventLoop};
    let event_loop = EventLoop::new();

    // Create a terminal window
    let mut term = window::TerminalWindow::new(&event_loop, 24, 80);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        term.on_event(event, control_flow);
    });
}
