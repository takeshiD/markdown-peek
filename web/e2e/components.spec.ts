// Images for docs/components.md: one still per GenUI component (and a GIF
// for the ones whose point is interaction), each built from the Markdown in
// docs/components/src/. Not part of the normal run (it rewrites files):
//
//   MDPEEK_COMPONENT_MEDIA=1 npx playwright test e2e/components.spec.ts
//
// GIFs are assembled with ImageMagick (`magick`).
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import type { Locator, Page } from "@playwright/test";
import { test, expect, openApp, repoRoot } from "./server";

const SRC = path.join(repoRoot, "docs/components/src");
const OUT = path.join(repoRoot, "docs/components/media");

type Act = (aid: Locator, page: Page) => Promise<void>;

interface Item {
  /** Source file (docs/components/src/<id>.md) and image name. */
  id: string;
  /** Candidate id the scripted model picks. */
  widget: string;
  /** Section heading it is picked for; none = the document-level view. */
  section?: string;
  /** Steps of the GIF, one frame after each. */
  gif?: Act[];
}

const click = (l: (a: Locator) => Locator): Act => async (aid) => {
  await l(aid).click();
};

const ITEMS: Item[] = [
  {
    id: "steps",
    widget: "StepNavigator",
    section: "手順",
    gif: [0, 1, 2, 3].map(() => click((a) => a.getByRole("button", { name: "Next step" }))),
  },
  {
    id: "checklist",
    widget: "Checklist",
    section: "前提条件",
    gif: [0, 1, 3].map((i) => click((a) => a.getByRole("checkbox").nth(i))),
  },
  { id: "table-status", widget: "DataTable:status", section: "移行状況" },
  {
    id: "table-sortable",
    widget: "DataTable:sortable",
    section: "設定値",
    gif: [
      click((a) => a.getByRole("button", { name: /^項目/ })),
      click((a) => a.getByRole("button", { name: /^項目/ })),
      async (a) => a.getByLabel("Filter table rows").pressSequentially("s", { delay: 80 }),
      async (a) => a.getByLabel("Filter table rows").pressSequentially("ec", { delay: 80 }),
      async (a) => a.getByLabel("Filter table rows").fill(""),
    ],
  },
  {
    id: "chart",
    widget: "Chart",
    section: "月別リクエスト数",
    gif: [
      async (a) => a.locator('[role="group"][aria-label*="chart" i]').first().focus(),
      ...[0, 1, 2, 3, 4].map((): Act => async (_a, page) => page.keyboard.press("ArrowRight")),
    ],
  },
  {
    id: "flow",
    widget: "NodeGraph",
    section: "実行フロー",
    gif: ["Planning", "Verification", "Release"].map((n) => click((a) => a.getByRole("group", { name: "Select a node" }).getByRole("button", { name: n, exact: true }))),
  },
  { id: "flow-diagram", widget: "NodeGraph:diagram", section: "構成図" },
  { id: "flow-stages", widget: "NodeGraph:stages", section: "処理の各段" },
  {
    id: "comparison",
    widget: "Comparison",
    section: "実行方式の比較",
    gif: ["実行速度", "コスト効率", "品質", "Average"].map((n) => click((a) => a.getByRole("radio", { name: n }))),
  },
  {
    id: "directory-tree",
    widget: "DirectoryTree",
    section: "ディレクトリ構成",
    gif: [
      click((a) => a.getByRole("treeitem", { name: /^mdpeek-core\// }).locator(".gui-dtree__row").first()),
      click((a) => a.getByRole("treeitem", { name: /^web\// }).locator(".gui-dtree__row").first()),
      click((a) => a.getByRole("button", { name: "Collapse all" })),
      click((a) => a.getByRole("button", { name: "Expand all" })),
    ],
  },
  { id: "config", widget: "ConfigViewer:config", section: "設定ファイル" },
  { id: "timeline", widget: "Timeline", section: "リリース履歴" },
  { id: "callout-risk", widget: "Callout:risk", section: "リスク" },
  { id: "doc-outline", widget: "SemanticOutline" },
  { id: "doc-risks", widget: "RiskPanel" },
  { id: "doc-questions", widget: "OpenQuestions" },
  { id: "doc-actions", widget: "ActionItems" },
  { id: "doc-decisions", widget: "DecisionLog" },
];

test.describe("component catalogue media", () => {
  test.skip(!process.env.MDPEEK_COMPONENT_MEDIA, "set MDPEEK_COMPONENT_MEDIA=1 to regenerate docs/components/media");
  test.use({ viewport: { width: 1440, height: 1100 } });

  for (const it of ITEMS) {
    test.describe(it.id, () => {
      const pick = { widget: it.widget, confidence: 0.95, helps: 0.95 };
      test.use({
        serverOptions: {
          fake: it.section
            ? { sections: { [it.section]: pick }, document: { helps: 0.05 }, default: { helps: 0.05 } }
            : { document: pick, default: { helps: 0.05 } },
        },
      });

      test("still and gif", async ({ page, server }) => {
        mkdirSync(OUT, { recursive: true });
        server.write(`${it.id}.md`, readFileSync(path.join(SRC, `${it.id}.md`), "utf8"));
        await page.emulateMedia({ colorScheme: "light", reducedMotion: "reduce" });
        await openApp(page, server);
        await page.getByRole("button", { name: `${it.id}.md`, exact: true }).click();
        await expect(page.getByTestId("breadcrumb-file")).toContainText(`${it.id}.md`);
        const kind = it.widget.split(":")[0];
        const aid = page.locator(`article [data-kind="${kind}"][data-scope="${it.section ? "section" : "document"}"]`).first();
        await expect(aid).toBeVisible();
        if ((await aid.getAttribute("data-open")) === "false") await aid.locator(".mp-aid__toggle").first().click();
        await aid.scrollIntoViewIfNeeded();
        await page.mouse.move(0, 999);
        await page.waitForTimeout(400);
        await aid.screenshot({ path: path.join(OUT, `${it.id}.png`) });

        if (!it.gif) return;
        const dir = path.join(OUT, `.frames-${it.id}`);
        rmSync(dir, { recursive: true, force: true });
        mkdirSync(dir);
        const frames: string[] = [];
        const shot = async () => {
          await page.mouse.move(0, 999);
          await page.waitForTimeout(350);
          const f = path.join(dir, `${String(frames.length).padStart(2, "0")}.png`);
          await aid.screenshot({ path: f });
          frames.push(f);
        };
        await shot();
        for (const act of it.gif) {
          await act(aid, page);
          await shot();
        }
        // Same canvas for every frame (a fold changes the height).
        const sizes = frames.map((f) => execFileSync("magick", ["identify", "-format", "%w %h", f]).toString().split(" ").map(Number));
        const w = Math.max(...sizes.map((s) => s[0]));
        const h = Math.max(...sizes.map((s) => s[1]));
        execFileSync("magick", [
          "-delay", "110", "-loop", "0",
          ...frames.flatMap((f) => [f, "-background", "#fbfbfa", "-gravity", "northwest", "-extent", `${w}x${h}`]),
          "-layers", "Optimize",
          path.join(OUT, `${it.id}.gif`),
        ]);
        rmSync(dir, { recursive: true, force: true });
      });
    });
  }
});
