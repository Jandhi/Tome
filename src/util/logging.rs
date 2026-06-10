use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, Once};

use log::{LevelFilter, Metadata, Record};

/// Directory (relative to the working dir) where per-run log files are written.
const LOG_DIR: &str = "output/logs";

/// Ensures the logger is installed exactly once per process. `log` only accepts a single
/// global logger, and many tests call `init_logger()`; without this guard every later call
/// would still create (and orphan) a fresh log file before failing to install itself.
static INIT: Once = Once::new();

/// A logger that writes every record to the terminal and, optionally, to a per-run log file,
/// so generation output survives after the terminal scrollback is gone.
struct TeeLogger {
    level: LevelFilter,
    /// `None` only if the log file couldn't be created (falls back to terminal-only).
    file: Option<Mutex<File>>,
}

impl log::Log for TeeLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let line = format!(
            "{} {:<5} [{}] {}",
            ts,
            record.level(),
            record.target(),
            record.args()
        );

        // Terminal — note this println! is NOT captured by this logger (no recursion).
        println!("{}", line);

        // File — File writes are unbuffered, so lines survive a panic/crash for later analysis.
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = writeln!(file, "{}", line);
            }
        }
    }

    fn flush(&self) {
        if let Some(file) = &self.file {
            if let Ok(mut file) = file.lock() {
                let _ = file.flush();
            }
        }
    }
}

/// Initialise logging: tee all records to the terminal and to `output/logs/run_<timestamp>.log`.
///
/// Idempotent — only the first call per process installs the logger; later calls are no-ops.
///
/// The level defaults to `Info` (capturing info/warn/error) and can be overridden via the
/// `RUST_LOG` env var, e.g. `RUST_LOG=debug` or `RUST_LOG=trace`.
///
/// A file is always written (including under `cargo test`). Note that a full `cargo test` run
/// shares one global logger across parallel tests, so the file will interleave their output;
/// run a single test (optionally with `-- --test-threads=1`) for a clean per-test log.
pub fn init_logger() {
    INIT.call_once(|| {
        let level = std::env::var("RUST_LOG")
            .ok()
            .and_then(|s| s.trim().parse::<LevelFilter>().ok())
            .unwrap_or(LevelFilter::Info);

        let file = open_log_file();

        let path_note = match &file {
            Some((path, _)) => format!("terminal + {}", path.display()),
            None => "terminal only".to_string(),
        };

        let logger = TeeLogger {
            level,
            file: file.map(|(_, f)| Mutex::new(f)),
        };

        match log::set_boxed_logger(Box::new(logger)) {
            Ok(()) => {
                log::set_max_level(level);
                log::info!("Logging to {} (level {})", path_note, level);
            }
            Err(err) => println!("Failed to initialize logger: {}", err),
        }
    });
}

/// Create the per-run log file under `LOG_DIR`, returning its path and handle.
/// Returns `None` (logging falls back to terminal-only) if the file can't be created.
fn open_log_file() -> Option<(PathBuf, File)> {
    let dir = PathBuf::from(LOG_DIR);
    if let Err(err) = create_dir_all(&dir) {
        println!("Failed to create log directory {:?}: {}", dir, err);
        return None;
    }

    let path = dir.join(format!(
        "run_{}.log",
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    ));
    match File::create(&path) {
        Ok(file) => Some((path, file)),
        Err(err) => {
            println!("Failed to create log file {:?}: {}", path, err);
            None
        }
    }
}
