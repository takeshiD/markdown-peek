// Reading-aid runs: spinner, stop / resume, model switch and "make default",
// invalid settings, auto_generate = false, and the entrance animation.
import { readFileSync } from "node:fs";
import { test, expect, openApp, editFile } from "./server";

test.describe("stop and resume", () => {
  test.use({ serverOptions: { fake: { delay_ms: 600 }, debounceMs: 100, batchSections: 1 } });

  test("spinner while choosing, stop keeps the aids so far, resume asks only the rest", async ({ page, server }) => {
    await page.goto(server.url);
    const toggle = page.getByTestId("aids-toggle");
    await expect(toggle).toHaveAttribute("data-busy", "true");
    await expect(toggle.locator("svg.animate-spin, svg[class*=animate-spin]")).toHaveCount(1);
    await expect(page.getByTestId("aids-progress")).toContainText(/Choosing reading aids with fake… \d+ of \d+/);
    // Wait for at least one aid, then stop.
    await expect(page.locator("article [data-widget]").first()).toBeVisible({ timeout: 10_000 });
    await page.getByTestId("aids-stop").click();
    const status = page.getByTestId("selector-status");
    await expect(status).toHaveAttribute("data-state", "stopped");
    await expect(toggle).toHaveAttribute("data-busy", "false");
    const shown = await page.locator("article [data-widget]").count();
    expect(shown).toBeGreaterThan(0);
    await expect(page.getByTestId("aids-resume")).toHaveText(/Resume/);
    // A reload shows the stopped state with the cached aids, without asking again.
    await page.reload();
    await expect(status).toHaveAttribute("data-state", "stopped");
    await expect(page.locator("article [data-widget]")).toHaveCount(shown);
    // Resume finishes the rest.
    await page.getByTestId("aids-resume").click();
    await expect(status).toHaveAttribute("data-state", "selecting");
    await expect(status).toHaveAttribute("data-state", "ready", { timeout: 20_000 });
    expect(await page.locator("article [data-widget]").count()).toBeGreaterThan(shown);
    await expect(page.getByTestId("aids-resume")).toHaveCount(0);
    await expect(page.getByTestId("aids-stop")).toHaveCount(0);
  });
});

