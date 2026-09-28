// The JavaScript half of the Android bridge, injected before the page's
// own scripts run.
//
// Deliberately the same shape as `apps/ios/OpenPdfEdit/bridge.js`: one
// global, `window.OpenPdfEditNative`, whose absence means "not running
// in a shell". The web layer cannot tell the two platforms apart and
// should not have to — `platform` is the only field that differs.
//
// Android's `@JavascriptInterface` is synchronous and speaks only
// strings, while the web layer expects promises. So a call posts a JSON
// message carrying an id, and the shell answers by invoking `_resolve`
// with that id. That is the same request/response contract WKWebView
// gives iOS for free.
(function () {
  "use strict";

  var host = window.OpenPdfEditAndroid;
  if (!host) return;

  var listeners = Object.create(null);
  var pending = Object.create(null);
  var nextId = 1;

  function call(action, extra) {
    var id = nextId++;
    var message = { id: id, action: action };
    if (extra) {
      for (var key in extra) message[key] = extra[key];
    }
    return new Promise(function (resolve, reject) {
      pending[id] = { resolve: resolve, reject: reject };
      try {
        host.post(JSON.stringify(message));
      } catch (e) {
        delete pending[id];
        reject(e);
      }
    });
  }

  window.OpenPdfEditNative = {
    platform: "android",

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

    /** Called by the shell to answer a `call()`. Not app-facing. */
    _resolve: function (id, ok, payload) {
      var entry = pending[id];
      if (!entry) return;
      delete pending[id];
      if (ok) entry.resolve(payload);
      else entry.reject(new Error(String(payload)));
    },

    /**
     * Tell the shell the page can accept a document.
     *
     * A PDF opened from Files or Gmail arrives as an intent long before
     * the editor exists, so the shell holds it until this is called.
     */
    ready: function () {
      return call("ready");
    },

    // --- documents from other apps ------------------------------------

    /**
     * Fetches a document the shell staged and hands it back as a File.
     *
     * Fetched rather than passed as base64, for the same reason as on
     * iOS: a scan can be tens of megabytes and base64 costs a third
     * again on top of the copy it decodes to. The shell serves it from
     * `/__incoming/<id>` on this same origin.
     */
    readDocument: async function (detail) {
      var response = await fetch(detail.path);
      if (!response.ok) throw new Error("could not read the incoming document");
      var blob = await response.blob();
      call("documentRead", { path: detail.path });
      return new File([blob], detail.name || "document.pdf", {
        type: "application/pdf",
      });
    },

    // --- documents leaving ------------------------------------------

    /**
     * Writes bytes to a file the customer chooses.
     *
     * This is the only way a file leaves the app. The web layer's usual
     * route is an `<a download>` at a blob: URL, and an Android WebView
     * does nothing with that unless the app implements a download
     * listener — the same silent failure that made every Save and
     * Export on iOS do nothing at all. Rather than mimic iOS's share
     * sheet, this opens the Storage Access Framework's create-document
     * picker, which is what "Save" means on Android: the customer picks
     * the place, and the app never needs storage permission.
     */
    saveFile: function (name, bytes, mime) {
      var binary = "";
      var view = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
      // In slices: String.fromCharCode.apply on a whole multi-megabyte
      // array overflows the argument list and throws.
      for (var i = 0; i < view.length; i += 0x8000) {
        binary += String.fromCharCode.apply(null, view.subarray(i, i + 0x8000));
      }
      return call("saveFile", {
        name: name,
        mime: mime || "application/octet-stream",
        data: btoa(binary),
      }).then(function (r) {
        return !!(r && r.saved);
      });
    },

    // --- account ---------------------------------------------------
    //
    // No `signIn` here, deliberately. Sign-in on Android wants a Custom
    // Tab and a callback the accounts server is configured to redirect
    // to, and neither exists yet. Leaving the method *absent* rather
    // than present-and-failing is the point: `nativeShell()?.signIn`
    // then reads false and the web layer takes its own path, instead of
    // calling something that rejects. A shell that answers a capability
    // it cannot perform is the failure mode that cost a day on iOS.
  };
})();
