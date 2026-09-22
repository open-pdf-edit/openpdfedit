// The iOS shell, seen from the web app.
//
// `apps/ios/OpenPdfEdit/bridge.js` injects `window.OpenPdfEditNative` into
// the page before any of this app's own script runs. Everything the shell
// can do goes through that one object, and this module is the only place
// that touches it — so the desktop, web and extension builds compile
// unchanged with no iOS types anywhere, exactly the way `extensionRuntime()`
// keeps `chrome` out of them.
//
// The types here restate the bridge's contract. They are a promise this
// file makes on the shell's behalf, so the two have to be changed together;
// the bridge's own comments are the reference.

/** A credit pack, priced by StoreKit in the customer's own currency. */
export interface NativeProduct {
  id: string;
  name: string;
  description: string;
  /** Already formatted for the customer's region. Never reformat it. */
  price: string;
}

/** A completed purchase. Not credits yet — the receipt has to be redeemed. */
export interface NativePurchase {
  status: "purchased";
  transactionId: string;
  productId: string;
  /** The signed transaction, for the server to verify. */
  receipt: string;
  /** Whether StoreKit's own check passed on the device. Reported, not
   *  acted on: the server verifies Apple's signature itself. */
  verifiedLocally: boolean;
}

export type NativePurchaseResult =
  | NativePurchase
  // Not an error. Someone who taps Cancel has not had a problem.
  | { status: "cancelled" }
  // Ask to Buy, or a payment method that settles later.
  | { status: "pending" };

export type NativeSignIn =
  | { status: "signed_in"; accessToken: string; refreshToken: string }
  | { status: "cancelled" };

export interface NativeDocument {
  path: string;
  name: string;
}

export interface NativeShell {
  platform: "ios";
  on(event: "document", fn: (detail: NativeDocument) => void): () => void;
  on(event: "receipt", fn: (detail: NativePurchase) => void): () => void;
  ready(): Promise<{ platform: string }>;
  readDocument(detail: NativeDocument): Promise<File>;
  signIn(): Promise<NativeSignIn>;
  products(): Promise<NativeProduct[]>;
  purchase(productId: string): Promise<NativePurchaseResult>;
  outstanding(): Promise<NativePurchase[]>;
  finish(transactionId: string): Promise<{ finished: boolean }>;
}

/// Purchases through the App Store — the part of a native shell the
/// purchase panel needs, and all of it.
///
/// Two things provide it: the iOS app's shell, and the Mac App Store
/// build's StoreKit commands (installed by `$lib/storekit-mac`). Sign-in
/// and documents are not here on purpose: on the Mac the desktop app
/// already does both itself, and only the iOS shell answers them.
export interface StoreKitBridge {
  products(): Promise<NativeProduct[]>;
  purchase(productId: string): Promise<NativePurchaseResult>;
  outstanding(): Promise<NativePurchase[]>;
  finish(transactionId: string): Promise<{ finished: boolean }>;
  on(event: "receipt", fn: (detail: NativePurchase) => void): () => void;
}

/// StoreKit, from whichever native side has it, or `null` where purchases
/// are not sold through the App Store (the web app, the extension, the
/// Developer ID desktop build — which sell credits by card instead).
export function storeKit(): StoreKitBridge | null {
  return (
    nativeShell() ??
    (globalThis as { OpenPdfEditStoreKit?: StoreKitBridge }).OpenPdfEditStoreKit ??
    null
  );
}

/** The shell, or `null` everywhere that is not it. */
export function nativeShell(): NativeShell | null {
  const shell = (globalThis as { OpenPdfEditNative?: NativeShell }).OpenPdfEditNative;
  return shell && typeof shell.purchase === "function" ? shell : null;
}

/// Whether purchases here must go through the App Store.
///
/// They must, and not as a preference: App Store Review Guideline 3.1.1
/// requires in-app purchase for digital content used in the app, and an app
/// that shows a card checkout instead is rejected — or removed. So the
/// web app's own <openapps-buy> is not merely redundant inside the shell,
/// it is the thing that must not be there.
export const MUST_USE_IN_APP_PURCHASE = true;
