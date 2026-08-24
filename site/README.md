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

- **Links are live**, pointing at the real repository and its releases.
  The only thing still missing is the Chrome Web Store listing URL — the
  extension currently links to the GitHub release (see
  `apps/extension/STORE.md`).
- **The tool list is the contract.** Every entry under "Every tool"
  corresponds to something the app actually does today; the page
  deliberately claims no more than that. When a feature lands or is
  removed, update the directory in the same change — a marketing page
  that overstates the product is worse than one that undersells it, and
  this one is also the closest thing to a feature list users have.
