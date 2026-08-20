// A small transient-notification store, the Toast counterpart to
// dialog.svelte.ts's modal-dialog store: module-level Svelte 5 runes
// state, one active item at a time, rendered by a single host mounted at
// the app root (see ToastHost.svelte). Unlike a dialog, a toast never
// blocks input — it announces something and clears itself.

export type ToastTone = "neutral" | "success" | "warning" | "danger";

interface ActiveToast {
  id: number;
  tone: ToastTone;
  title?: string;
  message: string;
}

let current = $state<ActiveToast | null>(null);
let nextId = 0;
let timer: ReturnType<typeof setTimeout> | null = null;

export function activeToast(): ActiveToast | null {
  return current;
}

/** Shows `message`, replacing whatever toast is currently up. Clears
 * itself after `durationMs` (default 6s — long enough to actually read a
 * sentence, short enough not to linger once you've moved on). */
export function showToast(
  message: string,
  options: { tone?: ToastTone; title?: string; durationMs?: number } = {},
) {
  if (timer) clearTimeout(timer);
  const id = ++nextId;
  current = { id, tone: options.tone ?? "neutral", title: options.title, message };
  timer = setTimeout(() => {
    // Only clear if this is still the toast that scheduled the timeout —
    // a newer showToast() call already replaced `current` and owns the
    // next dismissal.
    if (current?.id === id) current = null;
  }, options.durationMs ?? 6000);
}

export function dismissToast() {
  if (timer) clearTimeout(timer);
  current = null;
}