test.describe("models", () => {
  test("switching is for this session; Make default appears only then and saves the config", async ({ page, server }) => {
    await openApp(page, server);
    const menu = page.getByTestId("model-menu");
    await expect(menu).toContainText("fake");
    await expect(page.getByTestId("make-default")).toHaveCount(0);
    await menu.click();
    await expect(page.getByRole("menuitemradio", { name: /^fake \(.*default/ })).toHaveAttribute("aria-checked", "true");
    await page.getByRole("menuitemradio", { name: /^fake-alt / }).click();
    // The menu's exit animation must end before it can open again.
    await expect(page.getByRole("menu")).toHaveCount(0);
    await expect(page.getByTestId("selector-status")).toHaveAttribute("data-provider", "fake-alt");
    await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");
    // The default is unchanged until asked.
    expect(readFileSync(server.config, "utf8")).toMatch(/use = "fake"\n/);
    await expect(page.getByTestId("make-default")).toBeVisible();
    await page.getByTestId("make-default").click();
    await expect(page.getByTestId("toast").filter({ hasText: "“fake-alt” is now the default model" })).toBeVisible();
    await expect(page.getByTestId("make-default")).toHaveCount(0);
    const cfg = readFileSync(server.config, "utf8");
    expect(cfg).toMatch(/use = "fake-alt"/);
    expect(cfg).toContain("[models.jev]");
    // Switching back to the old default shows the button again.
    await menu.click();
    await page.getByRole("menuitemradio", { name: /^fake \(/ }).click();
    await expect(page.getByTestId("make-default")).toBeVisible();
  });
});

test.describe("invalid settings", () => {
  test.use({
    serverOptions: {
      extraConfig: '[models.weird]\napi = "teleport"\n\n[models.leaky]\napi = "anthropic"\nmodel = "claude-haiku-5-5"\napi_key = "sk-should-never-show"\n\n[select]\nprovider = "jev"\n',
    },
  });
  test("are reported once, clearly, without leaking a key; the bad models cannot be picked", async ({ page, server }) => {
    await openApp(page, server);
    const toast = page.getByTestId("toast").filter({ hasText: "Settings are invalid" });
    await expect(toast).toBeVisible();
    await expect(toast).toContainText("[select] was removed");
    await page.getByTestId("model-menu").click();
    await expect(page.getByTestId("model-problem-weird")).toBeVisible();
    await expect(page.getByTestId("model-problem-leaky")).toContainText("api_key is not allowed");
    await expect(page.getByRole("menuitemradio", { name: /^leaky / })).toHaveAttribute("data-disabled", "");
    expect(await page.content()).not.toContain("sk-should-never-show");
    await page.keyboard.press("Escape");
    await toast.getByRole("button", { name: "Dismiss notification" }).click();
    await expect(toast).toHaveCount(0);
    // Not shown again in this page.
    await page.locator("body").press("a");
    await page.locator("body").press("a");
    await expect(toast).toHaveCount(0);
  });
});

test.describe("auto_generate = false", () => {
  test.use({ serverOptions: { autoGenerate: false } });
  test("waits for the reader to start", async ({ page, server }) => {
    await openApp(page, server, { waitLayout: false });
    const status = page.getByTestId("selector-status");
    await expect(status).toHaveAttribute("data-state", "idle");
    await expect(page.locator("article [data-widget]")).toHaveCount(0);
    await page.getByTestId("aids-resume").click();
    await expect(status).toHaveAttribute("data-state", "ready");
    await expect(page.locator('article [data-kind="StepNavigator"]')).toBeVisible();
  });
});

test.describe("entrance animation", () => {
  test.use({ serverOptions: { fake: { delay_ms: 300 } } });
  test("new aids rise in with an overshoot; cached ones and reduced motion do not move", async ({ page, server }) => {
    await page.goto(server.url);
    const steps = page.locator('article [data-kind="StepNavigator"]');
    await expect(steps).toHaveAttribute("data-enter", "true");
    const anim = await steps.evaluate((el) =>
      el.getAnimations().map((a) => ({ name: (a as CSSAnimation).animationName, easing: (a.effect as KeyframeEffect).getTiming().easing })),
    );
    expect(anim.map((a) => a.name)).toEqual(["mp-aid-in", "mp-aid-glow"]);
    // A "back" curve: the second control point overshoots 1.
    expect(getComputedStyleEasing(await steps.evaluate((el) => getComputedStyle(el).animationTimingFunction))).toBeGreaterThan(1);
    await expect(steps).not.toHaveAttribute("data-enter", /.*/, { timeout: 5_000 });

    // Reload: the same aids come from the cache, without animation.
    await page.reload();
    await expect(steps).toBeVisible();
    await expect(steps).not.toHaveAttribute("data-enter", /.*/);

    // Reduced motion: a new aid only gets the fading ring.
    await page.emulateMedia({ reducedMotion: "reduce" });
    await editFile(server, "deploy-guide.md", (s) => s.replace("5. マージの凍結を解除する。", "5. マージの凍結を解除する。\n6. リリースノートを公開する。"));
    await expect(steps).toContainText("リリースノートを公開する", { timeout: 10_000 });
    const names = await steps.evaluate((el) => el.getAnimations().map((a) => (a as CSSAnimation).animationName));
    expect(names).not.toContain("mp-aid-in");
  });
});

/** The overshoot of the first `cubic-bezier(...)` in an animation-timing-function. */
function getComputedStyleEasing(tf: string): number {
  const m = tf.match(/cubic-bezier\(([^)]+)\)/);
  if (!m) return 0;
  return Number(m[1].split(",")[1]);
}
