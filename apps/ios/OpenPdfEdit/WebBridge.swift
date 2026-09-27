import Foundation
import UIKit
import WebKit

/// The one channel between the bundled web app and this shell.
///
/// Deliberately one channel and not several. Every call is a JSON object
/// with an `action`, answered by a promise on the JavaScript side, so
/// adding a capability means adding a case here and a method in `bridge.js`
/// — not another message handler, another injected global and another thing
/// to remember to register.
@MainActor
final class WebBridge: NSObject, WKScriptMessageHandlerWithReply {
    static let channel = "openpdfedit"

    private unowned let shell: Shell
    private let scheme: BundleSchemeHandler
    private let auth = AuthSession()
    weak var webView: WKWebView?

    /// True once the page has said it can accept a document. Until then a
    /// document that arrives is held rather than dropped.
    private(set) var isReady = false

    init(shell: Shell, scheme: BundleSchemeHandler) {
        self.shell = shell
        self.scheme = scheme
        super.init()
        shell.bridge = self

        // A transaction StoreKit delivers on its own — an interrupted
        // purchase finishing, an Ask to Buy approval, a purchase made on
        // the customer's other device. The page is told so it can redeem
        // it; if the page is not up yet, `outstanding` will find it at
        // startup instead.
        shell.store.onUnprompted = { [weak self] receipt in
            self?.push(event: "receipt", detail: Self.describe(receipt))
        }
    }

    func userContentController(
        _ controller: WKUserContentController,
        didReceive message: WKScriptMessage,
        replyHandler: @escaping (Any?, String?) -> Void
    ) {
        guard let body = message.body as? [String: Any],
              let action = body["action"] as? String
        else {
            replyHandler(nil, "malformed bridge message")
            return
        }

        switch action {
        case "ready":
            isReady = true
            shell.flushQueuedDocument()
            replyHandler(["platform": "ios"], nil)

        case "products":
            Task {
                do {
                    try await shell.store.loadProducts()
                    replyHandler(["products": shell.store.describeProducts()], nil)
                } catch {
                    replyHandler(nil, error.localizedDescription)
                }
            }

        case "purchase":
            guard let productId = body["productId"] as? String else {
                replyHandler(nil, "purchase needs a productId")
                return
            }
            Task {
                do {
                    switch try await shell.store.purchase(productId) {
                    case .bought(let receipt):
                        replyHandler(Self.describe(receipt), nil)
                    case .cancelled:
                        // Not an error. An app that shows "purchase failed"
                        // because someone tapped Cancel is telling them
                        // something went wrong when nothing did.
                        replyHandler(["status": "cancelled"], nil)
                    case .pending:
                        replyHandler(["status": "pending"], nil)
                    }
                } catch {
                    replyHandler(nil, error.localizedDescription)
                }
            }

        case "signIn":
            Task {
                do {
                    let session = try await auth.signIn()
                    replyHandler(
                        [
                            "status": "signed_in",
                            "accessToken": session.accessToken,
                            "refreshToken": session.refreshToken,
                        ],
                        nil
                    )
                } catch AuthSession.Failure.cancelled {
                    replyHandler(["status": "cancelled"], nil)
                } catch {
                    replyHandler(nil, error.localizedDescription)
                }
            }

        case "outstanding":
            Task {
                let receipts = await shell.store.outstanding()
                replyHandler(["receipts": receipts.map(Self.describe)], nil)
            }

        case "finish":
            guard let transactionId = body["transactionId"] as? String else {
                replyHandler(nil, "finish needs a transactionId")
                return
            }
            Task {
                await shell.store.finish(transactionId)
                replyHandler(["finished": true], nil)
            }

        case "documentRead":
            // The page has the bytes; the staged file can go.
            if let path = body["path"] as? String { scheme.release(path: path) }
            replyHandler(["released": true], nil)

        case "saveFile":
            // The way anything leaves this app.
            //
            // The web layer's only other means of producing a file is an
            // `<a download>` pointing at a blob: URL. In a browser that is
            // a download; in a WKWebView it is a navigation, and
            // `AppWebView.policy(for:)` cancels every scheme but this
            // app's own — so Save, Save a copy and Export Markdown all
            // clicked a link the shell threw away, silently. A share sheet
            // is the iOS answer: it offers "Save to Files" alongside Mail,
            // AirDrop and everything else the customer has, which is more
            // than a download would have given them anyway.
            guard let name = body["name"] as? String,
                  let base64 = body["data"] as? String,
                  let data = Data(base64Encoded: base64)
            else {
                replyHandler(nil, "saveFile needs a name and base64 data")
                return
            }
            do {
                replyHandler(["shared": try presentShareSheet(named: name, data: data)], nil)
            } catch {
                replyHandler(nil, "could not stage the file: \(error.localizedDescription)")
            }

        default:
            replyHandler(nil, "unknown action \(action)")
        }
    }

