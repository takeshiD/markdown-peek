// R13: reference screenshots of the design, written to docs/design/screenshots/.
// Run with `npm run screenshots` (also part of `npm run test:e2e`).
import path from "node:path";
import { mkdirSync } from "node:fs";
import { test, expect, openApp, editFile, repoRoot, startServer } from "./server";
import type { Page } from "@playwright/test";

const OUT = path.join(repoRoot, "docs/design/screenshots");
mkdirSync(OUT, { recursive: true });
const FAKE = { sections: { 手順: { widget: "StepNavigator", confidence: 0.88, helps: 0.77 } } };

async function settle(page: Page) {
  await page.waitForFunction(() => document.getAnimations().every((a) => a.playState !== "running" || a.effect?.getTiming().iterations === Infinity));
}
const shot = async (page: Page, name: string) => {
  await settle(page);
  await page.screenshot({ path: path.join(OUT, `${name}.png`) });
};

for (const theme of ["light", "dark"] as const) {
  test.describe(`${theme}`, () => {
    test.use({ colorScheme: theme, serverOptions: { fake: FAKE } });

    test(`desktop modes (${theme})`, async ({ page, server }) => {
      await openApp(page, server);
      await page.locator("article h2", { hasText: "前提条件" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
      await shot(page, `${theme}-desktop-split`);
      await page.locator("body").press("3");
      await expect(page.locator('article [data-kind="StepNavigator"]').first()).toBeVisible();
      await page.locator("article h2", { hasText: "前提条件" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
      await shot(page, `${theme}-desktop-adaptive`);
      await page.locator("body").press("1");
      await shot(page, `${theme}-desktop-read`);
    });

    test(`mobile (${theme})`, async ({ page, server }) => {
      await page.setViewportSize({ width: 390, height: 844 });
      await openApp(page, server);
      await shot(page, `${theme}-mobile-read`);
      await page.getByTestId("open-widgets").click();
      await expect(page.getByRole("dialog", { name: "Reading aids" })).toBeVisible();
      await shot(page, `${theme}-mobile-widgets-sheet`);
      await page.keyboard.press("Escape");
      await page.getByRole("button", { name: "More" }).click();
      await page.getByRole("menuitemradio", { name: "Adaptive" }).click();
      await page.locator("article h2", { hasText: "手順" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
      await shot(page, `${theme}-mobile-adaptive`);
    });

    test(`states (${theme})`, async ({ page, server }) => {
      // Empty: a document with nothing to add.
      await openApp(page, server);
      await page.getByRole("tab", { name: "Files" }).click();
      await page.getByRole("button", { name: "notes.md", exact: true }).click();
      await expect(page.getByTestId("widgets-empty")).toBeVisible();
      await shot(page, `${theme}-state-no-widgets`);

      // Error: /api/adaptive fails; the text stays readable.
      await page.route("**/api/adaptive", (r) => r.fulfill({ status: 500, body: "{}" }));
      await page.getByRole("button", { name: "deploy-guide.md", exact: true }).click();
      await page.getByRole("button", { name: "Choose reading aids again" }).click();
      await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "error");
      await shot(page, `${theme}-state-error`);
      await page.unroute("**/api/adaptive");
    });

    test(`selecting and live update (${theme})`, async ({ page }) => {
      const server = await startServer({ fake: { ...FAKE, delay_ms: 2500 } });
      try {
        await page.emulateMedia({ reducedMotion: "reduce" }); // keep the change marker steady for the picture
        await openApp(page, server);
        await page.locator("article h2", { hasText: "手順" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
        await editFile(server, "deploy-guide.md", (s) => s.replace("5. マージの凍結を解除する。", "5. マージの凍結を解除する。\n6. リリースノートを公開する。"));
        await expect(page.locator("article > ol").first()).toHaveClass(/mp-changed/);
        await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "selecting");
        await shot(page, `${theme}-state-live-update-selecting`);
        await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready", { timeout: 10_000 });
        await shot(page, `${theme}-state-live-update-done`);
      } finally {
        await server.stop();
      }
    });

    test(`fallback (${theme})`, async ({ page }) => {
      const server = await startServer({ provider: "jev" });
      try {
        await openApp(page, server);
        await page.getByTestId("selector-status").click();
        await expect(page.getByTestId("selector-fallback")).toBeVisible();
        await shot(page, `${theme}-state-fallback`);
      } finally {
        await server.stop();
      }
    });
  });
}
