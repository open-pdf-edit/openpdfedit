// The Android shell is its own Gradle build, not part of the Cargo
// workspace above it — the two toolchains share nothing but the web
// bundle that `scripts/sync-web.sh` copies into `app/src/main/assets`.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "OpenPdfEdit"
include(":app")
