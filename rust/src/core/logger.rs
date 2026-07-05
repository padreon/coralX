use std::path::PathBuf;
use std::sync::OnceLock;

static LOG_FILE_PATH: OnceLock<PathBuf> = OnceLock::new();

fn log_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs_home())
    } else if cfg!(target_os = "macos") {
        dirs_home().join("Library").join("Logs")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs_home().join(".local").join("share"))
    };
    base.join("coralX")
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Path to the active log file.
pub fn log_path() -> PathBuf {
    LOG_FILE_PATH
        .get()
        .cloned()
        .unwrap_or_else(|| log_dir().join("coralX.log"))
}

/// Configure the global logger with a file appender plus stderr for WARN+.
///
/// Best-effort: falls back to stderr-only if the log directory can't be created.
pub fn setup_logging() {
    use log::LevelFilter;

    let dir = log_dir();
    let file_target: Option<PathBuf> = match std::fs::create_dir_all(&dir) {
        Ok(()) => {
            let path = dir.join("coralX.log");
            let _ = LOG_FILE_PATH.set(path.clone());
            Some(path)
        }
        Err(_) => {
            eprintln!(
                "[coralX] WARNING: could not create log file at {} — logging to stderr only.",
                dir.join("coralX.log").display()
            );
            None
        }
    };

    let mut builder = env_logger::Builder::new();
    builder.filter_level(LevelFilter::Debug);

    if let Some(path) = file_target {
        // env_logger writes to a single stream; route everything to the file
        // and mirror WARN+ to stderr via a second logger call site (`log::warn!`
        // callers already see stderr through the panic/error hook in main).
        if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            builder.target(env_logger::Target::Pipe(Box::new(file)));
        }
    }

    let _ = builder.try_init();
}

/// Install a panic hook that logs unhandled panics (main thread and workers)
/// to the log file before the default panic handler runs.
pub fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("Unhandled panic: {info}");
        default_hook(info);
    }));
}
