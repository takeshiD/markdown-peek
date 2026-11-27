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

    test(`desktop aids on / off, model menu (${theme})`, async ({ page, server }) => {
      await openApp(page, server);
      await expect(page.locator('article [data-kind="StepNavigator"]').first()).toBeVisible();
      await page.locator("article h2", { hasText: "前提条件" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
      await shot(page, `${theme}-desktop-adaptive`);
      // Another model for this session: "Make default" appears.
      await page.getByTestId("model-menu").click();
      await page.getByRole("menuitemradio", { name: /^fake-alt / }).click();
      await expect(page.getByRole("menu")).toHaveCount(0);
      await expect(page.getByTestId("make-default")).toBeVisible();
      await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");
      await page.getByTestId("model-menu").click();
      await expect(page.getByRole("menuitemradio", { name: /^jev / })).toBeVisible();
      await shot(page, `${theme}-desktop-model-menu`);
      await page.keyboard.press("Escape");
      await page.locator("body").press("a");
      await shot(page, `${theme}-desktop-read`);
    });

    test(`mobile (${theme})`, async ({ page, server }) => {
      await page.setViewportSize({ width: 390, height: 844 });
      await openApp(page, server);
      await page.locator("article h2", { hasText: "手順" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
      await shot(page, `${theme}-mobile-adaptive`);
      await page.getByRole("button", { name: "More" }).click();
      await page.getByRole("menuitem", { name: "Turn reading aids off" }).click();
      await page.locator("article h1").evaluate((h) => h.scrollIntoView({ block: "start" }));
      await shot(page, `${theme}-mobile-read`);
    });

    test(`charts (${theme})`, async ({ page }) => {
      const server = await startServer({ file: "metrics.md" });
      try {
        await openApp(page, server);
        await expect(page.getByTestId("chart-bars")).toBeVisible();
        await page.locator("article h2", { hasText: "サービス別" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
        await shot(page, `${theme}-desktop-chart-bar`);
        await page.locator("article h2", { hasText: "月別" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
        await page.getByTestId("chart-line").focus();
        await page.keyboard.press("End");
        await page.keyboard.press("ArrowLeft");
        await shot(page, `${theme}-desktop-chart-line`);
        await page.setViewportSize({ width: 390, height: 844 });
        await page.locator("article h2", { hasText: "サービス別" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
        await shot(page, `${theme}-mobile-chart`);
      } finally {
        await server.stop();
      }
    });

    test(`graph and comparison (${theme})`, async ({ page }) => {
      const server = await startServer({ file: "architecture.md" });
      try {
        await openApp(page, server);
        await expect(page.getByTestId("node-graph")).toBeVisible();
        await page.getByTestId("node-graph").evaluate((h) => h.closest(".mp-note-slot")!.scrollIntoView({ block: "start" }));
        await shot(page, `${theme}-desktop-node-graph`);
        await page.getByTestId("comparison").evaluate((h) => h.closest(".mp-note-slot")!.scrollIntoView({ block: "start" }));
        await shot(page, `${theme}-desktop-comparison`);
        await page.getByRole("button", { name: "Float the outline over the document" }).click();
        await expect(page.getByTestId("outline-float")).toBeVisible();
        await shot(page, `${theme}-desktop-outline-floating`);
        await page.setViewportSize({ width: 390, height: 844 });
        await page.getByTestId("node-graph").evaluate((h) => h.scrollIntoView({ block: "start" }));
        await shot(page, `${theme}-mobile-node-graph`);
        await page.getByTestId("comparison").evaluate((h) => h.scrollIntoView({ block: "start" }));
        await shot(page, `${theme}-mobile-comparison`);
      } finally {
        await server.stop();
      }
    });

    test(`states (${theme})`, async ({ page, server }) => {
      // Empty: a document with nothing to add.
      await openApp(page, server);
      await page.getByRole("button", { name: "notes.md", exact: true }).click();
      await page.getByTestId("selector-status").click();
      await expect(page.getByTestId("widgets-empty")).toBeVisible();
      await shot(page, `${theme}-state-no-widgets`);
      await page.keyboard.press("Escape");

      // Error: /api/adaptive fails; the text stays readable.
      await page.getByRole("button", { name: "deploy-guide.md", exact: true }).click();
      await expect(page.locator('article [data-kind="StepNavigator"]')).toBeVisible();
      await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");
      await page.waitForTimeout(400); // let the switch's own run finish pushing
      await page.route("**/api/adaptive", (r) => r.fulfill({ status: 500, body: "{}" }));
      await page.reload();
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
        await expect(page.getByTestId("aids-stop")).toBeVisible();
        await shot(page, `${theme}-state-live-update-selecting`);
        await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready", { timeout: 10_000 });
        await shot(page, `${theme}-state-live-update-done`);
      } finally {
        await server.stop();
      }
    });

    test(`invalid model and stop / resume (${theme})`, async ({ page }) => {
      const server = await startServer({ provider: "jev" });
      try {
        await openApp(page, server);
        await expect(page.getByTestId("toast")).toBeVisible();
        await shot(page, `${theme}-state-model-invalid`);
      } finally {
        await server.stop();
      }
      const slow = await startServer({ fake: { ...FAKE, delay_ms: 1200 }, extraConfig: "", debounceMs: 150 });
      try {
        await page.emulateMedia({ reducedMotion: "reduce" });
        await page.goto(slow.url);
        await expect(page.getByTestId("aids-stop")).toBeVisible();
        await page.getByTestId("aids-stop").click();
        await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "stopped");
        await expect(page.getByTestId("aids-resume")).toBeVisible();
        await shot(page, `${theme}-state-stopped`);
      } finally {
        await slow.stop();
      }
    });
  });
}
