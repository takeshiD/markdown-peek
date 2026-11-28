// Regressions for issues #60, #61, #63, #66.
import { test, expect, openApp, editFile } from "./server";

test.describe("#60 a heading id that shadows a library global", () => {
  test.use({ serverOptions: { file: "mermaid-heading.md" } });

  test("`# Mermaid` / `## hljs` headings do not stop diagrams and highlighting", async ({ page, server }) => {
    const errors: string[] = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await openApp(page, server);
    // Named access: the heading is window.mermaid before the script loads.
    await expect(page.locator("article h1#mermaid")).toBeVisible();
    await expect(page.locator("article pre code.language-mermaid svg")).toBeVisible({ timeout: 15_000 });
    await expect(page.locator("article pre code.language-rust .hljs-keyword").first()).toBeVisible();
    expect(errors, errors.join("\n")).toEqual([]);
  });
});

test.describe("#61 pin and hide keep the aid findable", () => {
  test("a pinned aid stays in view while reading; hide offers Undo", async ({ page, server }) => {
    await page.setViewportSize({ width: 1280, height: 700 });
    await openApp(page, server);
    const steps = page.locator('article [data-kind="StepNavigator"]');
    await steps.scrollIntoViewIfNeeded();
    await steps.getByRole("button", { name: "Pin Steps to the top" }).click();
    // It moved to the sticky strip under the title and is still on screen.
    await expect(page.getByTestId("pinned-aids").locator('[data-kind="StepNavigator"]')).toBeInViewport();
    const toast = page.getByTestId("toast").filter({ hasText: "pinned to the top" });
    await expect(toast).toBeVisible();
    // Scrolling the text keeps it on screen.
    await page.locator("article h2", { hasText: "連絡先" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
    await expect(steps).toBeInViewport();
    // A heading jump lands below the strip, not under it.
    const stripBottom = await page.getByTestId("pinned-aids").evaluate((e) => e.getBoundingClientRect().bottom);
    await page.locator("article h2", { hasText: "設定値" }).evaluate((h) => h.scrollIntoView({ block: "start" }));
    const headTop = await page.locator("article h2", { hasText: "設定値" }).evaluate((e) => e.getBoundingClientRect().top);
    expect(headTop).toBeGreaterThanOrEqual(stripBottom - 1);
    // Unpin: back in its section, and "Show it" goes there.
    await steps.getByRole("button", { name: "Unpin Steps from the top" }).click();
    await expect(page.getByTestId("pinned-aids")).toHaveCount(0);
    await page.getByTestId("toast").filter({ hasText: "back in its section" }).getByRole("button", { name: "Show it" }).click();
    await expect(steps).toBeInViewport();

    // Hide: the toast says how to get it back, Undo does.
    await steps.getByRole("button", { name: "Hide Steps" }).click();
    await expect(steps).toHaveCount(0);
    const hidden = page.getByTestId("toast").filter({ hasText: "Steps hidden" });
    await expect(hidden).toBeVisible();
    await hidden.getByRole("button", { name: "Undo" }).click();
    await expect(steps).toBeVisible();
    await expect(steps).toBeInViewport();
    await expect(page.getByTestId("hidden-aids")).toHaveCount(0);
  });
});

test.describe("#63 table and task-list aids stand in for their block", () => {
  test.use({
    serverOptions: {
      fake: { sections: { 設定値: { widget: "DataTable:sortable", confidence: 0.9, helps: 0.9 } } },
    },
  });

  test("the aid replaces the block, Original switches back, aids off restores the text", async ({ page, server }) => {
    await openApp(page, server);
    const article = page.locator("article");
    const tasks = page.locator("article > ul:has(input[type=checkbox])");
    const table = page.locator("article > table");
    const checklist = article.locator('[data-kind="Checklist"]');
    const dataTable = article.locator('[data-kind="DataTable"]');
    await expect(checklist).toBeVisible();
    await expect(dataTable).toBeVisible();
    // Shown once: the original block is hidden, the aid sits where it was.
    await expect(tasks).toBeHidden();
    await expect(table).toBeHidden();
    const order = await article.evaluate((a) => {
      const kids = Array.from(a.children) as HTMLElement[];
      const slot = kids.findIndex((k) => k.querySelector('[data-kind="Checklist"]'));
      const list = kids.findIndex((k) => k.matches("ul") && k.querySelector("input[type=checkbox]"));
      return { slot, list };
    });
    expect(order.slot).toBe(order.list - 1);

    // Per aid: back to the Markdown, and again to the aid.
    await checklist.getByRole("button", { name: "Show the original checklist" }).click();
    await expect(tasks).toBeVisible();
    await expect(checklist.locator(".mp-aid__body")).toHaveCount(0);
    await expect(checklist).toContainText("original shown below");
    await page.reload();
    await openApp(page, server);
    await expect(tasks).toBeVisible();
    const toggle = checklist.getByRole("button", { name: "Show the original checklist" });
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await expect(tasks).toBeHidden();
    await expect(checklist.locator(".mp-aid__body")).toBeVisible();

    // A source jump into the hidden table lands on its aid.
    await page.getByRole("navigation", { name: "Outline" }).getByRole("link", { name: "設定値" }).click();
    await expect(dataTable).toBeInViewport();

    // Live edit: still replaced, with the new row.
    await editFile(server, "deploy-guide.md", (s) => s.replace("| retries | 2 | 冪等な操作のみ |", "| retries | 2 | 冪等な操作のみ |\n| burst | 4 | 追加 |"));
    await expect(dataTable).toContainText("burst", { timeout: 15_000 });
    await expect(table).toBeHidden();

    // Reading aids off: the text is back as written.
    await page.getByTestId("aids-toggle").click();
    await expect(tasks).toBeVisible();
    await expect(table).toBeVisible();
    await expect(article.locator(".mp-replaced")).toHaveCount(0);
  });
});

test.describe("#66 no View menu", () => {
  test("guided reading, follow changes and shortcuts are reachable without it", async ({ page, server }) => {
    await openApp(page, server);
    await expect(page.getByTestId("view-menu")).toHaveCount(0);
    await expect(page.getByRole("banner").getByRole("button", { name: /^View/ })).toHaveCount(0);
    // `?` opens the list, also with focus on a body control (a task checkbox's aid tick).
    await page.locator('article [data-kind="Checklist"]').getByRole("checkbox").first().focus();
    await page.keyboard.press("?");
    await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    // Mouse only: the palette button lists all three.
    await page.getByTestId("open-palette").click();
    const dialog = page.getByRole("dialog", { name: "Command palette" });
    await expect(dialog.getByRole("option", { name: /Guided reading/ })).toBeVisible();
    await expect(dialog.getByRole("option", { name: /Follow live changes/ })).toBeVisible();
    await dialog.getByRole("option", { name: /Keyboard shortcuts/ }).click();
    await expect(page.getByRole("dialog", { name: "Keyboard shortcuts" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await page.getByTestId("open-palette").click();
    await page.getByTestId("palette-input").fill("follow");
    await page.keyboard.press("Enter");
    await expect(page.getByRole("dialog")).toHaveCount(0);
    await page.getByTestId("open-palette").click();
    await expect(page.getByRole("dialog", { name: "Command palette" }).getByRole("option", { name: /Stop following live changes/ })).toBeVisible();
  });
});

test.describe("#65 a flow of the stages a section walks through", () => {
  test.use({ serverOptions: { file: "pipeline.md" } });

  test("the stage subsections become a flow under the section's own text; a node opens its subsection", async ({ page, server }) => {
    await openApp(page, server);
    const graph = page.locator('article [data-kind="NodeGraph"]');
    await expect(graph).toBeVisible();
    await expect(graph.locator("svg [data-node]")).toHaveCount(5);
    // Edge labels are the outputs the headings state.
    await expect(graph.getByTestId("node-graph")).toContainText("BlockTree");
    await expect(graph.getByTestId("node-graph")).toContainText("UI IR");
    // Set in after the intro paragraph, before 3.1.
    const order = await page.locator("article").evaluate((a) => {
      const kids = Array.from(a.children);
      return {
        slot: kids.findIndex((k) => k.querySelector('[data-kind="NodeGraph"]')),
        first: kids.findIndex((k) => k.tagName === "H3"),
      };
    });
    expect(order.slot).toBe(order.first - 1);
    // Picking "planner" then following its source goes to "### 3.3".
    await graph.locator('svg [data-node="planner"]').click();
    await graph.locator(".gui-srclink").last().click();
    await expect(page.locator("article h3", { hasText: "planner" })).toBeInViewport();
  });
});

test.describe("view header hints and the GenUI toggle state", () => {
  test("pin / hide explain themselves on hover; the toggle is colored on and grayed off", async ({ page, server }) => {
    await openApp(page, server);
    const steps = page.locator('article [data-kind="StepNavigator"]');
    await steps.getByRole("button", { name: "Pin Steps to the top" }).hover();
    await expect(page.getByRole("tooltip")).toHaveText("Pin: keep in view while you scroll");
    await steps.getByRole("button", { name: "Hide Steps" }).hover();
    await expect(page.getByRole("tooltip")).toHaveText("Hide: restore later from “hidden”");
    await page.mouse.move(0, 0);

    const toggle = page.getByTestId("aids-toggle");
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await expect(toggle).toHaveClass(/mp-genui-on/);
    const onColor = await toggle.evaluate((e) => getComputedStyle(e).color);
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await expect(toggle).not.toHaveClass(/mp-genui-on/);
    expect(await toggle.evaluate((e) => getComputedStyle(e).color)).not.toBe(onColor);
  });
});
