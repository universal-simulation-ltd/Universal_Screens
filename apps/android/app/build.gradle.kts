plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.universalsim.extender"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.universalsim.extender"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }

    buildFeatures { compose = true }
    composeOptions { kotlinCompilerExtensionVersion = "1.5.14" }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    // libextender_mobile.so per ABI is dropped into src/main/jniLibs by cargo-ndk
    // (see ../README.md). Nothing else to configure here.
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.06.00"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.activity:activity-compose:1.9.0")
    implementation("androidx.core:core-ktx:1.13.1")
    // QR scanning of the host's connection code.
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")
    // WebSocket client for the "cast to a browser" rendezvous (RoomSession): the
    // app joins a receiver tab's room over wss:// and sends control frames.
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
}
