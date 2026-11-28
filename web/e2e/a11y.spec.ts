// R12: keyboard reach, visible focus, skip link, landmarks, live region,
// axe (no serious / critical) in light and dark, reduced motion.
import AxeBuilder from "@axe-core/playwright";
import { test, expect, openApp } from "./server";
import type { Page } from "@playwright/test";

async function axe(page: Page, label: string) {
  // Let enter animations (popover / dialog fade-in) finish before measuring contrast.
  await page.waitForFunction(() => document.getAnimations().every((a) => a.playState !== "running" || a.effect?.getTiming().iterations === Infinity));
  await expect(page.locator("[data-enter]")).toHaveCount(0);
  const r = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"]).analyze();
  const bad = r.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(
    bad.map((v) => `${v.id}: ${v.nodes.slice(0, 3).map((n) => n.target.join(" ")).join(" | ")}`),
    `axe serious/critical in ${label}`,
  ).toEqual([]);
}

for (const scheme of ["light", "dark"] as const) {
  test(`axe: no serious/critical violations (${scheme})`, async ({ page, server }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await openApp(page, server);
    await axe(page, `${scheme} adaptive`);
    await page.locator("body").press("1");
    await axe(page, `${scheme} read`);
    await page.locator("body").press("2");
    await page.getByRole("button", { name: "Why this view: Steps" }).first().click();
    await axe(page, `${scheme} popover`);
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+k");
    await axe(page, `${scheme} palette`);
    await page.keyboard.press("Escape");
    await page.setViewportSize({ width: 390, height: 844 });
    await axe(page, `${scheme} mobile`);
  });
}

