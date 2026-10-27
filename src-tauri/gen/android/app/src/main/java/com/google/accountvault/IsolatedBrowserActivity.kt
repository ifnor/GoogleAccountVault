package com.google.accountvault

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.webkit.*
import android.widget.*
import androidx.annotation.Keep
import org.json.JSONArray
import org.json.JSONObject
import javax.crypto.Mac
import javax.crypto.spec.SecretKeySpec

@Keep
class IsolatedBrowserActivity : Activity() {

    private lateinit var webView: WebView
    private lateinit var progressBar: ProgressBar
    private lateinit var totpButton: TextView
    private var email: String = ""
    private var targetUrl: String = ""
    private var password: String = ""
    private var totpSecret: String = ""
    private var currentTotpCode: String = ""
    private var profileKey: String = ""

    private val mainHandler = Handler(Looper.getMainLooper())
    private var totpRunnable: Runnable? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        try {
            email = intent.getStringExtra("email") ?: ""
            targetUrl = intent.getStringExtra("url") ?: "https://accounts.google.com/"
            password = intent.getStringExtra("password") ?: ""
            totpSecret = intent.getStringExtra("totpSecret") ?: ""

            val rawHash = Math.abs(email.trim().lowercase().hashCode()).toString(16)
            profileKey = "vault_$rawHash"

            initViews()
            setupWebView()
            startTotpTicker()
            loadUrlWithIsolation()
        } catch (e: Throwable) {
            android.util.Log.e("GoogleAccountVault", "Fatal error in IsolatedBrowserActivity.onCreate", e)
            try {
                Toast.makeText(this, "正在切换至系统浏览器打开...", Toast.LENGTH_SHORT).show()
                val fallbackIntent = Intent(Intent.ACTION_VIEW, Uri.parse(targetUrl)).apply {
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                }
                startActivity(fallbackIntent)
            } catch (_: Throwable) {}
            finish()
        }
    }

    private fun dpToPx(dp: Int): Int {
        return (dp * resources.displayMetrics.density).toInt()
    }

    private fun initViews() {
        val rootLayout = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
            setBackgroundColor(Color.WHITE)
        }

        // ================= 1. Top Header Bar =================
        val topBar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                dpToPx(52)
            )
            gravity = Gravity.CENTER_VERTICAL
            setBackgroundColor(Color.parseColor("#F8FAFD"))
            setPadding(dpToPx(6), 0, dpToPx(6), 0)
        }

        // Back Button
        val backBtn = TextView(this).apply {
            text = "←"
            textSize = 22f
            setTextColor(Color.parseColor("#1A73E8"))
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(dpToPx(40), dpToPx(40))
            setOnClickListener {
                if (::webView.isInitialized && webView.canGoBack()) {
                    webView.goBack()
                } else {
                    finish()
                }
            }
        }
        topBar.addView(backBtn)

        // Center Title & Account info
        val centerInfo = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f)
            setPadding(dpToPx(8), 0, dpToPx(8), 0)
        }
        val titleView = TextView(this).apply {
            text = "Google 独立会话"
            textSize = 14f
            setTypeface(null, Typeface.BOLD)
            setTextColor(Color.parseColor("#202124"))
        }
        val subView = TextView(this).apply {
            text = email
            textSize = 11f
            setTextColor(Color.parseColor("#5F6368"))
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        }
        centerInfo.addView(titleView)
        centerInfo.addView(subView)
        topBar.addView(centerInfo)

        // Refresh Button
        val reloadBtn = TextView(this).apply {
            text = "↻"
            textSize = 20f
            setTextColor(Color.parseColor("#5F6368"))
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(dpToPx(40), dpToPx(40))
            setOnClickListener {
                if (::webView.isInitialized) webView.reload()
            }
        }
        topBar.addView(reloadBtn)

        // Close Button
        val closeBtn = TextView(this).apply {
            text = "✕"
            textSize = 18f
            setTextColor(Color.parseColor("#5F6368"))
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(dpToPx(40), dpToPx(40))
            setOnClickListener { finish() }
        }
        topBar.addView(closeBtn)

        rootLayout.addView(topBar)

        // ================= 2. Loading Progress Bar =================
        progressBar = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                dpToPx(3)
            )
            max = 100
            progress = 0
            visibility = View.GONE
        }
        rootLayout.addView(progressBar)

        // ================= 3. Main WebView =================
        webView = WebView(this).apply {
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                0,
                1f
            )
        }
        rootLayout.addView(webView)

        // ================= 4. Bottom Quick Autofill Toolbar =================
        val bottomBar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                dpToPx(52)
            )
            gravity = Gravity.CENTER_VERTICAL
            setBackgroundColor(Color.WHITE)
            elevation = dpToPx(6).toFloat()
            setPadding(dpToPx(8), dpToPx(6), dpToPx(8), dpToPx(6))
        }

        // Button 1: Fill Email
        val emailBtn = createQuickButton("📧 填邮箱", "#F1F3F4", "#3C4043", 1f) {
            fillEmailManually()
        }
        bottomBar.addView(emailBtn)

        // Button 2: Fill Password
        val pwdBtn = createQuickButton("🔑 填密码", "#F1F3F4", "#3C4043", 1f) {
            fillPasswordManually()
        }
        bottomBar.addView(pwdBtn)

        // Button 3: Fill TOTP
        totpButton = createQuickButton("⏱️ 2FA", "#E8F0FE", "#1A73E8", 1.25f) {
            fillTotpManually()
        }
        bottomBar.addView(totpButton)

        rootLayout.addView(bottomBar)

        setContentView(rootLayout)
    }

    private fun createQuickButton(
        label: String,
        bgColorHex: String,
        textColorHex: String,
        weight: Float,
        onClick: () -> Unit
    ): TextView {
        val shape = GradientDrawable().apply {
            shape = GradientDrawable.RECTANGLE
            cornerRadius = dpToPx(10).toFloat()
            setColor(Color.parseColor(bgColorHex))
        }
        return TextView(this).apply {
            text = label
            textSize = 11.5f
            setTextColor(Color.parseColor(textColorHex))
            gravity = Gravity.CENTER
            background = shape
            isClickable = true
            isFocusable = true
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, weight).apply {
                marginStart = dpToPx(3)
                marginEnd = dpToPx(3)
            }
            setOnClickListener { onClick() }
        }
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        if (::webView.isInitialized && webView.canGoBack()) {
            webView.goBack()
        } else {
            super.onBackPressed()
        }
    }

    private fun setupWebView() {
        val s = webView.settings
        s.javaScriptEnabled = true
        s.domStorageEnabled = true
        s.setSupportMultipleWindows(false)
        s.javaScriptCanOpenWindowsAutomatically = true
        s.allowFileAccess = false
        s.allowContentAccess = false

        // Standard mobile Chrome User-Agent without '; wv' to guarantee Google authentication compatibility
        s.userAgentString = "Mozilla/5.0 (Linux; Android 14; Mobile) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.6778.200 Mobile Safari/537.36"

        // Ensure third-party cookies are accepted for Google cross-domain authentication
        try {
            val cm = CookieManager.getInstance()
            cm.setAcceptCookie(true)
            cm.setAcceptThirdPartyCookies(webView, true)
        } catch (_: Throwable) {}

        webView.webChromeClient = object : WebChromeClient() {
            override fun onProgressChanged(view: WebView?, newProgress: Int) {
                if (newProgress < 100) {
                    progressBar.visibility = View.VISIBLE
                    progressBar.progress = newProgress
                } else {
                    progressBar.visibility = View.GONE
                }
            }
        }

        webView.webViewClient = object : WebViewClient() {
            override fun onPageFinished(view: WebView?, url: String?) {
                super.onPageFinished(view, url)
                injectAutofillScript()
                CookieIsolationHelper.saveCookies(this@IsolatedBrowserActivity, webView, profileKey, url)
            }

            override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean {
                val nextUrl = request?.url?.toString() ?: ""
                return handleUrl(nextUrl)
            }

            @Deprecated("Deprecated in Java")
            override fun shouldOverrideUrlLoading(view: WebView?, url: String?): Boolean {
                return handleUrl(url ?: "")
            }

            private fun handleUrl(nextUrl: String): Boolean {
                if (nextUrl.startsWith("http://") || nextUrl.startsWith("https://")) {
                    return false
                }
                return try {
                    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(nextUrl))
                    startActivity(intent)
                    true
                } catch (_: Throwable) {
                    true
                }
            }
        }
    }

    private fun loadUrlWithIsolation() {
        CookieIsolationHelper.setupCookies(this, webView, profileKey)
        webView.loadUrl(targetUrl)
    }

    private fun injectAutofillScript() {
        if (!::webView.isInitialized) return
        val emailJson = JSONObject.quote(email)
        val pwdJson = JSONObject.quote(password)
        val totpJson = JSONObject.quote(currentTotpCode)

        val js = """
            (function() {
                const targetEmail = $emailJson;
                const targetPassword = $pwdJson;
                const targetTotp = $totpJson;
                let emailFilled = false;
                let pwdFilled = false;
                let totpFilled = false;

                function autoFill() {
                    if (!window.location.hostname.includes("google.com")) return;

                    // 1. Email Fill
                    if (targetEmail && !emailFilled) {
                        const emailInput = document.getElementById("identifierId") || 
                                           document.querySelector('input[type="email"]') ||
                                           document.querySelector('input[name="identifier"]');
                        if (emailInput && emailInput.value !== targetEmail) {
                            emailInput.focus();
                            emailInput.value = targetEmail;
                            emailInput.dispatchEvent(new Event("input", { bubbles: true }));
                            emailInput.dispatchEvent(new Event("change", { bubbles: true }));
                            emailFilled = true;
                        }
                    }

                    // 2. Password Fill
                    if (targetPassword && !pwdFilled) {
                        const pwdInput = document.querySelector('input[name="Passwd"]') ||
                                         document.querySelector('input[type="password"]');
                        if (pwdInput && pwdInput.value !== targetPassword) {
                            pwdInput.focus();
                            pwdInput.value = targetPassword;
                            pwdInput.dispatchEvent(new Event("input", { bubbles: true }));
                            pwdInput.dispatchEvent(new Event("change", { bubbles: true }));
                            pwdFilled = true;
                        }
                    }

                    // 3. 2FA Code Fill
                    if (targetTotp && !totpFilled) {
                        const totpInput = document.querySelector('input[name="totpPin"]') ||
                                          document.getElementById("idvPin") ||
                                          document.querySelector('input[type="tel"][maxlength="6"]') ||
                                          document.querySelector('input[autocomplete="one-time-code"]');
                        if (totpInput && totpInput.value !== targetTotp) {
                            totpInput.focus();
                            totpInput.value = targetTotp;
                            totpInput.dispatchEvent(new Event("input", { bubbles: true }));
                            totpInput.dispatchEvent(new Event("change", { bubbles: true }));
                            totpFilled = true;
                        }
                    }
                }

                window.addEventListener("DOMContentLoaded", autoFill);
                window.addEventListener("load", autoFill);
                const observer = new MutationObserver(autoFill);
                if (document.documentElement) {
                    observer.observe(document.documentElement, { childList: true, subtree: true });
                }
                const timer = setInterval(autoFill, 500);
                setTimeout(() => {
                    clearInterval(timer);
                    observer.disconnect();
                }, 30000);
            })();
        """.trimIndent()

        webView.evaluateJavascript(js, null)
    }

    private fun fillEmailManually() {
        if (!::webView.isInitialized) return
        if (email.isBlank()) {
            Toast.makeText(this, "未配置邮箱", Toast.LENGTH_SHORT).show()
            return
        }
        copyToClipboard("Google 邮箱", email)
        val emailJson = JSONObject.quote(email)
        val js = """
            (function() {
                const targetEmail = $emailJson;
                const emailInput = document.getElementById("identifierId") || 
                                   document.querySelector('input[type="email"]') ||
                                   document.querySelector('input[name="identifier"]');
                if (emailInput) {
                    emailInput.focus();
                    emailInput.value = targetEmail;
                    emailInput.dispatchEvent(new Event("input", { bubbles: true }));
                    emailInput.dispatchEvent(new Event("change", { bubbles: true }));
                    return true;
                }
                return false;
            })();
        """.trimIndent()
        webView.evaluateJavascript(js) {
            Toast.makeText(this, "已复制并填入邮箱", Toast.LENGTH_SHORT).show()
        }
    }

    private fun fillPasswordManually() {
        if (!::webView.isInitialized) return
        if (password.isBlank()) {
            Toast.makeText(this, "未配置密码", Toast.LENGTH_SHORT).show()
            return
        }
        copyToClipboard("Google 密码", password)
        val pwdJson = JSONObject.quote(password)
        val js = """
            (function() {
                const targetPwd = $pwdJson;
                const pwdInput = document.querySelector('input[name="Passwd"]') ||
                                 document.querySelector('input[type="password"]');
                if (pwdInput) {
                    pwdInput.focus();
                    pwdInput.value = targetPwd;
                    pwdInput.dispatchEvent(new Event("input", { bubbles: true }));
                    pwdInput.dispatchEvent(new Event("change", { bubbles: true }));
                    return true;
                }
                return false;
            })();
        """.trimIndent()
        webView.evaluateJavascript(js) {
            Toast.makeText(this, "已复制并填入密码", Toast.LENGTH_SHORT).show()
        }
    }

    private fun fillTotpManually() {
        if (!::webView.isInitialized) return
        if (currentTotpCode.isBlank()) {
            Toast.makeText(this, "该账号未配置 2FA 密钥", Toast.LENGTH_SHORT).show()
            return
        }
        copyToClipboard("2FA 动态码", currentTotpCode)
        val totpJson = JSONObject.quote(currentTotpCode)
        val js = """
            (function() {
                const targetTotp = $totpJson;
                const totpInput = document.querySelector('input[name="totpPin"]') ||
                                  document.getElementById("idvPin") ||
                                  document.querySelector('input[type="tel"][maxlength="6"]') ||
                                  document.querySelector('input[autocomplete="one-time-code"]');
                if (totpInput) {
                    totpInput.focus();
                    totpInput.value = targetTotp;
                    totpInput.dispatchEvent(new Event("input", { bubbles: true }));
                    totpInput.dispatchEvent(new Event("change", { bubbles: true }));
                    return true;
                }
                return false;
            })();
        """.trimIndent()
        webView.evaluateJavascript(js) {
            Toast.makeText(this, "已复制并填入动态码: $currentTotpCode", Toast.LENGTH_SHORT).show()
        }
    }

    private fun copyToClipboard(label: String, text: String) {
        try {
            val cm = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            cm.setPrimaryClip(ClipData.newPlainText(label, text))
        } catch (_: Throwable) {}
    }

    private fun startTotpTicker() {
        if (totpSecret.isBlank()) {
            totpButton.text = "⏱️ 未配 2FA"
            return
        }

        totpRunnable = object : Runnable {
            override fun run() {
                val (code, remaining) = TotpHelper.generateTotp(totpSecret)
                currentTotpCode = code
                totpButton.text = "⏱️ $code (${remaining}s)"
                mainHandler.postDelayed(this, 1000)
            }
        }
        totpRunnable?.run()
    }

    override fun onPause() {
        super.onPause()
        if (::webView.isInitialized) {
            CookieIsolationHelper.saveCookies(this, webView, profileKey, webView.url)
        }
    }

    override fun onDestroy() {
        totpRunnable?.let { mainHandler.removeCallbacks(it) }
        if (::webView.isInitialized) {
            CookieIsolationHelper.saveCookies(this, webView, profileKey, webView.url)
            try {
                webView.stopLoading()
                webView.destroy()
            } catch (_: Throwable) {}
        }
        super.onDestroy()
    }
}

