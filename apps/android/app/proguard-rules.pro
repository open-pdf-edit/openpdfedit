# The bridge is reached by name from JavaScript, never from Kotlin, so
# R8 sees no caller and would strip or rename every method on it. The
# page would then call `post` on an object that no longer has it, and
# the failure is silent — the promise simply never settles, which is the
# same shape as the WebView bugs that cost a day on iOS.
-keepclassmembers class * {
    @android.webkit.JavascriptInterface <methods>;
}
-keep class com.openpdfedit.app.WebBridge { *; }

# Reflected over by the JSON parser in the bridge's own payloads.
-keepattributes JavascriptInterface, Signature, *Annotation*

# Play Billing speaks to a remote service and its model classes are
# deserialised by name.
-keep class com.android.billingclient.api.** { *; }
