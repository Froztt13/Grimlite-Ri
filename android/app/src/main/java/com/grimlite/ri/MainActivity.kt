package com.grimlite.ri

import android.Manifest
import android.app.Activity
import android.app.GameManager
import android.content.ActivityNotFoundException
import android.content.ComponentCallbacks2
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.provider.OpenableColumns
import android.provider.Settings
import android.util.Log
import android.view.MotionEvent
import android.view.WindowManager
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import com.google.androidgamesdk.GameActivity
import java.io.File
import kotlin.concurrent.thread

class MainActivity : GameActivity() {
    @Volatile
    private var pendingTabId: Long = 0L

    @Volatile
    private var pendingKind: Int = 0

    @Volatile
    private var pendingSaveContent: String = ""

    /** Native Android EditText bar docked directly above the soft keyboard for Flash fields. */
    private val gameField by lazy {
        NativeTextFieldBar(
            this,
            object : NativeTextFieldBar.FieldTarget {
                override fun edited(text: String, start: Int, end: Int) {
                    nativeOnGameFieldEdited(text, start, end)
                }

                override fun enter() {
                    nativeOnGameFieldEnter()
                }

                override fun closed() {
                    nativeOnGameFieldClosed()
                }
            },
        )
    }

    private val openDocumentLauncher =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val tabId = pendingTabId
            val kind = pendingKind
            val uri = if (result.resultCode == Activity.RESULT_OK) result.data?.data else null
            if (uri == null) {
                nativeOnDocumentPicked(tabId, kind, null)
                return@registerForActivityResult
            }
            thread(name = "GrimlitePick") {
                val copiedPath = try {
                    copyPickedDocument(uri).absolutePath
                } catch (e: Exception) {
                    Log.w(TAG, "Failed to copy picked document: $e")
                    null
                }
                nativeOnDocumentPicked(tabId, kind, copiedPath)
            }
        }

    private val createDocumentLauncher =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            val tabId = pendingTabId
            val kind = pendingKind
            val content = pendingSaveContent
            pendingSaveContent = ""
            val uri = if (result.resultCode == Activity.RESULT_OK) result.data?.data else null
            if (uri == null) {
                nativeOnDocumentSaved(tabId, kind, null)
                return@registerForActivityResult
            }
            thread(name = "GrimliteSave") {
                val savedName = try {
                    val out = contentResolver.openOutputStream(uri, "wt")
                        ?: throw java.io.IOException("Unable to open output stream")
                    out.use { it.write(content.toByteArray(Charsets.UTF_8)) }
                    resolveDisplayName(uri) ?: "saved"
                } catch (e: Exception) {
                    Log.w(TAG, "Failed to write saved document: $e")
                    null
                }
                nativeOnDocumentSaved(tabId, kind, savedName)
            }
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        instance = this

        window.setSoftInputMode(
            WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_HIDDEN or
                WindowManager.LayoutParams.SOFT_INPUT_ADJUST_NOTHING,
        )
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            window.attributes = window.attributes.apply {
                layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES
            }
        }
        WindowCompat.setDecorFitsSystemWindows(window, false)
        hideSystemBars()

        // Move task to background on Back press instead of killing logged-in sessions / bots.
        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    moveTaskToBack(true)
                }
            },
        )

        logAndroidGameMode()
        askPermissionsAndBatteryOnce()
    }

    override fun onDestroy() {
        if (instance === this) {
            instance = null
        }
        super.onDestroy()
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        if (level >= ComponentCallbacks2.TRIM_MEMORY_RUNNING_LOW) {
            Log.i(TAG, "MainActivity.onTrimMemory(level=$level) -> trimming GPU/shape caches")
            nativeTrimMemory(level)
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus) {
            hideSystemBars()
        }
    }

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        if (event.actionMasked == MotionEvent.ACTION_DOWN &&
            gameField.shown &&
            gameField.touchedOutside(event.rawX, event.rawY)
        ) {
            gameField.close()
        }
        return super.dispatchTouchEvent(event)
    }

    /** Called from Rust (`Grimlite Engine`) when a Flash text field gains focus. */
    fun showGameField(text: String, start: Int, end: Int, password: Boolean, maxChars: Int) {
        runOnUiThread {
            gameField.show(text, start, end, password, maxChars, "Type in the game")
        }
    }

    /** Called from Rust (`Grimlite Engine`) when the focused Flash text field's contents change. */
    fun updateGameField(text: String, start: Int, end: Int) {
        runOnUiThread {
            gameField.update(text, start, end)
        }
    }

    /** Called from Rust (`Grimlite Engine`) when the Flash text field loses focus. */
    fun hideGameField() {
        runOnUiThread {
            gameField.hide(false)
        }
    }

    /** Restores focus to GameActivity's SurfaceView after the native text field closes. */
    fun focusSurface() {
        mSurfaceView?.requestFocus()
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        askPermissionsAndBatteryOnce()
    }

    private fun hideSystemBars() {
        WindowCompat.getInsetsController(window, window.decorView).apply {
            hide(WindowInsetsCompat.Type.systemBars())
            systemBarsBehavior =
                WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
    }

    private fun logAndroidGameMode() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            try {
                val gm = getSystemService(GameManager::class.java)
                val modeName = when (gm?.gameMode) {
                    GameManager.GAME_MODE_PERFORMANCE -> "PERFORMANCE"
                    GameManager.GAME_MODE_BATTERY -> "BATTERY"
                    GameManager.GAME_MODE_STANDARD -> "STANDARD"
                    else -> "UNSUPPORTED (${gm?.gameMode})"
                }
                Log.i(TAG, "Android Game Mode registered & active: $modeName")
            } catch (e: Exception) {
                Log.w(TAG, "Unable to query GameManager: $e")
            }
        }
    }

    /**
     * Prompts once on startup (one dialog at a time, matching Turu):
     * 1. Notification permission on Android 13+ so the foreground bot notification is visible.
     * 2. Unrestricted battery usage (`ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`) so Doze mode
     *    never throttles sockets or background botting while the screen is off.
     */
    private fun askPermissionsAndBatteryOnce() {
        val prefs = getSharedPreferences("grimlite_asked", MODE_PRIVATE)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED &&
            !prefs.getBoolean("post_notifications", false)
        ) {
            prefs.edit().putBoolean("post_notifications", true).apply()
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), REQ_NOTIFICATIONS)
            return
        }

        val pm = getSystemService(PowerManager::class.java)
        if (pm != null &&
            !pm.isIgnoringBatteryOptimizations(packageName) &&
            !prefs.getBoolean("ignore_battery_opt", false)
        ) {
            prefs.edit().putBoolean("ignore_battery_opt", true).apply()
            try {
                startActivity(
                    Intent(
                        Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS,
                        Uri.parse("package:$packageName"),
                    ),
                )
            } catch (e: Exception) {
                Log.w(TAG, "Direct battery optimization prompt failed, falling back: $e")
                try {
                    startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
                } catch (_: Exception) {
                }
            }
        }
    }

    /**
     * Called from Rust (`Grimlite Engine` thread) to open Android's system file explorer
     * (`Intent.ACTION_OPEN_DOCUMENT`). `mimeTypes` contains one MIME type per line.
     */
    fun openDocument(tabId: Long, kind: Int, mimeTypes: String) {
        val types = mimeTypes.lines().map { it.trim() }.filter { it.isNotEmpty() }
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = if (types.size == 1) types[0] else "*/*"
            if (types.size > 1) {
                putExtra(Intent.EXTRA_MIME_TYPES, types.toTypedArray())
            }
        }
        pendingTabId = tabId
        pendingKind = kind
        runOnUiThread {
            try {
                openDocumentLauncher.launch(intent)
            } catch (e: ActivityNotFoundException) {
                Log.w(TAG, "No document picker available: $e")
                nativeOnDocumentPicked(tabId, kind, null)
            }
        }
    }

    /**
     * Called from Rust (`Grimlite Engine` thread) to save UTF-8 JSON / `.gbot` via Android's
     * system file saver (`Intent.ACTION_CREATE_DOCUMENT`).
     */
    fun saveDocument(
        tabId: Long,
        kind: Int,
        mimeType: String,
        fileName: String,
        content: String,
    ) {
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = mimeType
            putExtra(Intent.EXTRA_TITLE, fileName)
        }
        pendingTabId = tabId
        pendingKind = kind
        pendingSaveContent = content
        runOnUiThread {
            try {
                createDocumentLauncher.launch(intent)
            } catch (e: ActivityNotFoundException) {
                Log.w(TAG, "No document saver available: $e")
                pendingSaveContent = ""
                nativeOnDocumentSaved(tabId, kind, null)
            }
        }
    }

    /**
     * Called from Rust (`Grimlite Engine` thread) whenever background botting state changes.
     */
    fun updateBotService(active: Boolean, title: String, text: String) {
        GrimliteService.update(applicationContext, active, title, text)
    }

    private fun resolveDisplayName(uri: Uri): String? {
        val queried = contentResolver
            .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst() && !cursor.isNull(0)) cursor.getString(0) else null
            }
            ?: uri.lastPathSegment?.substringAfterLast('/')
        return queried
            ?.replace(Regex("[/\\\\:*?\"<>|\\u0000]"), "_")
            ?.trim()
            ?.takeIf { it.isNotEmpty() && it != "." && it != ".." }
    }

    private fun copyPickedDocument(uri: Uri): File {
        val dir = File(cacheDir, "picked/${System.nanoTime()}").apply { mkdirs() }
        val file = File(dir, resolveDisplayName(uri) ?: "picked_file")
        val input = contentResolver.openInputStream(uri)
            ?: throw java.io.IOException("Unable to open input stream for $uri")
        input.use { src -> file.outputStream().use { dst -> src.copyTo(dst) } }
        return file
    }

    companion object {
        private const val TAG = "GrimliteRi"
        private const val REQ_NOTIFICATIONS = 101

        @Volatile
        private var instance: MainActivity? = null

        init {
            System.loadLibrary("grimlite_android")
        }

        fun quitApp() {
            val act = instance ?: return
            act.runOnUiThread {
                act.finishAndRemoveTask()
            }
        }

        @JvmStatic
        external fun nativeOnDocumentPicked(tabId: Long, kind: Int, path: String?)

        @JvmStatic
        external fun nativeOnDocumentSaved(tabId: Long, kind: Int, name: String?)

        @JvmStatic
        external fun nativeStopAllBots()

        @JvmStatic
        external fun nativeTrimMemory(level: Int)

        @JvmStatic
        external fun nativeOnGameFieldEdited(text: String, start: Int, end: Int)

        @JvmStatic
        external fun nativeOnGameFieldEnter()

        @JvmStatic
        external fun nativeOnGameFieldClosed()
    }
}
