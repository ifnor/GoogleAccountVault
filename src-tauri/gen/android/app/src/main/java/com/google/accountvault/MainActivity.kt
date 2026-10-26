package com.google.accountvault

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.enableEdgeToEdge
import androidx.annotation.Keep
import androidx.core.content.FileProvider
import java.io.File

@Keep
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    instance = this
  }

  override fun onDestroy() {
    super.onDestroy()
    if (instance == this) {
      instance = null
    }
  }

  @Keep
  companion object {
    var instance: MainActivity? = null

    @Keep
    @JvmStatic
    fun installApk(context: Context, filePath: String): Boolean {
      return try {
        val file = File(filePath)
        if (!file.exists()) {
          android.util.Log.e("GoogleAccountVault", "APK file does not exist: $filePath")
          return false
        }

        file.setReadable(true, false)
        val targetContext = instance ?: context

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
          if (!targetContext.packageManager.canRequestPackageInstalls()) {
            try {
              val manageIntent = Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES).apply {
                data = Uri.parse("package:${targetContext.packageName}")
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
              }
              targetContext.startActivity(manageIntent)
            } catch (se: Exception) {
              android.util.Log.w("GoogleAccountVault", "Could not open unknown sources settings", se)
            }
          }
        }

        val apkUri: Uri = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
          FileProvider.getUriForFile(
            targetContext,
            "${targetContext.packageName}.fileprovider",
            file
          )
        } else {
          Uri.fromFile(file)
        }

        val intent = Intent(Intent.ACTION_VIEW).apply {
          setDataAndType(apkUri, "application/vnd.android.package-archive")
          addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
          addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        targetContext.startActivity(intent)
        true
      } catch (e: Exception) {
        android.util.Log.e("GoogleAccountVault", "Failed to launch package installer", e)
        false
      }
    }
  }
}
