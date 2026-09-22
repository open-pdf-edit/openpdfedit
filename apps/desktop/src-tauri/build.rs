use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Which capability files this build carries. tauri-build validates
    // every file it is pointed at against the plugins actually compiled
    // in, so the updater's permission cannot sit in a build that leaves
    // the updater out — the Mac App Store build, where the store is the
    // only thing allowed to update the app.
    let capabilities = if std::env::var_os("CARGO_FEATURE_UPDATER").is_some() {
        "./capabilities/**/*"
    } else {
        "./capabilities/default.json"
    };
    // A custom pattern turns off tauri-build's own rerun rule for this.
    println!("cargo:rerun-if-changed=capabilities");

    if std::env::var_os("CARGO_FEATURE_APPSTORE").is_some()
        && std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
    {
        link_appstore_native();
    }

    tauri_build::try_build(tauri_build::Attributes::new().capabilities_path_pattern(capabilities))
        .expect("tauri-build failed");
}

/// Builds appstore-native (StoreKit and security-scoped bookmarks, in
/// Swift) for this target's architecture and links it statically.
///
/// Per architecture, not universal: a universal Tauri build compiles each
/// target separately and joins the results, so each pass links its own
/// slice. The Swift runtime is part of macOS; the binary links the SDK's
/// stubs and finds the real thing at /usr/lib/swift.
fn link_appstore_native() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let package = manifest.join("appstore-native");
    let target = std::env::var("TARGET").unwrap();
    let arch = if target.starts_with("aarch64") { "arm64" } else { "x86_64" };

    let status = Command::new("swift")
        .args(["build", "-c", "release", "--arch", arch, "--package-path"])
        .arg(&package)
        .status()
        .expect("swift is needed for the appstore feature (Xcode's command line tools)");
    assert!(status.success(), "building appstore-native failed");

    // Asked, not assumed: with --arch SwiftPM switches build systems and
    // writes somewhere else than a plain `swift build` does.
    let bin = Command::new("swift")
        .args(["build", "-c", "release", "--arch", arch, "--show-bin-path", "--package-path"])
        .arg(&package)
        .output()
        .expect("swift build --show-bin-path");
    let out = String::from_utf8(bin.stdout).unwrap();
    println!("cargo:rustc-link-search=native={}", out.trim());
    println!("cargo:rustc-link-lib=static=AppStoreNative");
    println!("cargo:rustc-link-lib=framework=StoreKit");
    println!("cargo:rustc-link-lib=framework=Foundation");

    let sdk = String::from_utf8(
        Command::new("xcrun").args(["--sdk", "macosx", "--show-sdk-path"]).output().unwrap().stdout,
    )
    .unwrap();
    println!("cargo:rustc-link-search=native={}/usr/lib/swift", sdk.trim());
    // The compatibility shims a Swift object file asks the linker for by
    // name live beside the compiler, not in the SDK.
    let swift = String::from_utf8(Command::new("xcrun").args(["--find", "swift"]).output().unwrap().stdout).unwrap();
    let toolchain = PathBuf::from(swift.trim()).parent().unwrap().parent().unwrap().join("lib/swift/macosx");
    println!("cargo:rustc-link-search=native={}", toolchain.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    println!("cargo:rerun-if-changed=appstore-native/Sources");
    println!("cargo:rerun-if-changed=appstore-native/Package.swift");
}