object TotpHelper {
    fun generateTotp(secret: String): Pair<String, Int> {
        val clean = secret.trim().replace(" ", "").replace("-", "").replace("=", "").uppercase()
        if (clean.isEmpty()) return Pair("------", 30)
        return try {
            val keyBytes = decodeBase32(clean)
            val timeStep = 30L
            val epochSeconds = System.currentTimeMillis() / 1000L
            val currentInterval = epochSeconds / timeStep
            val remaining = (30 - (epochSeconds % 30)).toInt()

            val data = ByteArray(8)
            var v = currentInterval
            for (i in 7 downTo 0) {
                data[i] = (v and 0xFF).toByte()
                v = v shr 8
            }

            val mac = Mac.getInstance("HmacSHA1")
            mac.init(SecretKeySpec(keyBytes, "HmacSHA1"))
            val hash = mac.doFinal(data)

            val offset = (hash[hash.size - 1].toInt() and 0x0F)
            val binary = ((hash[offset].toInt() and 0x7F) shl 24) or
                    ((hash[offset + 1].toInt() and 0xFF) shl 16) or
                    ((hash[offset + 2].toInt() and 0xFF) shl 8) or
                    (hash[offset + 3].toInt() and 0xFF)

            val otp = binary % 1000000
            val codeStr = String.format("%06d", otp)
            Pair(codeStr, remaining)
        } catch (_: Throwable) {
            Pair("------", 30)
        }
    }

