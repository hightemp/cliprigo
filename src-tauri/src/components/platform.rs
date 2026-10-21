use crate::error::{AppError, AppResult};

/// Release asset names for the current OS/arch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assets {
    pub ytdlp: &'static str,
    /// Shaka static-ffmpeg-binaries suffix, e.g. `osx-arm64` (`ffmpeg-<suffix>`).
    pub ffmpeg_suffix: &'static str,
    /// Rust target triple used in Deno release names (`deno-<triple>.zip`).
    pub deno_triple: &'static str,
    pub exe_ext: &'static str,
}

pub fn current() -> AppResult<Assets> {
    assets_for(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn assets_for(os: &str, arch: &str) -> AppResult<Assets> {
    let assets = match (os, arch) {
        ("windows", "x86_64") => Assets {
            ytdlp: "yt-dlp.exe",
            ffmpeg_suffix: "win-x64",
            deno_triple: "x86_64-pc-windows-msvc",
            exe_ext: ".exe",
        },
        // No native arm64 ffmpeg build; x64 runs under Windows' emulation.
        ("windows", "aarch64") => Assets {
            ytdlp: "yt-dlp_arm64.exe",
            ffmpeg_suffix: "win-x64",
            deno_triple: "aarch64-pc-windows-msvc",
            exe_ext: ".exe",
        },
        ("macos", "aarch64") => Assets {
            ytdlp: "yt-dlp_macos",
            ffmpeg_suffix: "osx-arm64",
            deno_triple: "aarch64-apple-darwin",
            exe_ext: "",
        },
        ("macos", "x86_64") => Assets {
            ytdlp: "yt-dlp_macos",
            ffmpeg_suffix: "osx-x64",
            deno_triple: "x86_64-apple-darwin",
            exe_ext: "",
        },
        ("linux", "x86_64") => Assets {
            ytdlp: "yt-dlp_linux",
            ffmpeg_suffix: "linux-x64",
            deno_triple: "x86_64-unknown-linux-gnu",
            exe_ext: "",
        },
        ("linux", "aarch64") => Assets {
            ytdlp: "yt-dlp_linux_aarch64",
            ffmpeg_suffix: "linux-arm64",
            deno_triple: "aarch64-unknown-linux-gnu",
            exe_ext: "",
        },
        _ => return Err(AppError::UnsupportedPlatform(format!("{os}/{arch}"))),
    };
    Ok(assets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_platforms() {
        assert_eq!(
            assets_for("windows", "aarch64").unwrap().ytdlp,
            "yt-dlp_arm64.exe"
        );
        assert_eq!(
            assets_for("windows", "aarch64").unwrap().ffmpeg_suffix,
            "win-x64"
        );
        assert_eq!(assets_for("macos", "x86_64").unwrap().ytdlp, "yt-dlp_macos");
        assert_eq!(
            assets_for("linux", "aarch64").unwrap().deno_triple,
            "aarch64-unknown-linux-gnu"
        );
        assert!(current().is_ok());
    }

    #[test]
    fn unknown_platform_is_an_error() {
        assert!(matches!(
            assets_for("freebsd", "x86_64"),
            Err(AppError::UnsupportedPlatform(_))
        ));
    }
}
