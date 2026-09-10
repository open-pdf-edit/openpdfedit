import { expect, test } from "./fixtures";

// APP-36 / APP-42 — the product ships in eight languages, and the picker
// is the control jerry's screenshots were asking for.
//
// Asserted on what a reader sees, not on the catalogue: the catalogues
// are already checked statically by i18n.spec.ts, and a translation that
// is present in a file but never reaches the screen is the failure that
// static check cannot see.
test.slow();

test("the interface changes language, and remembers the choice", async ({ page, extensionId }) => {
  await page.goto(`chrome-extension://${extensionId}/index.html`);

  const empty = page.locator(".empty-state");
  await expect(empty).toContainText("Open a PDF to get started.");

  await page.getByRole("button", { name: "Language" }).click();
  await page.getByRole("menuitemradio", { name: "简体中文" }).click();

  await expect(empty, "the empty state should be in Chinese").toContainText("打开一个 PDF 开始使用。");
  await expect(
    page.getByRole("button", { name: "打开 PDF…" }).first(),
    "the topbar should follow too, not just the rail",
  ).toBeVisible();

  // `lang` is not decoration: it picks the right glyphs for Han
  // characters, which are drawn differently in Chinese and Japanese.
  await expect(page.locator("html")).toHaveAttribute("lang", "zh-Hans");

  // A picker that forgets is not a picker.
  await page.reload();
  await expect(page.locator(".empty-state"), "the choice must survive a reload")
    .toContainText("打开一个 PDF 开始使用。");
});

test("every offered language actually renders", async ({ page, extensionId }) => {
  // Guards the case where a locale is listed but its catalogue never
  // reaches the screen — the picker would look complete and do nothing.
  await page.goto(`chrome-extension://${extensionId}/index.html`);
  // The empty-state sentence, which is distinct in all eight — "Abrir
  // PDF…" would not distinguish Spanish from Portuguese.
  const expected: Record<string, string> = {
    "简体中文": "打开一个 PDF 开始使用。",
    "繁體中文": "開啟一個 PDF 開始使用。",
    "日本語": "PDF を開いて始めましょう。",
    "한국어": "PDF를 열어 시작하세요.",
    "Deutsch": "Öffnen Sie ein PDF, um zu beginnen.",
    "Español": "Abre un PDF para empezar.",
    "Português": "Abra um PDF para começar.",
    "English": "Open a PDF to get started.",
  };
  for (const [language, word] of Object.entries(expected)) {
    await page.getByRole("button", { name: /Language|语言|語言|言語|언어|Sprache|Idioma/ }).click();
    await page.getByRole("menuitemradio", { name: language, exact: true }).click();
    await expect(
      page.locator(".empty-state"),
      `${language} should render its own empty state`,
    ).toContainText(word);
  }
});