    /// Hands a document iOS gave us to the page.
    func deliver(_ url: URL) {
        // Files arriving through "Open in" are copied into this app's
        // Inbox, so there is nothing security-scoped to start accessing —
        // which is exactly why `LSSupportsOpeningDocumentsInPlace` is off.
        // The editor saves by producing a new file, so claiming to edit in
        // place would be a promise it cannot keep.
        let path = scheme.stage(url)
        push(event: "document", detail: ["path": path, "name": url.lastPathComponent])
    }

    /// Writes the bytes somewhere the share sheet can read them.
    ///
    /// Separated from presenting so it can be tested: the presentation
    /// needs a key window and a running app, while everything that can
    /// quietly be wrong — decoding, the filename, the directory — is
    /// here.
    ///
    /// A directory per export. Two saves of the same name would otherwise
    /// overwrite each other while both sheets were still open, and the
    /// second customer to tap Save would be sharing the first file.
    static func stageForSharing(named name: String, data: Data) throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("export-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        // A name is what the customer sees in Files and in Mail. An empty
        // one — which the page should never send, but might — would make
        // a directory-looking URL and a share of nothing.
        let file = directory.appendingPathComponent(name.isEmpty ? "document.pdf" : name)
        try data.write(to: file, options: .atomic)
        return file
    }

    /// Writes the bytes to a real file and offers it to the share sheet.
    ///
    /// A real file, not an in-memory `Data`: "Save to Files" and the mail
    /// composer both take the filename from the URL, and an activity item
    /// that is only `Data` arrives as "Untitled". The name the customer
    /// chose in the editor is the one they should be offered.
    ///
    /// Each save gets its own directory under the temporary one, so two
    /// exports of the same name cannot collide while both sheets are
    /// open. The system reclaims that directory; deleting it here would
    /// race the sheet, which reads the file only once the customer has
    /// picked a destination.
    private func presentShareSheet(named name: String, data: Data) throws -> Bool {
        let file = try Self.stageForSharing(named: name, data: data)

        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        guard var presenter = scene?.keyWindow?.rootViewController else { return false }
        while let next = presenter.presentedViewController { presenter = next }

        let sheet = UIActivityViewController(activityItems: [file], applicationActivities: nil)
        // iPad has no modal sheet: without an anchor UIKit raises rather
        // than guessing, and the app goes down on the customer's first
        // export. Anchored to the middle of the presenting view, since the
        // call arrives from the page with no originating rect.
        if let popover = sheet.popoverPresentationController {
            popover.sourceView = presenter.view
            popover.sourceRect = CGRect(x: presenter.view.bounds.midX,
                                        y: presenter.view.bounds.midY,
                                        width: 0, height: 0)
            popover.permittedArrowDirections = []
        }
        presenter.present(sheet, animated: true)
        return true
    }

    private func push(event: String, detail: [String: Any]) {
        guard let webView,
              let json = try? JSONSerialization.data(withJSONObject: detail),
              let text = String(data: json, encoding: .utf8)
        else { return }
        // `JSON.parse` of a JSON string literal, rather than interpolating
        // an object literal: the values include filenames, and a filename
        // with a quote in it would otherwise be a script injection into
        // this app's own page.
        let script = """
        window.OpenPdfEditNative && window.OpenPdfEditNative._emit(
          \(quoted(event)), JSON.parse(\(quoted(text)))
        );
        """
        webView.evaluateJavaScript(script)
    }

    private func quoted(_ value: String) -> String {
        let data = try? JSONSerialization.data(withJSONObject: [value])
        guard let data, var text = String(data: data, encoding: .utf8) else { return "\"\"" }
        text.removeFirst()
        text.removeLast()
        return text
    }

    private static func describe(_ receipt: Store.PurchaseReceipt) -> [String: Any] {
        [
            "status": "purchased",
            "transactionId": receipt.transactionId,
            "productId": receipt.productId,
            "receipt": receipt.jws,
            "verifiedLocally": receipt.verifiedLocally,
        ]
    }
}
