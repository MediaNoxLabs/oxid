import org.gradle.api.tasks.bundling.AbstractArchiveTask
import groovy.json.JsonSlurper

plugins {
    id("com.android.library") version "8.4.2"
    kotlin("android") version "1.9.24"
}

fun oxidRepositoryRoot(): File = generateSequence(project.rootDir) { it.parentFile }
    .firstOrNull { candidate ->
        File(candidate, "Cargo.toml").isFile && File(candidate, "apps/oxid/Cargo.toml").isFile
    }
    ?: error("Oxid Android builds must run from a repository-owned target directory")

fun rustlsPlatformVerifierMavenPath(): String {
    val repositoryRoot = oxidRepositoryRoot()
    val metadata = providers.exec {
        workingDir = repositoryRoot
        commandLine(
            "cargo",
            "metadata",
            "--format-version",
            "1",
            "--filter-platform",
            "aarch64-linux-android",
            "--manifest-path",
            File(repositoryRoot, "apps/oxid/Cargo.toml").absolutePath,
        )
    }.standardOutput.asText.get()
    @Suppress("UNCHECKED_CAST")
    val packages = (JsonSlurper().parseText(metadata) as Map<String, Any>)["packages"]
        as List<Map<String, Any>>
    val manifest = file(
        packages.first { candidate -> candidate["name"] == "rustls-platform-verifier-android" }
            .getValue("manifest_path") as String,
    )
    return File(manifest.parentFile, "maven").absolutePath
}

// The verifier is a transitive dependency of this library plugin, but Gradle
// resolves that dependency with the consuming app's repositories. Register the
// Cargo-vendored Maven repository for every generated Android project so the
// app can resolve the AAR as well as the plugin itself.
val rustlsPlatformVerifierRepository = rustlsPlatformVerifierMavenPath()
rootProject.allprojects {
    repositories {
        maven {
            url = uri(rustlsPlatformVerifierRepository)
            metadataSources {
                mavenPom()
                artifact()
            }
        }
    }
}

android {
    namespace = "io.medianox.oxid.mobile"
    compileSdk = 35

    defaultConfig {
        minSdk = 23
        targetSdk = 35
        consumerProguardFiles("consumer-rules.pro")
    }

    buildTypes {
        getByName("release") { isMinifyEnabled = false }
        getByName("debug") { isMinifyEnabled = false }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation("com.google.android.gms:play-services-code-scanner:16.1.0")
    implementation("rustls:rustls-platform-verifier:0.1.1")
}

tasks.withType<AbstractArchiveTask>().configureEach {
    archiveBaseName.set("oxid-mobile-plugin")
}
