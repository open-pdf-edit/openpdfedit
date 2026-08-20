// Post-processes dist/index.html after scripts/build-spa.sh has copied
// apps/desktop's SvelteKit adapter-static SPA build into dist/: MV3's
// `extension_pages` CSP (manifest.json: `script-src 'self'
// 'wasm-unsafe-eval'`) has no `'unsafe-inline'`, which forbids inline
// <script> *content* outright — only externally-sourced (`src=...`)
// same-origin scripts are allowed. SvelteKit's adapter-static output
// always has exactly one inline bootstrap <script> in index.html (no
// `src` attribute) that dynamically imports the app's entry chunk(s) and
// calls `kit.start(...)`; under this CSP, Chrome would silently refuse to
// run it and the extension page would just never boot, with nothing more
// than a CSP violation line in the devtools console to explain why.
//
// Checked whether @sveltejs/kit's `kit.output.bundleStrategy` config
// (added 2.13.0) sidesteps this before reaching for a post-process step,
// per task-9-brief.md point 4's preference — it doesn't, for either
// non-default value: `'inline'` makes things *worse* for this CSP (it
// inlines the whole app's JS/CSS into index.html too, on top of the
// bootstrap), and `'single'` still leaves the exact same shape of inline
// bootstrap script, just pointing `import()` at one merged bundle file
// instead of two separate entry chunks — confirmed empirically by
// building both ways during this task (see task-9-report.md) rather than
// assumed from the option's one-line doc comment. So this file is the
// fallback the brief anticipated needing, not a shortcut around checking
// first.
//
// This moves each inline <script>'s body out to its own dist/inline-N.js
// file and rewrites the tag to reference it via `src`, preserving
// execution order (SvelteKit's output has exactly one such tag today, but
// this handles more than one just in case a future SvelteKit version
// splits it up). Deliberately does NOT add `type="module"` to the
// rewritten tag: the bootstrap script's body assigns to
// `__sveltekit_<hash> = {...}` with no `var`/`let`/`const` — a bare
// identifier assignment that only works as an accidental global create in
// non-strict ("sloppy mode") script execution. Modules are always strict
// mode, where the same assignment throws `ReferenceError: assignment to
// undeclared variable` instead of creating the global SvelteKit's own
// runtime (`kit.start`) expects to find — confirmed by reading the
// generated bootstrap body itself, not assumed; an earlier draft of this
// script added `type="module"` by default (the natural choice for
// externalizing a script that itself uses dynamic `import()`) and this
// would have broken boot silently the same way the CSP violation it's
// fixing does. A plain classic `<script src="...">` preserves the
// original inline script's exact execution semantics (sloppy mode, global
// scope) — dynamic `import()` works the same in classic scripts as in
// modules, and every specifier the bootstrap passes it is already an
// absolute root-relative path (e.g. `/_app/immutable/entry/app.*.js`), so
// the module-vs-classic difference in how *relative* specifiers resolve
// never comes into play here either.
//
// Same class of packaging fix openscreenshot's copy-static.mjs precedent
// establishes for other Vite-default vs. MV3-rule mismatches in this
// workspace (apps/extension/scripts/copy-static.mjs there flattens
// nested HTML-entry output paths; this one un-inlines a script CSP
// forbids) — a small, targeted Node post-process step run after the
// relevant build, not a build-tool plugin.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const extDir = dirname(scriptDir);
const distDir = join(extDir, "dist");
const indexPath = join(distDir, "index.html");

const html = readFileSync(indexPath, "utf8");

// Matches a <script ...>...</script> pair whose opening tag has no `src`
// attribute anywhere in it — i.e. one whose content is inline JS, not a
// reference to an external file. The `(?![^>]*\ssrc=)` lookahead scans
// the rest of the opening tag for a `\ssrc=` (whitespace then `src=`,
// so it doesn't also match e.g. a hypothetical `data-src=` attribute)
// before allowing the match to proceed.
const inlineScriptRe = /<script(?![^>]*\ssrc=)([^>]*)>([\s\S]*?)<\/script>/gi;

let n = 0;
const written = [];
const rewritten = html.replace(inlineScriptRe, (whole, attrs, body) => {
  if (!body.trim()) {
    // An attribute-only/empty script tag (e.g. a hypothetical
    // `<script src="...">` this regex's negative lookahead already
    // excludes, or a genuinely empty inline tag) — nothing to
    // externalize, leave it exactly as-is rather than emit a pointless
    // empty dist/inline-N.js.
    return whole;
  }
  const fileName = `inline-${n}.js`;
  n++;
  written.push({ fileName, body });
  const attrsTrimmed = attrs.trim();
  return `<script${attrsTrimmed ? " " + attrsTrimmed : ""} src="./${fileName}"></script>`;
});

if (written.length === 0) {
  console.log(
    "externalize-inline.mjs: no inline <script> tags found in dist/index.html — nothing to externalize (unexpected for a SvelteKit adapter-static build; verify the SPA build actually ran)",
  );
} else {
  for (const { fileName, body } of written) {
    writeFileSync(join(distDir, fileName), body, "utf8");
  }
  writeFileSync(indexPath, rewritten, "utf8");
  console.log(
    `externalize-inline.mjs: externalized ${written.length} inline <script> tag(s) into ${written
      .map((w) => w.fileName)
      .join(", ")} — dist/index.html now has zero inline script content, satisfying manifest.json's CSP`,
  );
}
