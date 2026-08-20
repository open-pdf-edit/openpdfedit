# OpenPdfEdit marketing site

A self-contained static page following the OpenApps design system
(dark-first marketing grammar, Geist type, OpenPdfEdit red accent).
No build step, no external requests — fonts are vendored in `fonts/`.

## Preview

Open `index.html` directly, or serve the folder:

```sh
python3 -m http.server -d site 8080
```

## Before publishing

- **Download links are placeholders.** The two CTA buttons and the footer
  point at `github.com/openapps/openapps` — replace with the real
  repository/releases URLs (and the Chrome Web Store listing URL once the
  extension is published; see `apps/extension/STORE.md`).
- **The hero and pillar visuals are CSS-drawn mockups**, deliberately
  abstract so the page ships without screenshots. Swapping in real
  screenshots later: replace the `.hero-shot` / `.p-visual` contents with
  an `<img>`; the frames already carry the border/radius/shadow.
- Facts baked into the copy (keep them true): extension zip ~5 MB,
  Chrome 103+ minimum, v0.1.0, MIT OR Apache-2.0, OCR desktop-only,
  no Firefox yet.
