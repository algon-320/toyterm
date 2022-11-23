mod cache;
mod config;
mod control_function;
mod font;
mod pipe_channel;
mod sixel;
mod terminal;
mod utils;
mod view;
pub mod window;

#[cfg(feature = "multiplex")]
pub mod multiplexer;

lazy_static::lazy_static! {
    pub static ref TOYTERM_CONFIG: crate::config::Config = crate::config::build();
}
