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
    const pane = page.locator("article");
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

  test("R6: scrolling the body marks the current section in Outline and in the aids", async ({ page, server }) => {
    await openApp(page, server);
    await page.locator("article h2", { hasText: "手順" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
    const outline = page.getByRole("navigation", { name: "Outline" });
    await expect(outline.getByRole("link", { name: "手順", exact: true })).toHaveAttribute("aria-current", "location");
    await expect(page.locator("article").locator('[data-kind="StepNavigator"]')).toHaveAttribute("data-active", "true");
    await page.locator("article h2", { hasText: "前提条件" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
    await expect(outline.getByRole("link", { name: "前提条件" })).toHaveAttribute("aria-current", "location");
    await expect(outline.getByRole("link", { name: "手順", exact: true })).not.toHaveAttribute("aria-current", "location");
    await expect(page.locator("article").locator('[data-kind="StepNavigator"]')).toHaveAttribute("data-active", "false");
    await expect(page.locator("article").locator('[data-kind="Checklist"]')).toHaveAttribute("data-active", "true");
  });

  test("R7: why-this-widget shows the model, confidence, helps and alternatives", async ({ page, server }) => {
    await openApp(page, server);
    const status = page.getByTestId("selector-status");
    await expect(status).toHaveAttribute("data-provider", "fake");
    await expect(status).toHaveAttribute("data-state", "ready");
    await page.getByRole("button", { name: "Why this view: Steps" }).click();
    const pop = page.getByRole("dialog");
    await expect(pop.getByTestId("why-provider")).toHaveText("fake, model fake-e2e");
    await expect(pop.getByTestId("why-confidence")).toHaveText("88%");
    await expect(pop.getByTestId("why-helps")).toHaveText("77%");
    await expect(pop.getByTestId("why-alternatives")).not.toBeEmpty();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("button", { name: "Why this view: Steps" })).toBeFocused();
  });

  test("R8: pin and hide survive reload per document; hidden widgets can be restored", async ({ page, server }) => {
    await openApp(page, server);
    const pane = page.locator("article");
    await pane.getByRole("button", { name: "Pin Steps to the top" }).click();
    await expect(pane.getByRole("button", { name: "Unpin Steps from the top" })).toHaveAttribute("aria-pressed", "true");
    // Pinned: moved to the top of the text, under the title.
    await expect(pane.locator("[data-widget]").first()).toHaveAttribute("data-kind", "StepNavigator");
    await expect(pane.locator('.mp-note-slot[data-place="top"] [data-kind="StepNavigator"]')).toBeVisible();
    expect(await pane.evaluate((a) => (a.children[1] as HTMLElement).dataset.place)).toBe("top");
    await pane.getByRole("button", { name: "Hide Checklist" }).click();
    await expect(pane.locator('[data-kind="Checklist"]')).toHaveCount(0);
    await expect(page.getByTestId("live-region")).toHaveText(/hidden/);
    // Hidden: the task list it stood in for is back in the text (#63).
    await expect(pane.locator("ul:has(> li > input[type=checkbox])").first()).toBeVisible();

    await page.reload();
    await openApp(page, server);
    await expect(pane.locator("[data-widget]").first()).toHaveAttribute("data-kind", "StepNavigator");
    await expect(pane.getByRole("button", { name: "Unpin Steps from the top" })).toBeVisible();
    await expect(pane.locator('[data-kind="Checklist"]')).toHaveCount(0);

    // Another document has its own (empty) choices.
    await page.getByRole("button", { name: "deploy-guide-v2.md", exact: true }).click();
    await expect(page.getByTestId("breadcrumb-file")).toContainText("deploy-guide-v2.md");
    await expect(pane.locator('[data-kind="Checklist"]')).toHaveCount(1);
    await expect(pane.getByRole("button", { name: "Pin Steps to the top" })).toBeVisible();
    await page.getByRole("button", { name: "deploy-guide.md", exact: true }).click();

    await page.getByTestId("hidden-aids").click();
    await expect(page.getByTestId("hidden-aids")).toHaveText(/1 hidden/);
    await page.getByRole("menuitem", { name: /in 前提条件/ }).click();
    await expect(pane.locator('[data-kind="Checklist"]')).toHaveCount(1);
    await page.reload();
    await openApp(page, server);
    await expect(pane.locator('[data-kind="Checklist"]')).toHaveCount(1);
  });
});

test.describe("selector states", () => {
  test.describe("a model whose key is not set", () => {
    test.use({ serverOptions: { provider: "jev" } });
    test("R7: says the settings are invalid, shows no aids, and another model works", async ({ page, server }) => {
      await openApp(page, server);
      const status = page.getByTestId("selector-status");
      await expect(status).toHaveAttribute("data-state", "error");
      const toast = page.getByTestId("toast").filter({ hasText: "cannot be used" });
      await expect(toast).toBeVisible();
      await expect(toast).toContainText("environment variable MDPEEK_E2E_UNSET_KEY is not set");
      await expect(toast).toContainText(server.config);
      await expect(page.locator("[data-widget]")).toHaveCount(0);
      await status.click();
      await expect(page.getByTestId("selector-fallback")).toContainText("MDPEEK_E2E_UNSET_KEY");
      await page.keyboard.press("Escape");
      // The model menu marks it, and switching to a working model fills the text.
      await page.getByTestId("model-menu").click();
      await expect(page.getByTestId("model-problem-jev")).toBeVisible();
      await page.getByRole("menuitemradio", { name: /^fake \(/ }).click();
      await expect(status).toHaveAttribute("data-state", "ready");
      await expect(page.locator('article [data-kind="StepNavigator"]')).toBeVisible();
      await toast.getByRole("button", { name: "Dismiss notification" }).click();
      await expect(toast).toHaveCount(0);
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
      await expect(page.getByTestId("aids-error")).toContainText("GenUI could not be loaded");
      await expect(page.locator("article h1")).toBeVisible();
      fail = false;
      await page.getByTestId("aids-error").getByRole("button", { name: "Try again" }).click();
      await expect(status).toHaveAttribute("data-state", "ready");
    });
    });

  test("a document with nothing to add says so", async ({ page, server }) => {
    await openApp(page, server);
    await page.getByRole("button", { name: "notes.md", exact: true }).click();
    await expect(page.getByTestId("breadcrumb-file")).toContainText("notes.md");
    await expect(page.locator("[data-widget]")).toHaveCount(0);
    await page.getByTestId("selector-status").click();
    await expect(page.getByTestId("widgets-empty")).toBeVisible();
  });
});
