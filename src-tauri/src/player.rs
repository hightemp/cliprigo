//! Serves the YouTube player page from http://127.0.0.1. YouTube refuses to play an
//! embed whose page sends no Referer ("error 153"), and the app's own pages on macOS
//! and Linux (tauri://localhost) send none; a page from a local HTTP origin does.

use std::io;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// App state: the server, started by the first `player_url` call.
pub type Player = tokio::sync::OnceCell<PlayerServer>;

/// Local server for the player page.
pub struct PlayerServer {
    port: u16,
}

impl PlayerServer {
    pub async fn start() -> io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        tokio::spawn(serve(stream));
                    }
                    // E.g. out of file handles: wait instead of spinning.
                    Err(_) => tokio::time::sleep(Duration::from_millis(100)).await,
                }
            }
        });
        Ok(Self { port })
    }

    /// Page that plays `video_id`, or `None` for something that is not a video id.
    pub fn url(&self, video_id: &str, lang: &str) -> Option<String> {
        is_video_id(video_id).then(|| {
            format!(
                "http://127.0.0.1:{}/embed/{video_id}?hl={}",
                self.port,
                language(lang)
            )
        })
    }
}

/// YouTube video ids: 11 characters of the URL-safe base64 alphabet.
pub fn is_video_id(id: &str) -> bool {
    id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Player interface language: two lowercase letters, English otherwise.
fn language(lang: &str) -> &str {
    if lang.len() == 2 && lang.bytes().all(|b| b.is_ascii_lowercase()) {
        lang
    } else {
        "en"
    }
}

const MAX_REQUEST: usize = 8 * 1024;

async fn serve(mut stream: tokio::net::TcpStream) {
    let mut buf = Vec::with_capacity(1024);
    let read = tokio::time::timeout(Duration::from_secs(5), async {
        let mut chunk = [0u8; 1024];
        while !buf.windows(4).any(|w| w == b"\r\n\r\n") && buf.len() < MAX_REQUEST {
            match stream.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
            }
        }
    })
    .await;
    if read.is_err() {
        return;
    }
    let text = String::from_utf8_lossy(&buf);
    let (status, body) = respond(text.lines().next().unwrap_or_default());
    let reason = if status == 200 { "OK" } else { "Not Found" };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Content-Security-Policy: default-src 'none'; frame-src https://www.youtube.com; style-src 'unsafe-inline'\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Cache-Control: no-store\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Status and HTML for the first line of a request.
fn respond(request_line: &str) -> (u16, String) {
    let not_found = (404, "Not found".to_string());
    let mut parts = request_line.split(' ');
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return not_found;
    };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let Some(id) = path.strip_prefix("/embed/").filter(|id| is_video_id(id)) else {
        return not_found;
    };
    let lang = query
        .split('&')
        .find_map(|p| p.strip_prefix("hl="))
        .map_or("en", language);
    (200, page(id, lang))
}

fn page(id: &str, lang: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<style>html,body{{margin:0;height:100%;background:#000}}iframe{{display:block;border:0;width:100%;height:100%}}</style>
</head>
<body>
<iframe src="https://www.youtube.com/embed/{id}?autoplay=1&amp;rel=0&amp;playsinline=1&amp;hl={lang}" title="YouTube" allow="autoplay; encrypted-media; picture-in-picture; fullscreen" allowfullscreen referrerpolicy="strict-origin-when-cross-origin"></iframe>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_ids() {
        assert!(is_video_id("dQw4w9WgXcQ"));
        assert!(is_video_id("a-b_c-d_e-f"));
        assert!(!is_video_id("dQw4w9WgXc"));
        assert!(!is_video_id("dQw4w9WgXcQQ"));
        assert!(!is_video_id("dQw4w9WgX<Q"));
        assert!(!is_video_id(""));
    }

    #[test]
    fn page_embeds_the_video() {
        let (status, body) = respond("GET /embed/dQw4w9WgXcQ?hl=ru HTTP/1.1");
        assert_eq!(status, 200);
        assert!(body.contains(r#"src="https://www.youtube.com/embed/dQw4w9WgXcQ?"#));
        assert!(body.contains("hl=ru"));
        assert!(body.contains("allowfullscreen"));
    }

    #[test]
    fn anything_else_is_not_found() {
        for line in [
            "GET / HTTP/1.1",
            "GET /embed/ HTTP/1.1",
            "GET /embed/dQw4w9WgXcQ/x HTTP/1.1",
            "GET /embed/\"><script> HTTP/1.1",
            "POST /embed/dQw4w9WgXcQ HTTP/1.1",
            "",
        ] {
            assert_eq!(respond(line).0, 404, "{line}");
        }
        // Only a two-letter language makes it into the page.
        let (_, body) = respond("GET /embed/dQw4w9WgXcQ?hl=\"><x HTTP/1.1");
        assert!(!body.contains("<x"));
    }

    #[tokio::test]
    async fn serves_the_page_on_localhost() {
        let server = PlayerServer::start().await.unwrap();
        let url = server.url("dQw4w9WgXcQ", "en").unwrap();
        assert!(url.starts_with("http://127.0.0.1:"), "{url}");
        assert_eq!(server.url("nope", "en"), None);

        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), 200);
        let html = response.text().await.unwrap();
        assert!(html.contains("youtube.com/embed/dQw4w9WgXcQ"));

        let missing = client
            .get(url.replace("dQw4w9WgXcQ", "x"))
            .send()
            .await
            .unwrap();
        assert_eq!(missing.status(), 404);
    }
}
