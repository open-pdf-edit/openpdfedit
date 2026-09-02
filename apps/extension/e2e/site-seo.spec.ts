// The marketing site's machine-readable half.
//
// Every claim here failed silently before it was checked: the page
// declared `summary_large_image` and pointed at no image, so every share
// of it unfurled as a blank rectangle, and nobody sees their own
// link previews. Structured data has the same shape of problem — it is
// invisible to the person editing the page, so it goes stale the first
// time an answer is reworded and stays stale.
//
// Read off the files rather than a running server: this is a static
// site, and the assertions are about what gets deployed.
import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixtures";

const SITE = join(process.cwd(), "..", "..", "site");
const read = (name: string) => readFileSync(join(SITE, name), "utf8");

/** Tag-stripped, entity-resolved, whitespace-collapsed — the same
 *  normalisation the JSON-LD generator applies. */
function textOf(fragment: string): string {
  return fragment
    .replace(/<[^>]+>/g, "")
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&nbsp;/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function jsonLd(html: string): Record<string, unknown>[] {
  const match = html.match(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/);
  expect(match, "index.html has no JSON-LD block").not.toBeNull();
  const parsed = JSON.parse(match![1]) as { "@graph": Record<string, unknown>[] };
  return parsed["@graph"];
}

test("the social card points at an image that exists, at the size it claims", () => {
  const html = read("index.html");

  // The original bug: this card type shows a 1200x630 image, and there
  // was none, so every link to the site previewed blank.
  expect(html).toContain('name="twitter:card" content="summary_large_image"');
  expect(html).toContain('property="og:image" content="https://openpdfedit.com/og.png"');
  expect(html).toContain('name="twitter:image" content="https://openpdfedit.com/og.png"');

  const png = readFileSync(join(SITE, "og.png"));
  expect(png.subarray(0, 8)).toEqual(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
  // IHDR width/height, big-endian, straight after the 8-byte signature
  // and the 8-byte chunk header.
  expect(png.readUInt32BE(16), "og.png width").toBe(1200);
  expect(png.readUInt32BE(20), "og.png height").toBe(630);
  expect(html).toContain('property="og:image:width" content="1200"');
  expect(html).toContain('property="og:image:height" content="630"');
});

// The same class of bug as the blank social card, one line up in the
// browser: the site declared no icon at all and shipped no icon file,
// so every tab showed the generic blank-page glyph. Nobody looks at
// their own favicon either.
test("every page declares an icon, and the icon files are there", () => {
  const png = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

  for (const page of ["index.html", "privacy.html"]) {
    const html = read(page);
    expect(html, `${page} declares no favicon`).toContain('rel="icon" href="/favicon.ico"');
    expect(html, `${page} declares no PNG icon`).toContain('href="/favicon.png"');
    expect(html, `${page} declares no apple-touch-icon`).toContain(
      'rel="apple-touch-icon" href="/apple-touch-icon.png"',
    );
  }

  // A declared icon that 404s is worse than none — the browser retries
  // it on every page load.
  expect(statSync(join(SITE, "favicon.ico")).size).toBeGreaterThan(0);

  const favicon = readFileSync(join(SITE, "favicon.png"));
  expect(favicon.subarray(0, 8)).toEqual(Buffer.from(png));
  expect(favicon.readUInt32BE(16), "favicon.png width").toBe(32);

  const apple = readFileSync(join(SITE, "apple-touch-icon.png"));
  expect(apple.subarray(0, 8)).toEqual(Buffer.from(png));
  // 180 is what iOS asks for; anything smaller is upscaled on the home
  // screen.
  expect(apple.readUInt32BE(16), "apple-touch-icon.png width").toBe(180);
});

test("the structured data still says what the page says", () => {
  const html = read("index.html");
  const graph = jsonLd(html);

  expect(graph.map((n) => n["@type"])).toEqual([
    "Organization",
    "WebSite",
    "SoftwareApplication",
    "FAQPage",
  ]);

  // The drift guard. Two copies of every answer exist — one a person
  // reads, one a machine quotes — and the second is worse than useless
  // once it stops matching the first, because it is the copy that gets
  // repeated as fact. Regenerate with site/scripts/build-jsonld.py.
  const visible = [...html.matchAll(/<h3 class="faq-q">([\s\S]*?)<\/h3>\s*<p class="faq-a">([\s\S]*?)<\/p>/g)].map(
    (m) => ({ q: textOf(m[1]), a: textOf(m[2]) }),
  );
  expect(visible.length, "the page should have an FAQ").toBeGreaterThanOrEqual(8);

  const faq = graph.find((n) => n["@type"] === "FAQPage") as {
    mainEntity: { name: string; acceptedAnswer: { text: string } }[];
  };
  expect(faq.mainEntity.map((e) => ({ q: e.name, a: e.acceptedAnswer.text }))).toEqual(visible);

  const app = graph.find((n) => n["@type"] === "SoftwareApplication") as {
    softwareVersion: string;
    featureList: string[];
    offers: { name: string }[];
  };
  const pkg = JSON.parse(
    readFileSync(join(process.cwd(), "..", "desktop", "package.json"), "utf8"),
  ) as { version: string };
  expect(app.softwareVersion, "the declared version drifts from the app's").toBe(pkg.version);

  // "Free" alone would be a lie by omission once an answer engine
  // repeats it flatly, so both offers have to survive.
  expect(app.offers.map((o) => o.name)).toContain("Supporter tools");
  // The badge markup must not end up in a feature name.
  expect(app.featureList).toContain("OCR");
  expect(app.featureList.filter((f) => f.includes("Supporter"))).toEqual([]);
});

test("crawlers are told where to go, including the ones that answer in prose", () => {
  const robots = read("robots.txt");

  // Google-Extended is a separate opt-in from ordinary Search: a site
  // can rank perfectly and still be invisible to the thing summarising
  // it, which is the half this product is most often asked about.
  for (const agent of ["GPTBot", "ClaudeBot", "PerplexityBot", "Google-Extended", "OAI-SearchBot"]) {
    expect(robots, `${agent} is not addressed in robots.txt`).toContain(`User-agent: ${agent}`);
  }
  expect(robots).toContain("Sitemap: https://openpdfedit.com/sitemap.xml");

  const sitemap = read("sitemap.xml");
  const locs = [...sitemap.matchAll(/<loc>(.*?)<\/loc>/g)].map((m) => m[1]);
  expect(locs.length).toBeGreaterThan(0);
  // A sitemap may only list URLs on the host that serves it. The app
  // subdomain used to be in here, which makes a validator distrust the
  // whole file rather than index the stray URL.
  for (const loc of locs) {
    expect(new URL(loc).host, `${loc} is not on this sitemap's host`).toBe("openpdfedit.com");
  }
  for (const loc of locs) {
    expect(sitemap, `${loc} has no <lastmod>`).toMatch(/<lastmod>\d{4}-\d{2}-\d{2}<\/lastmod>/);
  }

  expect(read("llms.txt")).toContain("# OpenPdfEdit");
  expect(statSync(join(SITE, "llms.txt")).size).toBeGreaterThan(500);
});

test("every page declares a canonical and is allowed to be indexed", () => {
  for (const page of ["index.html", "privacy.html"]) {
    const html = read(page);
    expect(html, `${page} has no canonical`).toMatch(/<link rel="canonical" href="https:\/\/openpdfedit\.com\//);
    expect(html, `${page} has no robots directive`).toContain('name="robots"');
    expect(html, `${page} is excluded from indexing`).not.toMatch(/name="robots"[^>]*noindex/);
    // Without this a result gets a thumbnail rather than the card.
    expect(html, `${page} caps its image preview`).toContain("max-image-preview:large");
  }
});

test("the app host is one indexable page, not unlimited copies of one", () => {
  // Every path on app.openpdfedit.com answers with the same HTML and a
  // 200 — that is what makes a single-page app work, and it also means
  // a crawler that guesses a URL is told the page exists. The canonical
  // is what collapses them back into one.
  const dist = join(process.cwd(), "..", "webapp", "dist");
  const html = readFileSync(join(dist, "index.html"), "utf8");
  expect(html).toContain('<link rel="canonical" href="https://app.openpdfedit.com/">');
  expect(html).toContain('<meta name="description"');
  expect(html).toContain('name="robots"');

  // Served as files, so a request for them cannot fall through to the
  // SPA and answer with the app's HTML.
  const robots = readFileSync(join(dist, "robots.txt"), "utf8");
  expect(robots).toContain("Sitemap: https://app.openpdfedit.com/sitemap.xml");
  expect(robots).toContain("Disallow: /wasm-gen/");
  expect(readFileSync(join(dist, "sitemap.xml"), "utf8")).toContain(
    "<loc>https://app.openpdfedit.com/</loc>",
  );

  // The same SPA is built for a chrome-extension:// origin, where a
  // canonical pointing at a website would be wrong.
  const ext = readFileSync(join(process.cwd(), "dist", "index.html"), "utf8");
  expect(ext, "the extension build must not carry the web app's canonical").not.toContain("canonical");
});

test("the privacy policy answers the questions a policy has to answer", () => {
  const html = read("privacy.html");
  const headings = [...html.matchAll(/<h2[^>]*>([\s\S]*?)<\/h2>/g)].map((m) => textOf(m[1]));

  // It used to be an essay — accurate, well written, and organised
  // around what the author found interesting rather than around what a
  // reader, a store reviewer or a procurement form comes looking for.
  // Each of these is a question someone arrives with.
  for (const section of [
    "What is never collected",
    "What is stored on your device",
    "Payments",
    "Permissions",
    "Third parties",
    "Deleting your data",
    "Children",
    "Changes to this policy",
    "Contact",
  ]) {
    expect(headings, `the policy has no "${section}" section`).toContain(section);
  }

  // A policy with no reachable contact cannot honour a deletion
  // request, which is the one thing it promises to be able to do.
  expect(html, "no contact address").toMatch(/mailto:[^"']+@[^"']+/);

  // Dated, so a reader can tell whether it predates the thing they are
  // asking about.
  expect(html, "no last-updated date").toMatch(/Last updated \d{1,2} \w+ \d{4}/);
});

test("the policy's list of what is stored matches what the app stores", () => {
  const html = read("privacy.html");
  const stored = html.slice(html.indexOf("What is stored on your device"));

  // The claim was "the web app stores one thing in your browser: your
  // sign-in session" — true when written, false the moment the recent
  // documents list landed. A privacy policy going quietly out of date
  // as features arrive is the failure mode worth catching, so every
  // storage key in the app has to be accounted for here.
  const keyed: [string, RegExp][] = [
    ["openpdfedit.recents", /recently opened/i],
    ["openpdfedit.signatures", /signatures/i],
    ["openpdfedit.markdown.vault", /folder you chose/i],
    ["openpdfedit.ocr.lang", /OCR language/i],
    ["openapps.session", /sign-in session/i],
  ];

  const src = ["lib/recents.ts", "lib/signatures.svelte.ts", "routes/+page.svelte", "lib/openapps.ts"]
    .map((f) => readFileSync(join(process.cwd(), "..", "desktop", "src", f), "utf8"))
    .join("\n");

  for (const [key, described] of keyed) {
    expect(src, `${key} is no longer in the app — is the policy stale the other way?`).toContain(key);
    expect(stored, `the policy does not mention what ${key} stores`).toMatch(described);
  }
});
