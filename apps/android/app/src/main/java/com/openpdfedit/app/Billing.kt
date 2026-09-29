package com.openpdfedit.app

import android.app.Activity
import android.util.Log
import com.android.billingclient.api.AcknowledgePurchaseParams
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingFlowParams
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.ConsumeParams
import com.android.billingclient.api.ProductDetails
import com.android.billingclient.api.Purchase
import com.android.billingclient.api.PurchasesUpdatedListener
import com.android.billingclient.api.QueryProductDetailsParams
import com.android.billingclient.api.QueryPurchasesParams
import com.android.billingclient.api.acknowledgePurchase
import com.android.billingclient.api.consumePurchase
import com.android.billingclient.api.queryProductDetails
import com.android.billingclient.api.queryPurchasesAsync
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONArray
import org.json.JSONObject
import kotlin.coroutines.resume

/**
 * Play Billing, reduced to what the web app needs.
 *
 * The rule this file exists to enforce is the one
 * `apps/ios/OpenPdfEdit/Store.swift` enforces for StoreKit: **a purchase
 * is consumed only after the server has said the credits are in the
 * ledger.** Play keeps an unconsumed purchase and returns it from
 * `queryPurchasesAsync` on every launch, so one interrupted by a crash
 * or a dead network is retried next time. Consuming on the callback that
 * reports the purchase — which is what the obvious code does — throws
 * that away, and the customer is charged for credits nobody granted.
 *
 * Acknowledging is separate and not optional: Play refunds any purchase
 * left unacknowledged for three days. Consuming implies acknowledgement,
 * so the ordinary path needs no explicit call; this acknowledges only
 * when redemption is taking longer than that would allow.
 */
class Billing(private val activity: MainActivity) {

    /** Must match `app_iap_products.product_id` on the server and the
     *  products in the Play Console. Three places, one list each: the
     *  server decides what a product is worth, Play decides what it
     *  costs, and this only decides what to ask about. */
    private val productIds = listOf("credits_1000", "credits_5000")

    private var details: Map<String, ProductDetails> = emptyMap()

    /** A purchase flow in progress, waiting on `onPurchasesUpdated`. */
    private var pending: CompletableDeferred<PurchaseOutcome>? = null

    sealed interface PurchaseOutcome {
        data class Bought(val purchase: Purchase) : PurchaseOutcome
        data object Cancelled : PurchaseOutcome
        data object Pending : PurchaseOutcome
        data class Failed(val message: String) : PurchaseOutcome
    }

    private val listener = PurchasesUpdatedListener { result, purchases ->
        val waiting = pending
        pending = null
        val outcome = when {
            result.responseCode == BillingClient.BillingResponseCode.USER_CANCELED ->
                PurchaseOutcome.Cancelled
            result.responseCode != BillingClient.BillingResponseCode.OK ->
                PurchaseOutcome.Failed(describe(result))
            else -> {
                val p = purchases?.firstOrNull()
                when {
                    p == null -> PurchaseOutcome.Failed("Play reported success with no purchase")
                    // Ask to Buy, or a payment method that settles later.
                    // Not a failure and not credits yet.
                    p.purchaseState == Purchase.PurchaseState.PENDING -> PurchaseOutcome.Pending
                    else -> PurchaseOutcome.Bought(p)
                }
            }
        }
        if (waiting != null) {
            waiting.complete(outcome)
        } else if (outcome is PurchaseOutcome.Bought) {
            // Nobody asked for this one: an Ask to Buy approval, or a
            // purchase finished on another device. The page is told so it
            // can redeem it; if the page is not up yet, `outstanding()`
            // finds it at startup instead.
            activity.emitReceipt(describe(outcome.purchase))
        }
    }

    private val client: BillingClient = BillingClient.newBuilder(activity)
        .setListener(listener)
        .enablePendingPurchases(
            com.android.billingclient.api.PendingPurchasesParams.newBuilder()
                .enableOneTimeProducts()
                .build(),
        )
        .build()

    /** Connects if needed. Play disconnects freely — a dropped service,
     *  an app update — so every entry point goes through this rather
     *  than assuming a connection made at startup is still there. */
    private suspend fun connect(): Boolean {
        if (client.isReady) return true
        return suspendCancellableCoroutine { cont ->
            client.startConnection(object : com.android.billingclient.api.BillingClientStateListener {
                override fun onBillingSetupFinished(result: BillingResult) {
                    if (cont.isActive) cont.resume(result.responseCode == BillingClient.BillingResponseCode.OK)
                }
                override fun onBillingServiceDisconnected() {
                    if (cont.isActive) cont.resume(false)
                }
            })
        }
    }

