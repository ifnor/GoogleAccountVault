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
