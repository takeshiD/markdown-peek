// R1 (shell, body first), R2 (modes), R3 (resizable / collapsible panes).
import { test, expect, openApp } from "./server";

test.describe("R1 shell", () => {
  // Hold back the server's startup WebSocket push too, so "selecting" is observable.
  test.use({ serverOptions: { debounceMs: 60_000 } });
  test("top bar, rail, body and in-text aids render; body does not wait for selection", async ({ page, server }) => {
    // Hold /api/adaptive back: the document must already be readable.
    let release!: () => void;
    const held = new Promise<void>((r) => (release = r));
    await page.route("**/api/adaptive", async (route) => {
      await held;
      await route.continue();
    });
    await page.goto(server.url);
    await expect(page.locator("article.markdown-body h1")).toHaveText(/決済サービス デプロイ手順/);
    await expect(page.locator("article.markdown-body table")).toBeVisible();
    await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "selecting");

    const banner = page.getByRole("banner");
    await expect(banner.getByRole("navigation", { name: "Breadcrumb" })).toContainText("deploy-guide.md");
    await expect(banner.getByRole("radio", { name: "Read mode" })).toBeVisible();
    await expect(banner.getByRole("radio", { name: "Split mode" })).toHaveCount(0);
    await expect(banner.getByRole("radio", { name: "Adaptive mode" })).toBeVisible();
    await expect(banner.getByRole("button", { name: /Commands/ })).toBeVisible();
    await expect(banner.getByRole("button", { name: /^Theme:/ })).toBeVisible();
    const rail = page.getByRole("complementary", { name: "Files and outline" });
    await expect(rail.getByRole("tab", { name: "Files" })).toBeVisible();
    await expect(rail.getByRole("tab", { name: "Outline" })).toBeVisible();
    // No side pane: reading aids live inside the document.
    await expect(page.getByRole("complementary", { name: "Reading aids" })).toHaveCount(0);

    release();
    await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");
    await expect(page.locator("article [data-widget]").first()).toBeVisible();
  });

  test("theme menu switches light / dark / system and persists", async ({ page, server }) => {
    const pick = async (name: string) => {
      await page.getByTestId("theme-menu").click();
      await page.getByRole("menuitemradio", { name }).click();
      await expect(page.getByRole("menu")).toHaveCount(0);
    };
    await openApp(page, server);
    await pick("Dark");
    await expect(page.locator("html")).toHaveClass(/dark/);
    await page.reload();
    await openApp(page, server);
    await expect(page.locator("html")).toHaveClass(/dark/);
    await pick("Light");
    await expect(page.locator("html")).not.toHaveClass(/dark/);
    await page.emulateMedia({ colorScheme: "dark" });
    await pick("System");
    await expect(page.locator("html")).toHaveClass(/dark/);
    await page.emulateMedia({ colorScheme: "light" });
    await expect(page.locator("html")).not.toHaveClass(/dark/);
  });
});

