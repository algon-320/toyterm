mod cache;
mod clipboard;
mod control_function;
mod font;
mod multiplexer;
mod pipe_channel;
mod sixel;
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

    use glium::{glutin, Display};
    use glutin::{
        dpi::PhysicalSize,
        event_loop::{ControlFlow, EventLoop},
        window::WindowBuilder,
        ContextBuilder,
    };

    let event_loop = EventLoop::new();

    let width = 1000;
    let height = 500;
    let win_builder = WindowBuilder::new()
        .with_title("toyterm")
        .with_inner_size(PhysicalSize::new(width, height))
        .with_resizable(true);
    let ctx_builder = ContextBuilder::new().with_vsync(true).with_srgb(true);
    let display = Display::new(win_builder, ctx_builder, &event_loop).expect("display new");

    if cfg!(feature = "multiplex") {
        let mut mux = multiplexer::Multiplexer::new(display.clone());

        let term = window::TerminalWindow::new(display, width, height);
        mux.add(term);

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;
            mux.on_event(&event, control_flow);
        });
    } else {
        let mut term = window::TerminalWindow::new(display, width, height);

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;
            term.on_event(&event, control_flow);
        });
    }
}
