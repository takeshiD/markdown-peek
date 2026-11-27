// R5: Ctrl+K command palette — files, headings, modes, theme, re-select.
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

test("Ctrl+K filters and runs file, heading, mode, theme and re-select commands", async ({ page, server }) => {
  let adaptiveCalls = 0;
  page.on("request", (r) => r.url().endsWith("/api/adaptive") && adaptiveCalls++);
  await openApp(page, server);

  await run(page, "ロールバック", "ロールバック");
  await expect(page.locator("article h2", { hasText: "ロールバック" })).toBeInViewport();

  await run(page, "adaptive mode", "Adaptive mode");
  await expect(page.getByRole("radio", { name: "Adaptive mode" })).toHaveAttribute("aria-checked", "true");

  await run(page, "dark theme", "Dark theme");
  await expect(page.locator("html")).toHaveClass(/dark/);

  const before = adaptiveCalls;
  await run(page, "choose reading aids", "Choose reading aids again");
  await expect.poll(() => adaptiveCalls).toBeGreaterThan(before);
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");

  await run(page, "notes.md", "notes.md");
  await expect(page.getByTestId("breadcrumb-file")).toContainText("notes.md");
  await expect(page.locator("article h1")).toHaveText(/読書メモ/);

  // Esc closes without running; focus returns to the page.
  await page.keyboard.press("Control+k");
  await page.getByTestId("palette-input").fill("split");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("radio", { name: "Adaptive mode" })).toHaveAttribute("aria-checked", "true");
});

test("the top-bar button opens the same palette; arrow keys pick an item", async ({ page, server }) => {
  await openApp(page, server);
  await page.locator("body").press("1");
  await page.getByTestId("open-palette").click();
  await page.getByTestId("palette-input").fill("mode");
  await page.keyboard.press("ArrowDown"); // Read → Adaptive
  await page.keyboard.press("Enter");
  await expect(page.getByRole("radio", { name: "Adaptive mode" })).toHaveAttribute("aria-checked", "true");
});

test("partial queries run the intended command and never pin or hide by accident", async ({ page, server }) => {
  let adaptiveCalls = 0;
  page.on("request", (r) => r.url().endsWith("/api/adaptive") && adaptiveCalls++);
  await openApp(page, server);
  const docPrefs = () =>
    page.evaluate(() => JSON.stringify(JSON.parse(localStorage.getItem("mdpeek.ui.v1") ?? "{}").docs ?? {}));
  const top = async (query: string) => {
    await page.keyboard.press("Control+k");
    await page.getByTestId("palette-input").fill(query);
    return page.getByRole("dialog", { name: "Command palette" }).locator('[cmdk-item][data-selected="true"]');
  };

  // "again" → Choose reading aids again.
  let sel = await top("again");
  await expect(sel).toHaveText(/Choose reading aids again/);
  const before = adaptiveCalls;
  await page.keyboard.press("Enter");
  await expect.poll(() => adaptiveCalls).toBeGreaterThan(before);
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