test.describe("R2 modes", () => {
  test("keys 1/2 switch Read / Adaptive, the rail stays, and the mode survives reload", async ({ page, server }) => {
    await openApp(page, server);
    // Adaptive (default): aids inside the article, the rail beside it.
    const steps = page.locator('article .mp-note-slot [data-kind="StepNavigator"]');
    await expect(steps).toBeVisible();
    await expect(page.getByTestId("rail")).toBeVisible();

    await page.locator("body").press("1");
    await expect(page.locator("[data-widget]")).toHaveCount(0);
    await expect(page.getByTestId("rail")).toBeVisible();
    await expect(page.getByRole("radio", { name: "Read mode" })).toHaveAttribute("aria-checked", "true");
    await page.reload();
    await expect(page.getByRole("radio", { name: "Read mode" })).toHaveAttribute("aria-checked", "true");
    await expect(page.locator("article h1")).toBeVisible();
    await expect(page.locator("[data-widget]")).toHaveCount(0);

    await page.locator("body").press("2");
    await expect(steps).toBeVisible();
    await page.reload();
    await expect(page.getByRole("radio", { name: "Adaptive mode" })).toHaveAttribute("aria-checked", "true");
    await expect(steps).toBeVisible();
  });

  test("a stored Split mode from an older version opens as Adaptive", async ({ page, server }) => {
    await page.addInitScript(() => localStorage.setItem("mdpeek.ui.v1", JSON.stringify({ mode: "split", widgetsSize: 400, groupBy: "branch" })));
    await openApp(page, server);
    await expect(page.getByRole("radio", { name: "Adaptive mode" })).toHaveAttribute("aria-checked", "true");
    await expect(page.locator('article [data-kind="StepNavigator"]')).toBeVisible();
  });

  test("each aid sits in the text right after the lines it was built from, at any width", async ({ page, server }) => {
    await openApp(page, server);
    const order = () =>
      page.evaluate(() => {
        const kids = Array.from(document.querySelector("article")!.children) as HTMLElement[];
        const slot = kids.findIndex((k) => k.querySelector('[data-kind="StepNavigator"]'));
        const list = kids.findIndex((k, i) => k.tagName === "OL" && kids.slice(0, i).some((h) => h.tagName === "H2" && h.textContent?.includes("手順")));
        return { slot, list };
      });
    // The steps aid follows the ordered list it was built from.
    let o = await order();
    expect(o.list).toBeGreaterThan(0);
    expect(o.slot).toBe(o.list + 1);
    const article = (await page.locator("article").boundingBox())!;
    const aid = (await page.locator('article [data-kind="StepNavigator"]').boundingBox())!;
    expect(aid.x).toBeGreaterThanOrEqual(article.x - 1);
    expect(aid.x + aid.width).toBeLessThanOrEqual(article.x + article.width + 1); // within the text measure
    await page.setViewportSize({ width: 900, height: 900 });
    o = await order();
    expect(o.slot).toBe(o.list + 1);
  });
});

test.describe("R3 panes", () => {
  test("rail resizes with the mouse and arrow keys, collapses and expands, and all of it persists", async ({ page, server }) => {
    await openApp(page, server);
    const rail = page.getByTestId("rail");
    const handle = page.getByTestId("rail-handle");
    const w0 = (await rail.boundingBox())!.width;

    // Mouse drag.
    const hb = (await handle.boundingBox())!;
    await page.mouse.move(hb.x + hb.width / 2, hb.y + 200);
    await page.mouse.down();
    await page.mouse.move(hb.x + 80, hb.y + 200, { steps: 8 });
    await page.mouse.up();
    const w1 = (await rail.boundingBox())!.width;
    expect(w1).toBeGreaterThan(w0 + 50);

    // Keyboard: focus the separator, arrow keys.
    await handle.focus();
    await expect(handle).toBeFocused();
    await page.keyboard.press("ArrowLeft");
    const w2 = (await rail.boundingBox())!.width;
    expect(w2).toBeLessThan(w1 - 10);
    expect(w2).toBeGreaterThan(150);

    await page.reload();
    await expect.poll(async () => Math.round((await page.getByTestId("rail").boundingBox())!.width)).toBeGreaterThan(w2 - 3);
    expect(Math.round((await page.getByTestId("rail").boundingBox())!.width)).toBeLessThan(w2 + 3);

    // Collapse with `[`, persists, expand with the button.
    await page.locator("body").press("[");
    await expect.poll(async () => (await page.getByTestId("rail").boundingBox())?.width ?? 0).toBeLessThan(2);
    await page.reload();
    await expect(page.locator("article h1")).toBeVisible();
    await expect.poll(async () => (await page.getByTestId("rail").boundingBox())?.width ?? 0).toBeLessThan(2);
    await page.getByRole("button", { name: "Show files and outline" }).click();
    await expect.poll(async () => (await page.getByTestId("rail").boundingBox())!.width).toBeGreaterThan(150);
  });
});

test("switching modes and panes keeps the reading position", async ({ page, server }) => {
  await openApp(page, server);
  const heading = page.locator("article h2", { hasText: "ロールバック" });
  await heading.evaluate((h) => h.scrollIntoView({ block: "start" }));
  const topOf = () => heading.evaluate((h) => Math.round(h.getBoundingClientRect().top));
  const start = await topOf();
  for (const key of ["1", "2", "[", "["]) {
    await page.keyboard.press(key);
    await expect.poll(topOf, { timeout: 2000 }).toBeGreaterThan(start - 12);
    await expect.poll(topOf, { timeout: 2000 }).toBeLessThan(start + 12);
  }
});
