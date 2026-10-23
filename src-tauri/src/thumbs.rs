use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::components::fetch::Http;
use crate::error::{AppError, AppResult};
use crate::net::ProxySetting;

const ALLOWED_DOMAINS: &[&str] = &["ytimg.com", "ggpht.com"];
const EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// Only YouTube's image hosts: the UI must not be able to make the app fetch
/// arbitrary URLs.
pub fn is_allowed(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str().is_some_and(|host| {
            ALLOWED_DOMAINS
                .iter()
                .any(|d| host == *d || host.ends_with(&format!(".{d}")))
        })
}

/// Cache file for `url`: a hash of the URL plus the image extension.
pub fn cache_path(dir: &Path, url: &str) -> PathBuf {
    let hash = hex::encode(Sha256::digest(url.as_bytes()));
    let ext = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            Path::new(u.path())
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
        })
        .filter(|e| EXTENSIONS.contains(&e.as_str()))
        .unwrap_or_else(|| "jpg".into());
    dir.join(format!("{}.{ext}", &hash[..32]))
}

/// Thumbnails stored under the app cache, so the history shows them offline.
pub struct ThumbCache {
    dir: PathBuf,
    proxy: ProxySetting,
    tmp_counter: AtomicU64,
}

impl ThumbCache {
    pub fn new(dir: PathBuf, proxy: ProxySetting) -> AppResult<Self> {
        std::fs::create_dir_all(&dir)?;
        // Downloads interrupted by a crash or a failed rename.
        for entry in std::fs::read_dir(&dir)?.flatten() {
            if entry.file_name().to_string_lossy().starts_with(".tmp-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(Self {
            dir,
            proxy,
            tmp_counter: AtomicU64::new(0),
        })
    }

    /// Local path of the thumbnail, downloading it on first use.
    pub async fn get(&self, url: &str) -> AppResult<PathBuf> {
        if !is_allowed(url) {
            return Err(AppError::Other(format!(
                "thumbnail host not allowed: {url}"
            )));
        }
        let path = cache_path(&self.dir, url);
        if path.is_file() {
            return Ok(path);
        }
        // Unique temp name: two views may request the same image at once.
        let n = self.tmp_counter.fetch_add(1, Ordering::Relaxed);
        let tmp = self.dir.join(format!(".tmp-{}-{n}", std::process::id()));
        let http = Http::new(self.proxy.get().as_deref())?;
        let result = http.download(url, &tmp, |_, _| {}).await;
        match result {
            Ok(()) => match std::fs::rename(&tmp, &path) {
                Ok(()) => Ok(path),
                // Windows: another request already put the file there and it is open.
                Err(_) if path.is_file() => {
                    let _ = std::fs::remove_file(&tmp);
                    Ok(path)
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp);
                    Err(e.into())
                }
            },
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_temp_files_are_removed_on_start() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".tmp-123-0"), b"partial").unwrap();
        std::fs::write(dir.path().join("abc.jpg"), b"img").unwrap();
        ThumbCache::new(dir.path().to_owned(), crate::net::test_proxy()).unwrap();
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, ["abc.jpg"]);
    }

    #[test]
    fn allows_only_youtube_image_hosts_over_https() {
        for url in [
            "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg",
            "https://i.ytimg.com/vi_webp/dQw4w9WgXcQ/maxresdefault.webp",
            "https://i9.ytimg.com/vi/x/mqdefault.jpg?sqp=abc",
            "https://yt3.ggpht.com/abc=s88",
        ] {
            assert!(is_allowed(url), "{url}");
        }
        for url in [
            "http://i.ytimg.com/vi/x/hqdefault.jpg",
            "https://ytimg.com.evil.com/x.jpg",
            "https://evil.com/i.ytimg.com/x.jpg",
            "https://example.com/x.jpg",
            "file:///etc/passwd",
            "data:image/png;base64,AAAA",
            "not a url",
        ] {
            assert!(!is_allowed(url), "{url}");
        }
    }

    #[test]
    fn cache_path_is_stable_and_keeps_extension() {
        let dir = Path::new("/cache/thumbs");
        let a = cache_path(dir, "https://i.ytimg.com/vi/a/hqdefault.jpg?sqp=1");
        assert_eq!(
            a,
            cache_path(dir, "https://i.ytimg.com/vi/a/hqdefault.jpg?sqp=1")
        );
        assert_ne!(
            a,
            cache_path(dir, "https://i.ytimg.com/vi/b/hqdefault.jpg?sqp=1")
        );
        assert_eq!(a.parent(), Some(dir));
        assert_eq!(a.extension().unwrap(), "jpg");
        let webp = cache_path(dir, "https://i.ytimg.com/vi_webp/a/maxresdefault.webp");
        assert_eq!(webp.extension().unwrap(), "webp");
        // Unknown or missing extensions fall back to jpg.
        let other = cache_path(dir, "https://yt3.ggpht.com/abc=s88");
        assert_eq!(other.extension().unwrap(), "jpg");
        let stem = a.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(stem.len() >= 32 && stem.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
