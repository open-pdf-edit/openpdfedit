// In-app replacements for `window.alert`/`confirm`/`prompt`.
//
// These are not a styling preference — the native ones do not work here.
// A WKWebView only shows JavaScript dialogs if the embedding app's
// `WKUIDelegate` implements the corresponding callback, and wry (the
// webview layer under Tauri) implements only the file-upload,
// media-permission and new-window callbacks — not
// `runJavaScriptTextInputPanelWithPrompt`. The practical result is that
// `window.prompt()` returns `null` *immediately*, with no dialog shown.
//
// That silently broke every tool that asked the user for a value: Edit
// Text, Move Image, Add Text Field, Add Checkbox and the page Crop op
// all located their target correctly and then aborted at the prompt, so
// they looked completely dead. `window.confirm()` is unreliable for the
// same reason, which also made Redact and annotation-delete no-ops.
//
// Everything here is a plain promise, so call sites read the same as the
// native calls they replace: `const name = await showPrompt(...)`.

type DialogKind = "alert" | "confirm" | "prompt";

interface DialogRequest {
  kind: DialogKind;
  title: string;
  message: string;
  defaultValue: string;
  placeholder: string;
  confirmLabel: string;
  destructive: boolean;
  /** Mask the input. A password typed into a plain field is visible to
   * anyone near the screen and can be captured by a screenshot. */
  masked: boolean;
}

// `null` when nothing is open. A single active dialog at a time is
// enough: these are all modal, user-blocking interactions.
let current = $state<DialogRequest | null>(null);
let resolver: ((value: string | null) => void) | null = null;

export function activeDialog(): DialogRequest | null {
  return current;
}

function open(request: DialogRequest): Promise<string | null> {
  // A dialog opening while another is somehow still pending resolves the
  // old one as cancelled rather than leaving its caller awaiting forever.
  resolver?.(null);
  current = request;
  return new Promise((resolve) => {
    resolver = resolve;
  });
}

/** Called by DialogHost: `null` cancels, a string (possibly empty) confirms. */
export function resolveDialog(value: string | null) {
  const resolve = resolver;
  resolver = null;
  current = null;
  resolve?.(value);
}

export function showAlert(message: string, title = "OpenPdfEdit"): Promise<void> {
  return open({
    kind: "alert",
    title,
    message,
    defaultValue: "",
    placeholder: "",
    confirmLabel: "OK",
    masked: false,
    destructive: false,
  }).then(() => undefined);
}

export async function showConfirm(
  message: string,
  options: { title?: string; confirmLabel?: string; destructive?: boolean } = {},
): Promise<boolean> {
  const result = await open({
    kind: "confirm",
    title: options.title ?? "Are you sure?",
    message,
    defaultValue: "",
    placeholder: "",
    confirmLabel: options.confirmLabel ?? "OK",
    masked: false,
    destructive: options.destructive ?? false,
  });
  return result !== null;
}

export async function showPrompt(
  message: string,
  options: {
    title?: string;
    defaultValue?: string;
    placeholder?: string;
    confirmLabel?: string;
    /** Mask what's typed — for passwords. */
    password?: boolean;
  } = {},
): Promise<string | null> {
  return open({
    kind: "prompt",
    title: options.title ?? "openpdfedit",
    message,
    defaultValue: options.defaultValue ?? "",
    placeholder: options.placeholder ?? "",
    confirmLabel: options.confirmLabel ?? "OK",
    masked: options.password ?? false,
    destructive: false,
  });
}
