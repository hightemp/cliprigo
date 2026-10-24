//! "Check proxy" in Settings: can YouTube be reached with the proxy typed in?

use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::AppResult;

/// Answers 204 quickly; the same host yt-dlp talks to.
pub const YOUTUBE_PROBE_URL: &str = "https://www.youtube.com/generate_204";
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// Nothing listens at the proxy address, or it is unreachable.
    ProxyUnreachable,
    /// The proxy wants a login and password (HTTP 407).
    ProxyAuth,
    /// The proxy answered but refused to open a connection to YouTube.
    ProxyRefused,
    /// No connection to YouTube without a proxy.
    Unreachable,
    Timeout,
    /// YouTube (or the proxy) answered with an unexpected HTTP status.
    HttpStatus,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyCheck {
    pub ok: bool,
    /// False when no proxy was given: the system settings and environment apply.
    pub via_proxy: bool,
    pub elapsed_ms: u64,
    pub failure: Option<Failure>,
    /// Technical details for "Copy details", with credentials hidden.
    pub detail: String,
}

/// Requests `target` through `proxy` (validated like the setting; empty = none).
/// Only an invalid proxy URL is an error: a failed check is a normal result.
pub async fn check(proxy: &str, target: &str, timeout: Duration) -> AppResult<ProxyCheck> {
    let proxy = crate::net::validate_proxy(proxy)?;
    let mut builder = crate::net::client_builder()
        .user_agent(concat!("Cliprigo/", env!("CARGO_PKG_VERSION")))
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none());
    if let Some(proxy) = &proxy {
        builder = builder.proxy(reqwest::Proxy::all(proxy)?);
    }
    let client = builder.build()?;
    let via_proxy = proxy.is_some();

    let started = Instant::now();
    let result = client.get(target).send().await;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let (failure, detail) = match result {
        Ok(resp) if resp.status().is_success() => (None, resp.status().to_string()),
        Ok(resp) if resp.status() == reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED => {
            (Some(Failure::ProxyAuth), resp.status().to_string())
        }
        Ok(resp) => (Some(Failure::HttpStatus), resp.status().to_string()),
        Err(e) => {
            let detail = error_chain(&e);
            (Some(classify(&e, &detail, via_proxy)), detail)
        }
    };
    let detail = crate::diagnostics::scrub_credentials(&detail);
    if let Some(failure) = failure {
        tracing::warn!(?failure, via_proxy, elapsed_ms, %detail, "proxy check failed");
    }
    Ok(ProxyCheck {
        ok: failure.is_none(),
        via_proxy,
        elapsed_ms,
        failure,
        detail,
    })
}

/// reqwest's Display hides the cause ("error sending request"); the chain has it.
fn error_chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![e.to_string()];
    let mut source = e.source();
    while let Some(s) = source {
        let text = s.to_string();
        if !parts.iter().any(|p| p.contains(&text)) {
            parts.push(text);
        }
        source = s.source();
    }
    parts.join(": ")
}

