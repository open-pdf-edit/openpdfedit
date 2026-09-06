import StoreKitTest
import WebKit
import XCTest

@testable import OpenPdfEdit

/// The bridge, driven from JavaScript the way the web app drives it.
///
/// These load the real bundled editor into a real `WKWebView` and call the
/// real injected `window.OpenPdfEditNative`. That is the point: the bridge
/// is a contract written twice — once in `WebBridge.swift` and once in
/// `bridge.js` — and the two halves can drift silently. A renamed action, a
/// reply containing something `WKScriptMessageHandlerWithReply` will not
/// serialise, an event name spelled differently on each side: none of it
/// fails to compile, and all of it fails in front of a customer.
@MainActor
final class BridgeTests: XCTestCase {
    private var session: SKTestSession!
    private var shell: Shell!
    private var runtime: WebRuntime!
    private var webView: WKWebView!

    override func setUp() async throws {
        let url = try XCTUnwrap(
            Bundle(for: Self.self).url(forResource: "Products", withExtension: "storekit")
        )
        session = try SKTestSession(contentsOf: url)
        session.resetToDefaultState()
        session.clearTransactions()
        session.disableDialogs = true

        shell = Shell()
        runtime = WebRuntime(shell: shell)
        webView = runtime.makeWebView()
        // Off-screen but in a window: WebKit does not run JavaScript in a
        // view that was never attached to anything.
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 390, height: 844))
        window.addSubview(webView)
        webView.frame = window.bounds
        window.makeKeyAndVisible()

        try await load()
    }

    override func tearDown() {
        webView = nil
        runtime = nil
        shell = nil
        session = nil
    }

    private func load() async throws {
        webView.load(URLRequest(url: BundleSchemeHandler.indexURL))
        // Polled rather than delegate-driven: `WebRuntime` is the
        // navigation delegate, and swapping it out for the test would mean
        // testing a different object than the app uses.
        let deadline = Date().addingTimeInterval(30)
        while Date() < deadline {
            if let ready = try? await webView.evaluateJavaScript(
                "document.readyState === 'complete' && !!window.OpenPdfEditNative"
            ) as? Bool, ready {
                return
            }
            try await Task.sleep(nanoseconds: 100_000_000)
        }
        XCTFail("the bundled web app did not finish loading")
    }

    /// Runs an async JavaScript expression and returns its resolved value.
    private func js(_ source: String) async throws -> Any? {
        try await webView.callAsyncJavaScript(
            source,
            arguments: [:],
            contentWorld: .page
        )
    }

    func testThePageIsServedFromTheOriginTheServerAllowlists() async throws {
        let origin = try await js("return location.origin") as? String
        // If this ever changes, every request the app makes to the account
        // server starts failing CORS — a long way from here, and with no
        // hint that a URL scheme was renamed.
        XCTAssertEqual(origin, BundleSchemeHandler.origin)
        XCTAssertEqual(origin, "openpdfedit://localhost")
    }

    func testThePageIsASecureContext() async throws {
        // `crypto.subtle` is undefined outside one, and the account SDK
        // needs it. Nothing warns; sign-in simply stops working.
        let secure = try await js("return window.isSecureContext") as? Bool
        let subtle = try await js("return !!crypto.subtle") as? Bool
        XCTAssertEqual(secure, true)
        XCTAssertEqual(subtle, true)
    }

    func testTheWebAppSeesTheShell() async throws {
        let platform = try await js("return OpenPdfEditNative.platform") as? String
        XCTAssertEqual(platform, "ios")
        let ready = try await js("return await OpenPdfEditNative.ready()") as? [String: Any]
        XCTAssertEqual(ready?["platform"] as? String, "ios")
        XCTAssertTrue(runtime.bridge.isReady)
    }

    func testProductsCrossTheBridgeWithTheirPrices() async throws {
        let products = try await js("return await OpenPdfEditNative.products()") as? [[String: Any]]
        let listed = try XCTUnwrap(products)
        try requireLocalStoreKit(shell.store.products)
        XCTAssertEqual(listed.compactMap { $0["id"] as? String }.sorted(),
                       Store.productIdentifiers.sorted())
        for product in listed {
            // StoreKit's own formatting, in the customer's currency.
            // Rebuilding it on the JavaScript side is how ¥500 becomes $500.
            XCTAssertFalse((product["price"] as? String ?? "").isEmpty)
            XCTAssertFalse((product["name"] as? String ?? "").isEmpty)
        }
    }

    func testAPurchaseReachesJavaScriptAndStaysUnfinishedUntilFinished() async throws {
        try await shell.store.loadProducts()
        try requireLocalStoreKit(shell.store.products)

        let bought = try await js(
            "return await OpenPdfEditNative.purchase('credits_1000')"
        ) as? [String: Any]
        let purchase = try XCTUnwrap(bought)
        XCTAssertEqual(purchase["status"] as? String, "purchased")
        let transactionId = try XCTUnwrap(purchase["transactionId"] as? String)
        XCTAssertFalse((purchase["receipt"] as? String ?? "").isEmpty)
        XCTAssertEqual(purchase["verifiedLocally"] as? Bool, true)

        // The rule, seen from the side the web app is on: until the server
        // has granted the credits, this purchase is still outstanding and
        // will come back on the next launch.
        var outstanding = try await js(
            "return (await OpenPdfEditNative.outstanding()).map(r => r.transactionId)"
        ) as? [String]
        XCTAssertEqual(outstanding?.contains(transactionId), true)

        // Passed as an argument rather than interpolated into source: a
        // transaction id is data, and source-building with data is how a
        // quote in the wrong place becomes a syntax error at runtime.
        _ = try await webView.callAsyncJavaScript(
            "return await OpenPdfEditNative.finish(id)",
            arguments: ["id": transactionId],
            contentWorld: .page
        )

        outstanding = try await js(
            "return (await OpenPdfEditNative.outstanding()).map(r => r.transactionId)"
        ) as? [String]
        XCTAssertEqual(outstanding?.contains(transactionId), false)
    }

    func testAStoreFailureReachesJavaScriptAsAFailure() async throws {
        try await shell.store.loadProducts()
        try requireLocalStoreKit(shell.store.products)
        session.failTransactionsEnabled = true

        let result = try await js(
            "return await OpenPdfEditNative.purchase('credits_1000').catch(e => ({error: String(e)}))"
        ) as? [String: Any]
        // The dangerous failure mode is the opposite of a spurious error: a
        // purchase that did not happen coming back as one that did, and
        // credits granted for money never taken.
        XCTAssertNotNil(result?["error"])
        XCTAssertNotEqual(result?["status"] as? String, "purchased")
    }

    func testADocumentHandedOverByAnotherAppReachesJavaScriptAsAFile() async throws {
        let pdf = FileManager.default.temporaryDirectory
            .appendingPathComponent("handed-over.pdf")
        try Data("%PDF-1.7\n%%EOF\n".utf8).write(to: pdf)
        defer { try? FileManager.default.removeItem(at: pdf) }

        // Listen first: the event is dispatched synchronously from the
        // shell and nothing replays it.
        _ = try await js(
            """
            window.__handedOver = new Promise((resolve) => {
              OpenPdfEditNative.on('document', async (detail) => {
                const file = await OpenPdfEditNative.readDocument(detail);
                resolve({ name: file.name, size: file.size, type: file.type });
              });
            });
            return true;
            """
        )

        shell.bridge?.deliver(pdf)

        let received = try await js("return await window.__handedOver") as? [String: Any]
        let file = try XCTUnwrap(received)
        XCTAssertEqual(file["name"] as? String, "handed-over.pdf")
        XCTAssertEqual(file["type"] as? String, "application/pdf")
        XCTAssertEqual(file["size"] as? Int, 15)
    }

    func testAnUnknownActionIsRefusedRatherThanIgnored() async throws {
        let result = try await js(
            """
            try {
              await window.webkit.messageHandlers.openpdfedit.postMessage({ action: 'nonsense' });
              return 'resolved';
            } catch (e) {
              return String(e.message || e);
            }
            """
        ) as? String
        // Silence here would look like a hung promise to the web app.
        XCTAssertEqual(result?.contains("nonsense"), true, "got \(result ?? "nil")")
    }

    func testOnlyTheAppsOwnPagesMayNavigate() {
        XCTAssertEqual(runtime.policy(for: BundleSchemeHandler.indexURL), .allow)
        // A sign-in redirect, a support page: they belong in Safari, where
        // the address bar is visible. Navigating here would replace the
        // editor and leave no way back.
        XCTAssertEqual(runtime.policy(for: URL(string: "https://example.com")!), .cancel)
        XCTAssertEqual(runtime.policy(for: nil), .cancel)
    }
}
