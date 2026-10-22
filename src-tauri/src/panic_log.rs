//! Release builds use `panic = "abort"`: without this hook a crash leaves nothing in
//! the log, and the user only sees the system's "app stopped working" dialog.

use std::path::{Path, PathBuf};

/// Writes panics to `log_path` synchronously (the tracing writer may not get to
/// flush before abort), then runs the previous hook (stderr).
pub fn install(log_path: PathBuf) {
    install_filtered(log_path, |_| true);
}

/// `install` for panics on threads `log_thread` accepts (tests share one process).
fn install_filtered(log_path: PathBuf, log_thread: impl Fn(&str) -> bool + Send + Sync + 'static) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let current = std::thread::current();
        let thread = current.name().unwrap_or("unnamed");
        if log_thread(thread) {
            let message = info
                .payload_as_str()
                .unwrap_or("(non-string panic payload)");
            let location = info.location().map(|l| l.to_string());
            let backtrace = std::backtrace::Backtrace::force_capture().to_string();
            let report = format_report(thread, message, location.as_deref(), &backtrace);
            // Never panic inside the panic hook: a failed write is just lost.
            let _ = append(&log_path, &report);
        }
        previous(info);
    }));
}

/// One log entry: what panicked, where, and the backtrace.
pub fn format_report(
    thread: &str,
    message: &str,
    location: Option<&str>,
    backtrace: &str,
) -> String {
    let now = unix_now();
    let location = location.unwrap_or("unknown location");
    let mut report = format!("{now} PANIC thread '{thread}' at {location}: {message}\n");
    if !backtrace.trim().is_empty() {
        report.push_str("backtrace:\n");
        report.push_str(backtrace.trim_end());
        report.push('\n');
    }
    report
}

/// Seconds since the Unix epoch; the log's own lines carry readable timestamps.
fn unix_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix={secs}")
}

/// Appends `text` to the log, creating it if needed.
pub fn append(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_has_message_location_and_backtrace() {
        let r = format_report(
            "main",
            "index out of bounds",
            Some("src/queue.rs:42:7"),
            "0: foo\n1: bar",
        );
        assert!(r.contains("PANIC"), "{r}");
        assert!(r.contains("thread 'main'"), "{r}");
        assert!(r.contains("index out of bounds"), "{r}");
        assert!(r.contains("src/queue.rs:42:7"), "{r}");
        assert!(r.contains("0: foo\n1: bar"), "{r}");
        assert!(r.ends_with('\n'));
        let r = format_report("worker", "boom", None, "");
        assert!(r.contains("unknown location"), "{r}");
    }

    #[test]
    fn append_keeps_existing_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cliprigo.log");
        std::fs::write(&path, "INFO started\n").unwrap();
        append(&path, "PANIC one\n").unwrap();
        append(&path, "PANIC two\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "INFO started\nPANIC one\nPANIC two\n"
        );
        // A missing log is created.
        let fresh = dir.path().join("new.log");
        append(&fresh, "x\n").unwrap();
        assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "x\n");
    }

    /// The real hook, end to end. It is process-wide, so it only writes for panics on
    /// threads named here; other tests' panics go to the previous hook as before.
    #[test]
    fn hook_writes_panics_to_the_log() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cliprigo.log");
        install_filtered(path.clone(), |thread| thread == "panic-log-test");
        let result = std::thread::Builder::new()
            .name("panic-log-test".into())
            .spawn(|| panic!("boom {}", 42))
            .unwrap()
            .join();
        assert!(result.is_err());
        let log = std::fs::read_to_string(&path).unwrap();
        assert!(log.contains("boom 42"), "{log}");
        assert!(log.contains("thread 'panic-log-test'"), "{log}");
        assert!(log.contains("panic_log.rs"), "{log}");
    }
}
