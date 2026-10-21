use std::path::Path;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use reqwest::{Client, Response, StatusCode, header, redirect};
use sha2::Digest;
use tokio::io::AsyncWriteExt;

use crate::error::{AppError, AppResult};

const USER_AGENT: &str = concat!("Cliprigo/", env!("CARGO_PKG_VERSION"));
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

pub struct Http {
    client: Client,
    /// Used to read the `releases/latest` redirect without following it.
    no_redirect: Client,
}

impl Http {
    pub fn new() -> AppResult<Self> {
        let build = |policy| {
            Client::builder()
                .user_agent(USER_AGENT)
                .connect_timeout(Duration::from_secs(15))
                .redirect(policy)
                .build()
        };
        Ok(Self {
            client: build(redirect::Policy::limited(10))?,
            no_redirect: build(redirect::Policy::none())?,
        })
    }

    /// Resolves the latest release tag from the `releases/latest` redirect instead of the
    /// REST API, which is limited to 60 unauthenticated requests per hour.
    pub async fn latest_tag(&self, repo: &str) -> AppResult<String> {
        let url = format!("https://github.com/{repo}/releases/latest");
        let resp = check_status(self.no_redirect.get(&url).send().await?, &url, true)?;
        resp.headers()
            .get(header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|loc| loc.rsplit_once("/tag/"))
            .map(|(_, tag)| tag.to_string())
            .filter(|tag| !tag.is_empty())
            .ok_or_else(|| AppError::Network(format!("no release tag in redirect from {url}")))
    }

    pub async fn get_text(&self, url: &str) -> AppResult<String> {
        let resp = check_status(self.client.get(url).send().await?, url, false)?;
        Ok(resp.text().await?)
    }

    /// Release notes body via the REST API (rate limited, so callers treat it as optional).
    pub async fn release_body(&self, repo: &str, tag: &str) -> AppResult<String> {
        #[derive(serde::Deserialize)]
        struct Release {
            body: Option<String>,
        }
        let url = format!("https://api.github.com/repos/{repo}/releases/tags/{tag}");
        let resp = self
            .client
            .get(&url)
            .header(header::ACCEPT, "application/vnd.github+json")
            .send()
            .await?;
        let release: Release = check_status(resp, &url, false)?.json().await?;
        Ok(release.body.unwrap_or_default())
    }

    /// Streams `url` into `dest`, reporting `(downloaded, total)` at most every 100 ms.
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        mut on_progress: impl FnMut(u64, Option<u64>),
    ) -> AppResult<()> {
        let resp = check_status(self.client.get(url).send().await?, url, false)?;
        let total = resp.content_length();
        let mut file = tokio::fs::File::create(dest).await?;
        let mut stream = resp.bytes_stream();
        let mut downloaded = 0u64;
        let mut last_report = Instant::now();
        on_progress(0, total);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            if last_report.elapsed() >= PROGRESS_INTERVAL {
                on_progress(downloaded, total);
                last_report = Instant::now();
            }
        }
        file.flush().await?;
        file.sync_all().await?;
        on_progress(downloaded, total);
        Ok(())
    }
}

fn check_status(resp: Response, url: &str, allow_redirect: bool) -> AppResult<Response> {
    let status = resp.status();
    if status.is_success() || (allow_redirect && status.is_redirection()) {
        return Ok(resp);
    }
    let rate_limited = status == StatusCode::TOO_MANY_REQUESTS
        || (status == StatusCode::FORBIDDEN
            && resp
                .headers()
                .get("x-ratelimit-remaining")
                .is_some_and(|v| v == "0"));
    if rate_limited {
        return Err(AppError::RateLimited);
    }
    Err(AppError::Network(format!("HTTP {status} for {url}")))
}

pub fn release_asset_url(repo: &str, tag: &str, asset: &str) -> String {
    format!("https://github.com/{repo}/releases/download/{tag}/{asset}")
}

