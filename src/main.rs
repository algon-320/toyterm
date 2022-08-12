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
    // Make sure that configuration errors are detected earlier
    lazy_static::initialize(&TOYTERM_CONFIG);

    // Setup env_logger
    let our_logs = concat!(module_path!(), "=debug");
    let env = env_logger::Env::default().default_filter_or(our_logs);
    env_logger::Builder::from_env(env)
        .format_timestamp(None)
        .init();

    let event_loop = glium::glutin::event_loop::EventLoop::new();

    let title = "toyterm";
    let display = {
        use glium::glutin::{window::WindowBuilder, ContextBuilder};
        let win_builder = WindowBuilder::new().with_title(title).with_resizable(true);
        let ctx_builder = ContextBuilder::new().with_vsync(true).with_srgb(true);
        glium::Display::new(win_builder, ctx_builder, &event_loop).expect("display new")
    };

    #[cfg(not(feature = "multiplex"))]
    let mut term = window::TerminalWindow::new(display);

    #[cfg(feature = "multiplex")]
    let mut term = multiplexer::Multiplexer::new(display);

    event_loop.run(move |event, _, control_flow| {
        if let Some(event) = event.to_static() {
            term.on_event(&event, control_flow);
        }
    });
}
