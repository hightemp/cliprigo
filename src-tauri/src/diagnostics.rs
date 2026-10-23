use std::path::Path;

use crate::components::ComponentStatus;
use crate::db::Settings;

/// Everything a bug report needs, gathered by the `diagnostics` command.
pub struct DiagInfo {
    pub app_version: String,
    pub os: String,
    pub arch: String,
    pub components: Vec<ComponentStatus>,
    pub settings: Settings,
    pub log_tail: String,
}

/// Plain-text report the user can paste into a chat or an issue. Proxy credentials
/// are hidden everywhere, including in log lines.
pub fn format_report(info: &DiagInfo) -> String {
    use std::fmt::Write;

    let s = &info.settings;
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Cliprigo {} on {} {}",
        info.app_version, info.os, info.arch
    );
    let _ = writeln!(out, "\nComponents:");
    for c in &info.components {
        let name = format!("{:?}", c.component).to_lowercase();
        match (&c.version, c.installed) {
            (Some(v), true) => _ = writeln!(out, "  {name}: {v}"),
            (None, true) => _ = writeln!(out, "  {name}: installed, unknown version"),
            _ => _ = writeln!(out, "  {name}: not installed"),
        }
    }
    let proxy = if s.proxy.is_empty() {
        "(system)".to_string()
    } else {
        crate::net::mask_proxy(&s.proxy)
    };
    let _ = writeln!(out, "\nSettings:");
    let _ = writeln!(out, "  proxy: {proxy}");
    let _ = writeln!(out, "  concurrency: {}", s.concurrency);
    let _ = writeln!(
        out,
        "  default format: {} {:?}",
        s.default_format, s.default_quality
    );
    let _ = writeln!(out, "  filename template: {}", s.filename_template);
    let _ = writeln!(out, "  language: {}", s.language);
    let _ = writeln!(out, "  auto-update yt-dlp: {}", s.auto_update_ytdlp);
    let _ = writeln!(out, "\nRecent log:\n{}", scrub_credentials(&info.log_tail));
    out
}

/// Moves the log to `<name>.old` once it is over `max_bytes`, replacing the previous
/// one: yt-dlp errors are logged on every failure and the log would grow forever.
pub fn rotate_log(path: &Path, max_bytes: u64) -> std::io::Result<()> {
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() > max_bytes => {
            let mut old = path.as_os_str().to_owned();
            old.push(".old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(path, old)
        }
        _ => Ok(()),
    }
}

/// The last `max_bytes` of the log, starting at a line boundary.
pub fn read_log_tail(path: &Path, max_bytes: u64) -> String {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut file) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes);
    let mut buf = Vec::new();
    if file.seek(SeekFrom::Start(start)).is_err() || file.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    if start > 0 {
        let first_line_end = buf
            .iter()
            .position(|b| *b == b'\n')
            .map_or(buf.len(), |i| i + 1);
        buf.drain(..first_line_end);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Last `n` lines of `text`.
pub fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Replaces `user:password@` in any URL inside `text`.
pub fn scrub_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("://") {
        out.push_str(&rest[..i + 3]);
        rest = &rest[i + 3..];
        // Only these end an authority: passwords may contain `,` `)` `'` and the like.
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '?' | '#'))
            .unwrap_or(rest.len());
        let authority = &rest[..end];
        if let Some(at) = authority.rfind('@') {
            let userinfo = &authority[..at];
            out.push_str(if userinfo.contains(':') {
                "***:***"
            } else {
                "***"
            });
            out.push_str(&authority[at..]);
        } else {
            out.push_str(authority);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Component;

    fn info() -> DiagInfo {
        DiagInfo {
            app_version: "0.1.2".into(),
            os: "windows".into(),
            arch: "x86_64".into(),
            components: vec![
                ComponentStatus {
                    component: Component::Ytdlp,
                    installed: true,
                    version: Some("2026.08.19".into()),
                },
                ComponentStatus {
                    component: Component::Deno,
                    installed: false,
                    version: None,
                },
            ],
            settings: Settings {
                proxy: "http://user:secret@10.0.0.1:3128".into(),
                concurrency: 3,
                ..Default::default()
            },
            log_tail: "INFO probe finished\nWARN probe failed via http://user:secret@10.0.0.1:3128"
                .into(),
        }
    }

    #[test]
    fn report_has_versions_components_settings_and_log() {
        let report = format_report(&info());
        for needle in [
            "Cliprigo 0.1.2",
            "windows x86_64",
            "ytdlp: 2026.08.19",
            "deno: not installed",
            "concurrency: 3",
            "http://***:***@10.0.0.1:3128",
            "INFO probe finished",
        ] {
            assert!(report.contains(needle), "missing {needle:?} in:\n{report}");
        }
    }

    #[test]
    fn report_never_contains_proxy_password() {
        assert!(!format_report(&info()).contains("secret"));
    }

    #[test]
    fn empty_proxy_is_reported_as_system() {
        let mut i = info();
        i.settings.proxy = String::new();
        assert!(format_report(&i).contains("proxy: (system)"));
    }

    #[test]
    fn large_log_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("cliprigo.log");
        let old = dir.path().join("cliprigo.log.old");
        // Missing or small logs stay as they are.
        rotate_log(&log, 10).unwrap();
        std::fs::write(&log, "small").unwrap();
        rotate_log(&log, 10).unwrap();
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "small");
        // A large one replaces the previous .old.
        std::fs::write(&old, "older").unwrap();
        std::fs::write(&log, "0123456789ab").unwrap();
        rotate_log(&log, 10).unwrap();
        assert!(!log.exists());
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "0123456789ab");
    }

    #[test]
    fn log_tail_reads_only_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("cliprigo.log");
        assert_eq!(read_log_tail(&log, 100), "");
        std::fs::write(&log, "first line\nсредняя\nlast line\n").unwrap();
        assert_eq!(
            read_log_tail(&log, 1000),
            "first line\nсредняя\nlast line\n"
        );
        // A cut in the middle drops the partial first line (and split UTF-8).
        assert_eq!(read_log_tail(&log, 14), "last line\n");
    }

    #[test]
    fn tail_keeps_last_lines() {
        assert_eq!(tail_lines("a\nb\nc\nd", 2), "c\nd");
        assert_eq!(tail_lines("a\nb", 5), "a\nb");
        assert_eq!(tail_lines("", 3), "");
    }

    #[test]
    fn scrubs_credentials_in_any_url() {
        assert_eq!(
            scrub_credentials("via http://u:p@h:1 and socks5://only@h2:2, ok https://x.com/a"),
            "via http://***:***@h:1 and socks5://***@h2:2, ok https://x.com/a"
        );
        // Passwords may hold characters that end a URL in prose.
        for password in ["pa,ss", "pa)ss", "pa'ss", "pa\"ss", "pa]ss", "p@ss"] {
            let text = format!("proxy ('http://user:{password}@h:1') failed");
            let scrubbed = scrub_credentials(&text);
            assert_eq!(scrubbed, "proxy ('http://***:***@h:1') failed", "{text}");
        }
    }
}
