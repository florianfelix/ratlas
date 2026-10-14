#![allow(clippy::derivable_impls)]

use tracing::info;

mod app;
pub mod data;
mod utils;

pub use iced_wgpu::wgpu;

pub fn run() {
    utils::logger::init_logging();
    info!("Starting ratlas");
    let _ = app::application::Ratlas::run();
}
