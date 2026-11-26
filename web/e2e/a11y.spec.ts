// R12: keyboard reach, visible focus, skip link, landmarks, live region,
// axe (no serious / critical) in light and dark, reduced motion.
import AxeBuilder from "@axe-core/playwright";
import { test, expect, openApp } from "./server";
import type { Page } from "@playwright/test";

async function axe(page: Page, label: string) {
  // Let enter animations (popover / dialog fade-in) finish before measuring contrast.
  await page.waitForFunction(() => document.getAnimations().every((a) => a.playState !== "running" || a.effect?.getTiming().iterations === Infinity));
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
    await axe(page, `${scheme} split`);
    await page.locator("body").press("3");
    await axe(page, `${scheme} adaptive`);
    await page.getByRole("button", { name: "Why this widget: Steps" }).first().click();
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
  await expect(page.getByRole("complementary", { name: "Files and outline" })).toBeVisible();
  await expect(page.getByRole("complementary", { name: "Reading aids" })).toBeVisible();
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

test("every control is reachable by keyboard: modes, panes, widgets, why, pin", async ({ page, server }) => {
  await openApp(page, server);
  // Mode switch: arrow keys inside the radio group.
  await page.getByRole("radio", { name: "Split mode" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("radio", { name: "Adaptive mode" })).toBeFocused();
  await page.keyboard.press("Space");
  await expect(page.getByRole("radio", { name: "Adaptive mode" })).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("ArrowLeft");
  await expect(page.getByRole("radio", { name: "Split mode" })).toBeFocused();
  await page.keyboard.press("Space");
  await expect(page.getByRole("radio", { name: "Split mode" })).toHaveAttribute("aria-checked", "true");
  // A widget's buttons by Tab from its source chip.
  const steps = page.getByTestId("widgets-pane").locator('[data-kind="StepNavigator"]');
  await steps.locator("header .gui-srclink").focus();
  await page.keyboard.press("Tab");
  await expect(steps.getByRole("button", { name: "Why this widget: Steps" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("why-provider")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(steps.getByRole("button", { name: "Why this widget: Steps" })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(steps.getByRole("button", { name: "Pin Steps" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(steps.getByRole("button", { name: "Unpin Steps" })).toHaveAttribute("aria-pressed", "true");
  // Shortcuts help lists the keys.
  await page.locator("body").press("?");
  await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toContainText("Adaptive mode");
  await page.keyboard.press("Escape");
});

test("state changes are announced to screen readers", async ({ page, server }) => {
  await openApp(page, server);
  const live = page.getByTestId("live-region");
  await expect(live).toHaveAttribute("aria-live", "polite");
  await page.locator("body").press("1");
  await expect(live).toHaveText("Read mode");
  await page.locator("body").press("2");
  await page.getByRole("button", { name: "Choose reading aids again" }).click();
  await expect(live).toHaveText(/Reading aids updated: \d+ widgets? chosen by fake/);
});

test("prefers-reduced-motion stops animations and transitions", async ({ page, server }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openApp(page, server);
  await page.getByTestId("widgets-pane").locator('[data-kind="StepNavigator"] header .gui-srclink').click();
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

test("tabbing from page load: every stop shows a focus indicator (top bar, rail incl. tab panel, document, widgets; Outline and Files)", async ({ page, server }) => {
  const visibleFocus = () =>
    page.evaluate(() => {
      const el = document.activeElement as HTMLElement | null;
      if (!el || el === document.body) return { ok: true, label: "body", role: "", where: "" };
      const s = getComputedStyle(el);
      const outline = s.outlineStyle !== "none" && parseFloat(s.outlineWidth) > 0;
      const ring = s.boxShadow !== "none";
      const label = `${el.tagName}[${el.getAttribute("role") ?? ""}] ${(el.getAttribute("aria-label") ?? el.textContent ?? "").trim().slice(0, 30)}`;
      const where = el.closest("header") ? "topbar" : el.closest("[data-testid=rail]") ? "rail" : el.closest("[data-testid=widgets-pane]") ? "widgets" : el.closest("main") ? "document" : "other";
      return { ok: outline || ring, label, role: el.getAttribute("role") ?? "", where };
    });
  for (const tab of ["Outline", "Files"]) {
    await openApp(page, server);
    if (tab === "Files") {
      await page.getByRole("tab", { name: "Files" }).click();
      await page.reload();
      await openApp(page, server);
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
    // The pass really went through the rail's tab panel.
    expect(seen.has("tabpanel"), `${tab}: reached the rail tab panel`).toBe(true);
    for (const a of ["topbar", "rail", "document", "widgets"]) expect(areas.has(a), `${tab}: reached ${a}`).toBe(true);
  }
  // The tab panel's indicator is actually painted (a ring), not just a computed value.
  await page.getByRole("tabpanel").focus();
  await expect(page.getByRole("tabpanel")).toHaveCSS("box-shadow", /rgb|oklch|color/);
});
