// R5: Ctrl+K command palette — files, headings, aids toggle, models, theme.
import { test, expect, openApp } from "./server";
import type { Page } from "@playwright/test";

async function run(page: Page, query: string, item: RegExp | string) {
  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog", { name: "Command palette" });
  await expect(dialog).toBeVisible();
  await page.getByTestId("palette-input").fill(query);
  await expect(dialog.getByRole("option").first()).toContainText(item);
  await page.keyboard.press("Enter");
  await expect(dialog).toHaveCount(0);
}

test("Ctrl+K filters and runs file, heading, aids, model, theme commands", async ({ page, server }) => {
  await openApp(page, server);

  await run(page, "ロールバック", "ロールバック");
  await expect(page.locator("article h2", { hasText: "ロールバック" })).toBeInViewport();

  await run(page, "genui off", "Turn GenUI off");
  await expect(page.getByTestId("aids-toggle")).toHaveAttribute("aria-pressed", "false");
  await run(page, "genui on", "Turn GenUI on");
  await expect(page.getByTestId("aids-toggle")).toHaveAttribute("aria-pressed", "true");

  await run(page, "dark theme", "Dark theme");
  await expect(page.locator("html")).toHaveClass(/dark/);

  await run(page, "use model", "Use model fake-alt");
  await expect(page.getByTestId("model-menu")).toContainText("fake-alt");
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");
  await run(page, "default model", "Make fake-alt the default model");
  await expect(page.getByTestId("make-default")).toHaveCount(0);

  await run(page, "notes.md", "notes.md");
  await expect(page.getByTestId("breadcrumb-file")).toContainText("notes.md");
  await expect(page.locator("article h1")).toHaveText(/読書メモ/);

  // Esc closes without running; focus returns to the page.
  await page.keyboard.press("Control+k");
  await page.getByTestId("palette-input").fill("aids");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByTestId("aids-toggle")).toHaveAttribute("aria-pressed", "true");
});

test("the top-bar button opens the same palette; arrow keys pick an item", async ({ page, server }) => {
  await openApp(page, server);
  await page.locator("body").press("a");
  await page.getByTestId("open-palette").click();
  await page.getByTestId("palette-input").fill("theme");
  await page.keyboard.press("ArrowDown"); // Light → Dark
  await page.keyboard.press("Enter");
  await expect(page.locator("html")).toHaveClass(/dark/);
});

test("partial queries run the intended command and never pin or hide by accident", async ({ page, server }) => {
  await openApp(page, server);
  const docPrefs = () =>
    page.evaluate(() => JSON.stringify(JSON.parse(localStorage.getItem("mdpeek.ui.v1") ?? "{}").docs ?? {}));
  const top = async (query: string) => {
    await page.keyboard.press("Control+k");
    await page.getByTestId("palette-input").fill(query);
    return page.getByRole("dialog", { name: "Command palette" }).locator('[cmdk-item][data-selected="true"]');
  };

  // "alt" → Use model fake-alt.
  let sel = await top("alt");
  await expect(sel).toHaveText(/Use model fake-alt/);
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-provider", "fake-alt");
  await expect(page.locator("article [data-widget]").first()).toBeVisible();

  // A partial heading.
  sel = await top("ロール");
  await expect(sel).toHaveText(/ロールバック/);
  await page.keyboard.press("Enter");
  await expect(page.locator("article h2", { hasText: "ロールバック" })).toBeInViewport();

  // "dark" → Dark theme.
  sel = await top("dark");
  await expect(sel).toHaveText(/Dark theme/);
  await page.keyboard.press("Escape");

  // "notes" → notes.md, and nothing gets pinned or hidden.
  sel = await top("notes");
  await expect(sel).toHaveText(/notes\.md/);
  await page.keyboard.press("Enter");
  await expect(page.locator("article h1")).toHaveText(/読書メモ/);
  expect(await docPrefs()).not.toMatch(/"(pinned|hidden)":\["/);

  // Scattered letters alone never select a pin / hide / show action.
  await page.getByRole("button", { name: "deploy-guide.md", exact: true }).click();
  await expect(page.locator('article [data-kind="StepNavigator"]')).toBeVisible();
  sel = await top("nts");
  await expect(sel).not.toHaveText(/^(Pin|Unpin|Hide|Show) /);
  await page.keyboard.press("Escape");
  expect(await docPrefs()).not.toMatch(/"(pinned|hidden)":\["/);
});