/// Finds the hash for `name` in checksum listings like `<hash>  <name>`, `<hash> *<name>`,
/// a markdown list item `- <hash>  <name>`, or PowerShell `Get-FileHash` output.
pub fn find_checksum(listing: &str, name: &str) -> Option<String> {
    listing
        .lines()
        .find_map(|line| {
            let line = line.trim().trim_start_matches("- ");
            let (hash, file) = line.split_once(char::is_whitespace)?;
            let file = file.trim().trim_start_matches('*');
            (file == name && is_hex(hash)).then(|| hash.to_ascii_lowercase())
        })
        .or_else(|| find_powershell_checksum(listing, name))
}

/// `Get-FileHash` output, used for Deno's Windows builds:
/// `Hash : <HEX>` followed by `Path : C:\...\<name>`.
fn find_powershell_checksum(listing: &str, name: &str) -> Option<String> {
    let mut hash = None;
    for line in listing.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Hash" if is_hex(value) => hash = Some(value.to_ascii_lowercase()),
            "Path" => {
                let file = value.rsplit(['\\', '/']).next().unwrap_or(value);
                if file == name {
                    return hash;
                }
                hash = None;
            }
            _ => {}
        }
    }
    None
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Hex digest of a file, computed off the async runtime.
pub async fn file_digest<D: Digest + Send + 'static>(path: &Path) -> AppResult<String> {
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || -> AppResult<String> {
        use std::io::Read;
        let mut file = std::fs::File::open(path)?;
        let mut hasher = D::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hex::encode(hasher.finalize()))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

pub async fn verify<D: Digest + Send + 'static>(
    path: &Path,
    expected: &str,
    name: &str,
) -> AppResult<()> {
    let actual = file_digest::<D>(path).await?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        tracing::error!(name, expected, actual, "checksum mismatch");
        Err(AppError::Checksum(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_listing_formats() {
        let sums = "aa11  yt-dlp\nbb22  yt-dlp.exe\ncc33 *yt-dlp_macos\n";
        assert_eq!(find_checksum(sums, "yt-dlp.exe").as_deref(), Some("bb22"));
        assert_eq!(find_checksum(sums, "yt-dlp_macos").as_deref(), Some("cc33"));
        assert_eq!(find_checksum(sums, "yt-dlp_linux"), None);

        let body =
            "MD5 sums:\n - 2E42  ffmpeg-linux-arm64\n - 5bb9  ffmpeg-linux-arm64-ubuntu-22.04\n";
        assert_eq!(
            find_checksum(body, "ffmpeg-linux-arm64").as_deref(),
            Some("2e42")
        );
        assert_eq!(find_checksum(body, "MD5"), None);
    }

    /// Deno's Windows `.sha256sum` files are PowerShell `Get-FileHash` output
    /// (CRLF, uppercase hash, full build path), captured from the v2.9.8 release.
    #[test]
    fn checksum_in_powershell_format() {
        let listing = "\r\nAlgorithm : SHA256\r\nHash      : CBA84B26E0DF5AE7F74FC66B86A95B866872A86292EBD84FA1E959927EDD93BF\r\nPath      : C:\\a\\deno\\deno\\target\\release\\deno-x86_64-pc-windows-msvc.zip\r\n\r\n";
        assert_eq!(
            find_checksum(listing, "deno-x86_64-pc-windows-msvc.zip").as_deref(),
            Some("cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf")
        );
        // The path must name the requested file.
        assert_eq!(
            find_checksum(listing, "deno-aarch64-pc-windows-msvc.zip"),
            None
        );
    }

    #[tokio::test]
    async fn digest_of_file() {
        let dir = std::env::temp_dir().join(format!("cliprigo-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("abc.txt");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            file_digest::<sha2::Sha256>(&path).await.unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(
            verify::<md5::Md5>(&path, "900150983cd24fb0d6963f7d28e17f72", "abc")
                .await
                .is_ok()
        );
        assert!(matches!(
            verify::<md5::Md5>(&path, "00", "abc").await,
            Err(AppError::Checksum(_))
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
