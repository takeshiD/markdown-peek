// R1 (shell, body first), R2 (reading-aid toggle), R3 (resizable / collapsible panes).
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
    // One toggle for reading aids (no Read / Adaptive modes).
    await expect(banner.getByTestId("aids-toggle")).toHaveAttribute("aria-pressed", "true");
    await expect(banner.getByRole("radio", { name: /mode$/ })).toHaveCount(0);
    await expect(banner.getByRole("button", { name: /Commands/ })).toBeVisible();
    await expect(banner.getByRole("button", { name: /^Theme:/ })).toBeVisible();
    // Files on the left, the outline on the right.
    const rail = page.getByRole("complementary", { name: "Files" });
    await expect(rail.getByRole("navigation", { name: "Files" })).toBeVisible();
    const outline = page.getByRole("complementary", { name: "Document outline" });
    await expect(outline.getByRole("navigation", { name: "Outline" })).toBeVisible();
    expect((await outline.boundingBox())!.x).toBeGreaterThan((await page.locator("article").boundingBox())!.x + 400);
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

test.describe("R2 reading-aid toggle", () => {
  test("A turns reading aids off and on, the rail stays, and the choice survives reload", async ({ page, server }) => {
    await openApp(page, server);
    const toggle = page.getByTestId("aids-toggle");
    const steps = page.locator('article .mp-note-slot [data-kind="StepNavigator"]');
    await expect(steps).toBeVisible();
    await expect(page.getByTestId("rail")).toBeVisible();

    await page.locator("body").press("a");
    await expect(page.locator("[data-widget]")).toHaveCount(0);
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await expect(page.getByTestId("rail")).toBeVisible();
    await page.reload();
    await expect(page.locator("article h1")).toBeVisible();
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await expect(page.locator("[data-widget]")).toHaveCount(0);

    await toggle.click();
    await expect(steps).toBeVisible();
    await page.reload();
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await expect(steps).toBeVisible();
  });

  test("a stored Read mode from an older version opens with aids off; Split opens with aids on", async ({ page, server }) => {
    await page.addInitScript(() => {
      if (!sessionStorage.getItem("seeded")) {
        sessionStorage.setItem("seeded", "1");
        localStorage.setItem("mdpeek.ui.v1", JSON.stringify({ mode: "read", widgetsSize: 400, groupBy: "branch" }));
      }
    });
    await page.goto(server.url);
    await expect(page.getByTestId("aids-toggle")).toHaveAttribute("aria-pressed", "false");
    await page.evaluate(() => localStorage.setItem("mdpeek.ui.v1", JSON.stringify({ mode: "split" })));
    await page.reload();
    await expect(page.getByTestId("aids-toggle")).toHaveAttribute("aria-pressed", "true");
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
    await page.getByRole("button", { name: "Show files" }).click();
    await expect.poll(async () => (await page.getByTestId("rail").boundingBox())!.width).toBeGreaterThan(150);
  });
});

test.describe("R3 outline on the right", () => {
  test("] hides and shows it; it resizes and the size persists", async ({ page, server }) => {
    await openApp(page, server);
    const pane = page.getByTestId("outline-pane");
    await expect.poll(async () => (await pane.boundingBox())?.width ?? 0).toBeGreaterThan(180);
    await page.locator("body").press("]");
    await expect.poll(async () => (await pane.boundingBox())?.width ?? 0).toBeLessThan(2);
    await expect(page.getByRole("button", { name: "Show outline" })).toHaveAttribute("aria-pressed", "false");
    await page.reload();
    await expect(page.locator("article h1")).toBeVisible();
    await expect.poll(async () => (await page.getByTestId("outline-pane").boundingBox())?.width ?? 0).toBeLessThan(2);
    await page.getByRole("button", { name: "Show outline" }).click();
    await expect.poll(async () => (await pane.boundingBox())?.width ?? 0).toBeGreaterThan(180);
    // Keyboard resize on its divider.
    const w0 = (await pane.boundingBox())!.width;
    await page.getByTestId("outline-handle").focus();
    for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowLeft");
    await expect.poll(async () => (await pane.boundingBox())!.width).toBeGreaterThan(w0 + 10);
    const w1 = (await pane.boundingBox())!.width;
    await page.reload();
    await expect.poll(async () => Math.abs((await page.getByTestId("outline-pane").boundingBox())!.width - w1)).toBeLessThan(3);
  });

  test("floats over the document, can be dragged or moved by keys, keeps its place and docks back", async ({ page, server }) => {
    await openApp(page, server);
    await page.getByRole("button", { name: "Float the outline over the document" }).click();
    const float = page.getByTestId("outline-float");
    await expect(float).toBeVisible();
    await expect.poll(async () => (await page.getByTestId("outline-pane").boundingBox())?.width ?? 0).toBeLessThan(2);
    // Over the document column, and it stays put while the text scrolls.
    const doc = (await page.locator("#main").boundingBox())!;
    let b = (await float.boundingBox())!;
    expect(b.x + b.width).toBeLessThanOrEqual(doc.x + doc.width + 1);
    await page.locator("article h2", { hasText: "ロールバック" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
    expect((await float.boundingBox())!.y).toBeCloseTo(b.y, 0);
    // Scrollspy works in the floating outline too.
    await expect(float.getByRole("link", { name: "ロールバック" })).toHaveAttribute("aria-current", "location");
    // Drag by the header.
    const h = (await page.getByTestId("outline-float-handle").boundingBox())!;
    await page.mouse.move(h.x + 150, h.y + h.height / 2);
    await page.mouse.down();
    await page.mouse.move(h.x + 50, h.y + 160, { steps: 8 });
    await page.mouse.up();
    const moved = (await float.boundingBox())!;
    expect(moved.x).toBeLessThan(b.x - 80);
    expect(moved.y).toBeGreaterThan(b.y + 120);
    // Arrow keys on the grip.
    await float.getByRole("button", { name: "Move the outline (arrow keys)" }).focus();
    await page.keyboard.press("ArrowUp");
    await expect.poll(async () => (await float.boundingBox())!.y).toBeLessThan(moved.y - 20);
    b = (await float.boundingBox())!;
    await page.reload();
    await expect(page.getByTestId("outline-float")).toBeVisible();
    const after = (await page.getByTestId("outline-float").boundingBox())!;
    expect(Math.abs(after.x - b.x)).toBeLessThan(2);
    expect(Math.abs(after.y - b.y)).toBeLessThan(2);
    // Dock it back.
    await page.getByRole("button", { name: "Dock the outline on the right" }).click();
    await expect(page.getByTestId("outline-float")).toHaveCount(0);
    await expect.poll(async () => (await page.getByTestId("outline-pane").boundingBox())?.width ?? 0).toBeGreaterThan(180);
  });
});

test("toggling aids and panes keeps the reading position", async ({ page, server }) => {
  await openApp(page, server);
  const heading = page.locator("article h2", { hasText: "ロールバック" });
  await heading.evaluate((h) => h.scrollIntoView({ block: "start" }));
  const topOf = () => heading.evaluate((h) => Math.round(h.getBoundingClientRect().top));
  const start = await topOf();
  for (const key of ["a", "a", "[", "[", "]", "]"]) {
    await page.keyboard.press(key);
    await expect.poll(topOf, { timeout: 2000 }).toBeGreaterThan(start - 12);
    await expect.poll(topOf, { timeout: 2000 }).toBeLessThan(start + 12);
  }
});
