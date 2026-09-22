import Foundation

// Security-scoped bookmarks: how a sandboxed app gets back to a file it
// was handed once.
//
// The sandbox grants access to a file for as long as the app runs after
// someone picks it in an open or save panel. A path remembered for Recent
// files is therefore worth nothing after a relaunch — the file is exactly
// where it was, and opening it is refused, which the app could only
// report as "moved, renamed or deleted". A bookmark created while access
// is live is macOS's own token for asking again later, and it keeps
// working when the file is renamed or moved on the same volume.

/// A bookmark for `path`, base64-encoded, or `{"error": …}` as JSON.
/// Must be called while the app still has access — straight after an open
/// or a save through a panel.
@_cdecl("oa_bm_create")
public func oa_bm_create(_ pathPointer: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let url = URL(fileURLWithPath: String(cString: pathPointer))
    do {
        let data = try url.bookmarkData(options: .withSecurityScope,
                                        includingResourceValuesForKeys: nil, relativeTo: nil)
        return strdup(data.base64EncodedString())!
    } catch {
        return strdup("{\"error\":\"\(error.localizedDescription.replacingOccurrences(of: "\"", with: "'"))\"}")!
    }
}

/// Resolves a bookmark and starts accessing the file it names, returning
/// `{"path": …, "stale": bool}` or `{"error": …}`.
///
/// Access is started and deliberately never stopped: the document stays
/// open and saveable for the rest of the session, and the grant ends with
/// the process. `stale` means macOS wants a fresh bookmark — the file was
/// moved — which the caller creates now that access is live again.
@_cdecl("oa_bm_resolve")
public func oa_bm_resolve(_ base64Pointer: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    guard let data = Data(base64Encoded: String(cString: base64Pointer)) else {
        return strdup("{\"error\":\"not a bookmark\"}")!
    }
    var stale = false
    do {
        let url = try URL(resolvingBookmarkData: data, options: .withSecurityScope,
                          relativeTo: nil, bookmarkDataIsStale: &stale)
        guard url.startAccessingSecurityScopedResource() else {
            return strdup("{\"error\":\"access was refused\"}")!
        }
        let payload: [String: Any] = ["path": url.path, "stale": stale]
        let json = try JSONSerialization.data(withJSONObject: payload)
        return strdup(String(decoding: json, as: UTF8.self))!
    } catch {
        return strdup("{\"error\":\"\(error.localizedDescription.replacingOccurrences(of: "\"", with: "'"))\"}")!
    }
}
