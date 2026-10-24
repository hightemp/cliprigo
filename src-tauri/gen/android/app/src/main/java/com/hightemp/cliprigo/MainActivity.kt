package com.hightemp.cliprigo

import android.os.Bundle
import android.os.Environment
import android.system.Os
import androidx.activity.enableEdgeToEdge
import java.io.File

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    exportPaths()
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  /**
   * Tauri's back handlers call this when the WebView can't go back. Finishing the
   * activity would end the process and with it any running downloads, so Back leaves
   * the app running in the background, like Home.
   */
  @Deprecated("Called by Tauri's back handlers")
  override fun onBackPressed() {
    moveTaskToBack(true)
  }

  /** Paths the Rust side reads at startup, see src-tauri/src/components/bundled.rs. */
  private fun exportPaths() {
    Os.setenv("CLIPRIGO_NATIVE_LIB_DIR", applicationInfo.nativeLibraryDir, true)
    Os.setenv("CLIPRIGO_PACKAGES_DIR", File(noBackupFilesDir, "packages").path, true)
    val downloads = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
    Os.setenv("CLIPRIGO_DOWNLOAD_DIR", File(downloads, "Cliprigo").path, true)
    // The default /data/local/tmp is not writable for apps.
    Os.setenv("TMPDIR", cacheDir.path, true)
  }
}
