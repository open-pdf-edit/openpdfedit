/**
 * The documents you had open last, for the screen you land on.
 *
 * Opening a file is two or three clicks through a picker, and the file
 * you want next is nearly always one you had open recently. The empty
 * state knew nothing about that and offered only the picker.
 *
 * ## What is kept, and where
 *
 * A name, a timestamp, and an id — in this browser's `localStorage`,
 * on this machine, and nowhere else. No document content, not a byte;
 * the id is a key the backend can use to find the file again, not a
 * copy of it. Clearing it is one click, and it is the only record this
 * app keeps of what you have opened.
 *
 * ## Only what can actually be reopened
 *
 * A row that does nothing when clicked is worse than no row. What can
 * be reopened differs by where the app is running, so *the backend*
 * decides what to record here rather than the UI:
 *
 * - The desktop app has real paths, so anything it opens can be
 *   reopened.
 * - A browser with the File System Access API keeps the file handle in
 *   IndexedDB and asks permission before reading it again. That is
 *   Chrome and Edge.
 * - A browser without it — Firefox and Safari today — opens through a
 *   plain file input, which yields a `File` that cannot outlive the
 *   page. There is nothing to remember, so nothing is recorded and the
 *   list simply stays empty, rather than filling with rows that reopen
 *   a picker and pretend that was the point.
 */

/** One remembered document. `id` is opaque to the UI — only the backend
 * that issued it knows what it means (a filesystem path on the desktop,
 * an IndexedDB key in the browser). */
export interface RecentDocument {
  id: string;
  /** The file's name, for display. */
  name: string;
  /** Epoch milliseconds. */
  openedAt: number;
}

const KEY = "openpdfedit.recents";

/**
 * How many to keep.
 *
 * Short on purpose. The value of this list is that the file you want is
 * visible without reading — a landing screen with twenty rows on it is
 * a file manager, and a worse one than the picker it was meant to save
 * you from.
 */
const LIMIT = 6;

/** Reads the list, newest first. Never throws: storage can be refused
 * outright (a private window, an embedded webview) and a missing
 * convenience is not worth an error on the screen someone lands on. */
export function listRecents(): RecentDocument[] {
  let raw: string | null = null;
  try {
    raw = localStorage.getItem(KEY);
  } catch {
    return [];
  }
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(
        (entry): entry is RecentDocument =>
          !!entry &&
          typeof entry === "object" &&
          typeof (entry as RecentDocument).id === "string" &&
          typeof (entry as RecentDocument).name === "string" &&
          typeof (entry as RecentDocument).openedAt === "number",
      )
      .sort((a, b) => b.openedAt - a.openedAt)
      .slice(0, LIMIT);
  } catch {
    // Something else wrote here, or it was truncated. Start over
    // rather than leaving the landing screen broken forever.
    return [];
  }
}

function write(entries: RecentDocument[]): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(entries.slice(0, LIMIT)));
  } catch {
    // Full, or refused. The app is unaffected either way.
  }
}

/**
 * Records a document as the most recently opened, and returns the ids
 * that fell off the end so the caller can release whatever it was
 * holding for them.
 *
 * Opening the same file twice moves it to the top rather than listing
 * it twice — matched on `id`, so the desktop dedupes on path and the
 * browser on handle key.
 */
export function rememberRecent(id: string, name: string, now: number): string[] {
  const kept = listRecents().filter((entry) => entry.id !== id);
  const next = [{ id, name, openedAt: now }, ...kept];
  write(next);
  return next.slice(LIMIT).map((entry) => entry.id);
}

/** Drops one entry — a file that has been moved or deleted, or one
 * someone would rather not see listed. */
export function forgetRecent(id: string): void {
  write(listRecents().filter((entry) => entry.id !== id));
}

export function clearRecents(): void {
  try {
    localStorage.removeItem(KEY);
  } catch {
    // Nothing was stored to begin with.
  }
}

/**
 * "just now", "20 minutes ago", "yesterday" — how long ago, in the
 * roughest terms that are still true.
 *
 * Rough on purpose: the point of the timestamp is to order the list and
 * to say whether this is the file from a moment ago or from last week.
 * A precise time would invite reading it as a record of what you were
 * working on and when, which is more than this needs to be.
 */
export function describeWhen(openedAt: number, now: number): string {
  const seconds = Math.max(0, Math.round((now - openedAt) / 1000));
  if (seconds < 90) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} minutes ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return hours === 1 ? "an hour ago" : `${hours} hours ago`;
  const days = Math.round(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  const weeks = Math.round(days / 7);
  if (weeks < 5) return weeks === 1 ? "last week" : `${weeks} weeks ago`;
  return "a while ago";
}
