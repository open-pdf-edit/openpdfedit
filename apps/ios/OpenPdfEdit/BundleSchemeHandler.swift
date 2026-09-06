import Foundation
import UniformTypeIdentifiers
import WebKit

/// Serves the bundled web app from `openpdfedit://localhost/`.
///
/// ## Why a custom scheme and not `file://`
///
/// A `file://` page has an opaque origin. Every request it makes to the
/// account server would carry `Origin: null`, which is not something a CORS
/// allowlist can name without naming every other sandboxed document on the
/// internet at the same time. `openpdfedit://localhost` is a real, stable
/// origin: it goes in `OPENAPPS_SERVER_ALLOWED_ORIGINS` beside the
/// extension's, and nothing else can claim it.
///
/// The host is `localhost` on purpose. WebKit decides whether a page is a
/// secure context partly by host, and a page that is not one loses
/// `crypto.subtle` — which the account SDK needs.
///
/// ## Why not an embedded HTTP server
///
/// It would also give a real origin, and a guaranteed-secure one. But the
/// port is chosen at launch, so the origin changes between runs, and an
/// origin that changes cannot be allowlisted. A fixed port trades that for
/// a collision with whatever else wants it.
final class BundleSchemeHandler: NSObject, WKURLSchemeHandler {
    static let scheme = "openpdfedit"
    static let host = "localhost"
    static var origin: String { "\(scheme)://\(host)" }
    static var indexURL: URL { URL(string: "\(origin)/index.html")! }

    /// The directory `www` was copied into at build time.
    private let root: URL

    /// Documents handed to this app by another one, addressable from the
    /// page.
    ///
    /// A PDF arrives as a file on disk and has to reach JavaScript. Passing
    /// it as a base64 string through `evaluateJavaScript` works and costs a
    /// third more memory than the file, twice over — once for the string
    /// and once for the decoded copy — on a device where a large scan is
    /// already the biggest thing in the process. Serving it under a URL the
    /// page can `fetch` keeps it a stream.
    ///
    /// Main thread only, which is where both `WKURLSchemeHandler` callbacks
    /// and the document handoff already run.
    private var inbox: [String: URL] = [:]
    private var nextInboxId = 1

    /// Makes a file readable by the page, returning the path to fetch.
    @MainActor
    func stage(_ file: URL) -> String {
        let id = String(nextInboxId)
        nextInboxId += 1
        inbox[id] = file
        return "/__incoming/\(id)"
    }

    /// Forgets a staged document once the page has read it. Left in place
    /// it would pin the file for the life of the process.
    @MainActor
    func release(path: String) {
        guard let id = path.split(separator: "/").last else { return }
        inbox.removeValue(forKey: String(id))
    }

    override init() {
        guard let root = Bundle.main.url(forResource: "www", withExtension: nil) else {
            // Not recoverable and not worth pretending otherwise: without
            // the web app there is no app. This fires on the first launch
            // after someone builds without running scripts/sync-web.sh,
            // and a clear crash beats a blank screen.
            fatalError("www/ is missing from the app bundle — run apps/ios/scripts/sync-web.sh")
        }
        self.root = root
    }

    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        guard let url = task.request.url, let file = resolve(url) else {
            task.didFailWithError(URLError(.fileDoesNotExist))
            return
        }
        guard let data = try? Data(contentsOf: file, options: .mappedIfSafe) else {
            task.didFailWithError(URLError(.cannotOpenFile))
            return
        }

        let response = HTTPURLResponse(
            url: url,
            statusCode: 200,
            httpVersion: "HTTP/1.1",
            headerFields: [
                "Content-Type": Self.contentType(of: file),
                "Content-Length": String(data.count),
                // The pages are local and immutable for the life of a
                // build; WebKit still revalidates on navigation without
                // this and pays for it on every asset.
                "Cache-Control": "no-cache",
            ]
        )!
        task.didReceive(response)
        task.didReceive(data)
        task.didFinish()
    }

    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}

    /// Maps a URL onto a file inside `www`, refusing anything that would
    /// leave it.
    ///
    /// Internal rather than private so the tests can reach it. The
    /// containment check below is the only security-relevant line in this
    /// file, and testing it through `WKURLSchemeTask` — a protocol with no
    /// usable stand-in — would mean not testing it.
    ///
    /// The path comes from the page, and a page can ask for anything —
    /// including `../../../` up into the rest of the app bundle. Resolving
    /// symlinks and then checking the prefix is what makes that impossible
    /// rather than merely unlikely.
    func resolve(_ url: URL) -> URL? {
        guard url.host == Self.host else { return nil }
        var path = url.path
        if path.isEmpty || path == "/" { path = "/index.html" }

        if path.hasPrefix("/__incoming/") {
            // Not under `root`, so it skips the containment check below on
            // purpose: these are files this app staged itself, never a
            // path the page composed.
            return inbox[String(path.dropFirst("/__incoming/".count))]
        }

        let candidate = root.appendingPathComponent(path).standardizedFileURL
        guard candidate.path.hasPrefix(root.standardizedFileURL.path) else { return nil }

        var isDirectory: ObjCBool = false
        if FileManager.default.fileExists(atPath: candidate.path, isDirectory: &isDirectory) {
            if isDirectory.boolValue {
                let index = candidate.appendingPathComponent("index.html")
                return FileManager.default.fileExists(atPath: index.path) ? index : nil
            }
            return candidate
        }

        // Single-page-app fallback. The editor is one page reached by many
        // paths; a request for a path with no extension is a route, not a
        // missing file. A request for `/app/thing.js` that is not there is
        // a genuine 404, and answering it with HTML would turn a build
        // mistake into a baffling syntax error.
        return candidate.pathExtension.isEmpty
            ? root.appendingPathComponent("index.html")
            : nil
    }

    static func contentType(of file: URL) -> String {
        // WebAssembly is the one that matters: served as anything else,
        // `instantiateStreaming` refuses it and the editor never starts.
        switch file.pathExtension.lowercased() {
        case "html": return "text/html; charset=utf-8"
        case "js", "mjs": return "text/javascript; charset=utf-8"
        case "css": return "text/css; charset=utf-8"
        case "json", "webmanifest": return "application/json; charset=utf-8"
        case "wasm": return "application/wasm"
        case "traineddata": return "application/octet-stream"
        default:
            return UTType(filenameExtension: file.pathExtension)?.preferredMIMEType
                ?? "application/octet-stream"
        }
    }
}
