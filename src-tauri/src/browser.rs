//! The YouTube window: youtube.com itself, with a "Download" button that sends the
//! open video or playlist to the app.
//!
//! It runs as a second process of the app (`cliprigo --youtube-browser <url>`): a
//! WebKitGTK crash on a YouTube page then closes this window, not the downloads.
//! The page gets no access to the app's commands. The button navigates to an
//! address on `cliprigo.invalid`, which the window blocks and prints instead
//! (`OPEN <url>`); the app reads that from the process's stdout. The app writes
//! `NAVIGATE <url>` to its stdin to show another page, and closing stdin ends it.

use url::Url;

use crate::ytdlp::url::is_youtube;

pub const BROWSER_ARG: &str = "--youtube-browser";
pub const HOME: &str = "https://www.youtube.com/";
const ACTION_HOST: &str = "cliprigo.invalid";
const REPORT: &str = "OPEN ";
const NAVIGATE: &str = "NAVIGATE ";
/// The button's label, in the app's language.
const LABEL_ENV: &str = "CLIPRIGO_BROWSER_LABEL";
const PROXY_ENV: &str = "CLIPRIGO_BROWSER_PROXY";

/// Start page when the process was launched as the YouTube window.
pub fn browser_mode(args: impl IntoIterator<Item = String>) -> Option<String> {
    let mut args = args.into_iter().skip(1);
    if args.next().as_deref() != Some(BROWSER_ARG) {
        return None;
    }
    Some(
        args.next()
            .filter(|u| is_youtube(u))
            .unwrap_or_else(|| HOME.into()),
    )
}

/// True for the button's navigation, which the window must block.
pub fn is_action(url: &Url) -> bool {
    url.host_str() == Some(ACTION_HOST)
}

/// The YouTube link the button sent, if it is one.
pub fn action_target(url: &Url) -> Option<String> {
    if !is_action(url) {
        return None;
    }
    url.query_pairs()
        .find(|(k, _)| k == "url")
        .map(|(_, v)| v.into_owned())
        .filter(|v| is_youtube(v))
}

/// A YouTube link reported by the window process on stdout.
pub fn parse_report(line: &str) -> Option<String> {
    line.trim_end()
        .strip_prefix(REPORT)
        .filter(|u| is_youtube(u))
        .map(String::from)
}

/// Script added to every YouTube page: the floating "Download" button. It lives in
/// a closed shadow root, so YouTube's styles and scripts don't reach it, and is built
/// without innerHTML, which YouTube's Trusted Types policy forbids.
pub fn init_script(label: &str) -> String {
    let label = serde_json::to_string(label).expect("a string serializes");
    format!(
        r#"(() => {{
  if (window.top !== window) return;
  const LABEL = {label};
  const ACTION = "https://{ACTION_HOST}/open?url=";
  // Pages yt-dlp can download: videos, shorts, playlists and channels.
  const downloadable = () => {{
    const p = location.pathname;
    if (p === "/watch") return new URLSearchParams(location.search).has("v");
    return /^\/(shorts|live|playlist|channel|c|user|@)/.test(p);
  }};
  let host;
  const update = () => {{
    if (!document.documentElement) return;
    if (!host) {{
      host = document.createElement("div");
      const root = host.attachShadow({{ mode: "closed" }});
      const style = document.createElement("style");
      style.textContent = `button {{ position: fixed; right: 24px; bottom: 24px; z-index: 2147483647;
        display: flex; align-items: center; gap: 8px; padding: 12px 20px; border: 0;
        border-radius: 999px; background: #3d5af1; color: #fff; cursor: pointer;
        font: 600 15px system-ui, sans-serif; box-shadow: 0 8px 24px rgb(0 0 0 / 0.35); }}
        button:hover {{ background: #2f49d6; }}`;
      const button = document.createElement("button");
      button.textContent = "⬇ " + LABEL;
      button.addEventListener("click", () => {{
        location.href = ACTION + encodeURIComponent(location.href);
      }});
      root.append(style, button);
    }}
    if (!host.isConnected) document.documentElement.append(host);
    host.style.display = downloadable() ? "" : "none";
  }};
  // YouTube changes pages without reloading.
  setInterval(update, 500);
}})();
"#
    )
}

/// Runs this process as the YouTube window until it is closed or the app is gone.
pub fn run(mut context: tauri::Context<tauri::Wry>, start: String) {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    // The app's own window comes from the config; this process shows only YouTube.
    context.config_mut().app.windows.clear();
    let label = std::env::var(LABEL_ENV).unwrap_or_else(|_| "Download".into());
    let proxy = std::env::var(PROXY_ENV)
        .ok()
        .and_then(|p| p.parse::<Url>().ok());
    tauri::Builder::default()
        .setup(move |app| {
            let mut builder =
                WebviewWindowBuilder::new(app, "youtube", WebviewUrl::External(start.parse()?))
                    .title("YouTube — Cliprigo")
                    .inner_size(1180.0, 800.0)
                    .min_inner_size(480.0, 400.0)
                    // Separate from the app's webview: its own cookies and YouTube login.
                    .data_directory(app.path().app_data_dir()?.join("youtube"))
                    .initialization_script(init_script(&label))
                    .on_navigation(|url| {
                        if !is_action(url) {
                            return true;
                        }
                        if let Some(target) = action_target(url) {
                            report(&target);
                        }
                        false
                    });
            if let Some(proxy) = proxy {
                builder = builder.proxy_url(proxy);
            }
            let window = builder.build()?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                for line in std::io::stdin().lines().map_while(Result::ok) {
                    let Some(url) = line
                        .strip_prefix(NAVIGATE)
                        .filter(|u| is_youtube(u))
                        .and_then(|u| u.parse().ok())
                    else {
                        continue;
                    };
                    let _ = window.navigate(url);
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
                // The app closed the pipe or is gone.
                handle.exit(0);
            });
            Ok(())
        })
        .run(context)
        .expect("error while running the YouTube window");
}

