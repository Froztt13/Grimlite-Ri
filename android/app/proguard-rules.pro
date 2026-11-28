# R8 rules for Grimlite Ri release APK: shrinks unused AndroidX/Kotlin classes while
# preserving every JNI entry point called from Rust (`libgrimlite_android.so`).

# Keep all Grimlite Ri classes, methods, and fields called via JNI from Rust
# (`openDocument`, `saveDocument`, `updateBotService`, `showGameField`, `updateGameField`,
# `hideGameField`, `native*`, `GrimliteService`, etc.).
-keep class com.grimlite.ri.** { *; }

# Keep rustls-platform-verifier's Kotlin/Java half invoked over JNI during TLS verification.
-keep,includedescriptorclasses class org.rustls.platformverifier.** { *; }

# Keep GameActivity's Java half invoked over JNI by android-activity's native C++ glue.
-keep class com.google.androidgamesdk.** { *; }

# Keep stack traces readable in logcat crash reports.
-dontobfuscate
-keepattributes SourceFile,LineNumberTable
