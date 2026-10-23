pub(crate) mod fetch;
mod fsutil;
mod platform;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::db::now_unix;
use crate::error::{AppError, AppResult};
use crate::net::ProxySetting;
use crate::ytdlp::args::Tools;
use fetch::Http;
use platform::Assets;

const YTDLP_REPO: &str = "yt-dlp/yt-dlp";
const FFMPEG_REPO: &str = "shaka-project/static-ffmpeg-binaries";
const DENO_REPO: &str = "denoland/deno";

const MANIFEST_FILE: &str = "manifest.json";
const AUTO_UPDATE_INTERVAL_SECS: i64 = 6 * 60 * 60;
/// First run of a PyInstaller binary unpacks itself, which can be slow on cold disks.
const SMOKE_TEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Component {
    Ytdlp,
    Ffmpeg,
    Deno,
}

impl Component {
    pub const ALL: [Component; 3] = [Self::Ytdlp, Self::Ffmpeg, Self::Deno];

    fn repo(self) -> &'static str {
        match self {
            Self::Ytdlp => YTDLP_REPO,
            Self::Ffmpeg => FFMPEG_REPO,
            Self::Deno => DENO_REPO,
        }
    }

    /// Local executables this component provides (without extension).
    fn binaries(self) -> &'static [&'static str] {
        match self {
            Self::Ytdlp => &["yt-dlp"],
            Self::Ffmpeg => &["ffmpeg", "ffprobe"],
            Self::Deno => &["deno"],
        }
    }

    fn version_arg(self) -> &'static str {
        match self {
            Self::Ffmpeg => "-version",
            Self::Ytdlp | Self::Deno => "--version",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentStatus {
    pub component: Component,
    pub installed: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Download,
    Verify,
    Install,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub component: Component,
    pub stage: Stage,
    /// Release asset being processed.
    pub file: String,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub component: Component,
    pub current: Option<String>,
    pub latest: String,
    pub update_available: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Manifest {
    versions: HashMap<Component, String>,
    last_update_check: Option<i64>,
}

pub type ProgressFn<'a> = &'a (dyn Fn(Progress) + Send + Sync);

/// Downloads, verifies and updates the external tools in `<app_data_dir>/bin`.
pub struct ComponentManager {
    bin_dir: PathBuf,
    assets: Assets,
    proxy: ProxySetting,
    manifest: Mutex<Manifest>,
    /// One install at a time: avoids two writers racing on the same temp files.
    install_lock: tokio::sync::Mutex<()>,
}

impl ComponentManager {
    pub fn new(bin_dir: PathBuf, proxy: ProxySetting) -> AppResult<Self> {
        std::fs::create_dir_all(&bin_dir)?;
        fsutil::cleanup(&bin_dir);
        let manifest = std::fs::read(bin_dir.join(MANIFEST_FILE))
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        Ok(Self {
            assets: platform::current()?,
            proxy,
            manifest: Mutex::new(manifest),
            install_lock: tokio::sync::Mutex::new(()),
            bin_dir,
        })
    }

    /// A client for the current proxy setting, so a changed setting applies at once.
    fn http(&self) -> AppResult<Http> {
        Http::new(self.proxy.get().as_deref())
    }

    fn manifest(&self) -> std::sync::MutexGuard<'_, Manifest> {
        self.manifest.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn update_manifest(&self, f: impl FnOnce(&mut Manifest)) -> AppResult<()> {
        let mut manifest = self.manifest();
        f(&mut manifest);
        let json = serde_json::to_vec_pretty(&*manifest)?;
        let tmp = self
            .bin_dir
            .join(format!("{}{MANIFEST_FILE}", fsutil::TMP_PREFIX));
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, self.bin_dir.join(MANIFEST_FILE))?;
        Ok(())
    }

    fn binary_path(&self, name: &str) -> PathBuf {
        self.bin_dir.join(format!("{name}{}", self.assets.exe_ext))
    }

    fn tmp_path(&self, name: &str) -> PathBuf {
        self.bin_dir.join(format!("{}{name}", fsutil::TMP_PREFIX))
    }

    pub fn ytdlp_path(&self) -> PathBuf {
        self.binary_path("yt-dlp")
    }

    pub fn ffmpeg_path(&self) -> PathBuf {
        self.binary_path("ffmpeg")
    }

    pub fn deno_path(&self) -> PathBuf {
        self.binary_path("deno")
    }

    /// Paths for running yt-dlp; fails until every component is installed.
    pub fn tools(&self) -> AppResult<Tools> {
        if let Some(missing) = Component::ALL.into_iter().find(|c| !self.is_installed(*c)) {
            return Err(AppError::ComponentsMissing(format!("{missing:?}")));
        }
        Ok(Tools {
            ytdlp: self.ytdlp_path(),
            ffmpeg: self.ffmpeg_path(),
            deno: self.deno_path(),
            proxy: self.proxy.get(),
        })
    }

    fn has_release(&self, c: Component, tag: &str) -> bool {
        self.is_installed(c) && self.manifest().versions.get(&c).map(String::as_str) == Some(tag)
    }

    fn is_installed(&self, c: Component) -> bool {
        c.binaries().iter().all(|b| self.binary_path(b).is_file())
    }

    pub fn status(&self) -> Vec<ComponentStatus> {
        let manifest = self.manifest();
        Component::ALL
            .into_iter()
            .map(|component| {
                let installed = self.is_installed(component);
                ComponentStatus {
                    component,
                    installed,
                    version: installed
                        .then(|| manifest.versions.get(&component).cloned())
                        .flatten(),
                }
            })
            .collect()
    }

    /// Installs (or reinstalls) the latest release of `c`. Returns the installed version.
    pub async fn install(&self, c: Component, on_progress: ProgressFn<'_>) -> AppResult<String> {
        let (_guard, waited) = match self.install_lock.try_lock() {
            Ok(guard) => (guard, false),
            Err(_) => (self.install_lock.lock().await, true),
        };
        let tag = self.http()?.latest_tag(c.repo()).await?;
        // The install we waited for may have just put this release in place.
        if waited && self.has_release(c, &tag) {
            return Ok(tag);
        }
        tracing::info!(component = ?c, tag, "installing component");
        match c {
            Component::Ytdlp => self.install_ytdlp(&tag, on_progress).await?,
            Component::Ffmpeg => self.install_ffmpeg(&tag, on_progress).await?,
            Component::Deno => self.install_deno(&tag, on_progress).await?,
        }
        self.update_manifest(|m| {
            m.versions.insert(c, tag.clone());
        })?;
        tracing::info!(component = ?c, tag, "component installed");
        Ok(tag)
    }

    /// Downloads one release asset into a temp file, reporting progress.
    async fn fetch_asset(
        &self,
        c: Component,
        tag: &str,
        asset: &str,
        on_progress: ProgressFn<'_>,
    ) -> AppResult<PathBuf> {
        let tmp = self.tmp_path(asset);
        let url = fetch::release_asset_url(c.repo(), tag, asset);
        self.http()?
            .download(&url, &tmp, |downloaded, total| {
                on_progress(Progress {
                    component: c,
                    stage: Stage::Download,
                    file: asset.to_string(),
                    downloaded,
                    total,
                })
            })
            .await?;
        on_progress(Progress {
            component: c,
            stage: Stage::Verify,
            file: asset.to_string(),
            downloaded: 0,
            total: None,
        });
        Ok(tmp)
    }

    async fn install_ytdlp(&self, tag: &str, on_progress: ProgressFn<'_>) -> AppResult<()> {
        let c = Component::Ytdlp;
        let asset = self.assets.ytdlp;
        let sums = self
            .http()?
            .get_text(&fetch::release_asset_url(c.repo(), tag, "SHA2-256SUMS"))
            .await?;
        let expected = fetch::find_checksum(&sums, asset)
            .ok_or_else(|| AppError::Checksum(format!("{asset} (missing from SHA2-256SUMS)")))?;
        let tmp = self.fetch_asset(c, tag, asset, on_progress).await?;
        fetch::verify::<sha2::Sha256>(&tmp, &expected, asset).await?;
        self.finish(c, asset, &tmp, &self.ytdlp_path(), on_progress)
            .await
    }

    async fn install_ffmpeg(&self, tag: &str, on_progress: ProgressFn<'_>) -> AppResult<()> {
        let c = Component::Ffmpeg;
        // Shaka publishes MD5 sums only in the release notes, which need the rate-limited
        // REST API. The download itself is HTTPS from GitHub, so skipping is acceptable.
        let sums = match self.http()?.release_body(c.repo(), tag).await {
            Ok(body) => Some(body),
            Err(e) => {
                tracing::warn!(error = %e, "ffmpeg checksums unavailable, skipping verification");
                None
            }
        };
        for name in c.binaries() {
            let asset = format!(
                "{name}-{}{}",
                self.assets.ffmpeg_suffix, self.assets.exe_ext
            );
            let tmp = self.fetch_asset(c, tag, &asset, on_progress).await?;
            if let Some(sums) = &sums {
                match fetch::find_checksum(sums, &asset) {
                    Some(expected) => fetch::verify::<md5::Md5>(&tmp, &expected, &asset).await?,
                    None => tracing::warn!(asset, "no checksum listed for asset"),
                }
            }
            self.finish(c, &asset, &tmp, &self.binary_path(name), on_progress)
                .await?;
        }
        Ok(())
    }

    async fn install_deno(&self, tag: &str, on_progress: ProgressFn<'_>) -> AppResult<()> {
        let c = Component::Deno;
        let asset = format!("deno-{}.zip", self.assets.deno_triple);
        let sums = self
            .http()?
            .get_text(&fetch::release_asset_url(
                c.repo(),
                tag,
                &format!("{asset}.sha256sum"),
            ))
            .await?;
        let expected = fetch::find_checksum(&sums, &asset)
            .ok_or_else(|| AppError::Checksum(format!("{asset} (missing checksum)")))?;
        let zip_path = self.fetch_asset(c, tag, &asset, on_progress).await?;
        fetch::verify::<sha2::Sha256>(&zip_path, &expected, &asset).await?;

        let entry = format!("deno{}", self.assets.exe_ext);
        let tmp = self.tmp_path(&entry);
        let extracted = {
            let (zip_path, tmp, entry) = (zip_path.clone(), tmp.clone(), entry.clone());
            tokio::task::spawn_blocking(move || extract_zip_entry(&zip_path, &entry, &tmp))
                .await
                .map_err(|e| AppError::Other(e.to_string()))?
        };
        let _ = std::fs::remove_file(&zip_path);
        extracted?;
        self.finish(c, &asset, &tmp, &self.binary_path("deno"), on_progress)
            .await
    }

    async fn finish(
        &self,
        c: Component,
        asset: &str,
        tmp: &Path,
        dest: &Path,
        on_progress: ProgressFn<'_>,
    ) -> AppResult<()> {
        on_progress(Progress {
            component: c,
            stage: Stage::Install,
            file: asset.to_string(),
            downloaded: 0,
            total: None,
        });
        let result = async {
            fsutil::prepare(tmp).await?;
            smoke_test(c, tmp).await?;
            fsutil::replace(tmp, dest)
        }
        .await;
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        result
    }

    pub async fn check_updates(&self) -> AppResult<Vec<UpdateInfo>> {
        let mut result = Vec::with_capacity(Component::ALL.len());
        for component in Component::ALL {
            let latest = self.http()?.latest_tag(component.repo()).await?;
            let current = self
                .status()
                .into_iter()
                .find(|s| s.component == component)
                .and_then(|s| s.version);
            result.push(UpdateInfo {
                component,
                update_available: current.as_deref() != Some(latest.as_str()),
                current,
                latest,
            });
        }
        self.update_manifest(|m| m.last_update_check = Some(now_unix()))?;
        Ok(result)
    }

    /// Updates yt-dlp if it is installed and the last check is older than the interval.
    /// Returns the new version when an update was installed.
    pub async fn auto_update_ytdlp(
        &self,
        on_progress: ProgressFn<'_>,
    ) -> AppResult<Option<String>> {
        if !self.is_installed(Component::Ytdlp) {
            return Ok(None);
        }
        let last = self.manifest().last_update_check.unwrap_or(0);
        if now_unix() - last < AUTO_UPDATE_INTERVAL_SECS {
            return Ok(None);
        }
        let latest = self.http()?.latest_tag(YTDLP_REPO).await?;
        self.update_manifest(|m| m.last_update_check = Some(now_unix()))?;
        let current = self.manifest().versions.get(&Component::Ytdlp).cloned();
        if current.as_deref() == Some(latest.as_str()) {
            return Ok(None);
        }
        tracing::info!(?current, latest, "updating yt-dlp");
        self.install(Component::Ytdlp, on_progress).await.map(Some)
    }
}

/// Runs a new binary once, before it replaces the current one, so a broken download,
/// missing loader or antivirus block fails the install and keeps the working version.
async fn smoke_test(c: Component, path: &Path) -> AppResult<()> {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let name = name.trim_start_matches(fsutil::TMP_PREFIX);
    let output = tokio::time::timeout(
        SMOKE_TEST_TIMEOUT,
        crate::process::output(crate::process::command(path).arg(c.version_arg())),
    )
    .await
    .map_err(|_| AppError::Other(format!("{name} did not respond")))??;
    if !output.status.success() {
        return Err(AppError::Other(format!(
            "{name} failed to run: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

fn extract_zip_entry(zip_path: &Path, entry: &str, dest: &Path) -> AppResult<()> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| AppError::Other(format!("bad archive: {e}")))?;
    let mut src = archive
        .by_name(entry)
        .map_err(|e| AppError::Other(format!("{entry} not found in archive: {e}")))?;
    let mut out = std::fs::File::create(dest)?;
    std::io::copy(&mut src, &mut out)?;
    out.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn script(path: &Path, body: &str) {
        std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    }

    /// A new binary that doesn't run (antivirus, missing loader, broken download)
    /// must not replace the working one: is_installed() would trust it afterwards.
    #[cfg(unix)]
    #[tokio::test]
    async fn broken_update_keeps_the_working_binary() {
        let dir = tempfile::tempdir().unwrap();
        let manager =
            ComponentManager::new(dir.path().to_owned(), crate::net::test_proxy()).unwrap();
        let dest = manager.ytdlp_path();
        script(&dest, "echo old");
        let tmp = manager.tmp_path("yt-dlp");
        script(&tmp, "echo broken >&2; exit 1");

        let result = manager
            .finish(Component::Ytdlp, "yt-dlp", &tmp, &dest, &|_| {})
            .await;
        assert!(result.is_err(), "{result:?}");
        assert!(std::fs::read_to_string(&dest).unwrap().contains("echo old"));
        assert!(!tmp.exists());

        script(&tmp, "echo 2026.10.21");
        manager
            .finish(Component::Ytdlp, "yt-dlp", &tmp, &dest, &|_| {})
            .await
            .unwrap();
        assert!(
            std::fs::read_to_string(&dest)
                .unwrap()
                .contains("2026.10.21")
        );
    }

    /// An install queued behind another one (a reloaded Setup, auto-update during a
    /// manual update) must not download the same release again.
    #[test]
    fn install_is_skipped_only_when_this_release_is_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let manager =
            ComponentManager::new(dir.path().to_owned(), crate::net::test_proxy()).unwrap();
        assert!(!manager.has_release(Component::Deno, "v2.9.8"));
        std::fs::write(manager.deno_path(), b"").unwrap();
        assert!(!manager.has_release(Component::Deno, "v2.9.8"));
        manager
            .update_manifest(|m| {
                m.versions.insert(Component::Deno, "v2.9.8".into());
            })
            .unwrap();
        assert!(manager.has_release(Component::Deno, "v2.9.8"));
        assert!(!manager.has_release(Component::Deno, "v2.9.9"));
        assert!(!manager.has_release(Component::Ffmpeg, "v2.9.8"));
    }

    /// Checksum files of the latest releases must list every asset of every platform,
    /// in a format we can parse. Runs in CI: `cargo test checksums -- --ignored`.
    /// (Deno's Windows sums once came in PowerShell format and broke installs there.)
    #[tokio::test]
    #[ignore = "network"]
    async fn checksums_for_all_platforms() {
        let http = Http::new(crate::net::test_proxy().get().as_deref()).unwrap();
        let ytdlp_tag = http.latest_tag(YTDLP_REPO).await.unwrap();
        let ytdlp_sums = http
            .get_text(&fetch::release_asset_url(
                YTDLP_REPO,
                &ytdlp_tag,
                "SHA2-256SUMS",
            ))
            .await
            .unwrap();
        let deno_tag = http.latest_tag(DENO_REPO).await.unwrap();
        let ffmpeg_tag = http.latest_tag(FFMPEG_REPO).await.unwrap();
        // Optional in the installer (rate-limited API), but when available it must match.
        let ffmpeg_sums = http.release_body(FFMPEG_REPO, &ffmpeg_tag).await.ok();

        let mut missing = Vec::new();
        for (os, arch) in platform::SUPPORTED {
            let assets = platform::assets_for(os, arch).unwrap();
            if fetch::find_checksum(&ytdlp_sums, assets.ytdlp).is_none() {
                missing.push(format!("{os}/{arch}: {}", assets.ytdlp));
            }

            let deno_asset = format!("deno-{}.zip", assets.deno_triple);
            let deno_sums = http
                .get_text(&fetch::release_asset_url(
                    DENO_REPO,
                    &deno_tag,
                    &format!("{deno_asset}.sha256sum"),
                ))
                .await
                .unwrap();
            if fetch::find_checksum(&deno_sums, &deno_asset).is_none() {
                missing.push(format!("{os}/{arch}: {deno_asset} in {deno_sums:?}"));
            }

            if let Some(sums) = &ffmpeg_sums {
                for name in Component::Ffmpeg.binaries() {
                    let asset = format!("{name}-{}{}", assets.ffmpeg_suffix, assets.exe_ext);
                    if fetch::find_checksum(sums, &asset).is_none() {
                        missing.push(format!("{os}/{arch}: {asset}"));
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "no checksum found for:\n{}",
            missing.join("\n")
        );
    }

    /// Real downloads from GitHub: `cargo test live_install -- --ignored`.
    #[tokio::test]
    #[ignore = "network"]
    async fn live_install_all_components() {
        let dir = std::env::temp_dir().join(format!("cliprigo-bin-{}", std::process::id()));
        let manager = ComponentManager::new(dir.clone(), crate::net::test_proxy()).unwrap();
        let stages = Mutex::new(Vec::new());
        let on_progress = |p: Progress| {
            let mut stages = stages.lock().unwrap();
            if stages.last() != Some(&(p.component, p.file.clone())) {
                stages.push((p.component, p.file.clone()));
            }
        };

        for c in Component::ALL {
            let version = manager.install(c, &on_progress).await.unwrap();
            println!("{c:?} {version}");
        }
        assert!(
            manager
                .status()
                .iter()
                .all(|s| s.installed && s.version.is_some())
        );
        assert!(manager.ffmpeg_path().is_file() && manager.deno_path().is_file());

        // Leftover temp files must be gone, only binaries and the manifest remain.
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        files.sort();
        println!("{files:?}");
        assert!(files.iter().all(|f| !f.starts_with(fsutil::TMP_PREFIX)));

        // A fresh manager reads the persisted manifest.
        let reopened = ComponentManager::new(dir.clone(), crate::net::test_proxy()).unwrap();
        assert!(reopened.status().iter().all(|s| s.version.is_some()));

        let updates = reopened.check_updates().await.unwrap();
        assert!(updates.iter().all(|u| !u.update_available), "{updates:?}");
        // Right after a check, auto-update is throttled.
        assert_eq!(reopened.auto_update_ytdlp(&|_| {}).await.unwrap(), None);

        std::fs::remove_dir_all(dir).unwrap();
    }
}