    private fun decodeBase32(base32: String): ByteArray {
        val alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
        val out = java.io.ByteArrayOutputStream()
        var buffer = 0
        var bitsLeft = 0
        for (c in base32) {
            val index = alphabet.indexOf(c)
            if (index < 0) continue
            buffer = (buffer shl 5) or index
            bitsLeft += 5
            if (bitsLeft >= 8) {
                out.write((buffer shr (bitsLeft - 8)) and 0xFF)
                bitsLeft -= 8
            }
        }
        return out.toByteArray()
    }
}

object CookieIsolationHelper {
    fun setupCookies(context: Context, webView: WebView, profileKey: String) {
        try {
            if (androidx.webkit.WebViewFeature.isFeatureSupported(androidx.webkit.WebViewFeature.MULTI_PROFILE)) {
                try {
                    val profileStore = androidx.webkit.ProfileStore.getInstance()
                    val profile = profileStore.getOrCreateProfile(profileKey)
                    androidx.webkit.WebViewCompat.setProfile(webView, profileKey)
                    return
                } catch (e: Throwable) {
                    android.util.Log.w("IsolatedBrowser", "MultiProfile unavailable: ${e.message}")
                }
            }
        } catch (e: Throwable) {
            android.util.Log.w("IsolatedBrowser", "WebViewFeature check failed: ${e.message}")
        }

        // Fallback for older Android engines: restore account cookies into CookieManager
        try {
            val prefs = context.getSharedPreferences("vault_account_cookies", Context.MODE_PRIVATE)
            val saved = prefs.getString(profileKey, null) ?: return
            val cm = CookieManager.getInstance()
            cm.setAcceptCookie(true)
            cm.setAcceptThirdPartyCookies(webView, true)
            val arr = JSONArray(saved)
            for (i in 0 until arr.length()) {
                val item = arr.getJSONObject(i)
                val url = item.getString("url")
                val cookie = item.getString("cookie")
                cm.setCookie(url, cookie)
            }
            cm.flush()
        } catch (e: Throwable) {
            android.util.Log.e("IsolatedBrowser", "Error restoring cookies", e)
        }
    }

    fun saveCookies(context: Context, webView: WebView, profileKey: String, currentUrl: String?) {
        try {
            if (androidx.webkit.WebViewFeature.isFeatureSupported(androidx.webkit.WebViewFeature.MULTI_PROFILE)) {
                try {
                    val profileStore = androidx.webkit.ProfileStore.getInstance()
                    val profile = profileStore.getProfile(profileKey)
                    profile?.cookieManager?.flush()
                    return
                } catch (_: Throwable) {}
            }
        } catch (_: Throwable) {}

        try {
            val cm = CookieManager.getInstance()
            cm.flush()
            if (currentUrl.isNullOrBlank()) return
            val cookiesStr = cm.getCookie(currentUrl) ?: return

            val prefs = context.getSharedPreferences("vault_account_cookies", Context.MODE_PRIVATE)
            val arr = JSONArray()
            val obj = JSONObject().apply {
                put("url", currentUrl)
                put("cookie", cookiesStr)
            }
            arr.put(obj)
            prefs.edit().putString(profileKey, arr.toString()).apply()
        } catch (e: Throwable) {
            android.util.Log.e("IsolatedBrowser", "Error saving cookies", e)
        }
    }
}
