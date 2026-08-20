// A small store for saved signatures — drawn once in SignaturePad.svelte,
// reused across documents by dragging them onto a page (see +page.svelte's
// handlePlaceSignature). Persisted to localStorage, not a Tauri command:
// a signature template is personal browser/app-profile state, not
// something that belongs in any particular PDF, so there's no document
// to save it into and no backend round trip earns its keep here.
//
// Module-level $state, one array shared by every importer — same shape
// as dialog.svelte.ts/toast.svelte.ts, this file's siblings.

export interface SavedSignature {
  id: string;
  name: string;
  /** Ink strokes, normalized to the *tight bounding box of the drawing
   * itself* (not the whole pad canvas) so placement scaling has no dead
   * margin: x/y both range 0..1, in the same y-down orientation the
   * source `<canvas>` used. */
  strokes: [number, number][][];
  /** width/height of that bounding box, in the units it was drawn in —
   * lets placement preserve the signature's own proportions instead of
   * stretching it to fill whatever rectangle the user drags. */
  aspect: number;
  createdAt: string;
}

const STORAGE_KEY = "openpdfedit.signatures";

// Guarded because this module is also imported during SvelteKit's SSR
// prerender pass (see +page.svelte's refreshSignatures-era comment on
// the same gotcha), where there is no `localStorage` global at all.
function hasLocalStorage(): boolean {
  return typeof localStorage !== "undefined";
}

function load(): SavedSignature[] {
  if (!hasLocalStorage()) return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as SavedSignature[]) : [];
  } catch (e) {
    console.error("failed to load saved signatures", e);
    return [];
  }
}

function persist() {
  if (!hasLocalStorage()) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(signatures));
  } catch (e) {
    console.error("failed to save signatures", e);
  }
}

let signatures = $state<SavedSignature[]>(load());

export function savedSignatures(): SavedSignature[] {
  return signatures;
}

export function addSignature(name: string, strokes: [number, number][][], aspect: number): SavedSignature {
  const trimmed = name.trim();
  const sig: SavedSignature = {
    id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name: trimmed.length > 0 ? trimmed : `Signature ${signatures.length + 1}`,
    strokes,
    aspect,
    createdAt: new Date().toISOString(),
  };
  signatures = [...signatures, sig];
  persist();
  return sig;
}

export function removeSignature(id: string) {
  signatures = signatures.filter((s) => s.id !== id);
  persist();
}
