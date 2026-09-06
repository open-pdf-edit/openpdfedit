// Turning an App Store purchase into credits.
//
// Shared between the buy button and the recovery sweep that runs at
// startup, because both do exactly the same three things in the same
// order, and the order is the part that costs money to get wrong:
//
//   1. StoreKit takes the payment and hands back a signed transaction.
//   2. The server verifies it and grants the credits.
//   3. Only then is the transaction *finished*.
//
// StoreKit re-delivers an unfinished transaction on every launch. That is
// what makes a purchase survive a crash, a dead network or a force-quit
// between steps 1 and 2 — and finishing early throws it away, leaving
// someone charged for credits nobody granted with no record left to retry
// from. Nothing below finishes a transaction the server has not honoured.

import { getClient, notify } from "@openapps/ui";

import type { NativePurchase, NativeShell } from "$lib/native";
import { redeemAppleReceipt, type RedeemResult } from "$lib/openapps";

/// Redeem a receipt, then finish the transaction if — and only if — the
/// server granted the credits.
export async function collect(receipt: NativePurchase, shell: NativeShell): Promise<RedeemResult> {
  let result = await redeemAppleReceipt(accessToken(), receipt.receipt);

  // An access token that aged out. This is the trap the entitlement check
  // fell into first: `redeemAppleReceipt` is a plain fetch, so the SDK's
  // refresh-on-401 does not cover it, and a stale token would tell someone
  // who had just paid to sign in again. Refresh through the SDK and ask
  // once more before believing it.
  if (result.ok === false && result.kind === "unauthorized" && (await refreshSession())) {
    result = await redeemAppleReceipt(accessToken(), receipt.receipt);
  }

  if (result.ok) {
    await shell.finish(receipt.transactionId);
    // Everything on the page that shows a balance re-reads.
    notify();
  }
  return result;
}

/// Everything StoreKit still considers owing, redeemed.
///
/// Runs unprompted at startup rather than behind a "Restore purchases"
/// button: someone whose payment went through should not have to know that
/// word, and the case this exists for — a redemption that never reached
/// the server — is one they have no way to describe.
///
/// Returns how many were credited, for the caller to report or ignore.
export async function collectOutstanding(shell: NativeShell): Promise<number> {
  if (!getClient()?.isLoggedIn) return 0;
  let credited = 0;
  try {
    for (const receipt of await shell.outstanding()) {
      const result = await collect(receipt, shell);
      if (result.ok && !result.alreadyCredited) credited += result.credits;
    }
  } catch {
    // Best-effort and unasked-for. StoreKit will offer the same
    // transactions again on the next launch, so a failure here costs
    // nothing but a delay — and an error message for something nobody
    // requested is worse than silence.
  }
  return credited;
}

function accessToken(): string | undefined {
  return getClient()?.session?.accessToken;
}

/// Ask the SDK for something, so its refresh-on-401 runs. True if the
/// session works afterwards.
async function refreshSession(): Promise<boolean> {
  const client = getClient();
  if (!client?.isLoggedIn) return false;
  try {
    await client.credits.balance();
    return true;
  } catch {
    return false;
  }
}