    /** `[{id, name, description, price}]`, priced by Play in the
     *  customer's own currency. Never reformat `price`. */
    suspend fun products(): JSONArray {
        val out = JSONArray()
        if (!connect()) return out
        val params = QueryProductDetailsParams.newBuilder()
            .setProductList(productIds.map {
                QueryProductDetailsParams.Product.newBuilder()
                    .setProductId(it)
                    .setProductType(BillingClient.ProductType.INAPP)
                    .build()
            })
            .build()
        val result = client.queryProductDetails(params)
        val list = result.productDetailsList.orEmpty()
        details = list.associateBy { it.productId }
        for (d in list) {
            out.put(
                JSONObject()
                    .put("id", d.productId)
                    .put("name", d.name)
                    .put("description", d.description)
                    .put("price", d.oneTimePurchaseOfferDetails?.formattedPrice ?: ""),
            )
        }
        return out
    }

    suspend fun purchase(productId: String): JSONObject {
        if (!connect()) return JSONObject().put("status", "cancelled")
        // Details may be missing if `products()` was never called, or if
        // Play dropped them with the connection.
        val d = details[productId] ?: run { products(); details[productId] }
            ?: return JSONObject().put("status", "cancelled")

        val flow = BillingFlowParams.newBuilder()
            .setProductDetailsParamsList(
                listOf(
                    BillingFlowParams.ProductDetailsParams.newBuilder()
                        .setProductDetails(d)
                        .build(),
                ),
            )
            .build()

        val waiting = CompletableDeferred<PurchaseOutcome>()
        pending = waiting
        val launch = client.launchBillingFlow(activity, flow)
        if (launch.responseCode != BillingClient.BillingResponseCode.OK) {
            pending = null
            return JSONObject().put("status", "cancelled")
        }
        return when (val outcome = waiting.await()) {
            is PurchaseOutcome.Bought -> describe(outcome.purchase)
            PurchaseOutcome.Cancelled -> JSONObject().put("status", "cancelled")
            PurchaseOutcome.Pending -> JSONObject().put("status", "pending")
            is PurchaseOutcome.Failed -> throw IllegalStateException(outcome.message)
        }
    }

    /**
     * Purchases Play still considers owing, including from previous
     * launches.
     *
     * The recovery path for a purchase interrupted by a crash or a dead
     * network. It runs at startup rather than behind a "restore
     * purchases" button — someone whose payment went through should not
     * have to know that word.
     */
    suspend fun outstanding(): JSONArray {
        val out = JSONArray()
        if (!connect()) return out
        val params = QueryPurchasesParams.newBuilder()
            .setProductType(BillingClient.ProductType.INAPP)
            .build()
        val result = client.queryPurchasesAsync(params)
        for (p in result.purchasesList) {
            if (p.purchaseState != Purchase.PurchaseState.PURCHASED) continue
            // Unacknowledged after three days is auto-refunded by Play.
            // Anything still here has not been consumed, so the server
            // has not granted it yet.
            if (!p.isAcknowledged) {
                runCatching {
                    client.acknowledgePurchase(
                        AcknowledgePurchaseParams.newBuilder()
                            .setPurchaseToken(p.purchaseToken).build(),
                    )
                }
            }
            out.put(describe(p))
        }
        return out
    }

    /**
     * Marks a purchase done, **after** the server has granted its
     * credits.
     *
     * Calling this early is the expensive mistake: a consumed purchase
     * stops being returned by `queryPurchasesAsync`, so a redemption
     * that never reached the server becomes money taken for credits
     * nobody granted, with no record left to retry from.
     */
    suspend fun finish(purchaseToken: String): Boolean {
        if (!connect()) return false
        val result = client.consumePurchase(
            ConsumeParams.newBuilder().setPurchaseToken(purchaseToken).build(),
        )
        return result.billingResult.responseCode == BillingClient.BillingResponseCode.OK
    }

    /** The shape `native.ts` documents for a native purchase. */
    private fun describe(p: Purchase): JSONObject = JSONObject()
        .put("status", "purchased")
        // The token, not the order id: it is what the server verifies
        // against Play and what `finish` consumes, and an order id is
        // absent on a test purchase.
        .put("transactionId", p.purchaseToken)
        .put("productId", p.products.firstOrNull() ?: "")
        // Exactly what `/v1/payments/google/redeem` wants, unmodified.
        .put("receipt", p.originalJson)
        // Play signs with SHA1withRSA, which the server deliberately does
        // not treat as proof — it asks the Play Developer API instead. So
        // there is nothing verified locally to report.
        .put("verifiedLocally", false)

    private fun describe(r: BillingResult): String =
        "Play billing error ${r.responseCode}: ${r.debugMessage.ifBlank { "no detail" }}"

    fun close() {
        runCatching { client.endConnection() }.onFailure { Log.w("OpenPdfEdit", "billing close", it) }
    }
}
