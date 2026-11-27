// #64: a directory layout drawn with `├──` / `└──` in a code block becomes a
// collapsible tree set right after the block.
import { test, expect, openApp } from "./server";

test.use({ serverOptions: { file: "layout.md" } });

test("the tree renders with comments and markers; folders fold by click", async ({ page, server }) => {
  await openApp(page, server);
  const aid = page.locator('article [data-kind="DirectoryTree"]');
  await expect(aid).toBeVisible();
  // Set into the text right after the code block.
  const prevTag = await aid.evaluate((el) => {
    const n = el.closest(".mp-note-slot")!.previousElementSibling;
    return n?.tagName ?? "";
  });
  expect(prevTag).toBe("PRE");

  const tree = aid.getByRole("tree", { name: "Directory tree: ディレクトリ構成" });
  await expect(aid.getByTestId("directory-tree")).toContainText("9 folders, 6 files");
  const core = tree.getByRole("treeitem", { name: /^mdpeek-core\// });
  await expect(core).toHaveAttribute("aria-expanded", "true");
  await expect(core).toHaveAttribute("aria-level", "3");
  await expect(core).toHaveAttribute("data-marked", "true");
  // Comments are shown as written.
  await expect(core.locator(".gui-dtree__comment").first()).toHaveText("★ 解析の心臓部");
  await expect(tree.getByRole("treeitem", { name: /^Cargo\.toml \[workspace\]$/ })).toBeVisible();
  const libRs = tree.getByRole("treeitem", { name: "lib.rs", exact: true });
  await expect(libRs).toBeVisible();

  // Clicking a folder row closes it and hides its children; again opens it.
  await core.locator(":scope > .gui-dtree__row").click();
  await expect(core).toHaveAttribute("aria-expanded", "false");
  await expect(libRs).toHaveCount(0);
  await core.locator(":scope > .gui-dtree__row").click();
  await expect(core).toHaveAttribute("aria-expanded", "true");
  await expect(libRs).toBeVisible();

  // Collapse all / expand all.
  await aid.getByRole("button", { name: "Collapse all" }).click();
  await expect(tree.getByRole("treeitem")).toHaveCount(1);
  await aid.getByRole("button", { name: "Expand all" }).click();
  await expect(tree.getByRole("treeitem")).toHaveCount(15);
});

test("keyboard: arrows move and fold, Home / End, Enter jumps to the source line", async ({ page, server }) => {
  await openApp(page, server);
  const tree = page.locator('article [data-kind="DirectoryTree"]').getByRole("tree");
  const item = (name: string | RegExp) => tree.getByRole("treeitem", { name });
  const root = item(/^markdown-peek\//);

  // One tab stop: the tree is entered on its first item.
  await root.focus();
  await expect(root).toHaveAttribute("tabindex", "0");
  await page.keyboard.press("ArrowDown");
  await expect(item(/^Cargo\.toml \[workspace\]/)).toBeFocused();
  await page.keyboard.press("ArrowDown");
  const crates = item(/^crates\//);
  await expect(crates).toBeFocused();
  await expect(root).toHaveAttribute("tabindex", "-1");
  // Left closes an open folder, Right opens it, Right again enters it.
  await page.keyboard.press("ArrowLeft");
  await expect(crates).toHaveAttribute("aria-expanded", "false");
  await expect(item(/^mdpeek-core\//)).toHaveCount(0);
  await page.keyboard.press("ArrowRight");
  await expect(crates).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("ArrowRight");
  const core = item(/^mdpeek-core\//);
  await expect(core).toBeFocused();
  // Left on an open folder closes it; Left again moves to the parent.
  await page.keyboard.press("ArrowLeft");
  await expect(core).toHaveAttribute("aria-expanded", "false");
  await page.keyboard.press("ArrowLeft");
  await expect(crates).toBeFocused();
  // Space toggles.
  await page.keyboard.press(" ");
  await expect(crates).toHaveAttribute("aria-expanded", "false");
  await page.keyboard.press(" ");
  await expect(crates).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("End");
  const arch = item(/^architecture\.md/);
  await expect(arch).toBeFocused();
  await page.keyboard.press("Home");
  await expect(root).toBeFocused();

  // Enter jumps to the entry's line: the code block is shown and announced.
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  const pre = page.locator("article pre").first();
  await expect(pre).toHaveClass(/mp-source-hit/);
  await expect(pre).toBeFocused();
  await expect(page.getByTestId("live-region")).toHaveText(/Showing lines 22 to 22/);
});

test("the line chip of an entry jumps to its source line", async ({ page, server }) => {
  await openApp(page, server);
  const tree = page.locator('article [data-kind="DirectoryTree"]').getByRole("tree");
  const web = tree.getByRole("treeitem", { name: /^web\// });
  await web.locator(":scope > .gui-dtree__row").hover();
  const chip = web.locator(":scope > .gui-dtree__row .gui-dtree__line");
  await expect(chip).toHaveText("L17");
  await page.locator("article h1").scrollIntoViewIfNeeded();
  await chip.click();
  await expect(page.locator("article pre").first()).toHaveClass(/mp-source-hit/);
  await expect(page.getByTestId("live-region")).toHaveText(/Showing lines 17 to 17/);
  // The folder did not fold: the chip only jumps.
  await expect(web).toHaveAttribute("aria-expanded", "true");
});
