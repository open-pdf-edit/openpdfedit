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
    tauri_build::try_build(tauri_build::Attributes::new().capabilities_path_pattern(capabilities))
        .expect("tauri-build failed");
}
