// Things about the marketing site that are only wrong to the eye.
//
// The button labels sat 6px below the top of their pill and 14px above
// the bottom, because `.way-cta` set `padding-top:4px` — read as "space
// above the button", actually replacing the button's own top padding,
// since `.btn` is inline-flex. It shipped, and was found by someone
// looking at the page rather than by anything here.
//
// Measured rather than eyeballed: a range over the label's own text
// gives the glyph box, and the gap above it should match the gap below.
import { join } from "node:path";

import { expect, test } from "./fixtures";

const PAGE = `file://${join(process.cwd(), "..", "..", "site", "index.html")}`;

test("every button's label is centred in its pill", async ({ browser }) => {
  const ctx = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await ctx.newPage();
  await page.goto(PAGE);
  await page.evaluate(() => document.fonts.ready);

  const buttons = await page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>(".btn")].map((el) => {
      const box = el.getBoundingClientRect();
      const range = document.createRange();
      range.selectNodeContents(el);
      const text = range.getBoundingClientRect();
      return {
        label: (el.textContent ?? "").trim(),
        above: text.top - box.top,
        below: box.bottom - text.bottom,
      };
    }),
  );

  expect(buttons.length, "the page should have buttons to check").toBeGreaterThanOrEqual(5);
  for (const { label, above, below } of buttons) {
    // A pixel of slack: a font's ascender and descender are not
    // symmetrical, so the glyph box never centres exactly and chasing
    // that would make this test fail on a font update rather than on a
    // mistake. Four points of drift is a mistake.
    expect(
      Math.abs(above - below),
      `"${label}" sits ${above.toFixed(1)}px from the top and ${below.toFixed(1)}px from the bottom`,
    ).toBeLessThanOrEqual(1);
  }

  await ctx.close();
});