fn report(url: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{REPORT}{url}");
    let _ = out.flush();
}

/// The YouTube window process, seen from the app.
#[derive(Default)]
pub struct Browser {
    stdin: std::sync::Mutex<Option<std::process::ChildStdin>>,
}

impl Browser {
    /// Shows `url` in the YouTube window, starting it if needed. Links it sends are
    /// emitted as `app://open-url`; a crash as `app://browser-crashed`.
    pub fn open(
        &self,
        app: &tauri::AppHandle,
        url: &str,
        label: &str,
        proxy: Option<String>,
    ) -> crate::error::AppResult<()> {
        use std::io::{BufRead, Write};
        use std::process::{Command, Stdio};
        use tauri::Emitter;

        let mut stdin = self.stdin.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(pipe) = stdin.as_mut()
            && writeln!(pipe, "{NAVIGATE}{url}")
                .and_then(|()| pipe.flush())
                .is_ok()
        {
            return Ok(());
        }
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([BROWSER_ARG, url])
            .env(LABEL_ENV, label)
            .env_remove(PROXY_ENV)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Webviews can't answer a proxy's password prompt; such a proxy is left to
        // the system settings.
        if let Some(proxy) =
            proxy.filter(|p| p.parse::<Url>().is_ok_and(|u| u.username().is_empty()))
        {
            command.env(PROXY_ENV, proxy);
        }
        let mut child = command.spawn()?;
        *stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let app = app.clone();
        std::thread::spawn(move || {
            if let Some(stdout) = stdout {
                for line in std::io::BufReader::new(stdout)
                    .lines()
                    .map_while(Result::ok)
                {
                    let Some(url) = parse_report(&line) else {
                        continue;
                    };
                    let _ = app.emit("app://open-url", url);
                    if let Some(main) = tauri::Manager::get_webview_window(&app, "main") {
                        let _ = main.unminimize();
                        let _ = main.set_focus();
                    }
                }
            }
            let crashed = !child.wait().is_ok_and(|s| s.success());
            if crashed {
                tracing::warn!("YouTube window ended abnormally");
                let _ = app.emit("app://browser-crashed", ());
            }
            if let Some(browser) = tauri::Manager::try_state::<Browser>(&app) {
                *browser.stdin.lock().unwrap_or_else(|e| e.into_inner()) = None;
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn started_as_the_youtube_window() {
        assert_eq!(browser_mode(args(&["cliprigo"])), None);
        assert_eq!(
            browser_mode(args(&[
                "cliprigo",
                BROWSER_ARG,
                "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
            ])),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".into())
        );
        // Only YouTube is opened; anything else shows its home page.
        assert_eq!(
            browser_mode(args(&["cliprigo", BROWSER_ARG, "https://example.com/"])),
            Some(HOME.into())
        );
        assert_eq!(
            browser_mode(args(&["cliprigo", BROWSER_ARG])),
            Some(HOME.into())
        );
    }

    #[test]
    fn button_navigation_carries_the_page() {
        let url: Url = "https://cliprigo.invalid/open?url=https%3A%2F%2Fwww.youtube.com%2Fwatch%3Fv%3DdQw4w9WgXcQ%26t%3D5"
            .parse()
            .unwrap();
        assert!(is_action(&url));
        assert_eq!(
            action_target(&url).as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=5")
        );

        // Blocked, but nothing to open.
        let other: Url = "https://cliprigo.invalid/open?url=https%3A%2F%2Fexample.com%2F"
            .parse()
            .unwrap();
        assert!(is_action(&other));
        assert_eq!(action_target(&other), None);

        let page: Url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
            .parse()
            .unwrap();
        assert!(!is_action(&page));
    }

    #[test]
    fn reports_are_youtube_links_only() {
        assert_eq!(
            parse_report("OPEN https://www.youtube.com/watch?v=dQw4w9WgXcQ\r").as_deref(),
            Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
        );
        assert_eq!(parse_report("OPEN https://example.com/"), None);
        assert_eq!(parse_report("Gtk-WARNING: something"), None);
        assert_eq!(parse_report(""), None);
    }

    #[test]
    fn button_label_cannot_break_the_script() {
        let script = init_script("Скачать \"</script><x>");
        assert!(script.contains(r#""Скачать \"</script><x>""#));
        assert!(script.contains(ACTION_HOST));
    }
}
