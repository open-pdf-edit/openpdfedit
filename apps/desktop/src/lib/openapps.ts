// The OpenApps account integration: where it talks to, and the one paid
// feature it gates.
//
// Every URL and identifier here is defined once. OpenCapture learned this
// the expensive way — its base URL was written as a literal in two places,
// so moving to a custom domain fixed one of them and silently left the
// other pointing at the old host (see that project's PLAN.md, M21). There
// is exactly one place to change any of this.

import { OpenAppsError } from "@openapps/sdk";

/// A CNAME'd custom domain, not the shared OpenApps backend's own hostname
/// — same server, same account and credit ledger. Sessions are bearer
/// tokens rather than cookies, so nothing about identity is domain-scoped
/// and a second hostname pointed at the same backend changes nothing
/// functionally. It exists so signing in never shows a stranger's domain
/// in the address bar to someone who has only ever seen "OpenPdfEdit".
///
/// Two cosmetic seams this does *not* close, both because the server
/// builds them once at startup from its own `public_url` rather than
/// per-request: Google sign-in still visibly bounces through
/// `accounts.openapps.network` on the OAuth callback hop, and a wallet
/// signature prompt still names that host. Neither is a security
/// property — the server never checks either string against the request.
import { getClient } from "@openapps/ui";

export const OPENAPPS_BASE_URL = "https://auth.openpdfedit.com";

/// Holds the app key that turns a user's token into an actual charge.
/// The user token this app carries can read a balance and an entitlement
/// on its own, but it can never spend — only this service can, which is
/// why the unlock route lives here and the entitlement check does not.
export const OPENAPPS_GATEWAY_URL = "https://gateway.openapps.network";

/// The key the SDK's default browser store keeps the session under. Named
/// here so the sign-in popup and the window that opened it can agree on
/// which `storage` event means "the session changed" — the SDK's own
/// default, restated rather than re-chosen.
export const SESSION_STORAGE_KEY = "openapps.session";

/// What the sign-in popup posts back to its opener when it is done.
export const SIGNIN_DONE_MESSAGE = "openpdfedit-signin-done";

/// The ledger reference for the one-time watermark unlock. The server
/// keys the entitlement on this string, so it is a permanent identifier:
/// changing it would orphan every unlock already sold.
export const SUPPORTER_REF_ID = "openpdfedit_supporter_unlock";

/// What the unlock costs, in credits. Duplicated on the gateway, which is
/// the authority — this copy is only ever used to *say* the price, never
/// to charge it, so a drift between the two shows up as wrong wording
/// rather than a wrong charge.
export const SUPPORTER_COST = 1000;

/// Whether the Supporter tools — the watermark and OCR — need an
/// account.
///
/// They do, and they are the only paid things in this product. One
/// unlock covers both: the entitlement is "Supporter", not "watermark",
/// so nobody who has already paid is asked again when the second tool
/// appears. Kept as a named constant rather than inlined so the gate is
/// greppable, and so a build that wants them open (a self-hosted one,
/// say) has one line to change rather than a flow to unpick.
export const SUPPORTER_TOOLS_ARE_PREMIUM = true;

/// Has this account already redeemed the unlock?
///
/// Asks openapps-server directly rather than the gateway: this reads the
/// caller's own ledger and needs no app key, and routing a read through
/// the gateway would put the one service that *can* spend credits in the
/// path of a question that never should.
///
/// Three answers, not two.
///
/// "No" used to cover being refused as well as not having bought it,
/// and the two are not the same thing at all: an expired access token
/// answers 401, which read as "not unlocked" and asked someone who had
/// already paid to pay again. `unauthorized` is what tells the caller to
/// refresh the session and ask once more.
///
/// Anything else still answers `locked`. That is the safe direction for
/// a check that opens a paid feature, and a network blip then costs a
/// retry rather than a free unlock.
export type SupporterState = "unlocked" | "locked" | "unauthorized";

