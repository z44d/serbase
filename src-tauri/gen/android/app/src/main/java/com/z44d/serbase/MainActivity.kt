package com.z44d.serbase

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import java.io.File

class MainActivity : TauriActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        instance = this
    }

    override fun onDestroy() {
        super.onDestroy()
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

    companion object {
        private var instance: MainActivity? = null
    }
}
