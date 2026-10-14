use tracing::info;

mod app;
mod utils;

pub fn run() {
    utils::logger::init_logging();
    info!("Starting ratlas");
    let _ = app::application::Ratlas::run();
}
