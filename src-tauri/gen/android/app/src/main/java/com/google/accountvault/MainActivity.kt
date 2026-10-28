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
  
        @Keep
    @JvmStatic
    fun openIsolatedBrowser(
      context: Context,
      email: String,
      url: String,
      password: String,
      totpSecret: String
    ): Boolean {
      val targetContext = instance ?: context
      return try {
        val rawHash = Math.abs(email.trim().lowercase().hashCode()).toString(16)
        val intent = Intent(targetContext, IsolatedBrowserActivity::class.java).apply {
          putExtra("email", email)
          putExtra("url", url)
          putExtra("password", password)
          putExtra("totpSecret", totpSecret)
          // 独立并发文档任务栈标记（微信小程序式独立多任务）
          addFlags(Intent.FLAG_ACTIVITY_NEW_DOCUMENT)
          addFlags(Intent.FLAG_ACTIVITY_MULTIPLE_TASK)
          addFlags(Intent.FLAG_ACTIVITY_RETAIN_IN_RECENTS)
          data = Uri.parse("vault://account/$rawHash")
        }
        targetContext.startActivity(intent)
        true
      } catch (e: Throwable) {
        android.util.Log.e("GoogleAccountVault", "Failed to launch IsolatedBrowserActivity", e)
        try {
          val fallbackIntent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
          }
          targetContext.startActivity(fallbackIntent)
        } catch (_: Throwable) {}
        false
      }
    }
  }
}
