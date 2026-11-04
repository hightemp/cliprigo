package com.hightemp.cliprigo

import android.util.Log
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebView

/**
 * The WebView renderer can crash on its own (a Chromium bug) or be killed for memory.
 * Unhandled, WebView ends the whole app and with it running downloads. The downloads
 * live on the Rust side, so it is enough to load the page again: the UI reads the
 * queue and history back from there.
 */
object RenderCrash {
  fun handle(view: WebView, detail: RenderProcessGoneDetail): Boolean {
    Log.w("Cliprigo", "WebView renderer gone (crashed: ${detail.didCrash()}), reloading")
    view.post { view.reload() }
    return true
  }
}
