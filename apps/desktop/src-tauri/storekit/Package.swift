// swift-tools-version:5.9
// StoreKit 2 for the Mac App Store build, as a static library the Rust
// side links and calls through a C ABI. Built by src-tauri/build.rs when
// the `appstore` feature is on; nothing else compiles it.
import PackageDescription

let package = Package(
    name: "StoreKitBridge",
    platforms: [.macOS(.v12)],
    products: [.library(name: "StoreKitBridge", type: .static, targets: ["StoreKitBridge"])],
    targets: [.target(name: "StoreKitBridge", path: "Sources/StoreKitBridge")]
)
