use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::error::{AppError, AppResult};

/// Archives with shared libraries (Termux packages), unpacked into `packages_dir/<name>`.
const PACKAGES: [(&str, &str); 2] = [
    ("libpython.zip.so", "python"),
    ("libffmpeg.zip.so", "ffmpeg"),
];
/// Records which archive a package was unpacked from, to redo it after an app update.
const SOURCE_FILE: &str = ".source";

/// Tools shipped inside the Android APK (built by youtubedl-android): Python runs the
/// yt-dlp zipapp, ffmpeg and QuickJS are executables packed as native libraries,
/// because Android only lets apps execute files from their native library directory.
/// MainActivity unpacks Python's and ffmpeg's shared libraries and exports the paths.
#[derive(Debug, Clone)]
pub struct Bundled {
    /// `applicationInfo.nativeLibraryDir`.
    lib_dir: PathBuf,
    /// Where `libpython.zip.so` and `libffmpeg.zip.so` are unpacked.
    packages_dir: PathBuf,
    /// Held while unpacking, so callers wait for a background unpack to finish.
    prepared: Arc<Mutex<bool>>,
}

impl Bundled {
    pub fn new(lib_dir: PathBuf, packages_dir: PathBuf) -> Self {
        Self {
            lib_dir,
            packages_dir,
            prepared: Arc::default(),
        }
    }

    /// Paths exported by MainActivity; `None` outside the Android app.
    pub fn from_env() -> Option<Self> {
        Some(Self::new(
            std::env::var_os("CLIPRIGO_NATIVE_LIB_DIR")?.into(),
            std::env::var_os("CLIPRIGO_PACKAGES_DIR")?.into(),
        ))
    }

    pub fn python(&self) -> PathBuf {
        self.lib_dir.join("libpython.so")
    }

    pub fn ffmpeg(&self) -> PathBuf {
        self.lib_dir.join("libffmpeg.so")
    }

    pub fn quickjs(&self) -> PathBuf {
        self.lib_dir.join("libqjs.so")
    }

    fn python_home(&self) -> PathBuf {
        self.packages_dir.join("python/usr")
    }

    /// Unpacks the library archives unless this app version already did. Must finish
    /// before the tools run: they load these libraries.
    pub fn prepare(&self) -> AppResult<()> {
        let mut prepared = self.prepared.lock().unwrap_or_else(|e| e.into_inner());
        if !*prepared {
            for (archive, name) in PACKAGES {
                unpack(&self.lib_dir.join(archive), &self.packages_dir.join(name))?;
            }
            *prepared = true;
        }
        Ok(())
    }

    /// Environment the bundled Python and ffmpeg need to find their libraries.
    pub fn env(&self) -> Vec<(OsString, OsString)> {
        let home = self.python_home();
        let mut libs = OsString::from(home.join("lib"));
        libs.push(":");
        libs.push(self.packages_dir.join("ffmpeg/usr/lib"));
        vec![
            ("LD_LIBRARY_PATH".into(), libs),
            ("SSL_CERT_FILE".into(), home.join("etc/tls/cert.pem").into()),
            ("PYTHONHOME".into(), home.clone().into()),
            ("HOME".into(), home.into()),
        ]
    }
}

/// Unpacks into a temp dir renamed into place, so an interrupted unpack is redone.
fn unpack(archive: &Path, dest: &Path) -> AppResult<()> {
    let meta = std::fs::metadata(archive)?;
    let source = format!("{} {:?}", meta.len(), meta.modified().ok());
    if std::fs::read_to_string(dest.join(SOURCE_FILE)).ok() == Some(source.clone()) {
        return Ok(());
    }
    tracing::info!(archive = %archive.display(), "unpacking bundled libraries");
    let tmp = dest.with_extension("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    extract(archive, &tmp)?;
    std::fs::write(tmp.join(SOURCE_FILE), source)?;
    let _ = std::fs::remove_dir_all(dest);
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

#[cfg(unix)]
fn extract(archive: &Path, dest: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let bad = |e: zip::result::ZipError| AppError::Other(format!("bad archive: {e}"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?).map_err(bad)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(bad)?;
        let Some(path) = entry.enclosed_name().map(|p| dest.join(p)) else {
            return Err(AppError::Other(format!("unsafe path {:?}", entry.name())));
        };
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if entry.is_symlink() {
            let mut target = String::new();
            std::io::Read::read_to_string(&mut entry, &mut target)?;
            std::os::unix::fs::symlink(target, &path)?;
            continue;
        }
        std::io::copy(&mut entry, &mut std::fs::File::create(&path)?)?;
        if let Some(mode) = entry.unix_mode() {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn extract(_archive: &Path, _dest: &Path) -> AppResult<()> {
    Err(AppError::UnsupportedPlatform("bundled tools".into()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;

    /// A package archive like libpython.zip.so: files plus symlinks (libz.so -> libz.so.1).
    fn write_archive(path: &Path, version: &str) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        zip.add_directory("usr/lib/", opts).unwrap();
        zip.start_file("usr/lib/libz.so.1", opts).unwrap();
        zip.write_all(version.as_bytes()).unwrap();
        zip.add_symlink("usr/lib/libz.so", "libz.so.1", opts)
            .unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn packages_are_unpacked_once_with_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let lib_dir = dir.path().join("lib");
        std::fs::create_dir_all(&lib_dir).unwrap();
        write_archive(&lib_dir.join("libpython.zip.so"), "python 1");
        write_archive(&lib_dir.join("libffmpeg.zip.so"), "ffmpeg 1");
        let packages = dir.path().join("packages");
        let bundled = Bundled::new(lib_dir.clone(), packages.clone());

        bundled.prepare().unwrap();
        let python_lib = packages.join("python/usr/lib");
        // The dynamic linker follows the symlink to the real library.
        assert_eq!(
            std::fs::read_to_string(python_lib.join("libz.so")).unwrap(),
            "python 1"
        );
        assert!(python_lib.join("libz.so").is_symlink());
        assert_eq!(
            std::fs::read_to_string(packages.join("ffmpeg/usr/lib/libz.so.1")).unwrap(),
            "ffmpeg 1"
        );

        // Unchanged archives are not unpacked again.
        std::fs::write(python_lib.join("marker"), "").unwrap();
        bundled.prepare().unwrap();
        assert!(python_lib.join("marker").exists());

        // An app update brings a new archive (and a new process): replaced as a
        // whole, stale files go.
        write_archive(&lib_dir.join("libpython.zip.so"), "python 2, longer");
        Bundled::new(lib_dir, packages.clone()).prepare().unwrap();
        assert!(!python_lib.join("marker").exists());
        assert_eq!(
            std::fs::read_to_string(python_lib.join("libz.so")).unwrap(),
            "python 2, longer"
        );
    }

    /// An unpack killed halfway (app closed on first start) must not be trusted.
    #[test]
    fn interrupted_unpack_is_redone() {
        let dir = tempfile::tempdir().unwrap();
        let lib_dir = dir.path().join("lib");
        std::fs::create_dir_all(&lib_dir).unwrap();
        write_archive(&lib_dir.join("libpython.zip.so"), "python");
        write_archive(&lib_dir.join("libffmpeg.zip.so"), "ffmpeg");
        let packages = dir.path().join("packages");
        std::fs::create_dir_all(packages.join("python/usr/lib")).unwrap();
        let bundled = Bundled::new(lib_dir, packages.clone());

        bundled.prepare().unwrap();
        assert!(packages.join("python/usr/lib/libz.so.1").is_file());
    }
}
