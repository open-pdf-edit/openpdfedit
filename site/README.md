# OpenPdfEdit marketing site

A self-contained static page following the OpenApps design system
(dark-first marketing grammar, Geist type, OpenPdfEdit red accent).
No build step, no external requests — fonts are vendored in `fonts/`.

## Preview

Open `index.html` directly, or serve the folder:

```sh
python3 -m http.server -d site 8080
```

Deployed to `openpdfedit.com`; see `docs/PRODUCTION.md`.

## Before publishing

- **Links are live**, pointing at the real repository, its releases, and
  the web app at `app.openpdfedit.com`. The only thing still missing is
  the Chrome Web Store listing URL — the extension currently links to
  the GitHub release (see `apps/extension/STORE.md`).
- **Paid tools say so.** The watermark entry carries a Supporter tag and
  names the price. A page that lists a tool without mentioning it costs
  money is worse than one that never listed it — the user finds out at
  the moment they try to use it.
- **The tool list is the contract.** Every entry under "Every tool"
  corresponds to something the app actually does today; the page
  deliberately claims no more than that. When a feature lands or is
  removed, update the directory in the same change — a marketing page
  that overstates the product is worse than one that undersells it, and
  this one is also the closest thing to a feature list users have.
