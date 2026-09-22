import Foundation
import StoreKit

// StoreKit 2 for the Mac App Store build, reduced to what the web app
// needs and handed to Rust through a C ABI.
//
// This is apps/ios/OpenPdfEdit/Store.swift's logic on another platform,
// and it answers the page in exactly the shapes the iOS shell does, so the
// purchase panel and the server's redemption path are the same code on
// both. The rule both files exist to enforce is unchanged: **a transaction
// is finished only after the server has said the credits are in the
// ledger.** StoreKit re-delivers an unfinished transaction on every launch,
// which is what makes a purchase survive a crash or a dead network between
// payment and redemption. Finishing early throws that away.
//
// Every entry point but `oa_sk_start` blocks its caller until StoreKit
// answers, so the Rust side calls them off the main thread — which is also
// what leaves the main thread free to show the App Store's purchase sheet.

/// Must match `app_iap_products.product_id` on the server and the products
/// in App Store Connect — the same list the iOS app asks for.
private let productIdentifiers = ["credits_1000", "credits_5000"]

/// Verified transactions the server has not yet confirmed, and the loaded
/// catalogue. An actor rather than @MainActor as on iOS: nothing here
/// touches UI, and the callers are Rust worker threads.
private actor StoreModel {
    var products: [Product] = []
    var awaitingServer: [String: StoreKit.Transaction] = [:]

    func loadProducts() async throws -> [Product] {
        if products.isEmpty {
            products = try await Product.products(for: productIdentifiers).sorted { $0.price < $1.price }
        }
        return products
    }

    func hold(_ result: VerificationResult<StoreKit.Transaction>) -> [String: Any] {
        let transaction: StoreKit.Transaction
        let verified: Bool
        switch result {
        case .verified(let value): transaction = value; verified = true
        case .unverified(let value, _): transaction = value; verified = false
        }
        let id = String(transaction.id)
        awaitingServer[id] = transaction
        return [
            "status": "purchased",
            "transactionId": id,
            "productId": transaction.productID,
            // The signed transaction, exactly as the server verifies it.
            "receipt": result.jwsRepresentation,
            // Reported, not acted on: a Mac with a wrong clock fails this
            // locally and verifies perfectly on the server.
            "verifiedLocally": verified,
        ]
    }

    func take(_ transactionId: String) -> StoreKit.Transaction? {
        awaitingServer.removeValue(forKey: transactionId)
    }
}

private let model = StoreModel()
private var updatesTask: Task<Void, Never>?

// MARK: - helpers

private func json(_ value: Any) -> UnsafeMutablePointer<CChar> {
    let data = (try? JSONSerialization.data(withJSONObject: value)) ?? Data("null".utf8)
    return strdup(String(decoding: data, as: UTF8.self))!
}

private func failure(_ error: Error) -> UnsafeMutablePointer<CChar> {
    json(["error": error.localizedDescription])
}

/// Runs async StoreKit work to completion on the calling (non-main) thread.
private func blocking(_ work: @escaping @Sendable () async -> UnsafeMutablePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let semaphore = DispatchSemaphore(value: 0)
    let box = ResultBox()
    Task.detached {
        box.value = await work()
        semaphore.signal()
    }
    semaphore.wait()
    return box.value!
}

private final class ResultBox: @unchecked Sendable {
    var value: UnsafeMutablePointer<CChar>?
}

/// Whether a thrown purchase error means "they changed their mind".
///
/// StoreKit reports a cancellation two ways, and only one of them is the
/// `.userCancelled` result: dismissing the sheet at certain moments throws
/// instead. Shown as an error, that tells someone who simply closed the
/// sheet that a purchase which never happened went wrong.
private func isCancellation(_ error: Error) -> Bool {
    if let storeKit = error as? StoreKitError, case .userCancelled = storeKit { return true }
    let nsError = error as NSError
    return nsError.domain == SKErrorDomain && nsError.code == SKError.Code.paymentCancelled.rawValue
}

// MARK: - the C ABI

/// Starts listening for transactions that arrive on their own — an
/// interrupted purchase completing, an Ask to Buy approval, a purchase made
/// on another device. Called once, first: a transaction that arrives with
/// no listener is delivered again only on the next launch.
@_cdecl("oa_sk_start")
public func oa_sk_start(_ onReceipt: @escaping @convention(c) (UnsafePointer<CChar>) -> Void) {
    guard updatesTask == nil else { return }
    updatesTask = Task.detached {
        for await result in StoreKit.Transaction.updates {
            let receipt = await model.hold(result)
            let pointer = json(receipt)
            onReceipt(pointer)
            free(pointer)
        }
    }
}

/// `[{id, name, description, price}]`, the price already in the customer's
/// currency and formatted the way their region writes money.
@_cdecl("oa_sk_products")
public func oa_sk_products() -> UnsafeMutablePointer<CChar> {
    blocking {
        do {
            let products = try await model.loadProducts()
            return json(products.map { [
                "id": $0.id, "name": $0.displayName,
                "description": $0.description, "price": $0.displayPrice,
            ] })
        } catch {
            return failure(error)
        }
    }
}

/// A purchase: `{status: "purchased", …}`, `{status: "cancelled"}` or
/// `{status: "pending"}` — the iOS shell's shapes exactly.
@_cdecl("oa_sk_purchase")
public func oa_sk_purchase(_ productIdPointer: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let productId = String(cString: productIdPointer)
    return blocking {
        do {
            // Load on demand: a purchase tapped before the catalogue arrived
            // is a race, not an unknown product.
            guard let product = try await model.loadProducts().first(where: { $0.id == productId }) else {
                return json(["error": "The App Store has no product called \(productId)."])
            }
            switch try await product.purchase() {
            case .success(let verification): return json(await model.hold(verification))
            case .userCancelled: return json(["status": "cancelled"])
            case .pending: return json(["status": "pending"])
            @unknown default: return json(["status": "cancelled"])
            }
        } catch {
            return isCancellation(error) ? json(["status": "cancelled"]) : failure(error)
        }
    }
}

/// Everything StoreKit still considers owing, including from earlier
/// launches. Run at startup, so nobody has to know the word "restore".
@_cdecl("oa_sk_outstanding")
public func oa_sk_outstanding() -> UnsafeMutablePointer<CChar> {
    blocking {
        var receipts: [[String: Any]] = []
        for await result in StoreKit.Transaction.unfinished {
            receipts.append(await model.hold(result))
        }
        return json(receipts)
    }
}

/// Marks a transaction done, after the server has granted its credits.
/// An unknown id is a replayed confirmation, not a fault.
@_cdecl("oa_sk_finish")
public func oa_sk_finish(_ transactionIdPointer: UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar> {
    let transactionId = String(cString: transactionIdPointer)
    return blocking {
        guard let transaction = await model.take(transactionId) else { return json(["finished": false]) }
        await transaction.finish()
        return json(["finished": true])
    }
}

/// Frees a string this library returned.
@_cdecl("oa_sk_free")
public func oa_sk_free(_ pointer: UnsafeMutablePointer<CChar>) {
    free(pointer)
}
