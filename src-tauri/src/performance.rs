//! Low-overhead JSONL performance tracing.
//!
//! Producers only serialize an event and enqueue it. A dedicated thread owns
//! the file, batches writes, and flushes once per second so timing probes never
//! perform disk I/O on the game/import/recommendation hot paths.

use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, OnceLock,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
pub const ENABLE_FLAG: &str = "performance.enabled";
static SENDER: OnceLock<mpsc::Sender<String>> = OnceLock::new();
static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();
static SESSION_ID: OnceLock<u128> = OnceLock::new();
static ENABLED: AtomicBool = AtomicBool::new(false);

pub fn init(data_root: &Path) -> Result<(), String> {
    let logs = data_root.join("logs");
    let path = logs.join("performance.jsonl");
    let (sender, receiver) = mpsc::channel::<String>();
    let writer_path = path.clone();
    thread::Builder::new()
        .name("performance-log-writer".to_string())
        .spawn(move || writer_loop(&writer_path, receiver))
        .map_err(|error| format!("Could not start performance logger: {error}"))?;
    let _ = LOG_PATH.set(path);
    let _ = SENDER.set(sender);
    let _ = SESSION_ID.set(now_unix_ms());
    Ok(())
}

pub fn set_enabled(data_root: &Path, enabled: bool) -> Result<(), String> {
    let flag = data_root.join(ENABLE_FLAG);
    if enabled {
        fs::create_dir_all(data_root)
            .map_err(|error| format!("Could not create application data folder: {error}"))?;
        fs::write(&flag, b"enabled")
            .map_err(|error| format!("Could not enable performance logging: {error}"))?;
        ENABLED.store(true, Ordering::Release);
        event("coach", "debug_mode_enabled", json!({ "version": env!("CARGO_PKG_VERSION") }));
    } else {
        event("coach", "debug_mode_disabled", json!({}));
        ENABLED.store(false, Ordering::Release);
        if flag.is_file() {
            fs::remove_file(&flag)
                .map_err(|error| format!("Could not disable performance logging: {error}"))?;
        }
    }
    Ok(())
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

pub fn log_path() -> Option<PathBuf> {
    LOG_PATH.get().cloned()
}

pub fn event(component: &str, action: &str, details: Value) {
    enqueue(component, action, None, details);
}

pub fn duration(component: &str, action: &str, elapsed: Duration, details: Value) {
    enqueue(component, action, Some(elapsed.as_micros()), details);
}

pub fn ingest_exporter_trace(path: &Path) {
    if !is_enabled() {
        return;
    }
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    for line in text.lines().skip(1) {
        let mut fields = line.splitn(2, '\t');
        let (Some(action), Some(duration_us)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Ok(duration_us) = duration_us.parse::<u128>() else {
            continue;
        };
        enqueue("exporter", action, Some(duration_us), json!({}));
    }
}

fn enqueue(component: &str, action: &str, duration_us: Option<u128>, details: Value) {
    if !is_enabled() {
        return;
    }
    let Some(sender) = SENDER.get() else {
        return;
    };
    let line = json!({
        "timestampUnixMs": now_unix_ms(),
        "sessionId": SESSION_ID.get().copied().unwrap_or_default(),
        "component": component,
        "action": action,
        "durationUs": duration_us,
        "details": details,
    })
    .to_string();
    let _ = sender.send(line);
}

fn writer_loop(path: &Path, receiver: mpsc::Receiver<String>) {
    let Ok(first_line) = receiver.recv() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    rotate_if_needed(path);
    let Ok(file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let mut writer = BufWriter::new(file);
    let _ = writeln!(writer, "{first_line}");
    loop {
        match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(line) => {
                let _ = writeln!(writer, "{line}");
                while let Ok(line) = receiver.try_recv() {
                    let _ = writeln!(writer, "{line}");
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = writer.flush();
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = writer.flush();
                return;
            }
        }
    }
}

fn rotate_if_needed(path: &Path) {
    if fs::metadata(path).map(|meta| meta.len()).unwrap_or(0) < MAX_LOG_BYTES {
        return;
    }
    let previous = path.with_extension("jsonl.1");
    let _ = fs::remove_file(&previous);
    let _ = fs::rename(path, previous);
}

fn now_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exporter_trace_parser_ignores_malformed_rows() {
        let root = std::env::temp_dir().join(format!("ltac-perf-{}", now_unix_ms()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("performance_export.tsv");
        fs::write(&path, "stage\tduration_us\nserialize_matches\t123\nbad\tvalue\n").unwrap();
        ingest_exporter_trace(&path);
        let _ = fs::remove_dir_all(root);
    }
}
