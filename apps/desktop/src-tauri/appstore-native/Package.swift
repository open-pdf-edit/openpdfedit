// swift-tools-version:5.9
// The native half of the Mac App Store build, as a static library the Rust
// side links and calls through a C ABI: StoreKit 2 for purchases, and
// security-scoped bookmarks so the sandbox lets Recent files reopen.
// Built by src-tauri/build.rs when the `appstore` feature is on; the
// Developer ID build is neither sandboxed nor sold through the store, and
// never compiles it.
import PackageDescription

let package = Package(
    name: "AppStoreNative",
    platforms: [.macOS(.v12)],
    products: [.library(name: "AppStoreNative", type: .static, targets: ["AppStoreNative"])],
    targets: [.target(name: "AppStoreNative", path: "Sources/AppStoreNative")]
)
