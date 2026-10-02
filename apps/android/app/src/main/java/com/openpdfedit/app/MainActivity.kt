package com.openpdfedit.app

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import android.util.Log
import android.view.ViewGroup
import android.webkit.ConsoleMessage
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.browser.customtabs.CustomTabsIntent
import androidx.core.content.IntentCompat
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import androidx.webkit.WebViewAssetLoader
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import org.json.JSONObject
import java.io.ByteArrayInputStream

/**
 * The shell. Everything the app does is the web build running on device;
 * this class exists to host it, to serve it from an origin the page can
 * do cryptography on, to carry documents in and out, and to keep it from
 * navigating anywhere it shouldn't.
 *
 * Mirrors `apps/ios/OpenPdfEdit/AppWebView.swift` and its `WebBridge`
 * deliberately, because the web layer cannot tell the two apart and
 * should not have to.
 */
class MainActivity : AppCompatActivity() {

    private lateinit var webView: WebView
    private val bridge = WebBridge(this)
    private val billing by lazy { Billing(this) }

    /** True once the page has said it can accept a document. Until then
     *  a document that arrives is held rather than dropped — the OS
     *  delivers the intent long before the editor exists. */
    private var isReady = false
    private var queued: Uri? = null

    /** Documents handed to us by other apps, served at `/__incoming/<n>`
     *  so the page can `fetch` them as a stream rather than receive
     *  multi-megabyte base64. */
    private val incoming = mutableMapOf<String, Uri>()
    private var incomingSeq = 0

    /** A `signIn` waiting on the Custom Tab to come back, and whether
     *  we are still waiting for it to. Backing out of a Custom Tab
     *  produces no callback at all, so the return to this activity with
     *  nothing delivered is the only signal that it was cancelled. */
    private var pendingSignIn: Int? = null
    private var awaitingSignInReturn = false

    /** A `saveFile` waiting on the customer to choose a destination. */
    private var pendingSave: PendingSave? = null

    private class PendingSave(val callId: Int, val bytes: ByteArray)

    /** The `<input type="file">` awaiting a result, or null. */
    private var pendingFileCallback: ValueCallback<Array<Uri>>? = null

