/**
 * Taking delivery of a PDF from another app in the same browser.
 *
 * OpenCapture is the first sender: a full-page capture too long to annotate
 * in its own editor is kept as a PDF, and the obvious next step is this app.
 * It cannot simply open the file here, because there is no way in from
 * outside a page — a browser extension can neither fill `showOpenFilePicker`
 * nor an `<input type=file>`, both of which are native pickers, and this app
 * deliberately has no upload endpoint to post to. So the sender opens this
 * page with `?handoff=<sender>` and pushes the bytes in over
 * `window.postMessage`, from a content script sharing the same window.
 *
 * Where it lands is not new: `offerExternalFile` already exists for the iOS
 * shell, which has the same problem — a document arriving as bytes with no
 * picker and no handle behind it. This is a second way in to the same place.
 *
 * The handshake runs both ways round because neither side can be sure it is
 * second: the page may finish loading before the sender's script is injected,
 * or long after. So the page announces itself until it is answered, and a
 * sender that arrives first simply waits for the next announcement.
 *
 * Only ever a local file. Nothing here fetches, and nothing is uploaded —
 * the message carries the bytes themselves, from a script the user's own
 * browser is running. A page can of course post this message to itself, which
 * buys an attacker nothing: opening a document it already holds is what the
 * file picker is for.
 */

/** Marks a page as expecting a document, and says who from. */
export const HANDOFF_PARAM = "handoff";

/** Posted by this page: "there is somewhere to put it now". */
export const HANDOFF_READY = "openpdfedit:handoff-ready";

/** Posted by the sender: the document itself. */
export const HANDOFF_FILE = "openpdfedit:handoff-file";

/**
 * Who may hand a document over.
 *
 * A list rather than "anything with a `?handoff=`", so that the marker means
 * something specific and a stray link cannot put this page into a waiting
 * state on behalf of nobody.
 */
const SENDERS: ReadonlySet<string> = new Set(["opencapture"]);

/** How long to keep announcing before concluding nobody is sending. */
const ANNOUNCE_TIMEOUT_MS = 20_000;

/** How often to repeat the announcement while waiting. */
const ANNOUNCE_INTERVAL_MS = 250;

/** The sender named in a query string, if it is one this app accepts. */
export function handoffSender(search: string): string | null {
  const value = new URLSearchParams(search).get(HANDOFF_PARAM);
  return value !== null && SENDERS.has(value) ? value : null;
}

/**
 * The bytes out of a handoff message, whatever shape they survived in.
 *
 * A content script and a page are separate JavaScript worlds, and what a
 * structured clone leaves behind at that boundary is not uniform across
 * browsers — a `Uint8Array` can arrive as itself, as a plain `ArrayBuffer`,
 * or as an array-like with no prototype worth trusting. Base64 is accepted
 * as well because a string is the one thing that cannot arrive damaged.
 *
 * Returns null for anything that is not recognisably bytes, so a malformed
 * message is ignored rather than turned into an empty document.
 */
export function handoffBytes(value: unknown): Uint8Array | null {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (typeof value === "string") {
    try {
      const binary = atob(value);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      return bytes;
    } catch {
      return null;
    }
  }
  if (value && typeof value === "object" && "length" in (value as ArrayLike<number>)) {
    const arrayLike = value as ArrayLike<number>;
    if (typeof arrayLike.length !== "number") return null;
    return Uint8Array.from(arrayLike);
  }
  return null;
}

/** A PDF filename, or a sensible one if the sender did not send a usable one. */
export function handoffName(value: unknown): string {
  const name = typeof value === "string" ? value.trim() : "";
  // No path separators: the name is shown and used as a save-as suggestion,
  // and a sender is not owed the ability to put slashes in either.
  const flat = name.replace(/[/\\]/g, "").slice(0, 120);
  if (!flat) return "handoff.pdf";
  return flat.toLowerCase().endsWith(".pdf") ? flat : `${flat}.pdf`;
}

export interface HandoffDelivery {
  name: string;
  bytes: Uint8Array;
}

/**
 * Wait for a document to be handed over, announcing readiness until it is.
 *
 * Returns a function that stops waiting — call it when the page goes away, or
 * the announcements outlive the page that was making them.
 */
export function receiveHandoff(
  onDelivery: (delivery: HandoffDelivery) => void,
  win: Window = window,
): () => void {
  let done = false;
  let timer: ReturnType<typeof setInterval> | undefined;
  let deadline: ReturnType<typeof setTimeout> | undefined;

  const stop = (): void => {
    if (done) return;
    done = true;
    if (timer !== undefined) clearInterval(timer);
    if (deadline !== undefined) clearTimeout(deadline);
    win.removeEventListener("message", onMessage);
  };

  function onMessage(event: MessageEvent): void {
    // Same window, same origin. A content script's postMessage satisfies both
    // — it shares the window it was injected into — while anything arriving
    // from a frame or another origin does not, and has no business here.
    if (event.source !== win || event.origin !== win.location.origin) return;
    const data = event.data as { type?: unknown; name?: unknown; bytes?: unknown } | null;
    if (!data || data.type !== HANDOFF_FILE) return;
    const bytes = handoffBytes(data.bytes);
    if (!bytes || bytes.length === 0) return;
    const name = handoffName(data.name);
    stop();
    onDelivery({ name, bytes });
  }

  win.addEventListener("message", onMessage);
  const announce = (): void => {
    win.postMessage({ type: HANDOFF_READY }, win.location.origin);
  };
  announce();
  timer = setInterval(announce, ANNOUNCE_INTERVAL_MS);
  deadline = setTimeout(stop, ANNOUNCE_TIMEOUT_MS);
  return stop;
}
