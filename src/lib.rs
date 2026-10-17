#![allow(clippy::derivable_impls)]
#![allow(
    clippy::single_match,
    clippy::collapsible_if,
    clippy::unnecessary_map_or,
    clippy::field_reassign_with_default,
    clippy::useless_format
)]

use tracing::info;

mod app;
pub mod data;
pub mod playback;
mod radio_api;
mod utils;

pub use iced_wgpu::wgpu;

pub fn run() {
    utils::logger::init_logging();
    info!("Starting ratlas");
    let _ = app::application::Ratlas::run();
}
