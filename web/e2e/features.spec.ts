// R10: existing viewer features survive the rewrite.
import { test, expect, openApp } from "./server";

test.describe("explorer: the current branch only", () => {
  test.use({ serverOptions: { git: true } });

  test("Files lists this working tree only; other worktrees/branches and the compare feature are gone", async ({ page, server }) => {
    await openApp(page, server);
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

test.describe("node graph and comparison cards", () => {
  test.use({ serverOptions: { file: "architecture.md" } });

  test("an arrow flow becomes a node graph; picking a node and an outcome follows the document", async ({ page, server }) => {
    await openApp(page, server);
    const graph = page.getByTestId("node-graph");
    await expect(graph).toBeVisible();
    // Five nodes from the arrows, notes from the definition list.
    await expect(graph.locator("svg [data-node]")).toHaveCount(5);
    await expect(graph.locator('svg [data-node="planning"]')).toContainText("Orchestrator");
    // Layered top-down: Frontend and Backend share a row under Planning.
    const y = (id: string) => graph.locator(`svg [data-node="${id}"] rect`).getAttribute("y").then(Number);
    expect(await y("frontend")).toBe(await y("backend"));
    expect(await y("planning")).toBeLessThan(await y("frontend"));
    expect(await y("verification")).toBeLessThan(await y("release"));
    // RETRY (written after the main flow) is the dashed back edge.
    await expect(graph.locator(".gui-graph__edge.is-back")).toHaveCount(1);
    await expect(graph.locator(".gui-graph__edge.is-back")).toContainText("RETRY");

    // The decision node is selected first, with its labelled exits as a switch.
    const chips = graph.getByRole("group", { name: "Select a node" });
    await expect(chips.getByRole("button", { name: "Verification" })).toHaveAttribute("aria-pressed", "true");
    const result = graph.locator(".gui-graph__row");
    await expect(result).toContainText("Verification result");
    await expect(result.getByRole("radio", { name: "PASS" })).toHaveAttribute("aria-checked", "true");
    await expect(graph.locator(".gui-graph__edge.is-chosen")).toContainText("PASS");
    await result.getByRole("radio", { name: "RETRY" }).click();
    await expect(graph.locator(".gui-graph__edge.is-chosen")).toContainText("RETRY");
    await expect(graph.locator('svg [data-node="backend"]')).toHaveClass(/is-reached/);

    // Detail of a node picked by keyboard.
    await chips.getByRole("button", { name: "Backend" }).focus();
    await page.keyboard.press("Enter");
    const detail = graph.getByTestId("node-detail");
    await expect(detail).toContainText("Agent 02");
    await expect(detail).toContainText("Planning, Verification (RETRY)");
    await expect(detail).toContainText("Verification");
    // Its source chip jumps to the definition line.
    await detail.locator(".gui-srclink").click();
    await expect(page.locator("article ul").nth(1)).toHaveClass(/mp-source-hit/);
  });

  test("a scored comparison table becomes cards; the badge only repeats the document's recommendation", async ({ page, server }) => {
    await openApp(page, server);
    const cmp = page.getByTestId("comparison");
    await expect(cmp).toBeVisible();
    const cards = cmp.locator(".gui-cmp__card");
    await expect(cards).toHaveCount(2);
    await expect(cards.nth(0)).toContainText("Single Agent");
    // Average = (45 + 95 + 70) / 3 = 70; (95 + 45 + 95) / 3 = 78.3 → 78.
    await expect(cards.nth(0).locator(".gui-cmp__big")).toHaveText("70");
    await expect(cards.nth(1).locator(".gui-cmp__big")).toHaveText("78");
    await expect(cards.nth(1)).toContainText("Average of 3 scores");
    await expect(cards.nth(1).locator(".gui-cmp__badge")).toHaveText("Recommended");
    await expect(cards.nth(0).locator(".gui-cmp__badge")).toHaveCount(0);
    await expect(cmp).toContainText("Agent Swarm を推奨する");
    // Facts stay verbatim.
    await expect(cards.nth(0).locator(".gui-cmp__facts")).toContainText("28 min");
    await expect(cards.nth(1).locator(".gui-cmp__facts")).toContainText("2.4x");
    // Focus on one criterion: its cell becomes the figure and the best option is marked.
    await cmp.getByRole("radio", { name: "コスト効率" }).click();
    await expect(cards.nth(0).locator(".gui-cmp__big")).toHaveText("95");
    await expect(cards.nth(0).locator(".gui-cmp__best")).toHaveText("Best");
    await expect(cards.nth(1).locator(".gui-cmp__best")).toHaveCount(0);
    await expect(cards.nth(0).locator('.gui-cmp__rows > li[data-focus="true"]')).toContainText("コスト効率");
  });

  test("axe: graph and comparison have no serious/critical violations (light and dark)", async ({ page, server }) => {
    const { default: AxeBuilder } = await import("@axe-core/playwright");
    for (const scheme of ["light", "dark"] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      await openApp(page, server);
      await expect(page.getByTestId("comparison")).toBeVisible();
      const r = await new AxeBuilder({ page }).include("article").withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
      const bad = r.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
      expect(bad.map((v) => `${v.id}: ${v.nodes.slice(0, 3).map((n) => n.target.join(" ")).join(" | ")}`), scheme).toEqual([]);
    }
  });
});
