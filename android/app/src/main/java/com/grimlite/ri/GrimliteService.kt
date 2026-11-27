package com.grimlite.ri

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import android.util.Log
import androidx.core.content.ContextCompat

/**
 * Foreground keep-alive service + local ongoing notification + partial WakeLock while any
 * Grimlite Ri tab is running Bot, Maid, or Auto Attack in the background.
 */
class GrimliteService : Service() {
    private var wakeLock: PowerManager.WakeLock? = null
    private var currentTitle: String = "Grimlite Ri"
    private var currentText: String = "Bot running in background"

    override fun onCreate() {
        super.onCreate()
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(
            NotificationChannel(
                CHANNEL_ID,
                "Grimlite Ri Background Bot",
                NotificationManager.IMPORTANCE_LOW,
            ).apply {
                description = "Keeps Grimlite Ri connected and botting while in the background"
                setShowBadge(false)
            },
        )
        val pm = getSystemService(PowerManager::class.java)
        wakeLock = pm.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "grimlite:botting").apply {
            setReferenceCounted(false)
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP_BOTS -> {
                Log.i(TAG, "Notification action: Stop All Bots")
                MainActivity.nativeStopAllBots()
                stopKeepAlive()
                return START_NOT_STICKY
            }
            ACTION_QUIT_APP -> {
                Log.i(TAG, "Notification action: Quit Grimlite Ri")
                MainActivity.nativeStopAllBots()
                stopKeepAlive()
                MainActivity.quitApp()
                return START_NOT_STICKY
            }
            ACTION_STOP_SERVICE -> {
                stopKeepAlive()
                return START_NOT_STICKY
            }
        }

        intent?.getStringExtra(EXTRA_TITLE)?.let { currentTitle = it }
        intent?.getStringExtra(EXTRA_TEXT)?.let { currentText = it }

        val notification = buildNotification(currentTitle, currentText)
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
                startForeground(
                    NOTIFICATION_ID,
                    notification,
                    ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE,
                )
            } else {
                startForeground(NOTIFICATION_ID, notification)
            }
            wakeLock?.let { wl ->
                if (!wl.isHeld) {
                    wl.acquire()
                }
            }
        } catch (e: Exception) {
            Log.w(TAG, "Failed to start foreground bot service: $e")
        }

        return START_STICKY
    }

    private fun stopKeepAlive() {
        try {
            wakeLock?.let { wl ->
                if (wl.isHeld) {
                    wl.release()
                }
            }
        } catch (_: Exception) {
        }
        try {
            stopForeground(STOP_FOREGROUND_REMOVE)
        } catch (_: Exception) {
        }
        stopSelf()
    }

    override fun onDestroy() {
        try {
            wakeLock?.let { wl ->
                if (wl.isHeld) {
                    wl.release()
                }
            }
        } catch (_: Exception) {
        }
        getSystemService(NotificationManager::class.java).cancel(NOTIFICATION_ID)
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun buildNotification(title: String, text: String): Notification {
        val openIntent = PendingIntent.getActivity(
            this,
            1,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )
        val stopBotsIntent = PendingIntent.getService(
            this,
            2,
            Intent(this, GrimliteService::class.java).setAction(ACTION_STOP_BOTS),
            PendingIntent.FLAG_IMMUTABLE,
        )
        val quitIntent = PendingIntent.getService(
            this,
            3,
            Intent(this, GrimliteService::class.java).setAction(ACTION_QUIT_APP),
            PendingIntent.FLAG_IMMUTABLE,
        )

        return Notification.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(title)
            .setContentText(text)
            .setStyle(Notification.BigTextStyle().bigText(text))
            .setContentIntent(openIntent)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .addAction(Notification.Action.Builder(null, "Stop Bots", stopBotsIntent).build())
            .addAction(Notification.Action.Builder(null, "Quit", quitIntent).build())
            .build()
    }

    companion object {
        private const val TAG = "GrimliteRi"
        private const val CHANNEL_ID = "grimlite_botting"
        private const val NOTIFICATION_ID = 7101

        private const val EXTRA_TITLE = "title"
        private const val EXTRA_TEXT = "text"

        const val ACTION_STOP_BOTS = "com.grimlite.ri.STOP_BOTS"
        const val ACTION_QUIT_APP = "com.grimlite.ri.QUIT_APP"
        private const val ACTION_STOP_SERVICE = "com.grimlite.ri.STOP_SERVICE"

        @Volatile
        private var serviceRunning = false

        fun update(context: Context, active: Boolean, title: String, text: String) {
            val appCtx = context.applicationContext
            if (active) {
                val intent = Intent(appCtx, GrimliteService::class.java)
                    .putExtra(EXTRA_TITLE, title)
                    .putExtra(EXTRA_TEXT, text)
                try {
                    ContextCompat.startForegroundService(appCtx, intent)
                    serviceRunning = true
                } catch (e: Exception) {
                    Log.w(TAG, "Unable to start foreground service: $e")
                }
            } else if (serviceRunning) {
                serviceRunning = false
                try {
                    appCtx.stopService(Intent(appCtx, GrimliteService::class.java))
                } catch (_: Exception) {
                }
            }
        }
    }
}
