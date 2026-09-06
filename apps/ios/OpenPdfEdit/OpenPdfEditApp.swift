import SwiftUI

/// The iOS shell.
///
/// The editor itself is the same web app that runs at app.openpdfedit.com,
/// bundled into this app rather than fetched — so it works with no network
/// at all, which is the product's central claim and not something a wrapper
/// around a remote URL could honour. What the shell adds is the two things
/// a web page on iOS cannot do: take a PDF handed to it by another app, and
/// sell credits through the App Store.
@main
struct OpenPdfEditApp: App {
    @StateObject private var shell = Shell()

    var body: some Scene {
        WindowGroup {
            AppWebView(shell: shell)
                .ignoresSafeArea(.container, edges: .bottom)
                // A PDF opened from Files, Mail or a share sheet. iOS hands
                // it over as a URL into an inbox directory; the web app is
                // told about it as bytes, because there is no filesystem on
                // its side of the boundary.
                .onOpenURL { url in shell.openIncoming(url) }
        }
    }
}

/// State that outlives the web view: the StoreKit session and any document
/// that arrived before the page was ready to be told about it.
@MainActor
final class Shell: ObservableObject {
    let store = Store()
    /// Set by `AppWebView` once the web view exists.
    weak var bridge: WebBridge?

    /// A document that arrived before the page finished loading.
    ///
    /// iOS delivers the URL as soon as the app launches, which on a cold
    /// start is well before the editor is running. Dropping it would make
    /// "Open in OpenPdfEdit" work only when the app was already open, which
    /// is the case nobody tests.
    private var queued: URL?

    func openIncoming(_ url: URL) {
        if let bridge, bridge.isReady {
            bridge.deliver(url)
        } else {
            queued = url
        }
    }

    /// Called by the bridge once the page says it can accept a document.
    func flushQueuedDocument() {
        guard let url = queued, let bridge else { return }
        queued = nil
        bridge.deliver(url)
    }
}
