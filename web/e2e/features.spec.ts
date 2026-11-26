// R10: existing viewer features survive the rewrite.
import { test, expect, openApp, editFile } from "./server";

test.describe("explorer across worktrees", () => {
  test.use({ serverOptions: { git: true } });

  test("group by worktree / branch, switch group and file from the breadcrumb", async ({ page, server }) => {
    await openApp(page, server);
    await page.getByRole("tab", { name: "Files" }).click();
    const files = page.getByRole("navigation", { name: "Files" });
    await expect(files.getByRole("heading", { name: "repo", exact: true })).toBeVisible();
    await expect(files.getByRole("heading", { name: "repo-feature" })).toBeVisible();
    await files.getByRole("radio", { name: "Branch" }).click();
    await expect(files.getByRole("heading", { name: "main", exact: true })).toBeVisible();
    await expect(files.getByRole("heading", { name: "feature-x" })).toBeVisible();

    // Breadcrumb: switch to the other worktree, same file.
    const crumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    await crumbs.getByRole("button", { name: /^Repository: main/ }).click();
    await page.getByRole("menuitemradio", { name: "feature-x" }).click();
    await expect(crumbs.getByRole("button", { name: /^Repository: feature-x/ })).toBeVisible();
    await expect(page.getByTestId("breadcrumb-file")).toContainText("deploy-guide.md");
    // Breadcrumb: switch file within the group.
    await page.getByTestId("breadcrumb-file").click();
    await page.getByRole("menuitemradio", { name: "notes.md" }).click();
    await expect(page.locator("article h1")).toHaveText(/読書メモ/);
    await expect(files.locator('button[aria-current="page"]')).toHaveAttribute("data-path", /repo-feature\/notes\.md$/);
  });
});

test("diff: source / rendered, unified / split, swap, live re-diff, close", async ({ page, server }) => {
  await openApp(page, server);
  await page.getByRole("tab", { name: "Files" }).click();
  await page.getByRole("button", { name: "Compare deploy-guide.md" }).click();
  await page.getByRole("button", { name: "Compare with deploy-guide-v2.md" }).click();
  const body = page.getByTestId("diff-body");
  await expect(body.locator("tr.mdpeek-diff-del")).toContainText(["15 分間"]);
  await expect(body.locator("tr.mdpeek-diff-add").first()).toContainText("30 分間");
  await expect(page.getByTestId("diff-a")).toContainText("deploy-guide.md");
  await expect(page.getByTestId("diff-b")).toContainText("deploy-guide-v2.md");

  await page.getByRole("radio", { name: "Side by side" }).click();
  await expect(body.locator("table.mdpeek-diff-split")).toBeVisible();
  await page.getByRole("radio", { name: "Rendered" }).click();
  await expect(body.locator("table.mdpeek-rdiff-split")).toBeVisible();
  await page.getByRole("radio", { name: "Unified" }).click();
  await expect(body.locator(".mdpeek-rdiff .mdpeek-rdiff-block.mdpeek-diff-add").first()).toBeVisible();
  await page.getByRole("radio", { name: "Source" }).click();

  await page.getByRole("button", { name: "Swap left and right" }).click();
  await expect(page.getByTestId("diff-a")).toContainText("deploy-guide-v2.md");
  await expect(body.locator("tr.mdpeek-diff-del").first()).toContainText("30 分間");

  await editFile(server, "deploy-guide-v2.md", (s) => s.replace("30 分間", "45 分間"));
  await expect(body.locator("tr.mdpeek-diff-del").first()).toContainText("45 分間");
  await expect(body.locator("tr.mp-changed").first()).toBeVisible();

  await page.getByRole("button", { name: "Close comparison" }).click();
  await expect(page.locator("article h1")).toHaveText(/決済サービス/);
});

test("outline filter, front matter, code highlighting, mermaid, math source, GFM styling", async ({ page, server }) => {
  await openApp(page, server);
  const outline = page.getByRole("navigation", { name: "Outline" });
  await page.locator("body").press("/");
  await expect(page.getByTestId("outline-filter")).toBeFocused();
  await page.keyboard.type("ロールバ");
  await expect(outline.getByRole("link")).toHaveText(["ロールバック"]);
  await page.getByTestId("outline-filter").fill("zzz");
  await expect(outline).toContainText("No heading matches.");
  await page.getByTestId("outline-filter").fill("");

  const fm = page.getByTestId("frontmatter");
  await fm.locator("summary").click();
  await expect(fm.locator("pre")).toContainText("owner: platform-team");

  await expect(page.locator("article pre code.hljs.language-bash .hljs-string").first()).toBeVisible();
  await expect(page.locator("article pre code.language-mermaid svg")).toBeVisible({ timeout: 15_000 });
  await expect(page.locator("article")).toContainText("p_{99}"); // TeX source kept for MathJax (CDN, unchanged)
  await expect(page.locator("#MathJax-script")).toHaveCount(1);

  // GFM styling survives Tailwind's reset.
  const css = await page.evaluate(() => {
    const g = (sel: string, pseudo?: string) => {
      const el = document.querySelector(sel);
      if (!el) throw new Error(`no element for ${sel}`);
      return getComputedStyle(el, pseudo ?? null);
    };
    return {
      td: g("article table td").borderTopWidth,
      tdPad: g("article table td").paddingLeft,
      th: g("article table th").fontWeight,
      task: g("article li:has(> input[type=checkbox])").listStyleType,
      h2: g("article h2").borderBottomWidth,
      pre: g("article pre:not(:has(.language-mermaid))").backgroundColor,
      caution: g("article blockquote.markdown-alert-caution", "::before").content,
      foot: g("article .footnote-definition").fontSize,
      sup: g("article sup").verticalAlign,
      code: g("article li code").fontFamily,
    };
  });
  expect(css!.td).toBe("1px");
  expect(parseFloat(css!.tdPad)).toBeGreaterThan(6);
  expect(Number(css!.th)).toBeGreaterThanOrEqual(600);
  expect(css!.task).toBe("none");
  expect(css!.h2).toBe("1px");
  expect(css!.pre).not.toBe("rgba(0, 0, 0, 0)");
  expect(css!.caution).toBe('"Caution"');
  expect(parseFloat(css!.foot)).toBeLessThan(16);
  expect(css!.sup).toBe("super");
  expect(css!.code).toMatch(/mono|Menlo|Consolas/i);
  await expect(page.locator("article ol").first()).toHaveCSS("list-style-type", "decimal");
});

test("guided reading opens beside the text and closes", async ({ page, server }) => {
  await openApp(page, server);
  await page.getByRole("button", { name: "Guided reading" }).click();
  const panel = page.getByTestId("guided-panel");
  await expect(panel).toBeVisible();
  await expect(panel).toContainText("Guided reading");
  // LLM-only feature: without a configured LLM it reports the failure in the panel.
  await expect(panel.locator(".scrolly-text").first()).not.toHaveText("", { timeout: 20_000 });
  await panel.getByRole("button", { name: "Close guided reading" }).click();
  await expect(panel).toHaveCount(0);
});

test("Generated UI lens renders /api/gui components", async ({ page, server }) => {
  await openApp(page, server);
  await page.getByRole("button", { name: "Generated UI" }).click();
  const sheet = page.getByRole("dialog", { name: "Generated UI" });
  await expect(sheet).toBeVisible();
  await expect(sheet.getByTestId("lens-body").locator(".gui-panel").first()).toBeVisible({ timeout: 15_000 });
  await page.keyboard.press("Escape");
  await expect(sheet).toHaveCount(0);
});
