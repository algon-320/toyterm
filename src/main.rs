mod cache;
mod clipboard;
mod config;
mod control_function;
mod font;
mod pipe_channel;
mod sixel;
mod terminal;
mod utils;
mod window;

#[cfg(feature = "multiplex")]
mod multiplexer;

lazy_static::lazy_static! {
    pub static ref TOYTERM_CONFIG: crate::config::Config = crate::config::build();
}

fn main() {
    // Force to build the global config
    lazy_static::initialize(&TOYTERM_CONFIG);

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

    #[cfg(not(feature = "multiplex"))]
    {
        let mut term = window::TerminalWindow::new(display);

        event_loop.run(move |event, _, control_flow| {
            if let Some(event) = event.to_static() {
                *control_flow = ControlFlow::Poll;
                term.on_event(&event, control_flow);
            }
        });
    }

    #[cfg(feature = "multiplex")]
    {
        let mut mux = multiplexer::Multiplexer::new(display);
        mux.allocate_new_window();

        event_loop.run(move |event, _, control_flow| {
            if let Some(event) = event.to_static() {
                *control_flow = ControlFlow::Poll;
                mux.on_event(&event, control_flow);
            }
        });
    }
}
