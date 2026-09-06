// The JavaScript half of the iOS bridge, injected into the bundled web app
// at document start.
//
// Everything the shell can do is reachable through `window.OpenPdfEditNative`
// and nowhere else. The web app never touches `webkit.messageHandlers`
// directly: that global exists only inside this shell, and code that reaches
// for it is code that has to be guarded at every call site. One object, whose
// absence means "not running natively", is a single check.
(function () {
  "use strict";

  var handler =
    window.webkit &&
    window.webkit.messageHandlers &&
    window.webkit.messageHandlers.openpdfedit;
  if (!handler) return;

  var listeners = Object.create(null);

  function call(action, extra) {
    var message = { action: action };
    if (extra) {
      for (var key in extra) message[key] = extra[key];
    }
    return handler.postMessage(message);
  }

  window.OpenPdfEditNative = {
    platform: "ios",

    // --- events from the shell ---------------------------------------

    on: function (event, fn) {
      (listeners[event] || (listeners[event] = [])).push(fn);
      return function () {
        listeners[event] = (listeners[event] || []).filter(function (f) {
          return f !== fn;
        });
      };
    },

    /** Called by the shell. Not part of the app-facing API. */
    _emit: function (event, detail) {
      (listeners[event] || []).forEach(function (fn) {
        try {
          fn(detail);
        } catch (e) {
          console.error("OpenPdfEditNative listener for " + event + " threw", e);
        }
      });
    },

    /**
     * Tell the shell the page can accept a document.
     *
     * Until this is called, a PDF opened from Files is held rather than
     * delivered — on a cold start iOS hands it over long before the editor
     * exists, and a document delivered to nothing is a document lost.
     */
    ready: function () {
      return call("ready");
    },

    // --- documents from other apps ------------------------------------

    /**
     * Fetches a document the shell staged and hands it back as a File.
     *
     * Fetched rather than passed as base64: a scan can be tens of
     * megabytes, and a base64 string of one costs a third more again on
     * top of the copy it decodes to.
     */
    readDocument: async function (detail) {
      var response = await fetch(detail.path);
      if (!response.ok) throw new Error("could not read the incoming document");
      var blob = await response.blob();
      // Releasing it is what lets the shell stop holding the file open.
      call("documentRead", { path: detail.path });
      return new File([blob], detail.name || "document.pdf", {
        type: "application/pdf",
      });
    },

    // --- account --------------------------------------------------------

    /**
     * Signs in through ASWebAuthenticationSession.
     *
     * Resolves `{status:"signed_in", accessToken, refreshToken}`, or
     * `{status:"cancelled"}` when the sheet was dismissed — which is not an
     * error and must not be shown as one.
     */
    signIn: function () {
      return call("signIn");
    },

    // --- in-app purchase --------------------------------------------------

    /** `[{id, name, description, price}]`, priced in the customer's own
     *  currency by StoreKit. */
    products: function () {
      return call("products").then(function (r) {
        return r.products;
      });
    },

    /**
     * Buys a product.
     *
     * Resolves one of:
     *   {status:"purchased", transactionId, productId, receipt, verifiedLocally}
     *   {status:"cancelled"}   — they changed their mind; say nothing
     *   {status:"pending"}     — Ask to Buy, or a slow payment method
     *
     * A "purchased" result is not credits yet. Send `receipt` to the
     * server, and only when it answers call `finish` — see below.
     */
    purchase: function (productId) {
      return call("purchase", { productId: productId });
    },

    /**
     * Purchases StoreKit still considers owing, including from previous
     * launches.
     *
     * This is the recovery path for a purchase interrupted by a crash or a
     * dead network, and it should run at startup rather than behind a
     * "restore purchases" button — someone whose payment went through
     * should not have to know that word.
     */
    outstanding: function () {
      return call("outstanding").then(function (r) {
        return r.receipts;
      });
    },

    /**
     * Marks a transaction done, *after* the server has granted its credits.
     *
     * Calling this early is the expensive mistake: StoreKit stops
     * re-delivering a finished transaction, so a purchase whose redemption
     * never reached the server becomes money taken for credits nobody
     * granted, with no record left to retry from.
     */
    finish: function (transactionId) {
      return call("finish", { transactionId: transactionId });
    },
  };
})();
