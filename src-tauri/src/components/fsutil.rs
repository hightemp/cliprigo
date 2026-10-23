use std::path::Path;

use crate::error::AppResult;

pub const TMP_PREFIX: &str = ".tmp-";
pub const OLD_SUFFIX: &str = ".old";

/// Makes a freshly downloaded binary runnable in place, before it is checked.
pub async fn prepare(tmp: &Path) -> AppResult<()> {
    make_executable(tmp)?;
    #[cfg(target_os = "macos")]
    macos_fixup(tmp).await;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> AppResult<()> {
    Ok(())
}

/// Moves `tmp` over `dest`. Both must be in the same directory so the rename is atomic.
#[cfg(unix)]
pub fn replace(tmp: &Path, dest: &Path) -> AppResult<()> {
    // Running processes keep the old inode, so replacing in use binaries is safe.
    std::fs::rename(tmp, dest)?;
    Ok(())
}

/// Windows can't overwrite a running executable but can rename it, so the current
/// binary is moved aside (and cleaned up on next start) before the new one takes its place.
#[cfg(not(unix))]
pub fn replace(tmp: &Path, dest: &Path) -> AppResult<()> {
    if dest.exists() {
        let mut old = dest.as_os_str().to_owned();
        old.push(OLD_SUFFIX);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(dest, &old)?;
    }
    std::fs::rename(tmp, dest)?;
    Ok(())
}

/// Files we download ourselves don't get the quarantine attribute, but clear it just in
/// case. Apple Silicon refuses to run unsigned code, so ad-hoc sign when needed.
#[cfg(target_os = "macos")]
async fn macos_fixup(path: &Path) {
    use tokio::process::Command;

    let _ = Command::new("xattr")
        .args(["-d", "com.apple.quarantine"])
        .arg(path)
        .output()
        .await;
    let signed = Command::new("codesign")
        .arg("-v")
        .arg(path)
        .output()
        .await
        .is_ok_and(|o| o.status.success());
    if !signed {
        match Command::new("codesign")
            .args(["--force", "-s", "-"])
            .arg(path)
            .output()
            .await
        {
            Ok(o) if o.status.success() => tracing::info!(?path, "ad-hoc signed binary"),
            Ok(o) => tracing::warn!(
                ?path,
                stderr = %String::from_utf8_lossy(&o.stderr),
                "ad-hoc signing failed"
            ),
            Err(e) => tracing::warn!(?path, error = %e, "codesign not available"),
        }
    }
}

/// Removes leftovers from interrupted installs and binaries replaced on Windows.
pub fn cleanup(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if (name.starts_with(TMP_PREFIX) || name.ends_with(OLD_SUFFIX))
            && let Err(e) = std::fs::remove_file(entry.path())
        {
            tracing::debug!(file = %name, error = %e, "could not remove stale file");
        }
    }
}