export async function supporterState(accessToken: string | undefined): Promise<SupporterState> {
  if (!accessToken) return "unauthorized";
  try {
    const url = `${OPENAPPS_BASE_URL}/v1/credits/entitlement?ref_id=${encodeURIComponent(SUPPORTER_REF_ID)}`;
    const res = await fetch(url, { headers: { Authorization: `Bearer ${accessToken}` } });
    if (res.status === 401 || res.status === 403) return "unauthorized";
    if (!res.ok) return "locked";
    const body = (await res.json()) as { unlocked?: boolean };
    return body.unlocked === true ? "unlocked" : "locked";
  } catch {
    return "locked";
  }
}

export type UnlockResult =
  | { ok: true; newBalance: number }
  | { ok: false; kind: "insufficient"; have: number; need: number }
  | { ok: false; kind: "unauthorized" }
  | { ok: false; kind: "other"; message: string };

/// The one place this app ever spends credits.
///
/// The charge is idempotent server-side, keyed per user: clicking unlock
/// twice, or retrying a request whose response was lost, replays the same
/// ledger entry rather than charging again. That is why this can be
/// retried freely on a network error without risking a double charge.
export async function unlockSupporter(accessToken: string | undefined): Promise<UnlockResult> {
  if (!accessToken) return { ok: false, kind: "unauthorized" };
  try {
    const res = await fetch(`${OPENAPPS_GATEWAY_URL}/openpdfedit/supporter/unlock`, {
      method: "POST",
      headers: { Authorization: `Bearer ${accessToken}` },
    });
    const body = (await res.json().catch(() => ({}))) as {
      new_balance?: number;
      have?: number;
      need?: number;
      error?: string;
    };
    if (res.ok && typeof body.new_balance === "number") {
      return { ok: true, newBalance: body.new_balance };
    }
    // 402 is the gateway's "you can't afford this", and it carries both
    // numbers so the UI can say how short the account is rather than
    // just that it is.
    if (res.status === 402 && typeof body.have === "number" && typeof body.need === "number") {
      return { ok: false, kind: "insufficient", have: body.have, need: body.need };
    }
    // A dead or expired token. Distinguished from every other failure
    // because it has a fix — refresh and try again — and because
    // "couldn't unlock, please try again" is exactly the wrong thing to
    // tell someone whose only problem is that their token aged out.
    if (res.status === 401 || res.status === 403) {
      return { ok: false, kind: "unauthorized" };
    }
    return { ok: false, kind: "other", message: body.error ?? `unlock failed (${res.status})` };
  } catch (e) {
    return {
      ok: false,
      kind: "other",
      message: e instanceof OpenAppsError ? e.message : "network error",
    };
  }
}


/**
 * Sign in with the Telegram session this Mini App already has.
 *
 * The one login flow that needs no popup, no redirect and no injected
 * signer: `initData` is already in the page. That matters more here than
 * elsewhere — a Mini App webview hosts no wallet extension, and a full-page
 * OAuth redirect out of it is a poor experience even when it works.
 *
 * Resolves true if a session was established. Callers should fall back to
 * the ordinary sign-in flow on false rather than treating it as fatal:
 * `initData` expires, and a stale one is a normal thing to encounter.
 */
export async function signInWithTelegram(initData: string): Promise<boolean> {
  const challenge = await fetch(`${OPENAPPS_BASE_URL}/v1/auth/challenge`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ namespace: "telegram" }),
  });
  if (!challenge.ok) return false;
  const { challenge_id } = (await challenge.json()) as { challenge_id: string };

  const verified = await fetch(`${OPENAPPS_BASE_URL}/v1/auth/verify`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      challenge_id,
      proof: { type: "telegram_init_data", init_data: initData },
    }),
  });
  if (!verified.ok) return false;

  const session = (await verified.json()) as {
    access_token?: string;
    refresh_token?: string;
  };
  if (!session.access_token || !session.refresh_token) return false;

  // Hand the tokens to the shared client rather than writing storage
  // directly — adoptSession is what notifies every <openapps-*> element on
  // the page that there is a session now.
  getClient()?.adoptSession(session.access_token, session.refresh_token);
  return true;
}
