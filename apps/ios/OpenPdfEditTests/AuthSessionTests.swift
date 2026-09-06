import XCTest

@testable import OpenPdfEdit

/// The sign-in callback is where a credential crosses from Safari into this
/// app. Everything it accepts becomes a session.
final class AuthSessionTests: XCTestCase {
    func testAFragmentCarryingBothTokensIsAccepted() throws {
        let url = URL(
            string: "openpdfedit-auth://callback#access_token=abc.def&refresh_token=xyz"
        )!
        let session = try AuthSession.parse(url)
        XCTAssertEqual(session.accessToken, "abc.def")
        XCTAssertEqual(session.refreshToken, "xyz")
    }

    func testPercentEncodingSurvives() throws {
        // Tokens are base64url and should not need this, but a URL is a URL
        // and something upstream may encode. Decoding a "+" as a space
        // would corrupt a token silently, and the failure would land at the
        // server as "your session is invalid".
        let url = URL(
            string: "openpdfedit-auth://callback#access_token=a%2Bb%2Fc&refresh_token=d%3De"
        )!
        let session = try AuthSession.parse(url)
        XCTAssertEqual(session.accessToken, "a+b/c")
        XCTAssertEqual(session.refreshToken, "d=e")
    }

    func testAQueryStringIsNotAFragment() throws {
        // Deliberately refused. The tokens go in the fragment precisely so
        // they are never sent to a server; accepting them from the query
        // would quietly re-open the hole that choice closes.
        let url = URL(
            string: "openpdfedit-auth://callback?access_token=abc&refresh_token=xyz"
        )!
        XCTAssertThrowsError(try AuthSession.parse(url))
    }

    func testHalfASessionIsNoSession() throws {
        for fragment in ["access_token=abc", "refresh_token=xyz", "access_token=&refresh_token=x"] {
            let url = URL(string: "openpdfedit-auth://callback#\(fragment)")!
            XCTAssertThrowsError(
                try AuthSession.parse(url),
                "accepted \(fragment)"
            ) { error in
                XCTAssertEqual(error as? AuthSession.Failure, .malformedCallback)
            }
        }
    }

    func testTheCallbackSchemeIsNotTheAppsOwnScheme() {
        // Two things claiming one scheme is a puzzle for whoever debugs it
        // next, and this is the assertion that keeps a future rename from
        // collapsing them.
        XCTAssertNotEqual(AuthSession.callbackScheme, BundleSchemeHandler.scheme)
    }
}

extension AuthSession.Failure: Equatable {
    public static func == (lhs: AuthSession.Failure, rhs: AuthSession.Failure) -> Bool {
        switch (lhs, rhs) {
        case (.cancelled, .cancelled), (.malformedCallback, .malformedCallback):
            return true
        case (.failed(let a), .failed(let b)):
            return a == b
        default:
            return false
        }
    }
}
