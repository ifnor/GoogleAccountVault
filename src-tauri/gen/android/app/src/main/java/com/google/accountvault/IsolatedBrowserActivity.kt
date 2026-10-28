package com.google.accountvault

import android.app.Activity
import android.app.ActivityManager
import android.app.AlertDialog
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.MotionEvent
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
    private var email: String = ""
    private var targetUrl: String = ""
    private var password: String = ""
    private var totpSecret: String = ""
    private var recoveryEmail: String = ""
    private var currentTotpCode: String = ""
    private var profileKey: String = ""
    private var isDesktopMode: Boolean = false

    // Floating Pill states
    private var showFloatingPill: Boolean = true
    private lateinit var floatingPillContainer: FrameLayout
    private lateinit var floatingCollapsedView: LinearLayout
    private lateinit var floatingExpandedView: LinearLayout
    private lateinit var floatingSummaryText: TextView
    private var autoCollapseRunnable: Runnable? = null

    private val mainHandler = Handler(Looper.getMainLooper())
    private var totpRunnable: Runnable? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        try {
            email = intent.getStringExtra("email") ?: ""
            targetUrl = intent.getStringExtra("url") ?: "https://accounts.google.com/"
            password = intent.getStringExtra("password") ?: ""
            totpSecret = intent.getStringExtra("totpSecret") ?: ""
            recoveryEmail = intent.getStringExtra("recoveryEmail") ?: ""

            val rawHash = Math.abs(email.trim().lowercase().hashCode()).toString(16)
            profileKey = "vault_$rawHash"

            // 1. Edge-to-Edge System Status & Navigation Bar setup
            setupSystemBars()

            // 2. Custom Task Description for Multitasking Overview
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
                val taskTitle = if (email.isNotBlank()) "Google · $email" else "Google 独立沙箱"
                val taskDesc = ActivityManager.TaskDescription(
                    taskTitle,
                    null,
                    Color.parseColor("#1A73E8")
                )
                setTaskDescription(taskDesc)
            }

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

    private fun setupSystemBars() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
            window.decorView.systemUiVisibility =
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or
                View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
        }
        window.statusBarColor = Color.TRANSPARENT
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            window.decorView.systemUiVisibility =
                window.decorView.systemUiVisibility or View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR
            window.navigationBarColor = Color.parseColor("#F8FAFD")
        }
    }

    override fun onNewIntent(intent: Intent?) {
        super.onNewIntent(intent)
        setIntent(intent)
        val newEmail = intent?.getStringExtra("email")
        if (!newEmail.isNullOrBlank()) email = newEmail
        val newPwd = intent?.getStringExtra("password")
        if (!newPwd.isNullOrBlank()) password = newPwd
        val newTotp = intent?.getStringExtra("totpSecret")
        if (!newTotp.isNullOrBlank()) totpSecret = newTotp
        val newRec = intent?.getStringExtra("recoveryEmail")
        if (!newRec.isNullOrBlank()) recoveryEmail = newRec

        val newUrl = intent?.getStringExtra("url")
        if (!newUrl.isNullOrBlank() && newUrl != targetUrl) {
            targetUrl = newUrl
            if (::webView.isInitialized) {
                webView.loadUrl(targetUrl)
            }
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

        // ================= 1. Top Header Bar (WeChat Capsule Style) =================
        val topBar = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
            )
            gravity = Gravity.CENTER_VERTICAL
            setBackgroundColor(Color.parseColor("#F8FAFD"))
            setPadding(dpToPx(8), dpToPx(8), dpToPx(8), dpToPx(8))
        }

        // 1.1 Left: Back to Main App Button (返回管家本体)
        val hostBtnShape = GradientDrawable().apply {
            shape = GradientDrawable.RECTANGLE
            cornerRadius = dpToPx(14).toFloat()
            setColor(Color.parseColor("#E8F0FE"))
        }
        val backToHostBtn = TextView(this).apply {
            text = "⇲ 管家"
            textSize = 12f
            setTypeface(null, Typeface.BOLD)
            setTextColor(Color.parseColor("#1A73E8"))
            background = hostBtnShape
            gravity = Gravity.CENTER
            setPadding(dpToPx(8), dpToPx(4), dpToPx(8), dpToPx(4))
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT,
                dpToPx(30)
            )
            setOnClickListener {
                try {
                    val hostIntent = Intent(this@IsolatedBrowserActivity, MainActivity::class.java).apply {
                        addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT)
                    }
                    startActivity(hostIntent)
                } catch (_: Throwable) {
                    moveTaskToBack(true)
                }
            }
        }
        topBar.addView(backToHostBtn)

        // 1.2 Web Navigation Back Button (网页内后退)
        val webBackBtn = TextView(this).apply {
            text = "‹"
            textSize = 22f
            setTextColor(Color.parseColor("#5F6368"))
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(dpToPx(28), dpToPx(36)).apply {
                marginStart = dpToPx(2)
            }
            setOnClickListener {
                if (::webView.isInitialized && webView.canGoBack()) {
                    webView.goBack()
                } else {
                    moveTaskToBack(true)
                }
            }
        }
        topBar.addView(webBackBtn)

        // 1.3 Center: Title & Email Subtitle
        val centerInfo = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f).apply {
                marginStart = dpToPx(4)
                marginEnd = dpToPx(4)
            }
        }
        val titleView = TextView(this).apply {
            text = "Google 独立沙箱 🔒"
            textSize = 13f
            setTypeface(null, Typeface.BOLD)
            setTextColor(Color.parseColor("#202124"))
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        }
        val subView = TextView(this).apply {
            text = if (email.isNotBlank()) email else "独立会话"
            textSize = 10.5f
            setTextColor(Color.parseColor("#5F6368"))
            maxLines = 1
            ellipsize = android.text.TextUtils.TruncateAt.END
        }
        centerInfo.addView(titleView)
        centerInfo.addView(subView)
        topBar.addView(centerInfo)

        // 1.4 Right: WeChat Capsule Controller [ ··· | ✕ ]
        val capsuleContainer = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            layoutParams = LinearLayout.LayoutParams(dpToPx(76), dpToPx(32))
            gravity = Gravity.CENTER_VERTICAL
            val capsuleShape = GradientDrawable().apply {
                shape = GradientDrawable.RECTANGLE
                cornerRadius = dpToPx(16).toFloat()
                setColor(Color.WHITE)
                setStroke(dpToPx(1), Color.parseColor("#DADCE0"))
            }
            background = capsuleShape
        }

        // More options button (···)
        val moreBtn = TextView(this).apply {
            text = "···"
            textSize = 15f
            setTypeface(null, Typeface.BOLD)
            setTextColor(Color.parseColor("#3C4043"))
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f)
            setOnClickListener {
                showCapsuleMenu()
            }
        }
        capsuleContainer.addView(moreBtn)

        // Divider line (|)
        val capsuleDivider = View(this).apply {
            layoutParams = LinearLayout.LayoutParams(dpToPx(1), dpToPx(16))
            setBackgroundColor(Color.parseColor("#DADCE0"))
        }
        capsuleContainer.addView(capsuleDivider)

        // Close button (✕)
        val closeBtn = TextView(this).apply {
            text = "✕"
            textSize = 13f
            setTextColor(Color.parseColor("#3C4043"))
            gravity = Gravity.CENTER
            layoutParams = LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.MATCH_PARENT, 1f)
            setOnClickListener {
                closeSandboxTask()
            }
        }
        capsuleContainer.addView(closeBtn)

        topBar.addView(capsuleContainer)
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

        // ================= 3. Main Content Container (FrameLayout for 100% Fullscreen WebView + Floating Pill) =================
        val contentContainer = FrameLayout(this).apply {
            layoutParams = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                0,
                1f
            )
        }

        webView = WebView(this).apply {
            layoutParams = FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
        }
        contentContainer.addView(webView)

        // ================= 4. Setup Collapsible Floating Pill =================
        setupFloatingPill(contentContainer)

        rootLayout.addView(contentContainer)

        // ================= 5. WindowInsets (Status Bar & Bottom Gesture Bar Padding) =================
        rootLayout.setOnApplyWindowInsetsListener { _, insets ->
            val statusTop = insets.systemWindowInsetTop
            val navBottom = insets.systemWindowInsetBottom
            topBar.setPadding(dpToPx(8), statusTop + dpToPx(4), dpToPx(8), dpToPx(4))
            contentContainer.setPadding(0, 0, 0, navBottom)
            insets
        }

        setContentView(rootLayout)
    }

    private fun setupFloatingPill(container: FrameLayout) {
        floatingPillContainer = FrameLayout(this).apply {
            val lp = FrameLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
            ).apply {
                gravity = Gravity.BOTTOM or Gravity.END
                marginEnd = dpToPx(8)
                bottomMargin = dpToPx(70)
            }
            layoutParams = lp
            elevation = dpToPx(8).toFloat()
        }

        // 1. Collapsed View (默认小巧贴边药丸)
        floatingCollapsedView = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dpToPx(12), dpToPx(7), dpToPx(12), dpToPx(7))
            val bg = GradientDrawable().apply {
                shape = GradientDrawable.RECTANGLE
                cornerRadius = dpToPx(18).toFloat()
                setColor(Color.WHITE)
                setStroke(dpToPx(1), Color.parseColor("#DADCE0"))
            }
            background = bg
            setOnClickListener {
                expandFloatingPill()
            }
        }

        floatingSummaryText = TextView(this).apply {
            text = "⏱️ 凭据"
            textSize = 12f
            setTypeface(null, Typeface.BOLD)
            setTextColor(Color.parseColor("#1A73E8"))
        }
        floatingCollapsedView.addView(floatingSummaryText)
        floatingPillContainer.addView(floatingCollapsedView)

        // 2. Expanded View (展开横向快捷条)
        floatingExpandedView = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dpToPx(6), dpToPx(4), dpToPx(6), dpToPx(4))
            val bg = GradientDrawable().apply {
                shape = GradientDrawable.RECTANGLE
                cornerRadius = dpToPx(18).toFloat()
                setColor(Color.parseColor("#F8FAFD"))
                setStroke(dpToPx(1), Color.parseColor("#1A73E8"))
            }
            background = bg
            visibility = View.GONE
        }

        // 2.1 填邮箱
        val fEmailBtn = createMiniPillButton("📧 邮箱", "#FFFFFF", "#3C4043") {
            fillEmailManually()
            scheduleAutoCollapse()
        }
        floatingExpandedView.addView(fEmailBtn)

        // 2.2 填密码
        val fPwdBtn = createMiniPillButton("🔑 密码", "#FFFFFF", "#3C4043") {
            fillPasswordManually()
            scheduleAutoCollapse()
        }
        floatingExpandedView.addView(fPwdBtn)

        // 2.3 填辅助邮箱
        val fRecBtn = createMiniPillButton("🛡️ 辅助", "#FFFFFF", "#188038") {
            fillRecoveryEmailManually()
            scheduleAutoCollapse()
        }
        floatingExpandedView.addView(fRecBtn)

        // 2.4 填 2FA
        val fTotpBtn = createMiniPillButton("⏱️ 2FA", "#E8F0FE", "#1A73E8") {
            fillTotpManually()
            scheduleAutoCollapse()
        }
        floatingExpandedView.addView(fTotpBtn)

        // 2.5 收起按钮 (›)
        val fCloseBtn = TextView(this).apply {
            text = "›"
            textSize = 18f
            setTextColor(Color.parseColor("#5F6368"))
            gravity = Gravity.CENTER
            setPadding(dpToPx(6), 0, dpToPx(4), 0)
            setOnClickListener {
                collapseFloatingPill()
            }
        }
        floatingExpandedView.addView(fCloseBtn)

        floatingPillContainer.addView(floatingExpandedView)
        container.addView(floatingPillContainer)

        setupFloatingDrag()
    }

    private fun createMiniPillButton(
        label: String,
        bgColorHex: String,
        textColorHex: String,
        onClick: () -> Unit
    ): TextView {
        val bg = GradientDrawable().apply {
            shape = GradientDrawable.RECTANGLE
            cornerRadius = dpToPx(12).toFloat()
            setColor(Color.parseColor(bgColorHex))
            setStroke(dpToPx(1), Color.parseColor("#DADCE0"))
        }
        return TextView(this).apply {
            text = label
            textSize = 11.5f
            setTypeface(null, Typeface.BOLD)
            setTextColor(Color.parseColor(textColorHex))
            gravity = Gravity.CENTER
            background = bg
            setPadding(dpToPx(8), dpToPx(5), dpToPx(8), dpToPx(5))
            val lp = LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
            ).apply {
                marginStart = dpToPx(2)
                marginEnd = dpToPx(2)
            }
            layoutParams = lp
            setOnClickListener { onClick() }
        }
    }

    private fun expandFloatingPill() {
        floatingCollapsedView.visibility = View.GONE
        floatingExpandedView.visibility = View.VISIBLE
        scheduleAutoCollapse()
    }

    private fun collapseFloatingPill() {
        autoCollapseRunnable?.let { mainHandler.removeCallbacks(it) }
        floatingExpandedView.visibility = View.GONE
        floatingCollapsedView.visibility = View.VISIBLE
    }

    private fun scheduleAutoCollapse() {
        autoCollapseRunnable?.let { mainHandler.removeCallbacks(it) }
        autoCollapseRunnable = Runnable {
            collapseFloatingPill()
        }
        mainHandler.postDelayed(autoCollapseRunnable!!, 5000)
    }

    private var dY = 0f
    private fun setupFloatingDrag() {
        var startY = 0f
        floatingCollapsedView.setOnTouchListener { _, event ->
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    startY = event.rawY
                    dY = floatingPillContainer.y - event.rawY
                    false
                }
                MotionEvent.ACTION_MOVE -> {
                    val delta = Math.abs(event.rawY - startY)
                    if (delta > dpToPx(6)) {
                        val newY = event.rawY + dY
                        val parent = floatingPillContainer.parent as? View
                        if (parent != null) {
                            val maxY = parent.height - floatingPillContainer.height - dpToPx(10)
                            val minY = dpToPx(10).toFloat()
                            floatingPillContainer.y = newY.coerceIn(minY, maxY.toFloat())
                        }
                        true
                    } else {
                        false
                    }
                }
                else -> false
            }
        }
    }

    private fun updateFloatingPillText(remaining: Int = 30) {
        if (!::floatingSummaryText.isInitialized) return
        if (currentTotpCode.isNotBlank() && currentTotpCode != "------") {
            floatingSummaryText.text = "⏱️ $currentTotpCode (${remaining}s)"
        } else if (recoveryEmail.isNotBlank()) {
            floatingSummaryText.text = "🛡️ 辅助凭据"
        } else {
            floatingSummaryText.text = "🔑 快捷凭据"
        }
    }

    private fun showCapsuleMenu() {
        val current = if (::webView.isInitialized) (webView.url ?: targetUrl) else targetUrl
        val modeLabel = if (isDesktopMode) "📱 切换为移动版网页" else "🖥️ 切换为桌面版网页"
        val pillToggleLabel = if (showFloatingPill) "🔘 隐藏贴边悬浮球" else "🔘 显示贴边悬浮球"
        val recLabel = if (recoveryEmail.isNotBlank()) "🛡️ 填入辅助邮箱 ($recoveryEmail)" else "🛡️ 填入辅助邮箱 (未配置)"
        val totpLabel = if (currentTotpCode.isNotBlank() && currentTotpCode != "------") "⏱️ 填入 2FA 动态码 ($currentTotpCode)" else "⏱️ 填入 2FA 动态码 (未配置)"

        val items = arrayOf(
            "📧 填入/复制主邮箱 ($email)",
            "🔑 填入/复制密码",
            recLabel,
            totpLabel,
            "──────────────",
            "🔄 刷新页面",
            "📋 复制当前网址",
            "🌐 在系统外部浏览器打开",
            modeLabel,
            pillToggleLabel,
            "🧹 清理沙箱缓存 & Cookie",
            "ℹ️ 账号详情"
        )
        AlertDialog.Builder(this)
            .setTitle(if (email.isNotBlank()) email else "Google 独立沙箱")
            .setItems(items) { _, which ->
                when (which) {
                    0 -> fillEmailManually()
                    1 -> fillPasswordManually()
                    2 -> fillRecoveryEmailManually()
                    3 -> fillTotpManually()
                    4 -> {} // Separator
                    5 -> if (::webView.isInitialized) webView.reload()
                    6 -> {
                        copyToClipboard("网址", current)
                        Toast.makeText(this, "网址已复制到剪贴板", Toast.LENGTH_SHORT).show()
                    }
                    7 -> {
                        try {
                            val extIntent = Intent(Intent.ACTION_VIEW, Uri.parse(current)).apply {
                                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                            }
                            startActivity(extIntent)
                        } catch (e: Exception) {
                            Toast.makeText(this, "未找到可用外部浏览器", Toast.LENGTH_SHORT).show()
                        }
                    }
                    8 -> toggleDesktopMode()
                    9 -> {
                        showFloatingPill = !showFloatingPill
                        if (::floatingPillContainer.isInitialized) {
                            floatingPillContainer.visibility = if (showFloatingPill) View.VISIBLE else View.GONE
                        }
                        Toast.makeText(this, if (showFloatingPill) "已开启贴边悬浮球" else "已隐藏贴边悬浮球", Toast.LENGTH_SHORT).show()
                    }
                    10 -> clearSandboxStorage()
                    11 -> showAccountDetailsDialog()
                }
            }
            .setNegativeButton("取消", null)
            .show()
    }

    private fun toggleDesktopMode() {
        if (!::webView.isInitialized) return
        isDesktopMode = !isDesktopMode
        val s = webView.settings
        if (isDesktopMode) {
            s.userAgentString = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"
            Toast.makeText(this, "已切换为桌面版 UA", Toast.LENGTH_SHORT).show()
        } else {
            s.userAgentString = "Mozilla/5.0 (Linux; Android 14; Mobile) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.6778.200 Mobile Safari/537.36"
            Toast.makeText(this, "已切换为移动版 UA", Toast.LENGTH_SHORT).show()
        }
        webView.reload()
    }

    private fun clearSandboxStorage() {
        AlertDialog.Builder(this)
            .setTitle("清理独立沙箱")
            .setMessage("确定要清除【$email】此沙箱下的所有 Cookie 与本地缓存吗？这会登出当前 Google 会话。")
            .setPositiveButton("确定清理") { _, _ ->
                try {
                    if (::webView.isInitialized) {
                        webView.clearCache(true)
                        webView.clearFormData()
                        webView.clearHistory()
                    }
                    val prefs = getSharedPreferences("vault_account_cookies", Context.MODE_PRIVATE)
                    prefs.edit().remove(profileKey).apply()
                    if (androidx.webkit.WebViewFeature.isFeatureSupported(androidx.webkit.WebViewFeature.MULTI_PROFILE)) {
                        try {
                            val profileStore = androidx.webkit.ProfileStore.getInstance()
                            profileStore.getProfile(profileKey)?.cookieManager?.removeAllCookies(null)
                        } catch (_: Throwable) {}
                    }
                    Toast.makeText(this, "沙箱 Cookie 与缓存已清理", Toast.LENGTH_SHORT).show()
                    if (::webView.isInitialized) {
                        webView.loadUrl(targetUrl)
                    }
                } catch (e: Throwable) {
                    Toast.makeText(this, "清理失败: ${e.message}", Toast.LENGTH_SHORT).show()
                }
            }
            .setNegativeButton("取消", null)
            .show()
    }

    private fun showAccountDetailsDialog() {
        val pwdMasked = if (password.isNotBlank()) "••••••••" else "(未配置)"
        val totpStatus = if (totpSecret.isNotBlank()) "已配置 (当前码: $currentTotpCode)" else "(未配置)"
        val recStatus = if (recoveryEmail.isNotBlank()) recoveryEmail else "(未配置)"
        val msg = "主邮箱：$email\n密码：$pwdMasked\n辅助邮箱：$recStatus\n2FA：$totpStatus\n沙箱标识：$profileKey"
        AlertDialog.Builder(this)
            .setTitle("账号沙箱凭据")
            .setMessage(msg)
            .setPositiveButton("复制辅助邮箱") { _, _ ->
                if (recoveryEmail.isNotBlank()) {
                    copyToClipboard("辅助邮箱", recoveryEmail)
                    Toast.makeText(this, "辅助邮箱已复制", Toast.LENGTH_SHORT).show()
                }
            }
            .setNeutralButton("复制 2FA") { _, _ ->
                if (currentTotpCode.isNotBlank() && currentTotpCode != "------") {
                    copyToClipboard("2FA 动态码", currentTotpCode)
                    Toast.makeText(this, "2FA 动态码已复制", Toast.LENGTH_SHORT).show()
                }
            }
            .setNegativeButton("关闭", null)
            .show()
    }

    private fun closeSandboxTask() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
            finishAndRemoveTask()
        } else {
            finish()
        }
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        if (::webView.isInitialized && webView.canGoBack()) {
            webView.goBack()
        } else {
            // 如同微信小程序，根页面按返回键时退至后台（挂起），不直接销毁
            moveTaskToBack(true)
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
        val recJson = JSONObject.quote(recoveryEmail)

        val js = """
            (function() {
                const targetEmail = $emailJson;
                const targetPassword = $pwdJson;
                const targetTotp = $totpJson;
                const targetRec = $recJson;
                let emailFilled = false;
                let pwdFilled = false;
                let totpFilled = false;
                let recFilled = false;

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

                    // 4. Recovery Email Fill
                    if (targetRec && !recFilled) {
                        const recInput = document.getElementById("knowledge-preregistered-email-response") ||
                                         document.querySelector('input[name="knowledgePreregisteredEmailResponse"]') ||
                                         document.querySelector('input[type="email"][aria-label*="辅助"]') ||
                                         document.querySelector('input[type="email"][aria-label*="recovery"]');
                        if (recInput && recInput.value !== targetRec) {
                            recInput.focus();
                            recInput.value = targetRec;
                            recInput.dispatchEvent(new Event("input", { bubbles: true }));
                            recInput.dispatchEvent(new Event("change", { bubbles: true }));
                            recFilled = true;
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

    private fun fillRecoveryEmailManually() {
        if (!::webView.isInitialized) return
        if (recoveryEmail.isBlank()) {
            Toast.makeText(this, "该账号未配置辅助邮箱", Toast.LENGTH_SHORT).show()
            return
        }
        copyToClipboard("辅助邮箱", recoveryEmail)
        val recJson = JSONObject.quote(recoveryEmail)
        val js = """
            (function() {
                const targetRec = $recJson;
                const recInput = document.getElementById("knowledge-preregistered-email-response") ||
                                 document.querySelector('input[name="knowledgePreregisteredEmailResponse"]') ||
                                 document.querySelector('input[type="email"][aria-label*="辅助"]') ||
                                 document.querySelector('input[type="email"][aria-label*="recovery"]') ||
                                 document.querySelector('input[type="email"]') ||
                                 document.querySelector('input[name="identifier"]');
                if (recInput) {
                    recInput.focus();
                    recInput.value = targetRec;
                    recInput.dispatchEvent(new Event("input", { bubbles: true }));
                    recInput.dispatchEvent(new Event("change", { bubbles: true }));
                    return true;
                }
                return false;
            })();
        """.trimIndent()
        webView.evaluateJavascript(js) {
            Toast.makeText(this, "已复制并填入辅助邮箱: $recoveryEmail", Toast.LENGTH_SHORT).show()
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
            updateFloatingPillText(30)
            return
        }

        totpRunnable = object : Runnable {
            override fun run() {
                val (code, remaining) = TotpHelper.generateTotp(totpSecret)
                currentTotpCode = code
                updateFloatingPillText(remaining)
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
        autoCollapseRunnable?.let { mainHandler.removeCallbacks(it) }
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
