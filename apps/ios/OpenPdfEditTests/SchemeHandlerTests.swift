import XCTest

@testable import OpenPdfEdit

/// Serving the bundled web app: what is reachable, and what is not.
@MainActor
final class SchemeHandlerTests: XCTestCase {
    private var handler: BundleSchemeHandler!

    override func setUp() {
        super.setUp()
        handler = BundleSchemeHandler()
    }

    private func url(_ path: String) -> URL {
        URL(string: "\(BundleSchemeHandler.origin)\(path)")!
    }

    func testTheRootServesTheApp() throws {
        let resolved = try XCTUnwrap(handler.resolve(url("/")))
        XCTAssertEqual(resolved.lastPathComponent, "index.html")
    }

    func testARealAssetResolvesToItself() throws {
        let resolved = try XCTUnwrap(handler.resolve(url("/pdfium.wasm")))
        XCTAssertEqual(resolved.lastPathComponent, "pdfium.wasm")
        XCTAssertTrue(FileManager.default.fileExists(atPath: resolved.path))
    }

    func testAnUnknownRouteFallsBackToTheApp() throws {
        // The editor is one page reached by many paths. `/login` is not a
        // file and must not be a 404 — that is exactly the failure that
        // sank the extension's sign-in.
        let resolved = try XCTUnwrap(handler.resolve(url("/login")))
        XCTAssertEqual(resolved.lastPathComponent, "index.html")
    }

    func testAMissingAssetIsAMissingAssetAndNotHtml() {
        // Answering a missing script with the page's HTML turns a build
        // mistake into a baffling syntax error in the console.
        XCTAssertNil(handler.resolve(url("/app/does-not-exist.js")))
    }

    func testNothingOutsideTheWebAppIsReachable() {
        for path in [
            "/../Info.plist",
            "/../../Info.plist",
            "/%2e%2e/Info.plist",
            "/app/../../Info.plist",
        ] {
            let resolved = handler.resolve(url(path))
            if let resolved {
                XCTAssertTrue(
                    resolved.lastPathComponent == "index.html",
                    "\(path) escaped the web app and reached \(resolved.path)"
                )
            }
        }
    }

    func testAnotherHostIsNotServed() {
        XCTAssertNil(handler.resolve(URL(string: "openpdfedit://evil.example/index.html")!))
    }

    func testWasmIsServedAsWasm() {
        // Served as anything else, `WebAssembly.instantiateStreaming`
        // refuses it and the editor never starts.
        XCTAssertEqual(
            BundleSchemeHandler.contentType(of: URL(fileURLWithPath: "/x/pdfium.wasm")),
            "application/wasm"
        )
        XCTAssertEqual(
            BundleSchemeHandler.contentType(of: URL(fileURLWithPath: "/x/index.html")),
            "text/html; charset=utf-8"
        )
        XCTAssertEqual(
            BundleSchemeHandler.contentType(of: URL(fileURLWithPath: "/x/eng.traineddata")),
            "application/octet-stream"
        )
    }

    func testAStagedDocumentIsReachableThenReleased() throws {
        let temporary = FileManager.default.temporaryDirectory
            .appendingPathComponent("staged.pdf")
        try Data("%PDF-1.4".utf8).write(to: temporary)
        defer { try? FileManager.default.removeItem(at: temporary) }

        let path = handler.stage(temporary)
        XCTAssertEqual(handler.resolve(url(path)), temporary)

        // Left staged, the file stays pinned for the life of the process.
        handler.release(path: path)
        XCTAssertNil(handler.resolve(url(path)))
    }
}

/// Containment, checked against the shape of the check rather than only
/// against paths that happen to be in the bundle.
extension SchemeHandlerTests {
    func testASiblingDirectoryThatSharesThePrefixIsNotInside() {
        // `/…/www` and `/…/wwwsomething` share a string prefix and share no
        // directory. A prefix check without the trailing separator lets the
        // second through.
        XCTAssertNil(handler.resolve(url("/../wwwsomething/secret.txt")))
    }
}
