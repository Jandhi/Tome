use std::fs::{create_dir_all, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, Once};

use log::{LevelFilter, Metadata, Record};

/// Directory (relative to the working dir) where per-run log files are written.
const LOG_DIR: &str = "output/logs";

/// Old logs are pruned at startup down to this many bytes in total, newest kept first...
const KEEP_BYTES: u64 = 500 * 1024 * 1024;
/// ...but the newest few runs are always kept, however large.
const KEEP_RUNS: usize = 5;

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
/// `RUST_LOG` env var, e.g. `RUST_LOG=debug` or `RUST_LOG=trace`. Only a bare level
/// (`off`/`error`/`warn`/`info`/`debug`/`trace`) is honoured — unlike `env_logger`, the
/// per-module syntax (`RUST_LOG=tome=debug`) is not parsed and silently falls back to `Info`.
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

/// Create the per-run log file under `LOG_DIR/<YYYY-MM-DD>/`, returning its path and handle.
/// Grouping by day keeps the log directory tidy across many runs. Returns `None` (logging
/// falls back to terminal-only) if the file can't be created.
fn open_log_file() -> Option<(PathBuf, File)> {
    prune_old_logs();
    let now = chrono::Local::now();
    let dir = PathBuf::from(LOG_DIR).join(now.format("%Y-%m-%d").to_string());
    if let Err(err) = create_dir_all(&dir) {
        println!("Failed to create log directory {:?}: {}", dir, err);
        return None;
    }

    let path = dir.join(format!("run_{}.log", now.format("%Y%m%d_%H%M%S")));
    match File::create(&path) {
        Ok(file) => Some((path, file)),
        Err(err) => {
            println!("Failed to create log file {:?}: {}", path, err);
            None
        }
    }
}

/// Delete the oldest `.log` files under [`LOG_DIR`] (dated folders and loose
/// files alike) beyond [`KEEP_RUNS`] once the ones kept reach [`KEEP_BYTES`],
/// then any day folder left empty. Errors are ignored: pruning is best effort.
fn prune_old_logs() {
    fn collect(dir: &std::path::Path, out: &mut Vec<(std::time::SystemTime, u64, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                collect(&path, out);
            } else if path.extension().is_some_and(|e| e == "log") {
                let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                out.push((modified, meta.len(), path));
            }
        }
    }
    let root = PathBuf::from(LOG_DIR);
    let mut logs = Vec::new();
    collect(&root, &mut logs);
    logs.sort_by(|a, b| b.0.cmp(&a.0)); // newest first
    let mut kept: u64 = 0;
    for (i, (_, len, path)) in logs.iter().enumerate() {
        if i < KEEP_RUNS || kept + len <= KEEP_BYTES {
            kept += len;
            continue;
        }
        let _ = std::fs::remove_file(path);
    }
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let _ = std::fs::remove_dir(entry.path()); // only succeeds when empty
            }
        }
    }
}
