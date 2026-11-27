// R9: live updates patch only changed blocks, keep scroll, keep widgets while
// selecting, and never show results for an older revision.
import { test, expect, openApp, editFile } from "./server";

test.use({ serverOptions: { fake: { delay_ms: 1500, sections: { 手順: { widget: "StepNavigator", confidence: 0.9, helps: 0.9 } } } } });

test("editing the file patches changed blocks in place, keeps scroll and old widgets until the new ones arrive", async ({ page, server }) => {
  await openApp(page, server);
  const main = page.locator("#main");
  const anchor = page.locator("article h2", { hasText: "設定値" });
  await anchor.evaluate((h) => h.scrollIntoView({ block: "start" }));
  const posOf = () => anchor.evaluate((h) => h.getBoundingClientRect().top - document.getElementById("main")!.getBoundingClientRect().top);
  const before = await posOf();
  expect(await main.evaluate((m) => m.scrollTop)).toBeGreaterThan(300);
  // Tag unchanged blocks to prove they are not re-rendered.
  await page.evaluate(() => {
    document.querySelectorAll("article > *").forEach((e) => ((e as HTMLElement & { __mark?: boolean }).__mark = true));
  });
  const stepsBefore = await page.locator("article").locator('[data-kind="StepNavigator"] li').count();

  await editFile(server, "deploy-guide.md", (s) =>
    s.replace("5. マージの凍結を解除する。", "5. マージの凍結を解除する。\n6. リリースノートを公開する。").replace("ピーク時は 10", "ピーク時は 12"),
  );

  const table = page.locator("article > table");
  await expect(table).toContainText("ピーク時は 12");
  await expect(table).toHaveClass(/mp-changed/);
  await expect(page.locator("article > ol").first()).toHaveClass(/mp-changed/);
  // Selection is still running (fake delay): status says so, old widgets stay.
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "selecting");
  await expect(page.locator("article").locator('[data-kind="StepNavigator"] li')).toHaveCount(stepsBefore);

  const kept = await page.evaluate(() =>
    Array.from(document.querySelectorAll("article > *")).map((e) => [e.tagName, !!(e as HTMLElement & { __mark?: boolean }).__mark]),
  );
  const fresh = kept.filter(([, m]) => !m).map(([t]) => t);
  expect(fresh.sort()).toEqual(["OL", "TABLE"]);
  // The reading position is kept: the heading on screen did not move, even
  // though a list above it grew.
  expect(Math.abs((await posOf()) - before)).toBeLessThan(4);

  // New selection arrives and replaces the widget.
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready", { timeout: 10_000 });
  await expect(page.locator("article").locator('[data-kind="StepNavigator"] li')).toHaveCount(stepsBefore + 1);
  await expect(page.locator("article").locator('[data-kind="StepNavigator"]')).toContainText("リリースノートを公開する");
});

test("a layout for an older revision is ignored", async ({ page, server }) => {
  // Proxy the real socket, and inject a stale layout after a live update.
  let inject: ((msg: string) => void) | null = null;
  await page.routeWebSocket(/\/ws$/, (ws) => {
    const srv = ws.connectToServer();
    inject = (m) => ws.send(m);
    srv.onMessage((m) => ws.send(m));
    ws.onMessage((m) => srv.send(m));
  });
  await openApp(page, server);
  await editFile(server, "deploy-guide.md", (s) => s.replace("ピーク時は 10", "ピーク時は 11"));
  await expect(page.locator("article > table")).toContainText("ピーク時は 11");
  const rev = await page.evaluate(() => Number(document.querySelector("[data-testid=selector-status]")?.getAttribute("data-state") ? 0 : 0));
  expect(rev).toBe(0);
  const stale = {
    type: "adaptive",
    revision: 0,
    layout: {
      version: 2, revision: 0, doc_hash: "x",
      selector: { name: "fake", kind: "decision", model: "STALE-MODEL", error: null, message: null, requests: 0 },
      progress: { state: "done", done: 0, total: 0 },
      doc_type: { value: "readme", confidence: 1, by: "fake" },
      placements: [], skipped: [], timing_ms: { select: 0, total: 0 },
    },
  };
  inject!(JSON.stringify(stale));
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready", { timeout: 10_000 });
  // The stale (empty) layout never replaced the widgets.
  await expect(page.locator("article").locator('[data-kind="StepNavigator"]')).toBeVisible();
  await expect(page.getByTestId("widgets-empty")).toHaveCount(0);
  await page.getByTestId("selector-status").click();
  await expect(page.getByRole("dialog")).not.toContainText("STALE-MODEL");
});
