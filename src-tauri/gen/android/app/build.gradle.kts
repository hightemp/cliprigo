import java.security.MessageDigest
import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Release signing; CI writes keystore.properties from repository secrets.
val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 37
    namespace = "com.hightemp.cliprigo"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.hightemp.cliprigo"
        // Android 11: apps write to Download/ by path without storage permissions.
        minSdk = 30
        targetSdk = 37
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (keystoreProperties.containsKey("storeFile")) {
            create("release") {
                storeFile = rootProject.file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("password")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("password")
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            signingConfig = signingConfigs.findByName("release")
            optimization {
               enable = true
            }
            proguardFiles(
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    buildFeatures {
        buildConfig = true
    }
    packaging {
        jniLibs {
            // The bundled tools are executed, so they must be extracted to disk.
            useLegacyPackaging = true
        }
    }
    sourceSets["main"].jniLibs.srcDir("build/bundled-tools")
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

rust {
    rootDirRel = "../../../"
}

// yt-dlp needs Python, ffmpeg and a JavaScript runtime. youtubedl-android builds them
// for Android as executables packed like native libraries, the only files apps may
// run. Only these executables are taken, never its (GPL) Kotlin code: they run as
// separate processes, see src-tauri/src/components/bundled.rs.
val bundledTools: Configuration by configurations.creating { isTransitive = false }
val bundledToolsSha256 = mapOf(
    "library-0.18.1.aar" to "579b5fb480892b1abc2b218c2089699d52759cc8d7ba256bf876453f0365faef",
    "ffmpeg-0.18.1.aar" to "0a87ffa6cf912b0fe76c1a99b9107f543ee2f247935fae2c71f0822eb7bc5f49",
)

val unpackBundledTools by tasks.registering(Sync::class) {
    val aars = bundledTools.incoming.files
    val expected = bundledToolsSha256
    doFirst {
        aars.forEach { aar ->
            val actual = MessageDigest.getInstance("SHA-256").digest(aar.readBytes())
                .joinToString("") { "%02x".format(it) }
            check(expected[aar.name] == actual) { "${aar.name}: unexpected SHA-256 $actual" }
        }
    }
    from(aars.elements.map { files -> files.map { zipTree(it) } }) {
        include("jni/**")
        eachFile { path = path.removePrefix("jni/") }
        includeEmptyDirs = false
    }
    into(layout.buildDirectory.dir("bundled-tools"))
}
tasks.named("preBuild") { dependsOn(unpackBundledTools) }

dependencies {
    bundledTools("io.github.junkfood02.youtubedl-android:library:0.18.1@aar")
    bundledTools("io.github.junkfood02.youtubedl-android:ffmpeg:0.18.1@aar")
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
