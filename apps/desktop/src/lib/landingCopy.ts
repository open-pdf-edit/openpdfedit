/**
 * The web build's page copy, shown in the empty state (APP-96).
 *
 * apps/webapp/scripts/build.sh writes an <h1> and ~400 words into the
 * served index.html itself — `#landing-copy` — because a crawler that runs
 * no JavaScript otherwise finds an empty <div>. This moves that same
 * element into the empty state, under "Open PDF", where it reads as the
 * page it is; opening a document replaces the empty state and the copy
 * goes with it, then comes back when the document is closed.
 *
 * Moved rather than re-rendered, so what a person reads is exactly what a
 * crawler indexed. Never hidden with CSS: search engines discount text
 * they can see is hidden. For a UI language other than English the copy
 * is taken out of the page instead, since it is written in English and
 * would sit under a translated interface.
 *
 * The desktop app and the extension are built from the same source but
 * never have `#landing-copy` in their page, so this does nothing there.
 */
let kept: HTMLElement | null = null;

export function landingCopy(slot: HTMLElement, locale: string) {
  const copy = kept ?? document.getElementById("landing-copy");
  if (!copy) return;
  kept = copy;

  const place = (lang: string) => {
    if (lang.toLowerCase().startsWith("en")) slot.append(copy);
    else copy.remove();
  };
  place(locale);

  return {
    update: place,
    // The slot leaves the page with the empty state and takes the copy
    // with it; `kept` is what brings it back next time.
  };
}
