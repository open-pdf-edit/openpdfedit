import Foundation
import StoreKit

/// StoreKit 2, reduced to what the web app needs.
///
/// The rule this file exists to enforce: **a transaction is finished only
/// after the server has said the credits are in the ledger.** StoreKit
/// keeps an unfinished transaction alive across launches and re-delivers
/// it, so a purchase interrupted by a crash, a dead network or a force-quit
/// is retried on the next start. Finishing eagerly — which is what the
/// obvious code does — throws that safety net away, and the customer is
/// charged for credits nobody ever granted.
@MainActor
final class Store: ObservableObject {
    /// Must match `app_iap_products.product_id` on the server, and the
    /// products in App Store Connect. There is no third place: the server
    /// decides what a product is worth, App Store Connect decides what it
    /// costs, and this list only decides what to ask about.
    static let productIdentifiers = ["credits_1000", "credits_5000"]

    @Published private(set) var products: [Product] = []

    /// Verified transactions the server has not yet confirmed, keyed by
    /// transaction id.
    private var awaitingServer: [String: StoreKit.Transaction] = [:]

    private var updates: Task<Void, Never>?
    /// Called when StoreKit delivers a transaction out of band — an
    /// interrupted purchase completing, an Ask to Buy approval, a purchase
    /// made on another device.
    var onUnprompted: ((PurchaseReceipt) -> Void)?

    init() {
        // Started before anything else, and never cancelled while the app
        // is alive. Apple's own guidance: a transaction can arrive at any
        // moment, including before `products` has loaded, and one that
        // arrives with no listener is delivered again only on the next
        // launch.
        // `Task {}` inside a @MainActor type inherits that isolation, so
        // `receive` is an ordinary main-actor call from here.
        updates = Task { [weak self] in
            for await result in StoreKit.Transaction.updates {
                self?.receive(result)
            }
        }
    }

    deinit { updates?.cancel() }

    func loadProducts() async throws {
        products = try await Product.products(for: Self.productIdentifiers)
            .sorted { $0.price < $1.price }
    }

    /// What the web app is told about a product.
    ///
    /// `displayPrice` is StoreKit's, already in the customer's currency and
    /// formatted the way their region writes money. Formatting a price
    /// ourselves is a reliable way to show ¥500 as $500.
    func describeProducts() -> [[String: Any]] {
        products.map { product in
            [
                "id": product.id,
                "name": product.displayName,
                "description": product.description,
                "price": product.displayPrice,
            ]
        }
    }

    enum PurchaseOutcome {
        case bought(PurchaseReceipt)
        /// The customer changed their mind. Not an error, and showing them
        /// one is the most common way apps get this wrong.
        case cancelled
        /// Ask to Buy, or a payment method that settles later. The credits
        /// arrive through `Transaction.updates`, possibly days later.
        case pending
    }

    struct PurchaseReceipt {
        let transactionId: String
        let productId: String
        /// The signed transaction, exactly as the server will verify it.
        let jws: String
        /// Whether StoreKit's own check passed on this device.
        ///
        /// Passed along rather than acted on. The server verifies Apple's
        /// signature itself and is the only authority that matters; a local
        /// failure here is worth reporting but is not grounds for throwing
        /// away a receipt the customer paid for — a device with a wrong
        /// clock fails locally and verifies perfectly on the server.
        let verifiedLocally: Bool
    }

    func purchase(_ productId: String) async throws -> PurchaseOutcome {
        // Load on demand if nobody has yet. A purchase tapped before the
        // catalogue arrived is a race, not a mistake, and answering it with
        // "the App Store has no product called credits_1000" would send
        // whoever reads that error looking in App Store Connect.
        if products.isEmpty {
            try await loadProducts()
        }
        guard let product = products.first(where: { $0.id == productId }) else {
            throw StoreError.unknownProduct(productId)
        }
        do {
            switch try await product.purchase() {
            case .success(let verification):
                return .bought(hold(verification))
            case .userCancelled:
                return .cancelled
            case .pending:
                return .pending
            @unknown default:
                return .cancelled
            }
        } catch {
            // StoreKit reports a cancellation two ways, and only one of
            // them is the `.userCancelled` result above: dismissing the
            // sheet at certain moments *throws* instead. Left to
            // propagate, that shows someone who changed their mind an
            // error message about a purchase that never happened.
            if Self.isCancellation(error) { return .cancelled }
            throw error
        }
    }

    /// Whether an error thrown by `purchase()` means "they changed their
    /// mind" rather than "something went wrong".
    ///
    /// Static and separate so it can be tested: neither shape can be
    /// produced on demand from a test session, and the mapping is the part
    /// that matters.
    nonisolated static func isCancellation(_ error: Error) -> Bool {
        if let storeKit = error as? StoreKitError, case .userCancelled = storeKit {
            return true
        }
        if let purchase = error as? Product.PurchaseError, case .productUnavailable = purchase {
            return false
        }
        let nsError = error as NSError
        return nsError.domain == SKErrorDomain
            && nsError.code == SKError.Code.paymentCancelled.rawValue
    }

    /// Everything StoreKit still considers owing, including from previous
    /// launches.
    ///
    /// This is the recovery path, and it runs at startup rather than on a
    /// button. A customer whose purchase was interrupted should not have to
    /// know the word "restore".
    func outstanding() async -> [PurchaseReceipt] {
        var receipts: [PurchaseReceipt] = []
        for await result in StoreKit.Transaction.unfinished {
            receipts.append(hold(result))
        }
        return receipts
    }

    /// Marks a transaction done, after the server has granted its credits.
    ///
    /// Unknown ids are ignored rather than refused: the web app may replay
    /// a confirmation for a transaction already finished on a previous
    /// launch, and that is a duplicate, not a fault.
    func finish(_ transactionId: String) async {
        guard let transaction = awaitingServer.removeValue(forKey: transactionId) else { return }
        await transaction.finish()
    }

    private func receive(_ result: VerificationResult<StoreKit.Transaction>) {
        onUnprompted?(hold(result))
    }

    private func hold(_ result: VerificationResult<StoreKit.Transaction>) -> PurchaseReceipt {
        let transaction: StoreKit.Transaction
        let verified: Bool
        switch result {
        case .verified(let value):
            transaction = value
            verified = true
        case .unverified(let value, _):
            transaction = value
            verified = false
        }
        let id = String(transaction.id)
        awaitingServer[id] = transaction
        return PurchaseReceipt(
            transactionId: id,
            productId: transaction.productID,
            jws: result.jwsRepresentation,
            verifiedLocally: verified
        )
    }
}

enum StoreError: LocalizedError {
    case unknownProduct(String)

    var errorDescription: String? {
        switch self {
        case .unknownProduct(let id):
            return "The App Store has no product called \(id)."
        }
    }
}
