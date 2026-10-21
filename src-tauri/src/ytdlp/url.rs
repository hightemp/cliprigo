use url::Url;

const YOUTUBE_HOSTS: &[&str] = &[
    "youtube.com",
    "www.youtube.com",
    "m.youtube.com",
    "music.youtube.com",
    "youtu.be",
];

/// Path prefixes of channel pages whose root shows tabs instead of videos.
const CHANNEL_PREFIXES: &[&str] = &["channel", "c", "user"];

/// True for links yt-dlp handles via its YouTube extractors.
pub fn is_youtube(input: &str) -> bool {
    parse(input).is_some()
}

/// Trims the link and rewrites a bare channel URL to its "Videos" tab: the channel root
/// resolves to a list of tabs (Videos/Live/Shorts) rather than to videos.
pub fn normalize(input: &str) -> String {
    let trimmed = input.trim();
    let Some(mut url) = parse(trimmed) else {
        return trimmed.to_string();
    };
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|seg| !seg.is_empty()).collect())
        .unwrap_or_default();
    let is_channel_root = match segments.as_slice() {
        [handle] => handle.starts_with('@'),
        [prefix, _] => CHANNEL_PREFIXES.contains(prefix),
        _ => false,
    };
    if !is_channel_root {
        return trimmed.to_string();
    }
    let path = format!("/{}/videos", segments.join("/"));
    url.set_path(&path);
    url.to_string()
}

/// A link that points at one video, even when it also carries a `list=` parameter
/// (copied while watching a playlist). Such links are treated as a single video.
pub fn is_single_video(input: &str) -> bool {
    let Some(url) = parse(input) else {
        return false;
    };
    let mut segments = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty());
    let first = segments.next();
    if url.host_str() == Some("youtu.be") {
        return first.is_some();
    }
    match first {
        Some("watch") => url.query_pairs().any(|(k, v)| k == "v" && !v.is_empty()),
        Some("shorts" | "live" | "embed") => segments.next().is_some(),
        _ => false,
    }
}

/// Parses an http(s) URL on a YouTube host.
fn parse(input: &str) -> Option<Url> {
    let url = Url::parse(input.trim()).ok()?;
    let host = url.host_str()?;
    (matches!(url.scheme(), "http" | "https") && YOUTUBE_HOSTS.contains(&host)).then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_youtube_hosts() {
        for u in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/abcdefghijk",
            "https://www.youtube.com/playlist?list=PLBCF2DAC6FFB574DE",
            "https://www.youtube.com/@YouTube",
            "  https://youtu.be/dQw4w9WgXcQ  ",
        ] {
            assert!(is_youtube(u), "{u}");
        }
        for u in [
            "https://example.com/watch?v=x",
            "https://notyoutube.com/watch?v=x",
            "https://youtube.com.evil.com/watch?v=x",
            "ftp://youtube.com/watch?v=x",
            "youtube.com/watch?v=x",
            "",
            "not a url",
        ] {
            assert!(!is_youtube(u), "{u}");
        }
    }

    #[test]
    fn normalizes_channel_roots_to_videos_tab() {
        assert_eq!(
            normalize(" https://www.youtube.com/@YouTube "),
            "https://www.youtube.com/@YouTube/videos"
        );
        assert_eq!(
            normalize("https://www.youtube.com/@YouTube/"),
            "https://www.youtube.com/@YouTube/videos"
        );
        assert_eq!(
            normalize("https://www.youtube.com/channel/UCBR8-60-B28hp2BmDPdntcQ"),
            "https://www.youtube.com/channel/UCBR8-60-B28hp2BmDPdntcQ/videos"
        );
        assert_eq!(
            normalize("https://www.youtube.com/c/YouTube"),
            "https://www.youtube.com/c/YouTube/videos"
        );
        assert_eq!(
            normalize("https://www.youtube.com/user/YouTube"),
            "https://www.youtube.com/user/YouTube/videos"
        );
    }

    #[test]
    fn leaves_other_links_alone() {
        for u in [
            "https://www.youtube.com/@YouTube/shorts",
            "https://www.youtube.com/@YouTube/videos",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com/playlist?list=PLBCF2DAC6FFB574DE",
            "https://example.com/@user",
            "not a url",
        ] {
            assert_eq!(normalize(u), u, "{u}");
        }
    }

    #[test]
    fn detects_single_video_links() {
        assert!(is_single_video(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        ));
        assert!(is_single_video(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLBCF2DAC6FFB574DE"
        ));
        assert!(is_single_video(
            "https://youtu.be/dQw4w9WgXcQ?list=PLBCF2DAC6FFB574DE"
        ));
        assert!(is_single_video(
            "https://www.youtube.com/shorts/abcdefghijk"
        ));
        assert!(!is_single_video(
            "https://www.youtube.com/playlist?list=PLBCF2DAC6FFB574DE"
        ));
        assert!(!is_single_video("https://www.youtube.com/@YouTube/videos"));
        assert!(!is_single_video("https://www.youtube.com/watch"));
        assert!(!is_single_video("https://example.com/watch?v=dQw4w9WgXcQ"));
    }
}
