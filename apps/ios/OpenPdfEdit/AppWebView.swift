import SwiftUI
import WebKit

/// The web view the whole app is.
struct AppWebView: UIViewRepresentable {
    let shell: Shell

    func makeCoordinator() -> WebRuntime { WebRuntime(shell: shell) }

    func makeUIView(context: Context) -> WKWebView {
        let webView = context.coordinator.makeWebView()
        webView.load(URLRequest(url: BundleSchemeHandler.indexURL))
        return webView
    }

    func updateUIView(_ webView: WKWebView, context: Context) {}
}

/// Everything needed to put the bundled web app on screen: the scheme
/// handler that serves it, the bridge it talks to the shell through, and the
/// navigation policy that keeps it from wandering off.
///
/// Separate from `AppWebView` and able to build its own web view, so the
/// tests can stand one up without SwiftUI. The bridge is the seam where a
/// mistake stays invisible until a customer hits it — a renamed action, a
/// reply that is not JSON-serialisable — and a contract worth testing needs
/// something to test it against.
@MainActor
final class WebRuntime: NSObject, WKNavigationDelegate, WKUIDelegate {
    let scheme = BundleSchemeHandler()
    let bridge: WebBridge
    private let shim: WKUserScript?

    init(shell: Shell) {
        bridge = WebBridge(shell: shell, scheme: scheme)
        // Injected at document start so the page can call the bridge from
        // its very first script, rather than having to wait for something
        // to tell it the shell exists.
        if let url = Bundle.main.url(forResource: "bridge", withExtension: "js"),
           let source = try? String(contentsOf: url, encoding: .utf8) {
            shim = WKUserScript(
                source: source,
                injectionTime: .atDocumentStart,
                forMainFrameOnly: true
            )
        } else {
            shim = nil
        }
        super.init()
    }

    func makeWebView() -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.setURLSchemeHandler(scheme, forURLScheme: BundleSchemeHandler.scheme)
        configuration.userContentController.addScriptMessageHandler(
            bridge,
            contentWorld: .page,
            name: WebBridge.channel
        )
        if let shim {
            configuration.userContentController.addUserScript(shim)
        }
        // The editor renders and saves entirely on the device, so nothing
        // here should ever want a video to autoplay or a picture-in-picture
        // window. Left at their defaults these merely add surface.
        configuration.allowsInlineMediaPlayback = false
        configuration.mediaTypesRequiringUserActionForPlayback = .all

        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = self
        webView.uiDelegate = self
        // No rubber-band scroll on the shell. The editor manages its own
        // scrolling; the page itself bouncing under it looks like a bug.
        webView.scrollView.bounces = false
        webView.isOpaque = false
        webView.backgroundColor = UIColor(red: 0.067, green: 0.067, blue: 0.067, alpha: 1)
        webView.scrollView.backgroundColor = webView.backgroundColor
        bridge.webView = webView
        return webView
    }

    /// Anything that is not this app's own page opens in Safari.
    ///
    /// A privacy policy, a support page, a sign-in redirect: all of them
    /// belong in the browser, where the customer can see the address they
    /// are on. Letting them navigate this web view instead would replace
    /// the editor with a web page and leave no way back.
    func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        decisionHandler(policy(for: navigationAction.request.url))
    }

    /// The decision itself, separated from the callback so it can be
    /// checked without a navigation.
    func policy(for url: URL?) -> WKNavigationActionPolicy {
        guard let url else { return .cancel }
        if url.scheme == BundleSchemeHandler.scheme { return .allow }
        if url.scheme == "https" || url.scheme == "mailto" {
            UIApplication.shared.open(url)
        }
        return .cancel
    }

    /// `window.open` — which is how the account panel starts sign-in in
    /// every build that is not this one.
    func webView(
        _ webView: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        if let url = navigationAction.request.url, url.scheme == "https" {
            UIApplication.shared.open(url)
        }
        return nil
    }

    #if DEBUG
    /// A one-line report on whether the shell handed the page a working
    /// environment.
    ///
    /// The two that can silently go wrong are `isSecureContext` — lose it
    /// and `crypto.subtle` disappears, taking sign-in with it — and the
    /// origin, which has to stay exactly the string the account server
    /// allowlists. Both are decided by how the scheme handler is
    /// registered, neither shows up as an error, and both are invisible
    /// until something far away breaks.
    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        let probe = """
        JSON.stringify({
          origin: location.origin,
          secureContext: window.isSecureContext,
          subtleCrypto: !!(window.crypto && window.crypto.subtle),
          webAssembly: typeof WebAssembly,
          bridge: !!window.OpenPdfEditNative
        })
        """
        webView.evaluateJavaScript(probe) { value, _ in
            NSLog("OpenPdfEdit shell: %@", (value as? String) ?? "no answer")
        }
    }

    func webView(
        _ webView: WKWebView,
        didFailProvisionalNavigation navigation: WKNavigation!,
        withError error: Error
    ) {
        NSLog("OpenPdfEdit shell: navigation failed: %@", error.localizedDescription)
    }
    #endif
}
