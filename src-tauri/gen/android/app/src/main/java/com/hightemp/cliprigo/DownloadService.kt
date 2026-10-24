package com.hightemp.cliprigo

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.IBinder

/**
 * Android freezes a backgrounded app together with its child processes, which stalls
 * yt-dlp. While downloads run, this foreground service (with its notification) keeps
 * the app process alive. Started and stopped by the UI through PlatformPlugin.keepAlive.
 */
class DownloadService : Service() {
  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    getSystemService(NotificationManager::class.java).createNotificationChannel(
      NotificationChannel(CHANNEL, getString(R.string.app_name), NotificationManager.IMPORTANCE_LOW)
    )
    val notification = notification(this, intent?.getStringExtra(EXTRA_TEXT))
    startForeground(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    running = true
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    running = false
    super.onDestroy()
  }

  /** Android 15 limits data sync services to 6 hours a day. */
  override fun onTimeout(startId: Int, fgsType: Int) {
    stopSelf()
  }

  companion object {
    const val EXTRA_TEXT = "text"
    private const val CHANNEL = "downloads"
    private const val NOTIFICATION_ID = 1

    @Volatile
    var running = false
      private set

    /** New text for the notification of the running service. */
    fun update(context: Context, text: String) {
      context.getSystemService(NotificationManager::class.java)
        .notify(NOTIFICATION_ID, notification(context, text))
    }

    private fun notification(context: Context, text: String?): Notification {
      val openApp = PendingIntent.getActivity(
        context,
        0,
        Intent(context, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
        PendingIntent.FLAG_IMMUTABLE,
      )
      return Notification.Builder(context, CHANNEL)
        .setSmallIcon(android.R.drawable.stat_sys_download)
        .setContentTitle(context.getString(R.string.app_name))
        .setContentText(text)
        .setContentIntent(openApp)
        .setOngoing(true)
        .build()
    }
  }
}
