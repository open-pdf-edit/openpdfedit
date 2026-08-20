// Native `title` tooltips inherit the OS/browser's hover delay (WebView2
// and WKWebView both sit north of half a second before showing one),
// which reads as sluggish on a toolbar meant for quick scanning. This
// action shows a small custom tooltip immediately on hover instead.
//
// Positioned via `getBoundingClientRect` and appended to <body> (not the
// triggering element) so it's never clipped by a scrolling or
// `overflow: hidden` ancestor — the tool rail in particular.
export function tooltip(node: HTMLElement, label: string | undefined | null) {
  let el: HTMLDivElement | null = null;
  let currentLabel = label;

  function show() {
    if (!currentLabel) return;
    el = document.createElement("div");
    el.className = "oa-tooltip";
    el.textContent = currentLabel;
    el.setAttribute("role", "tooltip");
    document.body.appendChild(el);

    const rect = node.getBoundingClientRect();
    const tipRect = el.getBoundingClientRect();
    const margin = 8;

    // Default: to the right of the trigger, vertically centered — the
    // right layout for a left-edge vertical rail. Flips to the left if
    // that would run off the window's right edge.
    let left = rect.right + margin;
    if (left + tipRect.width > window.innerWidth - margin) {
      left = rect.left - tipRect.width - margin;
    }
    let top = rect.top + rect.height / 2 - tipRect.height / 2;
    top = Math.max(margin, Math.min(top, window.innerHeight - tipRect.height - margin));

    el.style.left = `${left}px`;
    el.style.top = `${top}px`;
    requestAnimationFrame(() => el?.classList.add("oa-tooltip--visible"));
  }

  function hide() {
    el?.remove();
    el = null;
  }

  node.addEventListener("mouseenter", show);
  node.addEventListener("mouseleave", hide);
  node.addEventListener("mousedown", hide);

  return {
    update(newLabel: string | undefined | null) {
      currentLabel = newLabel;
    },
    destroy() {
      node.removeEventListener("mouseenter", show);
      node.removeEventListener("mouseleave", hide);
      node.removeEventListener("mousedown", hide);
      hide();
    },
  };
}
