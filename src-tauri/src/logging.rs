use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Elapsed time since the process started.
static START: OnceLock<Instant> = OnceLock::new();

pub fn elapsed_ms() -> u128 {
    START.get().map(|start| start.elapsed().as_millis()).unwrap_or(0)
}

const MAX_BYTES: u64 = 10 * 1024 * 1024;

static SINK: OnceLock<Mutex<Option<File>>> = OnceLock::new();

pub fn init(log_dir: &Path) {
    let _ = START.set(Instant::now());
    let file = open_log(log_dir);
    let _ = SINK.set(Mutex::new(file));

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write("PANIC", &info.to_string());
        previous(info);
    }));

    write("INFO", "mix starting");
}

pub fn info(message: &str) {
    write("INFO", message);
}

pub fn warn(message: &str) {
    write("WARN", message);
}

fn write(level: &str, message: &str) {
    let stamp = elapsed_ms();
    eprintln!("[{level} {stamp}ms] {message}");
    let Some(sink) = SINK.get() else { return };
    if let Ok(mut guard) = sink.lock() {
        if let Some(file) = guard.as_mut() {
            let _ = writeln!(file, "{level} {stamp}ms {message}");
            let _ = file.flush();
        }
    }
}

fn open_log(dir: &Path) -> Option<File> {
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join("mix.log");
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > MAX_BYTES {
            let _ = std::fs::rename(&path, dir.join("mix.log.1"));
        }
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}