test("skip link, landmarks and visible focus", async ({ page, server }) => {
  await openApp(page, server);
  await page.keyboard.press("Tab");
  const skip = page.getByRole("link", { name: "Skip to document" });
  await expect(skip).toBeFocused();
  await expect(skip).toBeInViewport();
  await page.keyboard.press("Enter");
  await expect(page.locator("#main")).toBeFocused();

  await expect(page.getByRole("banner")).toHaveCount(1);
  await expect(page.getByRole("main")).toHaveCount(1);
  await expect(page.getByRole("complementary", { name: "Files" })).toBeVisible();
  await expect(page.getByRole("complementary", { name: "Document outline" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Breadcrumb" })).toBeVisible();

  // Tab through the first 25 stops: every focused element shows a ring/outline.
  await page.locator("body").focus();
  for (let i = 0; i < 25; i++) {
    await page.keyboard.press("Tab");
    const vis = await page.evaluate(() => {
      const el = document.activeElement as HTMLElement | null;
      if (!el || el === document.body) return "body";
      const s = getComputedStyle(el);
      const ring = s.boxShadow !== "none" || (s.outlineStyle !== "none" && parseFloat(s.outlineWidth) > 0);
      return ring ? "ok" : `${el.tagName} ${el.getAttribute("aria-label") ?? el.textContent?.slice(0, 20)}`;
    });
    expect(vis, `focus ring on tab stop ${i}`).not.toMatch(/^(?!ok|body)/);
  }
});

test("every control is reachable by keyboard: aids toggle, model, panes, widgets, why, pin", async ({ page, server }) => {
  await openApp(page, server);
  // The aids toggle and the model menu.
  const toggle = page.getByTestId("aids-toggle");
  await toggle.focus();
  await page.keyboard.press("Space");
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await page.keyboard.press("Space");
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  await page.keyboard.press("Tab");
  await expect(page.getByTestId("model-menu")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("menuitemradio", { name: /^fake-alt / })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("model-menu")).toBeFocused();
  await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", "ready");
  // A widget's buttons by Tab from its source chip.
  const steps = page.locator("article").locator('[data-kind="StepNavigator"]');
  await steps.locator("header .gui-srclink").focus();
  await page.keyboard.press("Tab");
  await expect(steps.getByRole("button", { name: "Why this view: Steps" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("why-provider")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(steps.getByRole("button", { name: "Why this view: Steps" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(steps.getByRole("button", { name: "Pin Steps to the top" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator('article [data-kind="StepNavigator"]').getByRole("button", { name: "Unpin Steps from the top" })).toHaveAttribute("aria-pressed", "true");
  // Shortcuts help lists the keys.
  await page.locator("body").press("?");
  await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toContainText("GenUI on or off");
  await page.keyboard.press("Escape");
});

test("state changes are announced to screen readers", async ({ page, server }) => {
  await openApp(page, server);
  const live = page.getByTestId("live-region");
  await expect(live).toHaveAttribute("aria-live", "polite");
  await page.locator("body").press("a");
  await expect(live).toHaveText("GenUI off");
  await page.locator("body").press("a");
  await expect(live).toHaveText("GenUI on");
  await page.getByTestId("model-menu").click();
  await page.getByRole("menuitemradio", { name: /^fake-alt / }).click();
  await expect(live).toHaveText(/GenUI updated: \d+ views? chosen by fake-alt/);
});

test("prefers-reduced-motion stops animations and transitions", async ({ page, server }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openApp(page, server);
  await page.locator("article").locator('[data-kind="StepNavigator"] header .gui-srclink').click();
  const t = await page.evaluate(() => {
    const ol = document.querySelector("article > ol")!;
    const s = getComputedStyle(ol);
    const btn = getComputedStyle(document.querySelector("header button")!);
    return { anim: s.animationDuration, cls: ol.className, trans: btn.transitionDuration, smooth: getComputedStyle(document.getElementById("main")!).scrollBehavior };
  });
  expect(t.cls).toContain("mp-source-hit");
  expect(parseFloat(t.anim)).toBeLessThan(0.01);
  expect(t.trans.split(",").every((d) => parseFloat(d) < 0.01)).toBe(true);
  expect(t.smooth).toBe("auto");
});

test("tabbing from page load: every stop shows a focus indicator (top bar, files, document, aids, outline; docked and floating)", async ({ page, server }) => {
  const visibleFocus = () =>
    page.evaluate(() => {
      const el = document.activeElement as HTMLElement | null;
      if (!el || el === document.body) return { ok: true, label: "body", role: "", where: "" };
      const s = getComputedStyle(el);
      const outline = s.outlineStyle !== "none" && parseFloat(s.outlineWidth) > 0;
      const ring = s.boxShadow !== "none";
      const label = `${el.tagName}[${el.getAttribute("role") ?? ""}] ${(el.getAttribute("aria-label") ?? el.textContent ?? "").trim().slice(0, 30)}`;
      const where = el.closest("header") ? "topbar" : el.closest("[data-testid=rail]") ? "rail" : el.closest("[data-testid=outline-pane], [data-testid=outline-float]") ? "outline" : el.closest("[data-widget]") ? "aids" : el.closest("main") ? "document" : "other";
      return { ok: outline || ring, label, role: el.getAttribute("role") ?? "", where };
    });
  for (const tab of ["docked", "floating"]) {
    await openApp(page, server);
    if (tab === "floating") {
      await page.getByRole("button", { name: "Float the outline over the document" }).click();
      await expect(page.getByTestId("outline-float")).toBeVisible();
    }
    // Start from the very top of the page, exactly like a fresh keyboard user.
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    const missing: string[] = [];
    const seen = new Set<string>();
    const areas = new Set<string>();
    for (let i = 0; i < 120; i++) {
      await page.keyboard.press("Tab");
      const f = await visibleFocus();
      seen.add(f.role);
      areas.add(f.where);
      if (!f.ok) missing.push(f.label);
    }
    expect(missing, `${tab}: tab stops without a visible outline/ring`).toEqual([]);
    for (const a of ["topbar", "rail", "document", "aids", "outline"]) expect(areas.has(a), `${tab}: reached ${a}`).toBe(true);
  }
});