fn classify(e: &reqwest::Error, detail: &str, via_proxy: bool) -> Failure {
    let lower = detail.to_lowercase();
    if e.is_timeout() || lower.contains("timed out") {
        Failure::Timeout
    } else if lower.contains("407") || lower.contains("proxy authorization required") {
        Failure::ProxyAuth
    } else if via_proxy
        && ["underlying connection", "tcp connect", "dns error"]
            .iter()
            .any(|s| lower.contains(s))
    {
        // reqwest reports these as "tunnel error" too, but the proxy was never reached.
        Failure::ProxyUnreachable
    } else if lower.contains("tunnel") {
        Failure::ProxyRefused
    } else if e.is_connect() {
        if via_proxy {
            Failure::ProxyUnreachable
        } else {
            Failure::Unreachable
        }
    } else {
        Failure::Unknown
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    const HTTPS_TARGET: &str = "https://www.youtube.com/generate_204";
    /// Plain http: the proxy answers the request itself, no CONNECT tunnel.
    const HTTP_TARGET: &str = "http://www.youtube.com/generate_204";
    const TIMEOUT: Duration = Duration::from_secs(5);

    /// A fake proxy that answers every request with `response` (None = stays silent).
    async fn fake_proxy(response: Option<&'static str>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let _ = socket.read(&mut buf).await;
                    match response {
                        Some(r) => {
                            let _ = socket.write_all(r.as_bytes()).await;
                            let _ = socket.shutdown().await;
                        }
                        None => tokio::time::sleep(Duration::from_secs(60)).await,
                    }
                });
            }
        });
        format!("http://{addr}")
    }

    async fn closed_port() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn success_through_proxy() {
        let proxy = fake_proxy(Some("HTTP/1.1 204 No Content\r\n\r\n")).await;
        let r = check(&proxy, HTTP_TARGET, TIMEOUT).await.unwrap();
        assert!(r.ok, "{r:?}");
        assert!(r.via_proxy);
        assert_eq!(r.failure, None);
    }

    #[tokio::test]
    async fn nothing_listening() {
        let r = check(&closed_port().await, HTTPS_TARGET, TIMEOUT)
            .await
            .unwrap();
        assert!(!r.ok);
        assert_eq!(r.failure, Some(Failure::ProxyUnreachable), "{r:?}");
    }

    #[tokio::test]
    async fn proxy_wants_credentials() {
        let resp = "HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n";
        let proxy = fake_proxy(Some(resp)).await;
        for target in [HTTPS_TARGET, HTTP_TARGET] {
            let r = check(&proxy, target, TIMEOUT).await.unwrap();
            assert_eq!(r.failure, Some(Failure::ProxyAuth), "{target}: {r:?}");
        }
    }

    /// The error seen on Windows: "Tunnel connection failed: 400 Bad Request".
    #[tokio::test]
    async fn proxy_refuses_tunnel() {
        let proxy = fake_proxy(Some(
            "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n",
        ))
        .await;
        let r = check(&proxy, HTTPS_TARGET, TIMEOUT).await.unwrap();
        assert_eq!(r.failure, Some(Failure::ProxyRefused), "{r:?}");
    }

    #[tokio::test]
    async fn silent_proxy_times_out() {
        let proxy = fake_proxy(None).await;
        let r = check(&proxy, HTTPS_TARGET, Duration::from_millis(500))
            .await
            .unwrap();
        assert_eq!(r.failure, Some(Failure::Timeout), "{r:?}");
    }

    #[tokio::test]
    async fn unexpected_status() {
        let resp = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n";
        let proxy = fake_proxy(Some(resp)).await;
        let r = check(&proxy, HTTP_TARGET, TIMEOUT).await.unwrap();
        assert_eq!(r.failure, Some(Failure::HttpStatus), "{r:?}");
        assert!(r.detail.contains("403"), "{r:?}");
    }

    #[tokio::test]
    async fn invalid_proxy_is_an_error_and_details_hide_credentials() {
        let err = check("127.0.0.1:8888", HTTPS_TARGET, TIMEOUT)
            .await
            .unwrap_err();
        assert_eq!(err.code(), "invalid_proxy");

        let port = closed_port().await.replace("http://", "");
        let r = check(&format!("http://user:secret@{port}"), HTTPS_TARGET, TIMEOUT)
            .await
            .unwrap();
        assert!(!r.detail.contains("secret"), "{r:?}");
    }

    #[tokio::test]
    #[ignore = "network"]
    async fn live_youtube_reachable() {
        let proxy = crate::net::test_proxy().get().unwrap_or_default();
        let r = check(&proxy, YOUTUBE_PROBE_URL, CHECK_TIMEOUT)
            .await
            .unwrap();
        eprintln!("{r:?}");
        assert!(r.ok, "{r:?}");
    }
}
