// #62: a diagram drawn with box-drawing characters in a code block is redrawn
// as a node graph; every node keeps the source line it was read from.
import { test, expect, openApp } from "./server";

test.use({ serverOptions: { file: "ascii-diagrams.md" } });

const graphFor = (page: import("@playwright/test").Page, heading: string) =>
  page.locator(`[data-widget="section:NodeGraph:${heading}"]`).getByTestId("node-graph");

test("box-and-arrow diagrams become node graphs that jump back to their lines", async ({ page, server }) => {
  await openApp(page, server);

  // §1: the chain inside the "Rust core" box, then the fan-out below it.
  const arch = graphFor(page, "全体アーキテクチャ");
  await expect(arch).toBeVisible();
  await expect(arch.locator("svg [data-node]")).toHaveCount(9);
  await expect(arch.locator(".gui-graph__caption")).toHaveText("9 nodes, 8 links");
  for (const id of ["document.md", "parser", "ir(validate)", "web renderer (preact)", "tui renderer (ratatui)"]) {
    await expect(arch.locator(`svg [data-node="${id}"]`)).toHaveCount(1);
  }
  // The label written beside the vertical connector labels both edges.
  await expect(arch.locator(".gui-graph__edge", { hasText: "UI IR (JSON, serde)" })).toHaveCount(2);
  // The two renderers share a row under ir(validate).
  const y = (id: string) => arch.locator(`svg [data-node="${id}"] rect`).getAttribute("y").then(Number);
  expect(await y("web renderer (preact)")).toBe(await y("tui renderer (ratatui)"));
  expect(await y("ir(validate)")).toBeLessThan(await y("web renderer (preact)"));
  // Text under a node is its note.
  const archChips = arch.getByRole("group", { name: "Select a node" });
  await archChips.getByRole("button", { name: "planner" }).click();
  await expect(arch.getByTestId("node-detail")).toContainText("(semantic model → UI plan)");
  await expect(arch.getByTestId("node-detail")).toContainText("analyzer");

  // §10: a branch from the junction under "SourceRange parser".
  const path = graphFor(page, "依存関係(クリティカルパス)");
  await expect(path.locator("svg [data-node]")).toHaveCount(7);
  await expect(path.locator(".gui-graph__caption")).toHaveText("7 nodes, 5 links");
  const chips = path.getByRole("group", { name: "Select a node" });
  await chips.getByRole("button", { name: "#16 差分" }).click();
  const detail = path.getByTestId("node-detail");
  await expect(detail).toContainText("BlockId 安定化");
  await expect(detail).toContainText("SourceRange parser");
  // Its source chip jumps to the diagram in the body (line 29 is in the block).
  await detail.locator(".gui-srclink").click();
  await expect(page.locator('article [data-line-start="27"]')).toHaveClass(/mp-source-hit/);

  // A directory tree is not a flow: no graph for that section.
  await expect(page.locator('[data-widget="section:NodeGraph:ディレクトリ"]')).toHaveCount(0);
  await expect(page.locator('[data-kind="NodeGraph"]')).toHaveCount(2);
});
