package com.openpdfedit.app

import android.net.Uri
import android.util.Base64
import android.webkit.JavascriptInterface
import org.json.JSONObject

/**
 * The one channel between the bundled web app and this shell.
 *
 * Deliberately one channel and not several, matching
 * `apps/ios/OpenPdfEdit/WebBridge.swift`: every call is a JSON object
 * with an `action`, answered by a promise on the JavaScript side, so
 * adding a capability means adding a case here and a method in
 * `bridge.js` — not another interface, another injected global and
 * another thing to remember to register.
 *
 * Every method that reaches the WebView must hop to the main thread:
 * `@JavascriptInterface` methods run on a private binder thread, and
 * touching a WebView off the main thread is undefined behaviour rather
 * than an error you will see.
 */
class WebBridge(private val shell: MainActivity) {

    @JavascriptInterface
    fun post(message: String) {
        val body = runCatching { JSONObject(message) }.getOrNull() ?: return
        val id = body.optInt("id", -1)
        if (id < 0) return
        when (body.optString("action")) {
            "ready" -> shell.onMain {
                shell.markReady()
                shell.resolve(id, JSONObject().put("platform", "android"))
            }

            // The page has the bytes; the staged document can go.
            "documentRead" -> shell.onMain {
                shell.releaseIncoming(body.optString("path"))
                shell.resolve(id, JSONObject().put("released", true))
            }

            // The way anything leaves this app. See `bridge.js` for why
            // an `<a download>` is not an option in a WebView.
            "saveFile" -> {
                val name = body.optString("name").ifBlank { "document.pdf" }
                val mime = body.optString("mime").ifBlank { "application/octet-stream" }
                val bytes = runCatching {
                    Base64.decode(body.optString("data"), Base64.DEFAULT)
                }.getOrNull()
                if (bytes == null) {
                    shell.onMain { shell.reject(id, "saveFile needs base64 data") }
                    return
                }
                shell.onMain { shell.startSave(id, name, mime, bytes) }
            }

            // --- in-app purchase ---------------------------------
            "products" -> shell.onMain { shell.billingProducts(id) }

            "purchase" -> {
                val productId = body.optString("productId")
                if (productId.isBlank()) {
                    shell.onMain { shell.reject(id, "purchase needs a productId") }
                    return
                }
                shell.onMain { shell.billingPurchase(id, productId) }
            }

            "outstanding" -> shell.onMain { shell.billingOutstanding(id) }

            "finish" -> {
                val transactionId = body.optString("transactionId")
                if (transactionId.isBlank()) {
                    shell.onMain { shell.reject(id, "finish needs a transactionId") }
                    return
                }
                shell.onMain { shell.billingFinish(id, transactionId) }
            }

            "signIn" -> shell.onMain { shell.startSignIn(id) }

            else -> shell.onMain { shell.reject(id, "unknown action") }
        }
    }

    companion object {
        /** The global the injected `bridge.js` looks for. */
        const val NAME = "OpenPdfEditAndroid"

        /** `content://` URIs and filenames both reach JavaScript, so
         *  both are quoted as JSON rather than interpolated — a
         *  filename with a quote in it would otherwise be a script
         *  injection into this app's own page. */
        fun quote(value: String): String = JSONObject.quote(value)

        fun displayName(uri: Uri): String =
            uri.lastPathSegment?.substringAfterLast('/')?.takeIf { it.isNotBlank() }
                ?: "document.pdf"
    }
}
