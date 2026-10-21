/// Hosts that must never go through an HTTP proxy: the dev server and local IPC.
const LOCAL_HOSTS: &[&str] = &["localhost", "127.0.0.1", "::1"];

/// `existing` NO_PROXY value with the local hosts appended (no duplicates).
pub fn merge_no_proxy(existing: Option<&str>) -> String {
    let mut entries: Vec<String> = existing
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(String::from)
        .collect();
    // "*" already bypasses the proxy for everything.
    if entries.iter().any(|e| e == "*") {
        return entries.join(",");
    }
    for host in LOCAL_HOSTS {
        if !entries.iter().any(|e| e.eq_ignore_ascii_case(host)) {
            entries.push((*host).to_string());
        }
    }
    entries.join(",")
}

/// With `HTTP_PROXY` set and no `NO_PROXY`, the webview would fetch the dev server
/// (and yt-dlp would reach local addresses) through the proxy. Local traffic never
/// needs it, so exclude it for this process and its children.
///
/// Must run before any other thread starts: changing the environment is not
/// thread-safe.
pub fn bypass_proxy_for_localhost() {
    let merged = merge_no_proxy(
        std::env::var("NO_PROXY")
            .or_else(|_| std::env::var("no_proxy"))
            .ok()
            .as_deref(),
    );
    // SAFETY: called first thing in `run()`, while the process is single-threaded.
    unsafe {
        std::env::set_var("NO_PROXY", &merged);
        std::env::set_var("no_proxy", &merged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_local_hosts_and_keeps_existing_entries() {
        assert_eq!(merge_no_proxy(None), "localhost,127.0.0.1,::1");
        assert_eq!(merge_no_proxy(Some("")), "localhost,127.0.0.1,::1");
        assert_eq!(
            merge_no_proxy(Some("corp.local, 10.0.0.0/8")),
            "corp.local,10.0.0.0/8,localhost,127.0.0.1,::1"
        );
        assert_eq!(
            merge_no_proxy(Some("127.0.0.1,LOCALHOST")),
            "127.0.0.1,LOCALHOST,::1"
        );
        assert_eq!(merge_no_proxy(Some("*")), "*");
    }
}
