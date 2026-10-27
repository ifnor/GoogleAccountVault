# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.

# Keep MainActivity and all its companion methods (required for JNI call from Rust)
-keep class com.google.accountvault.MainActivity {
    public static *;
    public *;
}
-keepclassmembers class com.google.accountvault.MainActivity {
    public static *;
    public *;
}
-keep class com.google.accountvault.MainActivity$Companion {
    public *;
}
-keepclassmembers class com.google.accountvault.MainActivity$Companion {
    public *;
}


# Keep IsolatedBrowserActivity & Helpers
-keep class com.google.accountvault.IsolatedBrowserActivity {
    public *;
    public static *;
}
-keepclassmembers class com.google.accountvault.IsolatedBrowserActivity {
    public *;
    public static *;
}
-keep class com.google.accountvault.TotpHelper {
    public *;
    public static *;
}
-keep class com.google.accountvault.CookieIsolationHelper {
    public *;
    public static *;
}


# Keep AndroidX WebKit and Chromium support lib boundary interfaces
-keep class androidx.webkit.** { *; }
-keep interface androidx.webkit.** { *; }
-dontwarn androidx.webkit.**
-keep class org.chromium.support_lib_boundary.** { *; }
-keep interface org.chromium.support_lib_boundary.** { *; }
-dontwarn org.chromium.support_lib_boundary.**
