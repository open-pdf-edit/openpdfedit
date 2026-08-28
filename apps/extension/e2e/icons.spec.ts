// Every icon a button asks for has to exist.
//
// `Icon` builds a CSS mask URL from the name it is given, so a name with
// no file behind it is not an error anywhere — it is a button with an
// invisible glyph and a working click target. Nothing in a build,
// typecheck or unit test notices, and the app looks slightly broken to
// whoever happens to open that panel.
//
// Read off the source rather than the DOM so it covers buttons behind a
// document, a panel or a paid gate, which a rendered page would not.
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test } from "./fixtures";

const DESKTOP = join(process.cwd(), "..", "desktop");

function sourceFiles(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === "node_modules" || entry.name.startsWith(".")) continue;
    const path = join(dir, entry.name);
    if (entry.isDirectory()) sourceFiles(path, out);
    else if (/\.(svelte|ts)$/.test(entry.name)) out.push(path);
  }
  return out;
}

test("every icon named in the app is vendored", () => {
  const available = new Set(
    readdirSync(join(DESKTOP, "static", "icons"))
      .filter((f) => f.endsWith(".svg"))
      .map((f) => f.replace(/\.svg$/, "")),
  );
  expect(available.size, "no icons found — has static/icons moved?").toBeGreaterThan(20);

  const wanted = new Map<string, string>();
  for (const file of sourceFiles(join(DESKTOP, "src"))) {
    const source = readFileSync(file, "utf8");
    // <Icon name="x" …>. Dynamic names (`name={tool.icon}`) are covered
    // by the tool table below, which is where they all come from.
    for (const m of source.matchAll(/<Icon\s[^>]*name="([a-z0-9-]+)"/g)) {
      wanted.set(m[1], file);
    }
    // The rail's tools name theirs in a table rather than in markup.
    if (file.endsWith("tools.ts")) {
      for (const m of source.matchAll(/icon:\s*"([a-z0-9-]+)"/g)) wanted.set(m[1], file);
    }
  }

  expect(wanted.size, "no icon names found — has the markup changed shape?").toBeGreaterThan(20);

  const missing = [...wanted]
    .filter(([name]) => !available.has(name))
    .map(([name, file]) => `${name} (${file.slice(file.indexOf("/src/") + 1)})`);
  expect(missing, "these render as an invisible glyph on a working button").toEqual([]);
});
