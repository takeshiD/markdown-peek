// R1 (shell, body first), R2 (modes), R3 (resizable / collapsible panes).
import { test, expect, openApp } from "./server";

test.describe("R1 shell", () => {
  // Hold back the server's startup WebSocket push too, so "selecting" is observable.
  test.use({ serverOptions: { debounceMs: 60_000 } });
  test("top bar, rail, body and widgets render; body does not wait for selection", async ({ page, server }) => {
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
    await expect(banner.getByRole("radio", { name: "Split mode" })).toBeVisible();
    await expect(banner.getByRole("radio", { name: "Adaptive mode" })).toBeVisible();
    await expect(banner.getByRole("button", { name: /Commands/ })).toBeVisible();
    await expect(banner.getByRole("button", { name: /^Theme:/ })).toBeVisible();
    const rail = page.getByRole("complementary", { name: "Files and outline" });
    await expect(rail.getByRole("tab", { name: "Files" })).toBeVisible();
    await expect(rail.getByRole("tab", { name: "Outline" })).toBeVisible();
    await expect(page.getByRole("complementary", { name: "Reading aids" })).toBeVisible();

    release();
    await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");
    await expect(page.getByTestId("widgets-pane").locator("[data-widget]").first()).toBeVisible();
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
  test("keys 1/2/3 switch Read / Split / Adaptive and the mode survives reload", async ({ page, server }) => {
    await openApp(page, server);
    // Split (default): widgets in the right pane.
    await expect(page.getByTestId("widgets-pane")).toBeVisible();
    await expect(page.getByTestId("rail")).toBeVisible();

    await page.locator("body").press("1");
    await expect(page.getByTestId("widgets-pane")).toHaveCount(0);
    await expect(page.getByTestId("rail")).toHaveCount(0);
    await expect(page.locator("[data-widget]")).toHaveCount(0);
    await expect(page.getByRole("radio", { name: "Read mode" })).toHaveAttribute("aria-checked", "true");
    await page.reload();
    await expect(page.getByRole("radio", { name: "Read mode" })).toHaveAttribute("aria-checked", "true");
    await expect(page.getByTestId("widgets-pane")).toHaveCount(0);

    await page.locator("body").press("3");
    await expect(page.getByTestId("widgets-pane")).toHaveCount(0);
    // Section widgets sit in the article, beside their section heading.
    const steps = page.locator('article .mp-note-slot[data-place="side"] [data-kind="StepNavigator"]');
    await expect(steps).toBeVisible();
    const heading = page.locator("article h2", { hasText: "手順" });
    const hBox = (await heading.boundingBox())!;
    const nBox = (await steps.boundingBox())!;
    expect(nBox.x).toBeGreaterThan(hBox.x + 500); // in the right margin
    expect(Math.abs(nBox.y - hBox.y)).toBeLessThan(260); // level with its section
    await page.reload();
    await expect(page.getByRole("radio", { name: "Adaptive mode" })).toHaveAttribute("aria-checked", "true");
    await expect(steps).toBeVisible();

    await page.locator("body").press("2");
    await expect(page.getByTestId("widgets-pane").locator('[data-kind="StepNavigator"]')).toBeVisible();
  });

  test("on-screen switch changes mode; Adaptive at a narrow width puts widgets right after their section", async ({ page, server }) => {
    await openApp(page, server);
    await page.getByRole("radio", { name: "Adaptive mode" }).click();
    await page.setViewportSize({ width: 900, height: 900 });
    const inline = page.locator('article .mp-note-slot[data-place="inline"] [data-kind="StepNavigator"]');
    await expect(inline).toBeVisible();
    await expect(page.locator('article .mp-note-slot[data-place="side"] [data-kind="StepNavigator"]')).toBeHidden();
    // The inline slot follows the last block of the 手順 section and precedes the next heading.
    const order = await page.evaluate(() => {
      const kids = Array.from(document.querySelector("article")!.children) as HTMLElement[];
      const slot = kids.findIndex((k) => k.dataset.place === "inline" && k.querySelector('[data-kind="StepNavigator"]'));
      const steps = kids.findIndex((k) => k.tagName === "H2" && k.textContent?.includes("手順"));
      const next = kids.findIndex((k, i) => i > steps && k.tagName === "H2");
      return { slot, steps, next };
    });
    expect(order.slot).toBeGreaterThan(order.steps);
    expect(order.slot).toBeLessThan(order.next);
    expect(order.slot).toBe(order.next - 1);
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

  test("widget pane resizes with the mouse and the keyboard, collapses with ] and persists", async ({ page, server }) => {
    await openApp(page, server);
    const pane = page.getByTestId("widgets-pane");
    const wStart = (await pane.boundingBox())!.width;
    const hb = (await page.getByTestId("widgets-handle").boundingBox())!;
    await page.mouse.move(hb.x + hb.width / 2, hb.y + 300);
    await page.mouse.down();
    await page.mouse.move(hb.x + 60, hb.y + 300, { steps: 6 });
    await page.mouse.up();
    const w0 = (await pane.boundingBox())!.width;
    expect(w0).toBeLessThan(wStart - 40);
    await page.getByTestId("widgets-handle").focus();
    for (let i = 0; i < 4; i++) await page.keyboard.press("ArrowLeft");
    const w1 = (await pane.boundingBox())!.width;
    expect(w1).toBeGreaterThan(w0 + 10);
    await page.reload();
    // Restored within a pixel (sizes are stored in whole pixels).
    await expect.poll(async () => Math.abs((await page.getByTestId("widgets-pane").boundingBox())!.width - w1)).toBeLessThan(2.5);

    await page.locator("body").press("]");
    await expect.poll(async () => (await page.getByTestId("widgets-pane").boundingBox())?.width ?? 0).toBeLessThan(2);
    await page.reload();
    await expect(page.locator("article h1")).toBeVisible();
    await expect.poll(async () => (await page.getByTestId("widgets-pane").boundingBox())?.width ?? 0).toBeLessThan(2);
    await page.locator("body").press("]");
    await expect.poll(async () => (await page.getByTestId("widgets-pane").boundingBox())!.width).toBeGreaterThan(200);
  });
});

test("switching modes and panes keeps the reading position", async ({ page, server }) => {
  await openApp(page, server);
  const heading = page.locator("article h2", { hasText: "ロールバック" });
  await heading.evaluate((h) => h.scrollIntoView({ block: "start" }));
  const topOf = () => heading.evaluate((h) => Math.round(h.getBoundingClientRect().top));
  const start = await topOf();
  for (const key of ["3", "1", "2", "[", "]"]) {
    await page.keyboard.press(key);
    await expect.poll(topOf, { timeout: 2000 }).toBeGreaterThan(start - 12);
    await expect.poll(topOf, { timeout: 2000 }).toBeLessThan(start + 12);
  }
});
