package com.z44d.serbase

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.FileObserver
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import java.io.File

class MainActivity : TauriActivity() {
    companion object {
        private var instance: MainActivity? = null
        private const val NOTIFICATION_PERMISSION_CODE = 1001
    }

    private var signalObserver: FileObserver? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        instance = this
        requestNotificationPermission()
        startSignalObserver()
    }

    override fun onDestroy() {
        super.onDestroy()
        signalObserver?.stopWatching()
        instance = null
    }

    override fun onResume() {
        super.onResume()
        checkServerSignal()
    }

    override fun onPause() {
        super.onPause()
        checkServerSignal()
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == NOTIFICATION_PERMISSION_CODE) {
            checkServerSignal()
        }
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS)
                != PackageManager.PERMISSION_GRANTED
            ) {
                requestPermissions(
                    arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                    NOTIFICATION_PERMISSION_CODE
                )
            }
        }
    }

    private fun startSignalObserver() {
        val file = serverSignalFile()
        val dir = file.parentFile ?: return
        if (!dir.exists()) dir.mkdirs()
        signalObserver?.stopWatching()
        signalObserver = object : FileObserver(dir, FileObserver.CLOSE_WRITE or FileObserver.DELETE or FileObserver.MOVED_TO or FileObserver.MOVED_FROM) {
            override fun onEvent(event: Int, path: String?) {
                if (path == file.name) {
                    checkServerSignal()
                }
            }
        }.apply { startWatching() }
    }

    private fun startServerService() {
        val intent = Intent(this, ForegroundService::class.java)
        ContextCompat.startForegroundService(this, intent)
    }

    private fun stopServerService() {
        val intent = Intent(this, ForegroundService::class.java)
        stopService(intent)
    }

    private fun serverSignalFile(): File {
        val tauriDataDir = File(filesDir.parentFile, "app_tauri")
        return File(tauriDataDir, "servers_active.signal")
    }

    private fun checkServerSignal() {
        if (serverSignalFile().exists()) {
            startServerService()
        } else {
            stopServerService()
        }
    }
}
