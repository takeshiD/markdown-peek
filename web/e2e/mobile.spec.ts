// R4: 390x844 phone — single pane, no horizontal scroll, sheets, 44px targets.
import { test, expect, openApp } from "./server";
import type { Page } from "@playwright/test";

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

async function noHorizontalScroll(page: Page) {
  const m = await page.evaluate(() => {
    const main = document.getElementById("main")!;
    return { doc: document.documentElement.scrollWidth, vw: window.innerWidth, main: main.scrollWidth, mainW: main.clientWidth };
  });
  expect(m.doc).toBeLessThanOrEqual(m.vw);
  expect(m.main).toBeLessThanOrEqual(m.mainW);
}

/**
 * Every visible interactive control inside `scope` is at least 44x44, is the
 * topmost element at its own centre (nothing covers it), and does not overlap
 * any other control in the same scope.
 */
async function tapTargets(page: Page, scope: string) {
  // Let sheet / menu enter animations settle so boxes are final.
  await page.waitForFunction(() => document.getAnimations().every((a) => a.playState !== "running" || a.effect?.getTiming().iterations === Infinity));
  const problems = await page.evaluate((sel) => {
    const out: string[] = [];
    const name = (el: Element) => `${el.tagName} "${(el.getAttribute("aria-label") ?? el.textContent ?? "").trim().slice(0, 30)}"`;
    const ctrls: { el: HTMLElement; r: DOMRect }[] = [];
    for (const root of Array.from(document.querySelectorAll(sel))) {
      for (const el of Array.from(root.querySelectorAll<HTMLElement>("button, a[href], input, [role=tab], [role=menuitemradio]:not([data-disabled]), [role=menuitem], summary"))) {
        const r = el.getBoundingClientRect();
        const style = getComputedStyle(el);
        if (r.width === 0 || r.height === 0 || style.visibility === "hidden") continue;
        if (el.closest("article") && !el.closest(".mp-note-slot")) continue; // links inside the prose are text
        const midY = r.top + r.height / 2;
        if (midY <= 0 || midY >= window.innerHeight) continue; // scrolled (mostly) out of view: tap after scrolling
        ctrls.push({ el, r });
        if (r.width < 43.5 || r.height < 43.5) out.push(`too small: ${name(el)} ${Math.round(r.width)}x${Math.round(r.height)}`);
        const cx = r.left + r.width / 2;
        const cy = r.top + r.height / 2;
        const hit = document.elementFromPoint(cx, cy);
        if (!hit || (hit !== el && !el.contains(hit))) out.push(`covered: ${name(el)} at ${Math.round(cx)},${Math.round(cy)} by ${hit ? name(hit.closest("button,a,input") ?? hit) : "nothing"}`);
      }
    }
    for (let i = 0; i < ctrls.length; i++)
      for (let j = i + 1; j < ctrls.length; j++) {
        const a = ctrls[i];
        const b = ctrls[j];
        if (a.el.contains(b.el) || b.el.contains(a.el)) continue;
        const ox = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
        const oy = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
        if (ox > 0.5 && oy > 0.5) out.push(`overlap: ${name(a.el)} and ${name(b.el)}`);
      }
    return out;
  }, scope);
  expect(problems, `tap target problems in ${scope}`).toEqual([]);
}

test("single pane without horizontal scroll, aids off and on", async ({ page, server }) => {
  await openApp(page, server);
  await expect(page.getByTestId("rail")).toHaveCount(0);
  await expect(page.locator("article h1")).toBeInViewport();
  await noHorizontalScroll(page);
  await tapTargets(page, "header");
  for (const item of ["Turn reading aids off", "Turn reading aids on"]) {
    await page.getByRole("button", { name: "More" }).tap();
    await page.getByRole("menuitem", { name: item }).tap();
    await expect(page.getByRole("menu")).toHaveCount(0);
    await noHorizontalScroll(page);
  }
  // Adaptive on a phone: aids sit inside the text, after their lines.
  await expect(page.locator('article .mp-note-slot [data-kind="StepNavigator"]')).toBeVisible();
  await page.locator('article [data-kind="StepNavigator"]').scrollIntoViewIfNeeded();
  await noHorizontalScroll(page);
  await tapTargets(page, "article .mp-note-slot");
});

test("files and outline open in a sheet; the model can be switched from the menu", async ({ page, server }) => {
  await openApp(page, server);
  await page.getByRole("button", { name: "Files and outline" }).tap();
  const sheet = page.getByRole("dialog", { name: "Outline" });
  await expect(sheet).toBeVisible();
  await expect(sheet.getByRole("link", { name: "ロールバック" })).toBeVisible();
  await tapTargets(page, "[role=dialog]");
  await sheet.getByRole("button", { name: "Files" }).tap();
  const files = page.getByRole("dialog", { name: "Files" });
  await expect(files.getByRole("button", { name: "notes.md", exact: true })).toBeVisible();
  await tapTargets(page, "[role=dialog]");
  await files.getByRole("button", { name: "Close" }).tap();
  await expect(page.getByRole("dialog")).toHaveCount(0);

  // Outline jump from the sheet closes it and scrolls the body.
  await page.getByRole("button", { name: "Files and outline" }).tap();
  await page.getByRole("dialog").getByRole("button", { name: "Outline" }).tap();
  await page.getByRole("dialog").getByRole("link", { name: "ロールバック" }).tap();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator("article h2", { hasText: "ロールバック" })).toBeInViewport();

  // No reading-aid sheet on a phone either: the aids are in the text.
  await expect(page.getByTestId("open-widgets")).toHaveCount(0);
  await page.getByTestId("selector-status").tap();
  const pop = page.getByRole("dialog", { name: "Reading aids" });
  await expect(pop).toBeVisible();
  await expect(pop.getByTestId("why-provider")).toContainText("fake");
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "More" }).tap();
  await tapTargets(page, "[role=menu]");
  await page.getByRole("menuitemradio", { name: /^fake-alt / }).tap();
  await expect(page.getByRole("menu")).toHaveCount(0);
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-provider", "fake-alt");
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");
  await page.getByRole("button", { name: "More" }).tap();
  await page.getByRole("menuitem", { name: /Make “fake-alt” the default/ }).tap();
  await expect(page.getByTestId("toast").filter({ hasText: "is now the default model" })).toBeVisible();
  await expect(page.locator('article [data-kind="StepNavigator"]')).toBeAttached();
  await noHorizontalScroll(page);
});
