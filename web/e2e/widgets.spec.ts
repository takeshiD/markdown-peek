// R6 source ranges + scroll sync, R7 why / selector status, R8 pin / hide.
import { test, expect, openApp } from "./server";

const FAKE = {
  document: { widget: "ActionItems", confidence: 0.9, helps: 0.9 },
  sections: { 手順: { widget: "StepNavigator", confidence: 0.88, helps: 0.77 } },
};

test.describe("with the fake selector", () => {
  test.use({ serverOptions: { fake: FAKE } });

  test("R6: each widget shows its source lines; the chip scrolls to and highlights the block", async ({ page, server }) => {
    await openApp(page, server);
    const pane = page.getByTestId("widgets-pane");
    const steps = pane.locator('[data-kind="StepNavigator"]');
    const chip = steps.locator("header .gui-srclink");
    await expect(chip).toHaveText("L21–26");
    for (const w of await pane.locator("[data-widget]").all()) await expect(w.locator("header .gui-srclink")).toHaveText(/^L\d+(–\d+)?$/);
    await chip.click();
    const ol = page.locator("article > ol").first();
    await expect(ol).toHaveAttribute("data-line-start", "21");
    await expect(ol).toBeInViewport();
    await expect(ol).toHaveClass(/mp-source-hit/);
    await expect(ol).toBeFocused();
    await expect(page.getByTestId("live-region")).toHaveText(/Showing lines 21 to 26/);
    // An item-level chip inside a widget works too.
    await page.locator("article h1").scrollIntoViewIfNeeded();
    await steps.locator(".gui-panel .gui-srclink").first().click();
    await expect(ol).toBeInViewport();
  });

  test("R6: scrolling the body marks the current section in Outline and in the widgets", async ({ page, server }) => {
    await openApp(page, server);
    await page.locator("article h2", { hasText: "手順" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
    const outline = page.getByRole("navigation", { name: "Outline" });
    await expect(outline.getByRole("link", { name: "手順", exact: true })).toHaveAttribute("aria-current", "location");
    await expect(page.getByTestId("widgets-pane").locator('[data-kind="StepNavigator"]')).toHaveAttribute("data-active", "true");
    await page.locator("article h2", { hasText: "コマンド" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
    await expect(outline.getByRole("link", { name: "コマンド" })).toHaveAttribute("aria-current", "location");
    await expect(outline.getByRole("link", { name: "手順", exact: true })).not.toHaveAttribute("aria-current", "location");
    await expect(page.getByTestId("widgets-pane").locator('[data-kind="StepNavigator"]')).toHaveAttribute("data-active", "false");
    await expect(page.getByTestId("widgets-pane").locator('[data-kind="ConfigViewer"]')).toHaveAttribute("data-active", "true");
  });

  test("R7: why-this-widget shows provider, model, confidence, helps, alternatives and fallback", async ({ page, server }) => {
    await openApp(page, server);
    const status = page.getByTestId("selector-status");
    await expect(status).toHaveAttribute("data-provider", "fake");
    await expect(status).toHaveAttribute("data-state", "ready");
    await page.getByRole("button", { name: "Why this reading aid: Steps" }).click();
    const pop = page.getByRole("dialog");
    await expect(pop.getByTestId("why-provider")).toHaveText("fake, model fake-e2e");
    await expect(pop.getByTestId("why-confidence")).toHaveText("88%");
    await expect(pop.getByTestId("why-helps")).toHaveText("77%");
    await expect(pop.getByTestId("why-alternatives")).not.toBeEmpty();
    await expect(pop.getByTestId("why-fallback")).toHaveText("none");
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: "Why this reading aid: Steps" })).toBeFocused();
  });

  test("R8: pin and hide survive reload per document; hidden widgets can be restored", async ({ page, server }) => {
    await openApp(page, server);
    const pane = page.getByTestId("widgets-pane");
    await pane.getByRole("button", { name: "Pin Steps" }).click();
    await expect(pane.getByRole("button", { name: "Unpin Steps" })).toHaveAttribute("aria-pressed", "true");
    await expect(pane.locator("[data-widget]").first()).toHaveAttribute("data-kind", "StepNavigator");
    await pane.getByRole("button", { name: "Hide Commands" }).click();
    await expect(pane.locator('[data-kind="ConfigViewer"]')).toHaveCount(0);
    await expect(page.getByTestId("live-region")).toHaveText(/hidden/);

    await page.reload();
    await openApp(page, server);
    await expect(pane.locator("[data-widget]").first()).toHaveAttribute("data-kind", "StepNavigator");
    await expect(pane.getByRole("button", { name: "Unpin Steps" })).toBeVisible();
    await expect(pane.locator('[data-kind="ConfigViewer"]')).toHaveCount(0);
    // Adaptive mode honours the same choices.
    await page.locator("body").press("3");
    await expect(page.locator('article [data-kind="ConfigViewer"]')).toHaveCount(0);
    await page.locator("body").press("2");

    // Another document has its own (empty) choices.
    await page.getByRole("tab", { name: "Files" }).click();
    await page.getByRole("button", { name: "deploy-guide-v2.md", exact: true }).click();
    await expect(page.getByTestId("breadcrumb-file")).toContainText("deploy-guide-v2.md");
    await expect(pane.locator('[data-kind="ConfigViewer"]')).toHaveCount(1);
    await expect(pane.getByRole("button", { name: "Pin Steps" })).toBeVisible();
    await page.getByRole("button", { name: "deploy-guide.md", exact: true }).click();

    await pane.getByRole("button", { name: "Hidden (1)" }).click();
    await page.getByRole("menuitem", { name: /in コマンド/ }).click();
    await expect(pane.locator('[data-kind="ConfigViewer"]')).toHaveCount(1);
    await page.reload();
    await openApp(page, server);
    await expect(pane.locator('[data-kind="ConfigViewer"]')).toHaveCount(1);
  });
});

test.describe("selector states", () => {
  test.describe("jev without network permission", () => {
    test.use({ serverOptions: { provider: "jev" } });
    test("R7: fallback is shown with its reason and widgets still come from rules", async ({ page, server }) => {
      await openApp(page, server);
      const status = page.getByTestId("selector-status");
      await expect(status).toHaveAttribute("data-state", "fallback");
      await expect(status).toContainText("fallback");
      await status.click();
      await expect(page.getByTestId("selector-fallback")).toContainText(/Network use is off|No TYPESAFE_API_KEY/);
      await page.keyboard.press("Escape");
      await page.getByTestId("widgets-pane").getByRole("button", { name: /^Why this reading aid/ }).first().click();
      await expect(page.getByTestId("why-fallback")).not.toHaveText("none");
    });
  });

  test.describe("without a startup push", () => {
    // Keep the server's WebSocket push out of the way so this test only sees
    // what GET /api/adaptive returns (the app connects within ~100 ms).
    test.use({ serverOptions: { debounceMs: 60_000 } });
    test("R7: an error loading reading aids is shown, the text stays, and retry recovers", async ({ page, server }) => {
      let fail = true;
      await page.route("**/api/adaptive", (route) => (fail ? route.fulfill({ status: 500, body: "{}" }) : route.continue()));
      await page.goto(server.url);
      const status = page.getByTestId("selector-status");
      await expect(status).toHaveAttribute("data-state", "error");
      await expect(page.getByTestId("widgets-pane").getByRole("alert")).toContainText("Reading aids could not be loaded");
      await expect(page.locator("article h1")).toBeVisible();
      fail = false;
      await page.getByTestId("widgets-pane").getByRole("button", { name: "Try again" }).click();
      await expect(status).toHaveAttribute("data-state", "ready");
    });
    });

  test("a document with nothing to add says so", async ({ page, server }) => {
    await openApp(page, server);
    await page.getByRole("tab", { name: "Files" }).click();
    await page.getByRole("button", { name: "notes.md", exact: true }).click();
    await expect(page.getByTestId("widgets-empty")).toBeVisible();
  });
});