    private val pickFile = registerForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        pendingFileCallback?.onReceiveValue(if (uri == null) null else arrayOf(uri))
        pendingFileCallback = null
    }

    /**
     * The Storage Access Framework's create-document picker.
     *
     * `StartActivityForResult` rather than the `CreateDocument` contract
     * because that contract fixes the MIME type when it is registered,
     * and this one call site saves PDFs, Markdown and text.
     */
    private val createDocument = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val save = pendingSave
        pendingSave = null
        if (save == null) return@registerForActivityResult
        val uri = result.data?.data
        if (result.resultCode != Activity.RESULT_OK || uri == null) {
            // Cancelling is not an error. An app that reports "save
            // failed" because someone dismissed the picker is telling
            // them something went wrong when nothing did.
            resolve(save.callId, JSONObject().put("saved", false))
            return@registerForActivityResult
        }
        val written = runCatching {
            contentResolver.openOutputStream(uri)?.use { it.write(save.bytes) } ?: error("no stream")
        }
        if (written.isSuccess) {
            resolve(save.callId, JSONObject().put("saved", true))
        } else {
            reject(save.callId, "could not write the file: ${written.exceptionOrNull()?.message}")
        }
    }

    /**
     * Serves `app/src/main/assets/www` at
     * `https://appassets.androidplatform.net/`, and staged incoming
     * documents at `/__incoming/<n>` beside it.
     *
     * An https origin rather than a custom scheme, for the same reason
     * iOS uses `openpdfedit://localhost`: the page must be a **secure
     * context** or `crypto.subtle` is undefined and the account SDK
     * cannot start. `androidplatform.net` is reserved by Google for
     * exactly this and never resolves on the network.
     *
     * This origin is also what the accounts server must allow in CORS —
     * the Android counterpart to `openpdfedit://localhost`.
     */
    private val assetLoader: WebViewAssetLoader by lazy {
        val assets = WebViewAssetLoader.AssetsPathHandler(this)
        WebViewAssetLoader.Builder()
            .addPathHandler("/__incoming/") { path -> serveIncoming(path) }
            .addPathHandler("/") { path -> assets.handle("www/$path") }
            .build()
    }

    @SuppressLint("SetJavaScriptEnabled")
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        webView = WebView(this).apply {
            setBackgroundColor(Color.parseColor("#111111"))
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            settings.allowFileAccess = false
            settings.allowContentAccess = false
            settings.mediaPlaybackRequiresUserGesture = true
            overScrollMode = WebView.OVER_SCROLL_NEVER
            addJavascriptInterface(bridge, WebBridge.NAME)
            webViewClient = ShellWebViewClient()
            webChromeClient = ShellChromeClient()
        }
        // The inspector, in debug builds only — the same trade the
        // desktop makes with its `devtools` Cargo feature, and left out
        // of release for the same reason: a reviewer finding a web
        // inspector on a native app is a question nobody needs to
        // answer. It is also what makes the bridge testable from adb.
        if (0 != (applicationInfo.flags and android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE)) {
            WebView.setWebContentsDebuggingEnabled(true)
        }
        setContentView(webView)
        applyWindowInsets()
        injectBridge()

        if (savedInstanceState == null) {
            webView.loadUrl("$ORIGIN/index.html")
        }
        stage(intent)

        onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
            override fun handleOnBackPressed() {
                if (webView.canGoBack()) {
                    webView.goBack()
                    return
                }
                // Ask the page before closing. Only it knows whether a
                // panel is open or a document has unsaved edits, and
                // `finish()` is the one answer that throws the work
                // away — on a phone with gesture navigation, a swipe
                // from the edge used to discard every unsaved change
                // with no prompt at all.
                webView.evaluateJavascript(
                    "window.OpenPdfEditNative && window.OpenPdfEditNative._hasBack()" +
                        " ? (window.OpenPdfEditNative._back(), 'handled') : 'none'",
                ) { answer ->
                    // A bundle too old to know about back, or a page
                    // that failed to load, answers 'none' or null. Back
                    // then means what it always did, because a back
                    // button that does nothing is worse than one that
                    // is too eager.
                    if (answer == null || !answer.contains("handled")) finish()
                }
            }
        })
    }

    /**
     * Puts `bridge.js` in before the page's own scripts.
     *
     * `addDocumentStartJavaScript` is the exact counterpart of
     * WKWebView's `.atDocumentStart` injection, but it is a WebView
     * *feature* rather than an API level — an old System WebView simply
     * does not have it. The fallback runs the same script from
     * `onPageStarted`, which is marginally later but still before the
     * app's own bundle has executed.
     */
    private fun injectBridge() {
        val source = runCatching {
            assets.open("bridge.js").bufferedReader().use { it.readText() }
        }.getOrElse {
            Log.e(TAG, "bridge.js missing from assets — the shell has no bridge", it)
            return
        }
        bridgeSource = source
        if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            WebViewCompat.addDocumentStartJavaScript(webView, source, setOf(ORIGIN))
        }
    }

    private var bridgeSource: String? = null

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.action == Intent.ACTION_VIEW &&
            intent.data?.scheme == "openpdfedit-auth"
        ) {
            // Cleared before onResume runs, so the cancellation check
            // below does not race this and report a success as a cancel.
            awaitingSignInReturn = false
            intent.data?.let { completeSignIn(it) }
            return
        }
        stage(intent)
    }

    // --- documents in ----------------------------------------------------

    /** Takes a document from a VIEW or SEND intent and offers it to the
     *  page, holding it if the page is not up yet. */
    private fun stage(intent: Intent?) {
        val uri = when (intent?.action) {
            Intent.ACTION_VIEW -> intent.data
            Intent.ACTION_SEND -> IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java)
            else -> null
        } ?: return
        if (!isReady) {
            queued = uri
            return
        }
        deliver(uri)
    }

    fun flushQueued() {
        val uri = queued ?: return
        queued = null
        deliver(uri)
    }

    private fun deliver(uri: Uri) {
        val id = (++incomingSeq).toString()
        incoming[id] = uri
        val name = queryDisplayName(uri) ?: WebBridge.displayName(uri)
        emit("document", JSONObject().put("path", "$ORIGIN/__incoming/$id").put("name", name))
    }

    /** The name the other app gave the file, which is what the customer
     *  recognises — `content://` URIs carry no usable filename. */
    private fun queryDisplayName(uri: Uri): String? = runCatching {
        contentResolver.query(uri, null, null, null, null)?.use { c ->
            val i = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (i >= 0 && c.moveToFirst()) c.getString(i) else null
        }
    }.getOrNull()

    private fun serveIncoming(path: String): WebResourceResponse? {
        val uri = incoming[path.trim('/')] ?: return null
        val bytes = runCatching {
            contentResolver.openInputStream(uri)?.use { it.readBytes() }
        }.getOrNull() ?: return null
        return WebResourceResponse("application/pdf", null, ByteArrayInputStream(bytes))
    }

    fun releaseIncoming(path: String) {
        incoming.remove(path.substringAfterLast('/'))
    }

    // --- bridge plumbing -------------------------------------------------

    fun onMain(block: () -> Unit) = runOnUiThread(block)

    fun markReady() {
        isReady = true
        flushQueued()
    }

    fun startSave(callId: Int, name: String, mime: String, bytes: ByteArray) {
        pendingSave = PendingSave(callId, bytes)
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = mime
            putExtra(Intent.EXTRA_TITLE, name)
        }
        runCatching { createDocument.launch(intent) }.onFailure {
            pendingSave = null
            reject(callId, "no app can save files on this device")
        }
    }

    /**
     * Signs in through a Custom Tab.
     *
     * The Android answer to iOS's `ASWebAuthenticationSession`, and for
     * the same reasons: a real address bar, so someone typing a Google
     * password can see whose page they are typing it into, and the
     * browser's own cookies, so an account already signed in on the
     * device is usually one tap. Loading the login page into this
     * WebView instead would replace the editor with an OAuth screen and
     * lose whatever document is open behind it.
     */
    fun startSignIn(callId: Int) {
        pendingSignIn?.let { resolve(it, JSONObject().put("status", "cancelled")) }
        pendingSignIn = callId
        awaitingSignInReturn = true
        val url = Uri.parse(LOGIN_URL).buildUpon().appendQueryParameter("native", "android").build()
        runCatching { CustomTabsIntent.Builder().build().launchUrl(this, url) }
            .onFailure {
                // No browser at all, which is rare but not impossible on
                // a stripped device. Falling back to a plain VIEW intent
                // still reaches whatever can open a link.
                runCatching { startActivity(Intent(Intent.ACTION_VIEW, url)) }
                    .onFailure { _ ->
                        pendingSignIn = null
                        awaitingSignInReturn = false
                        reject(callId, "no browser is available to sign in with")
                    }
            }
    }

    /** The tokens come back in the fragment, which never reaches a
     *  server, so they cannot appear in an access log or a referrer on
     *  the way past. */
    private fun completeSignIn(uri: Uri) {
        val callId = pendingSignIn ?: return
        pendingSignIn = null
        awaitingSignInReturn = false
        val values = (uri.fragment ?: "").split('&').mapNotNull {
            val i = it.indexOf('=')
            if (i <= 0) null else Uri.decode(it.substring(0, i)) to Uri.decode(it.substring(i + 1))
        }.toMap()
        val access = values["access_token"].orEmpty()
        val refresh = values["refresh_token"].orEmpty()
        if (access.isBlank() || refresh.isBlank()) {
            reject(callId, "the sign-in page came back without a session")
            return
        }
        resolve(
            callId,
            JSONObject()
                .put("status", "signed_in")
                .put("accessToken", access)
                .put("refreshToken", refresh),
        )
    }

    override fun onResume() {
        super.onResume()
        // Back here with no callback delivered: they dismissed the tab.
        // Not an error — an app that says "sign-in failed" because
        // someone changed their mind is reporting a problem that did not
        // happen.
        if (awaitingSignInReturn) {
            awaitingSignInReturn = false
            pendingSignIn?.let { resolve(it, JSONObject().put("status", "cancelled")) }
            pendingSignIn = null
        }
    }

    // --- in-app purchase -------------------------------------------------
    //
    // Each of these answers the promise the page is holding, including
    // when the work throws: a bridge call that never resolves leaves the
    // purchase panel spinning for the rest of the session, which is
    // worse than an error the customer can read.

    private fun billingCall(callId: Int, block: suspend () -> Any) {
        lifecycleScope.launch {
            runCatching { block() }
                .onSuccess { value ->
                    when (value) {
                        is JSONObject -> resolve(callId, value)
                        else -> answerRaw(callId, true, value.toString())
                    }
                }
                .onFailure { reject(callId, it.message ?: "in-app purchase failed") }
        }
    }

    fun billingProducts(callId: Int) = billingCall(callId) {
        JSONObject().put("products", billing.products())
    }

    fun billingPurchase(callId: Int, productId: String) = billingCall(callId) {
        billing.purchase(productId)
    }

    fun billingOutstanding(callId: Int) = billingCall(callId) {
        JSONObject().put("receipts", billing.outstanding())
    }

    fun billingFinish(callId: Int, transactionId: String) = billingCall(callId) {
        JSONObject().put("finished", billing.finish(transactionId))
    }

    /** A purchase Play delivered on its own — an Ask to Buy approval, a
     *  purchase made on another device, an interrupted one completing. */
    fun emitReceipt(detail: JSONObject) = onMain { emit("receipt", detail) }

    fun resolve(callId: Int, payload: JSONObject) = answer(callId, true, payload.toString())

    fun reject(callId: Int, message: String) = answer(callId, false, WebBridge.quote(message))

    private fun answerRaw(callId: Int, ok: Boolean, payloadJson: String) = answer(callId, ok, payloadJson)

    private fun answer(callId: Int, ok: Boolean, payloadJson: String) {
        val js = "window.OpenPdfEditNative && window.OpenPdfEditNative._resolve($callId, $ok, $payloadJson);"
        webView.evaluateJavascript(js, null)
    }

    private fun emit(event: String, detail: JSONObject) {
        // `JSON.parse` of a quoted string rather than an interpolated
        // object literal: the values include filenames chosen by another
        // app, and a filename with a quote in it would otherwise be a
        // script injection into this app's own page.
        val js = "window.OpenPdfEditNative && window.OpenPdfEditNative._emit(" +
            "${WebBridge.quote(event)}, JSON.parse(${WebBridge.quote(detail.toString())}));"
        webView.evaluateJavascript(js, null)
    }

    /**
     * Keeps the page clear of the status bar, the gesture bar and the
     * keyboard.
     *
     * From Android 15 an app targeting SDK 35 or above is drawn edge to
     * edge whether it asks to be or not, and nothing warns you: the
     * toolbar simply moves up behind the status bar, where the system
     * also takes the top ~95px of touches, so the upper half of every
     * button stops responding. Raising `targetSdk` to 36 — which Play
     * requires — is what turned this on here.
     *
     * Padding the WebView rather than asking the page to use
     * `safe-area-inset-*`: the same bundle runs in a browser and in two
     * other shells, and the page has no business knowing which one it
     * is in. Measured insets are also the only ones that are right —
     * the status bar is not a constant, and on this device it is 95px
     * rather than the 24dp the documentation implies.
     *
     * `ime` is in the same mask deliberately. The keyboard is just
     * another inset once the window is edge to edge, and
     * `adjustResize` alone no longer moves anything, which is why the
     * Cancel button on a note dialog sat behind the keyboard.
     */
    private fun applyWindowInsets() {
        androidx.core.view.ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
            val bars = insets.getInsets(
                androidx.core.view.WindowInsetsCompat.Type.systemBars() or
                    androidx.core.view.WindowInsetsCompat.Type.displayCutout() or
                    androidx.core.view.WindowInsetsCompat.Type.ime(),
            )
            // Margins, not padding. A WebView treats padding as part of
            // its scrollable area rather than as a smaller viewport: the
            // page still laid out 854dp tall and simply drew its first
            // 136px of toolbar behind the status bar, which is the bug
            // this is here to fix. Shrinking the view is what gives the
            // page a viewport that ends where the system bars begin —
            // and it is also what makes the keyboard inset work, since
            // the dialog centres itself in whatever height it is given.
            val lp = view.layoutParams as ViewGroup.MarginLayoutParams
            if (lp.leftMargin != bars.left || lp.topMargin != bars.top ||
                lp.rightMargin != bars.right || lp.bottomMargin != bars.bottom
            ) {
                lp.setMargins(bars.left, bars.top, bars.right, bars.bottom)
                view.layoutParams = lp
            }
            insets
        }
        applySystemBarAppearance()
    }

    /**
     * Keeps the system bars legible against whatever the page is
     * showing behind them.
     *
     * Two halves of one answer: the strip behind each bar is the
     * window background, and the icons drawn on it are the system's.
     * Get one without the other and the clock vanishes — a dark icon
     * set on a black strip is what the first attempt at this produced.
     *
     * It is re-applied on a configuration change because `uiMode` is in
     * this activity's `configChanges`: the activity is deliberately not
     * recreated when the system switches to dark, so that the open
     * document and every unsaved edit survive it, which also means
     * nothing re-reads the theme unless we do.
     */
    private fun applySystemBarAppearance() {
        val night = resources.configuration.uiMode and
            android.content.res.Configuration.UI_MODE_NIGHT_MASK ==
            android.content.res.Configuration.UI_MODE_NIGHT_YES
        window.setBackgroundDrawableResource(R.color.chrome_surface)
        val controller = androidx.core.view.WindowCompat
            .getInsetsController(window, window.decorView)
        controller.isAppearanceLightStatusBars = !night
        controller.isAppearanceLightNavigationBars = !night
    }

    override fun onConfigurationChanged(newConfig: android.content.res.Configuration) {
        super.onConfigurationChanged(newConfig)
        applySystemBarAppearance()
    }

    /** `AcceptTypes.kt`, with Android's own extension table behind it. */
    private fun mimeTypesFor(acceptTypes: Array<String>): Array<String> {
        val known = android.webkit.MimeTypeMap.getSingleton()
        return mimeTypesFor(acceptTypes) { known.getMimeTypeFromExtension(it) }
    }

    // --- webview clients -------------------------------------------------

    private inner class ShellChromeClient : WebChromeClient() {

        override fun onShowFileChooser(
            webView: WebView,
            filePathCallback: ValueCallback<Array<Uri>>,
            fileChooserParams: FileChooserParams,
        ): Boolean {
            pendingFileCallback?.onReceiveValue(null)
            pendingFileCallback = filePathCallback
            val mimeTypes = mimeTypesFor(fileChooserParams.acceptTypes)
            return runCatching { pickFile.launch(mimeTypes) }.isSuccess
        }

        override fun onConsoleMessage(message: ConsoleMessage): Boolean {
            Log.i(TAG, "console ${message.messageLevel()}: ${message.message()} " +
                "(${message.sourceId()}:${message.lineNumber()})")
            return true
        }
    }

    private inner class ShellWebViewClient : WebViewClient() {

        override fun onPageStarted(view: WebView, url: String?, favicon: android.graphics.Bitmap?) {
            super.onPageStarted(view, url, favicon)
            if (!WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
                bridgeSource?.let { view.evaluateJavascript(it, null) }
            }
        }

        override fun shouldInterceptRequest(
            view: WebView,
            request: WebResourceRequest,
        ): WebResourceResponse? = assetLoader.shouldInterceptRequest(request.url)

        /**
         * Only the app's own origin may navigate inside the WebView.
         *
         * An `https` link is someone's actual intent to visit a web page,
         * so it goes to their browser where the address bar is. Anything
         * else is dropped rather than guessed at. Same policy as
         * `AppWebView.policy(for:)` on iOS — and the same trap: a
         * navigation the shell cancels is silent, so anything the app
         * genuinely needs to *do* must go through the bridge, not a link.
         */
        override fun shouldOverrideUrlLoading(
            view: WebView,
            request: WebResourceRequest,
        ): Boolean {
            val url = request.url
            if (url.toString().startsWith("$ORIGIN/")) return false
            if (url.scheme == "https" || url.scheme == "mailto") {
                runCatching { startActivity(Intent(Intent.ACTION_VIEW, url)) }
            }
            return true
        }
    }

    override fun onDestroy() {
        billing.close()
        super.onDestroy()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        webView.saveState(outState)
    }

    override fun onRestoreInstanceState(savedInstanceState: Bundle) {
        super.onRestoreInstanceState(savedInstanceState)
        webView.restoreState(savedInstanceState)
    }

    companion object {
        const val ORIGIN = "https://appassets.androidplatform.net"

        /** Matches `WEBAPP_ORIGIN` + `WEBAPP_LOGIN_PATH` in
         *  `apps/desktop/src/lib/openapps.ts`. The real web origin, not
         *  the bundled copy: an OAuth redirect can only land on https. */
        const val LOGIN_URL = "https://openpdfedit.com/app/login"
        private const val TAG = "OpenPdfEdit"
    }
}
