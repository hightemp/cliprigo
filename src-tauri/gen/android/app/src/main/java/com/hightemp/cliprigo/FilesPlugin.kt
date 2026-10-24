package com.hightemp.cliprigo

import android.app.Activity
import android.content.Intent
import android.webkit.MimeTypeMap
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

/** Called from src-tauri/src/android.rs. */
@TauriPlugin
class FilesPlugin(private val activity: Activity) : Plugin(activity) {
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
}
