import AuthenticationServices
import Foundation

/// Signing in, the only way iOS allows.
///
/// The web app's own flow opens a popup and waits for it to post the
/// session back. There are no popups here — `window.open` in a WKWebView
/// has nowhere to go — and pushing the sign-in page into this web view
/// would replace the editor with an OAuth screen and lose any unsaved
/// document behind it.
///
/// `ASWebAuthenticationSession` is Apple's answer, and it is a better one:
/// it shows the real address bar, so someone typing a Google password can
/// see whose page they are typing it into, and it shares Safari's cookies,
/// so an account already signed in on the device usually needs one tap.
///
/// The tokens come back in the callback URL's **fragment**, not its query.
/// A fragment is never sent to a server, so it cannot end up in an access
/// log or a referrer header on the way past.
@MainActor
final class AuthSession: NSObject, ASWebAuthenticationPresentationContextProviding {
    /// Deliberately not `openpdfedit`, which the bundled web app is served
    /// from. Two things claiming one scheme is a puzzle waiting for
    /// whoever debugs it next.
    static let callbackScheme = "openpdfedit-auth"

    /// Where the login page lives. The real web origin, not the bundled
    /// copy: an OAuth redirect can only land on `https`.
    static let loginURL = "https://app.openpdfedit.com/login"

    struct Session {
        let accessToken: String
        let refreshToken: String
    }

    enum Failure: LocalizedError {
        case cancelled
        case malformedCallback
        case failed(String)

        var errorDescription: String? {
            switch self {
            case .cancelled: return "cancelled"
            case .malformedCallback: return "The sign-in page came back without a session."
            case .failed(let message): return message
            }
        }
    }

    /// Held for the life of the sign-in: releasing it cancels the session.
    private var session: ASWebAuthenticationSession?

    func signIn() async throws -> Session {
        var components = URLComponents(string: Self.loginURL)!
        // Tells the login page to finish by redirecting to the app rather
        // than by posting a message to an opener it does not have.
        components.queryItems = [URLQueryItem(name: "native", value: "ios")]

        let callback: URL = try await withCheckedThrowingContinuation { continuation in
            let session = ASWebAuthenticationSession(
                url: components.url!,
                callbackURLScheme: Self.callbackScheme
            ) { url, error in
                if let url {
                    continuation.resume(returning: url)
                } else if let error = error as? ASWebAuthenticationSessionError,
                          error.code == .canceledLogin {
                    continuation.resume(throwing: Failure.cancelled)
                } else {
                    continuation.resume(
                        throwing: Failure.failed(error?.localizedDescription ?? "sign-in failed")
                    )
                }
            }
            session.presentationContextProvider = self
            // Off, so the sheet can see the Safari session the customer is
            // probably already signed into. The alternative asks someone
            // who is signed into Google on this very device to type their
            // password again.
            session.prefersEphemeralWebBrowserSession = false
            self.session = session
            session.start()
        }

        return try Self.parse(callback)
    }

    /// Pure, and `nonisolated` so it can be tested without a main-actor
    /// hop: this is the one function here that decides whether a string
    /// becomes a credential, and it should be trivial to exercise.
    nonisolated static func parse(_ callback: URL) throws -> Session {
        guard let fragment = callback.fragment else { throw Failure.malformedCallback }
        var values: [String: String] = [:]
        for pair in fragment.split(separator: "&") {
            let parts = pair.split(separator: "=", maxSplits: 1)
            guard parts.count == 2 else { continue }
            values[String(parts[0])] = String(parts[1]).removingPercentEncoding
        }
        guard let access = values["access_token"], !access.isEmpty,
              let refresh = values["refresh_token"], !refresh.isEmpty
        else { throw Failure.malformedCallback }
        return Session(accessToken: access, refreshToken: refresh)
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        let scene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first { $0.activationState == .foregroundActive }
        return scene?.keyWindow ?? ASPresentationAnchor()
    }
}
