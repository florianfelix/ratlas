use directories::ProjectDirs;
use tracing_subscriber::{Registry, fmt, prelude::*};

// use crate::DATA_DIR;

pub fn init_logging() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "debug,ratlas=debug".into());

    if let Some(proj_dirs) = ProjectDirs::from("de", "ff", "ratlas") {
        let confdir = proj_dirs.config_dir();
        if !std::fs::exists(confdir).unwrap() {
            std::fs::create_dir_all(confdir).expect("Unable to create confdir");
        }
        let log_path = confdir.join("ratlas.log");
        let log_file = std::fs::File::create(log_path).expect("Failed to create log file");
        let panic_log_file = log_file
            .try_clone()
            .expect("Failed to clone log file handle");

        let stdout_layer = fmt::layer()
            .with_writer(std::io::stdout)
            .with_ansi(true)
            .without_time()
            .with_target(false)
            .with_line_number(true);

        let file_layer = fmt::layer()
            .with_writer(log_file)
            .with_ansi(false)
            .pretty()
            .without_time()
            .with_file(true)
            .with_target(false)
            .with_line_number(true);

        Registry::default()
            .with(env_filter)
            .with(stdout_layer)
            .with(file_layer)
            .init();

        setup_panic_hook(Some(panic_log_file));
    }
}

fn setup_panic_hook(log_file: Option<std::fs::File>) {
    let shared_file = log_file.map(std::sync::Mutex::new);

    std::panic::set_hook(Box::new(move |panic_info| {
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unknown");
        let backtrace = std::backtrace::Backtrace::capture();

        let panic_message = format!(
            "\n================ !!! CRITICAL ERROR / CRASH !!! ================\n\
             Application panicked in thread '{}'\n\
             Details: {}\n\
             Backtrace:\n{}\n\
             ================================================================\n",
            thread_name, panic_info, backtrace
        );

        if let Some(ref mutex) = shared_file
            && let Ok(mut guard) = mutex.lock()
        {
            let _ = epaint_to_file(&mut guard, &panic_message);
        }

        eprintln!("{}", panic_message);
    }));
}

fn epaint_to_file(file: &mut std::fs::File, message: &str) -> std::io::Result<()> {
    use std::io::Write;
    file.write_all(message.as_bytes())?;
    file.flush()
}
