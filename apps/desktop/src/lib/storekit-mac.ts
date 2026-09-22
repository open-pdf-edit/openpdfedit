// StoreKit for the Mac App Store build, handed to the page in the iOS
// shell's shape so the purchase panel and the redemption path are the same
// code on both.
//
// The Rust side (src-tauri/src/appstore.rs) calls StoreKit 2 in Swift and
// answers exactly as the iOS shell does: products, a purchase outcome,
// outstanding transactions, finish, and a "receipt" event for purchases
// that complete on their own. Installed once, at startup, and only in the
// build made with VITE_TARGET=appstore; everywhere else `storeKit()` finds
// nothing and the card checkout is shown instead.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { NativePurchase, NativePurchaseResult, NativeProduct, StoreKitBridge } from "./native";

export function installMacStoreKit(): void {
  const bridge: StoreKitBridge = {
    products: () => invoke<NativeProduct[]>("storekit_products"),
    purchase: (productId) => invoke<NativePurchaseResult>("storekit_purchase", { productId }),
    outstanding: () => invoke<NativePurchase[]>("storekit_outstanding"),
    finish: (transactionId) => invoke<{ finished: boolean }>("storekit_finish", { transactionId }),
    on: (_event, fn) => {
      let stop: (() => void) | undefined;
      let stopped = false;
      void listen<NativePurchase>("storekit-receipt", (e) => fn(e.payload)).then((unlisten) => {
        if (stopped) unlisten();
        else stop = unlisten;
      });
      return () => {
        stopped = true;
        stop?.();
      };
    },
  };
  (globalThis as { OpenPdfEditStoreKit?: StoreKitBridge }).OpenPdfEditStoreKit = bridge;
}
