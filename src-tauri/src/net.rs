use std::sync::{Arc, RwLock};

use crate::error::{AppError, AppResult};

const SCHEMES: &[&str] = &["http", "https", "socks5", "socks5h"];

/// Checks a proxy URL from the settings. Empty means "no explicit proxy" (system
/// settings and environment variables apply).
pub fn validate_proxy(input: &str) -> AppResult<Option<String>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let invalid = || AppError::InvalidProxy(mask_proxy(trimmed));
    let url = url::Url::parse(trimmed).map_err(|_| invalid())?;
    if !SCHEMES.contains(&url.scheme()) || url.host_str().is_none_or(str::is_empty) {
        return Err(invalid());
    }
    Ok(Some(trimmed.to_string()))
}

/// Proxy URL safe to show in logs and reports: credentials are hidden.
pub fn mask_proxy(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return "(unparsable proxy URL)".into();
    };
    let had_user = !parsed.username().is_empty();
    let had_password = parsed.password().is_some();
    let _ = parsed.set_username(if had_user { "***" } else { "" });
    let _ = parsed.set_password(had_password.then_some("***"));
    parsed.to_string().trim_end_matches('/').to_string()
}

/// The proxy currently configured in settings, shared by everything that goes online.
#[derive(Clone, Default)]
pub struct ProxySetting(Arc<RwLock<Option<String>>>);

impl ProxySetting {
    pub fn get(&self) -> Option<String> {
        self.0.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set(&self, proxy: Option<String>) {
        *self.0.write().unwrap_or_else(|e| e.into_inner()) = proxy;
    }
}

/// Proxy for live tests: `CLIPRIGO_TEST_PROXY=http://127.0.0.1:8888 cargo test live_ ...`.
#[cfg(test)]
pub fn test_proxy() -> ProxySetting {
    let setting = ProxySetting::default();
    setting.set(
        std::env::var("CLIPRIGO_TEST_PROXY")
            .ok()
            .filter(|p| !p.trim().is_empty()),
    );
    setting
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_means_no_proxy() {
        assert_eq!(validate_proxy("").unwrap(), None);
        assert_eq!(validate_proxy("   ").unwrap(), None);
    }

    #[test]
    fn accepts_common_proxy_urls() {
        for url in [
            "http://127.0.0.1:8888",
            "https://proxy.example.com:443",
            "socks5://127.0.0.1:1080",
            "socks5h://user:pass@host:1080",
            "http://user:p%40ss@10.0.0.1:3128",
        ] {
            assert_eq!(validate_proxy(url).unwrap().as_deref(), Some(url), "{url}");
        }
        // Surrounding spaces are trimmed.
        assert_eq!(
            validate_proxy("  http://127.0.0.1:8888 ")
                .unwrap()
                .as_deref(),
            Some("http://127.0.0.1:8888")
        );
    }

    #[test]
    fn rejects_invalid_urls() {
        for url in [
            "127.0.0.1:8888",
            "ftp://127.0.0.1:21",
            "http://",
            "not a url",
            "http://host:99999",
        ] {
            let err = validate_proxy(url).unwrap_err();
            assert_eq!(err.code(), "invalid_proxy", "{url}");
        }
    }

    #[test]
    fn masks_credentials() {
        assert_eq!(mask_proxy("http://127.0.0.1:8888"), "http://127.0.0.1:8888");
        assert_eq!(
            mask_proxy("http://user:secret@201.18.212.73:8888"),
            "http://***:***@201.18.212.73:8888"
        );
        assert_eq!(
            mask_proxy("socks5://user@host:1080"),
            "socks5://***@host:1080"
        );
        assert!(!mask_proxy("garbage with secret").contains("secret"));
    }

    #[test]
    fn shared_setting() {
        let a = ProxySetting::default();
        let b = a.clone();
        a.set(Some("http://127.0.0.1:8888".into()));
        assert_eq!(b.get().as_deref(), Some("http://127.0.0.1:8888"));
    }
}
