// R10: existing viewer features survive the rewrite.
import { test, expect, openApp } from "./server";

test.describe("explorer: the current branch only", () => {
  test.use({ serverOptions: { git: true } });

  test("Files lists this working tree only; other worktrees/branches and the compare feature are gone", async ({ page, server }) => {
    await openApp(page, server);
    await page.getByRole("tab", { name: "Files" }).click();
    const files = page.getByRole("navigation", { name: "Files" });
    await expect(files).toContainText("repo (main)");
    await expect(files.getByRole("button", { name: "notes.md", exact: true })).toBeVisible();
    // The second worktree (branch feature-x) exists on disk but is not listed.
    await expect(files.locator('button[data-path*="repo-feature"]')).toHaveCount(0);
    await expect(files.getByRole("radio", { name: "Branch" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: /^Compare/ })).toHaveCount(0);
    expect((await page.request.post(`${server.url}/api/diff`, { data: { a: "x", b: "y" } })).status()).toBe(404);
    // Selecting a file in another worktree is refused by the server.
    const other = (await page.evaluate(() => fetch("/api/tree").then((r) => r.json()))).tree.groups[0].root.replace(/repo$/, "repo-feature/notes.md");
    expect((await page.request.post(`${server.url}/api/select`, { data: { path: other } })).status()).toBe(403);

    // Breadcrumb: the repository label is static; the file menu switches files.
    const crumbs = page.getByRole("navigation", { name: "Breadcrumb" });
    await expect(crumbs.getByTestId("breadcrumb-repo")).toHaveText("repo (main)");
    await page.getByTestId("breadcrumb-file").click();
    await page.getByRole("menuitemradio", { name: "notes.md" }).click();
    await expect(page.locator("article h1")).toHaveText(/読書メモ/);
    await expect(files.locator('button[aria-current="page"]')).toHaveAttribute("data-path", /repo\/notes\.md$/);
  });
});

test.describe("charts from tables", () => {
  test.use({ serverOptions: { file: "metrics.md" } });

  test("a measure table becomes a bar chart under it; a time table a line chart; text tables stay tables", async ({ page, server }) => {
    await openApp(page, server);
    const bar = page.locator('article [data-kind="Chart"]').filter({ has: page.getByTestId("chart-bars") });
    await expect(bar).toBeVisible();
    // Set into the text right after its table.
    const after = await page.evaluate(() => {
      const kids = Array.from(document.querySelector("article")!.children) as HTMLElement[];
      const i = kids.findIndex((k) => k.querySelector("[data-testid=chart-bars]"));
      return kids[i - 1]?.tagName;
    });
    expect(after).toBe("TABLE");
    const rows = bar.locator(".gui-bars__row");
    await expect(rows).toHaveCount(5);
    await expect(rows.nth(1)).toContainText("checkout");
    await expect(rows.nth(1)).toContainText("340 ms");
    // The original cell text is what the tooltip and the data table say.
    await expect(rows.nth(1)).toHaveAttribute("title", "checkout: 340 ms");
    await expect(bar.locator("table.sr-only tbody tr")).toHaveCount(5);
    // A second measure: one at a time, never a second axis.
    await bar.getByRole("radio", { name: "エラー件数" }).click();
    await expect(rows.nth(1)).toContainText("12");
    await expect(rows.nth(2)).toContainText("—"); // "-" in the table: no value, no bar
    await expect(rows.nth(2).locator(".gui-bars__bar")).toHaveCount(0);

    const line = page.getByTestId("chart-line");
    await line.scrollIntoViewIfNeeded();
    await expect(line.locator("svg path.gui-line__path")).toHaveAttribute("d", /^M[\d.]+,[\d.]+(L[\d.]+,[\d.]+){5}$/);
    await line.focus();
    await page.keyboard.press("End");
    await expect(page.getByTestId("chart-tip")).toHaveText("2026-11: 1,810,000");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("ArrowLeft");
    await page.keyboard.press("ArrowLeft");
    await expect(page.getByTestId("chart-tip")).toHaveText("2026-08: 1,310,000");

    // The 担当 table has no numbers: no chart for it.
    await expect(page.locator('article [data-kind="Chart"]')).toHaveCount(2);
    // Nothing (e.g. the screen-reader table) stretches the page itself.
    expect(await page.evaluate(() => document.scrollingElement!.scrollHeight <= document.scrollingElement!.clientHeight)).toBe(true);
  });
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
  await page.getByTestId("view-menu").click();
  await page.getByRole("menuitemcheckbox", { name: /Guided reading/ }).click();
  const panel = page.getByTestId("guided-panel");
  await expect(panel).toBeVisible();
  await expect(panel).toContainText("Guided reading");
  // LLM-only feature: without a configured LLM it reports the failure in the panel.
  await expect(panel.locator(".scrolly-text").first()).not.toHaveText("", { timeout: 20_000 });
  await panel.getByRole("button", { name: "Close guided reading" }).click();
  await expect(panel).toHaveCount(0);
});
