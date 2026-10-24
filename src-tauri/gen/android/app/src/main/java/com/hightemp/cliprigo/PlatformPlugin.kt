package com.hightemp.cliprigo

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.webkit.MimeTypeMap
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class OpenArgs {
  lateinit var path: String
}

@InvokeArg
class KeepAliveArgs {
  /** Notification text; null stops the service. */
  var text: String? = null
}

/** Called from src-tauri/src/android.rs. */
@TauriPlugin
class PlatformPlugin(private val activity: Activity) : Plugin(activity) {
  private var askedForNotifications = false

  /** Other apps can't read our paths: they get a content:// URI with read access. */
  @Command
  fun open(invoke: Invoke) {
    try {
      val file = File(invoke.parseArgs(OpenArgs::class.java).path)
      val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
      val type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension) ?: "*/*"
      val view = Intent(Intent.ACTION_VIEW)
        .setDataAndType(uri, type)
        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      activity.startActivity(Intent.createChooser(view, null))
      invoke.resolve()
    } catch (ex: Exception) {
      invoke.reject(ex.message)
    }
  }

  /** Keeps downloads running in the background, see DownloadService. */
  @Command
  fun keepAlive(invoke: Invoke) {
    try {
      val text = invoke.parseArgs(KeepAliveArgs::class.java).text
      val service = Intent(activity, DownloadService::class.java)
      if (text == null) {
        activity.stopService(service)
      } else if (DownloadService.running) {
        // Starting it again may be refused while the app is in the background.
        DownloadService.update(activity, text)
      } else {
        askForNotifications()
        service.putExtra(DownloadService.EXTRA_TEXT, text)
        ContextCompat.startForegroundService(activity, service)
      }
      invoke.resolve()
    } catch (ex: Exception) {
      invoke.reject(ex.message)
    }
  }

  /** Without it the service still runs, only its notification is hidden. Asked once. */
  private fun askForNotifications() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU || askedForNotifications) return
    askedForNotifications = true
    val permission = Manifest.permission.POST_NOTIFICATIONS
    if (ContextCompat.checkSelfPermission(activity, permission) != PackageManager.PERMISSION_GRANTED) {
      activity.requestPermissions(arrayOf(permission), 0)
    }
  }
}
