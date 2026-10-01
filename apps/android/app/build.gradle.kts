import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Upload signing, kept out of the repository entirely: the keystore and
// its passwords live in ~/.config/openpdfedit-android/. Play identifies
// an app by this key forever, so losing it is not a build problem, it is
// an "upload a new app" problem — back that directory up somewhere other
// than this machine. A missing file is not an error, so a debug build
// and CI both still work; only `bundleRelease` needs it.
val signingProps = Properties().apply {
    val f = File(System.getProperty("user.home"), ".config/openpdfedit-android/signing.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}

android {
    namespace = "com.openpdfedit.app"
    compileSdk = 36

    defaultConfig {
        // The same identifier the iOS and Mac App Store builds use. One
        // app across three stores; on Play it is also permanent, so it is
        // deliberately the one already in use rather than a new coinage.
        applicationId = "com.openpdfedit.app"
        // Android 8. The engine is WebAssembly, which needs Chrome 57+,
        // and the System WebView updates independently of the OS — but
        // below 26 the shipped WebView on a never-updated device is old
        // enough to be a real risk, and the editor is not usable on that
        // hardware anyway.
        minSdk = 26
        targetSdk = 36
        versionCode = 2
        versionName = "1.0.1"
    }

    androidResources {
        // `.wasm` must reach the WebView byte-for-byte: a compressed
        // asset cannot be streamed, and `WebAssembly.instantiateStreaming`
        // is how both PDFium and the Rust core are loaded. `.traineddata`
        // is already compressed, so packing it again only costs build
        // time.
        noCompress += listOf("wasm", "traineddata")
    }

    signingConfigs {
        if (signingProps.getProperty("storeFile") != null) {
            create("upload") {
                storeFile = file(signingProps.getProperty("storeFile"))
                storePassword = signingProps.getProperty("storePassword")
                keyAlias = signingProps.getProperty("keyAlias")
                keyPassword = signingProps.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            signingConfig = signingConfigs.findByName("upload")
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.activity:activity-ktx:1.9.3")
    // WebViewAssetLoader — serves the bundled web app over a real https
    // origin so the page is a secure context, which the account SDK's
    // crypto.subtle requires. This is the Android counterpart to iOS's
    // `openpdfedit://localhost` scheme handler.
    implementation("androidx.webkit:webkit:1.12.1")
    // Custom Tabs, for sign-in. The Android answer to
    // ASWebAuthenticationSession: a real address bar, and it shares the
    // browser's cookies so an already-signed-in account is one tap.
    implementation("androidx.browser:browser:1.8.0")
    // Play Billing. Credits are consumables, and the discipline this
    // library needs is the same one StoreKit needs on iOS: a purchase is
    // consumed only after the server has put the credits in the ledger,
    // never on the callback that reports it.
    implementation("com.android.billingclient:billing-ktx:8.3.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
}
