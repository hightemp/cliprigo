package com.hightemp.cliprigo

import android.content.Intent
import android.os.Bundle
import android.os.Environment
import android.system.Os
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import java.io.File

class MainActivity : TauriActivity() {
  private var webView: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    exportPaths()
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    keepShared(intent)
  }

  override fun onNewIntent(intent: Intent) {
    super.onNewIntent(intent)
    keepShared(intent)
    crashRendererForTest(intent)
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    crashRendererForTest(intent)
  }

  /**
   * Debug builds only: `adb shell am start -n com.hightemp.cliprigo/.MainActivity
   * --ez crash_renderer true` crashes the WebView renderer, to check that the app
   * survives it (see RenderCrash).
   */
  private fun crashRendererForTest(intent: Intent?) {
    if (!BuildConfig.DEBUG || intent?.getBooleanExtra("crash_renderer", false) != true) return
    intent.removeExtra("crash_renderer")
    webView?.loadUrl("chrome://crash")
  }

  /** A link shared from another app; the UI takes it when it shows, see PlatformPlugin. */
  private fun keepShared(intent: Intent?) {
    if (intent?.action != Intent.ACTION_SEND) return
    PlatformPlugin.shared = intent.getStringExtra(Intent.EXTRA_TEXT) ?: return
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
