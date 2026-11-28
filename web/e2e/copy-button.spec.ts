// #67: the copy button of a code block is keyboard-reachable, is not added to
// mermaid diagrams, and is a 44px target on phones.
import { test, expect, openApp } from "./server";

test.use({ serverOptions: { file: "mermaid-heading.md" } });

test("copy button: keyboard reachable, none on mermaid", async ({ page, server }) => {
  await openApp(page, server);
  const rust = page.locator("article pre:has(code.language-rust)");
  const copy = rust.getByRole("button", { name: /^Copy/ });
  await expect(copy).toHaveCount(1);
  await copy.focus();
  await expect(copy).toBeFocused();
  await expect(copy).toBeVisible();
  await expect(rust.locator("code")).not.toContainText("Copy");
  await expect(page.locator("article pre:has(code.language-mermaid)").getByRole("button", { name: /^Copy/ })).toHaveCount(0);
});

test.describe("phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  test("copy button is at least 44px", async ({ page, server }) => {
    await openApp(page, server);
    const copy = page.locator("article pre:has(code.language-rust)").getByRole("button", { name: /^Copy/ });
    await expect(copy).toBeVisible();
    const box = await copy.boundingBox();
    expect(box!.width).toBeGreaterThanOrEqual(44);
    expect(box!.height).toBeGreaterThanOrEqual(44);
  });
});
