use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Removed, private or otherwise not viewable.
    Unavailable,
    /// "Sign in to confirm you're not a bot": YouTube distrusts the IP or yt-dlp is outdated.
    BotCheck,
    AgeRestricted,
    Unsupported,
    Network,
    FormatUnavailable,
    Ffmpeg,
    /// yt-dlp produced no result in time; the message carries its last output.
    Timeout,
    Other,
}

impl ErrorKind {
    /// Error code sent to the UI, see `AppError::code`.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "ytdlp_unavailable",
            Self::BotCheck => "ytdlp_bot_check",
            Self::AgeRestricted => "ytdlp_age_restricted",
            Self::Unsupported => "ytdlp_unsupported",
            Self::Network => "ytdlp_network",
            Self::FormatUnavailable => "ytdlp_format_unavailable",
            Self::Ffmpeg => "ytdlp_ffmpeg",
            Self::Timeout => "ytdlp_timeout",
            Self::Other => "ytdlp_other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YtdlpError {
    pub kind: ErrorKind,
    /// The last `ERROR:` line without the prefix, or the last stderr line.
    pub message: String,
}

impl From<YtdlpError> for crate::error::AppError {
    fn from(e: YtdlpError) -> Self {
        Self::Ytdlp {
            kind: e.kind,
            message: e.message,
        }
    }
}

/// Classifies yt-dlp's stderr into a kind the UI can explain to the user.
pub fn classify(stderr: &str) -> YtdlpError {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let errors: Vec<&str> = lines
        .iter()
        .filter_map(|l| l.strip_prefix("ERROR:"))
        .map(str::trim)
        .collect();
    let message = errors
        .last()
        .or(lines.last())
        .map(|s| s.to_string())
        .unwrap_or_default();
    // Only ERROR lines decide the kind: warnings often mention unrelated problems.
    let haystack = errors.join("\n").to_lowercase();
    YtdlpError {
        kind: kind_of(&haystack),
        message,
    }
}

/// Patterns are checked in order; the first match wins.
const PATTERNS: &[(ErrorKind, &[&str])] = &[
    (ErrorKind::BotCheck, &["not a bot"]),
    (
        ErrorKind::AgeRestricted,
        &["confirm your age", "age-restricted", "age restricted"],
    ),
    (
        ErrorKind::Unavailable,
        &[
            "video unavailable",
            "video is unavailable",
            "private video",
            "has been removed",
            "account associated with this video has been terminated",
            "members-only",
        ],
    ),
    (
        ErrorKind::Ffmpeg,
        &["ffmpeg not found", "ffprobe not found", "postprocessing:"],
    ),
    (
        ErrorKind::FormatUnavailable,
        &["requested format is not available"],
    ),
    (
        ErrorKind::Unsupported,
        &["unsupported url", "http error 404", "is not a valid url"],
    ),
    (
        ErrorKind::Network,
        &[
            "unable to download",
            "unable to connect",
            "name resolution",
            "getaddrinfo",
            "timed out",
            "connection refused",
            "connection reset",
            "network is unreachable",
            "ssl:",
        ],
    ),
];

fn kind_of(haystack: &str) -> ErrorKind {
    PATTERNS
        .iter()
        .find(|(_, needles)| needles.iter().any(|n| haystack.contains(n)))
        .map_or(ErrorKind::Other, |(kind, _)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Messages captured from yt-dlp 2026.08.19.
    const BOT: &str = "WARNING: [youtube] No title found in player responses; falling back to title from initial data. Other metadata may also be missing\nERROR: [youtube] GvgqDSnpRQM: Sign in to confirm you\u{2019}re not a bot. Use --cookies-from-browser or --cookies for the authentication.";
    const UNAVAILABLE: &str = "ERROR: [youtube] aaaaaaaaaaa: This video is unavailable";
    const NOT_FOUND: &str = "ERROR: [generic] nothing: Unable to download webpage: HTTP Error 404: Not Found (caused by <HTTPError 404: Not Found>)";
    const PROXY: &str = "ERROR: [youtube] dQw4w9WgXcQ: Unable to download API page: ('Unable to connect to proxy', NewConnectionError(\"HTTPSConnection(host='127.0.0.1', port=9): Failed to establish a new connection: [Errno 111] Connection refused\"))";

    fn kind(s: &str) -> ErrorKind {
        classify(s).kind
    }

    #[test]
    fn real_messages() {
        assert_eq!(kind(BOT), ErrorKind::BotCheck);
        assert_eq!(kind(UNAVAILABLE), ErrorKind::Unavailable);
        assert_eq!(kind(NOT_FOUND), ErrorKind::Unsupported);
        assert_eq!(kind(PROXY), ErrorKind::Network);
    }

    #[test]
    fn other_known_messages() {
        let cases = [
            (
                "ERROR: [youtube] x: Video unavailable. This video has been removed by the uploader",
                ErrorKind::Unavailable,
            ),
            (
                "ERROR: [youtube] x: Private video. Sign in if you've been granted access to this video",
                ErrorKind::Unavailable,
            ),
            (
                "ERROR: [youtube] x: Sign in to confirm your age. This video may be inappropriate for some users.",
                ErrorKind::AgeRestricted,
            ),
            (
                "ERROR: Unsupported URL: https://example.com/",
                ErrorKind::Unsupported,
            ),
            (
                "ERROR: [youtube] x: Requested format is not available. Use --list-formats for a list of available formats",
                ErrorKind::FormatUnavailable,
            ),
            (
                "ERROR: Postprocessing: ffprobe and ffmpeg not found. Please install or provide the path using --ffmpeg-location",
                ErrorKind::Ffmpeg,
            ),
            (
                "ERROR: [youtube] x: Unable to download webpage: <urlopen error [Errno -3] Temporary failure in name resolution>",
                ErrorKind::Network,
            ),
            ("ERROR: [youtube] x: Read timed out.", ErrorKind::Network),
            ("ERROR: something new and strange", ErrorKind::Other),
        ];
        for (msg, expected) in cases {
            assert_eq!(kind(msg), expected, "{msg}");
        }
    }

    #[test]
    fn message_is_last_error_line_without_prefix() {
        let e = classify(BOT);
        assert!(
            e.message
                .starts_with("[youtube] GvgqDSnpRQM: Sign in to confirm"),
            "{}",
            e.message
        );
        let e = classify("ERROR: first\nsome noise\nERROR: second\n");
        assert_eq!(e.message, "second");
    }

    #[test]
    fn warnings_alone_do_not_decide_the_kind() {
        // A warning mentioning "unavailable videos" must not turn into Unavailable.
        let e = classify(
            "WARNING: [youtube:tab] YouTube said: INFO - 3 unavailable videos are hidden\nERROR: boom",
        );
        assert_eq!(e.kind, ErrorKind::Other);
        assert_eq!(e.message, "boom");
    }

    #[test]
    fn no_error_line_falls_back_to_last_line() {
        let e = classify("Traceback (most recent call last):\n  File \"x\"\nKeyError: 'id'\n");
        assert_eq!(e.kind, ErrorKind::Other);
        assert_eq!(e.message, "KeyError: 'id'");
        assert_eq!(classify("").message, "");
    }
}
