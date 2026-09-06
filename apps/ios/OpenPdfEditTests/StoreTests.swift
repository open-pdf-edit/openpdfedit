import StoreKit
import StoreKitTest
import XCTest

@testable import OpenPdfEdit

/// The purchase rules, against a real StoreKit running the local product
/// configuration.
///
/// `SKTestSession` is StoreKit itself with a test backend, not a stand-in
/// for it — the transactions here are signed, `Transaction.unfinished`
/// behaves as it does in production, and the JWS these produce is the same
/// shape the server verifies. That matters, because every rule worth
/// testing in `Store` is a rule about StoreKit's own bookkeeping.
final class StoreTests: XCTestCase {
    private var session: SKTestSession!

    override func setUpWithError() throws {
        // By URL, not by name: `configurationFileNamed:` looks in
        // `Bundle.main`, which for a hosted unit test is the *app* — and
        // the configuration ships with the tests. Named lookup finds
        // nothing, throws nothing, and leaves an empty catalogue.
        let url = try XCTUnwrap(
            Bundle(for: Self.self).url(forResource: "Products", withExtension: "storekit"),
            "Products.storekit is not in the test bundle"
        )
        session = try SKTestSession(contentsOf: url)
        session.resetToDefaultState()
        session.clearTransactions()
        // Otherwise every purchase waits for a confirmation sheet nobody
        // is there to tap.
        session.disableDialogs = true
    }

    override func tearDown() {
        session = nil
    }

    @MainActor
    func testTheConfiguredProductsAreTheOnesTheServerKnows() async throws {
        let store = Store()
        try await store.loadProducts()
        try requireLocalStoreKit(store.products)

        XCTAssertEqual(
            store.products.map(\.id).sorted(),
            Store.productIdentifiers.sorted(),
            "a product id here that the App Store does not have is a pack nobody can buy"
        )
        // Cheapest first, so the list does not depend on the order Apple
        // happens to answer in.
        XCTAssertEqual(store.products.map(\.id), ["credits_1000", "credits_5000"])
        for product in store.products {
            XCTAssertFalse(product.displayPrice.isEmpty)
        }
    }

    @MainActor
    func testAPurchaseIsNotFinishedUntilItIsTold() async throws {
        let store = Store()
        try await store.loadProducts()
        try requireLocalStoreKit(store.products)

        guard case .bought(let receipt) = try await store.purchase("credits_1000") else {
            return XCTFail("the purchase did not complete")
        }
        XCTAssertEqual(receipt.productId, "credits_1000")
        XCTAssertTrue(receipt.verifiedLocally)

        // The rule this whole class exists for. Until the server has
        // granted the credits, StoreKit must still consider this owing —
        // that is what makes the purchase survive a crash between paying
        // and being credited.
        var outstanding = await store.outstanding()
        XCTAssertTrue(
            outstanding.contains { $0.transactionId == receipt.transactionId },
            "the transaction was finished before the server confirmed it"
        )

        await store.finish(receipt.transactionId)

        outstanding = await store.outstanding()
        XCTAssertFalse(
            outstanding.contains { $0.transactionId == receipt.transactionId },
            "finishing did not clear the transaction"
        )
    }

    @MainActor
    func testTheReceiptIsTheJwsTheServerExpects() async throws {
        let store = Store()
        try await store.loadProducts()
        try requireLocalStoreKit(store.products)
        guard case .bought(let receipt) = try await store.purchase("credits_5000") else {
            return XCTFail("the purchase did not complete")
        }
        defer { Task { await store.finish(receipt.transactionId) } }

        // The server splits on ".", checks `alg`, and reads the x5c chain
        // out of the header. If any of that stops being true the app and
        // the server disagree about what a receipt is, and every purchase
        // fails at redemption — a long way from here.
        let parts = receipt.jws.split(separator: ".")
        XCTAssertEqual(parts.count, 3, "a JWS has three dot-separated parts")

        let header = try XCTUnwrap(Self.decodeBase64URL(String(parts[0])))
        let fields = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: header) as? [String: Any]
        )
        XCTAssertEqual(fields["alg"] as? String, "ES256")
        XCTAssertFalse((fields["x5c"] as? [String] ?? []).isEmpty, "no certificate chain")

        let payload = try XCTUnwrap(Self.decodeBase64URL(String(parts[1])))
        let transaction = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: payload) as? [String: Any]
        )
        XCTAssertEqual(transaction["productId"] as? String, "credits_5000")
        // The server refuses anything that is not a Consumable, because
        // credits are spent. A product mistyped in App Store Connect fails
        // there; this catches it here.
        XCTAssertEqual(transaction["type"] as? String, "Consumable")
        XCTAssertNotNil(transaction["bundleId"] as? String)
    }

    @MainActor
    func testAskToBuyIsPendingRatherThanBoughtOrFailed() async throws {
        session.askToBuyEnabled = true
        let store = Store()
        try await store.loadProducts()
        try requireLocalStoreKit(store.products)

        guard case .pending = try await store.purchase("credits_1000") else {
            return XCTFail("Ask to Buy should be pending, not bought and not an error")
        }
    }

    @MainActor
    func testBuyingSomethingTheStoreDoesNotSellIsRefusedLocally() async throws {
        let store = Store()
        try await store.loadProducts()
        try requireLocalStoreKit(store.products)

        do {
            _ = try await store.purchase("credits_999999")
            XCTFail("an unknown product should not be purchasable")
        } catch StoreError.unknownProduct {
            // As intended.
        }
    }

    @MainActor
    func testFinishingSomethingUnknownIsNotAnError() async {
        // The web app replays confirmations. A transaction finished on a
        // previous launch is a duplicate, not a fault.
        await Store().finish("no-such-transaction")
    }

    private static func decodeBase64URL(_ value: String) -> Data? {
        var text = value.replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        while text.count % 4 != 0 { text += "=" }
        return Data(base64Encoded: text)
    }
}

/// The cancellation mapping, which no test session can produce on demand.
///
/// Both shapes are real: StoreKit returns `.userCancelled` as a purchase
/// result in the ordinary case, and throws in others. Getting the second
/// wrong shows someone who tapped Cancel an error about a purchase that
/// never happened.
final class CancellationTests: XCTestCase {
    func testStoreKitsThrownCancellationIsACancellation() {
        XCTAssertTrue(Store.isCancellation(StoreKitError.userCancelled))
    }

    func testTheLegacyStoreKitErrorIsAlsoACancellation() {
        let error = NSError(
            domain: SKErrorDomain,
            code: SKError.Code.paymentCancelled.rawValue
        )
        XCTAssertTrue(Store.isCancellation(error))
    }

    func testARealFailureIsNotACancellation() {
        XCTAssertFalse(Store.isCancellation(StoreKitError.networkError(URLError(.timedOut))))
        XCTAssertFalse(
            Store.isCancellation(NSError(domain: SKErrorDomain, code: SKError.Code.unknown.rawValue))
        )
        XCTAssertFalse(Store.isCancellation(StoreError.unknownProduct("x")))
    }
}
