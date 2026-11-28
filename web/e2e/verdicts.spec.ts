// Verdicts: an acceptance record's `R1: satisfied — …` list as one row per
// requirement, filterable by outcome, re-check and cited check.
import { test, expect, openApp } from "./server";

const DOC = `# 受入記録

## 検収判定

合格。前回 R3 で差し戻した点は直っており、REG1 が再実行で pass した。

- R1: satisfied — C1: make build でビルドできた。
- R2: satisfied — C14 で DB への参照が無いことを確認した。C3 と利用者試行 S2 でも見た。
- R3: satisfied — 前回差し戻した点(入力欄で 'esc' がキー操作になる)は直っていた。REG1、C3 が pass。基本操作は C6〜C8 で確認した。
- R4: satisfied — C9 で URL が1引数のまま渡ることを確認した。既定ブラウザでの起動は試していない。
- R5: not satisfied — C3 は pass したが C12 が fail。

## 後続事項

- 設定の警告を出す
`;

test.describe("Verdicts", () => {
  test.use({
    serverOptions: {
      fake: {
        sections: { 検収判定: { widget: "Verdicts", confidence: 0.95, helps: 0.95 } },
        document: { helps: 0.05 },
        default: { helps: 0.05 },
      },
    },
  });

  test("one row per requirement; filter by note and by cited check; text on demand", async ({ page, server }) => {
    server.write("delivery.md", DOC);
    await openApp(page, server);
    await page.getByRole("button", { name: "delivery.md", exact: true }).click();
    const vd = page.getByTestId("verdicts");
    await expect(vd).toBeVisible();
    const rows = vd.locator(".gui-vd__item");
    await expect(rows.locator(".gui-vd__id")).toHaveText(["R1", "R2", "R3", "R4", "R5"]);
    // The prose is folded away until asked for.
    await expect(vd.locator(".gui-vd__text")).toHaveCount(0);

    // Outcome and notes are read from the text.
    await expect(rows.nth(4).locator(".gui-vd__verdict")).toHaveAttribute("data-outcome", "fail");
    await expect(rows.nth(2).getByText("Re-checked")).toBeVisible();
    await expect(rows.nth(3).getByText("Partly untested")).toBeVisible();

    // Filters: only the counts that exist are offered.
    const filters = vd.getByRole("radiogroup", { name: "Show items" });
    await expect(filters.getByRole("radio")).toHaveText(["All5", "Met4", "Not met1", "Re-checked1", "Partly untested1"]);
    await filters.getByRole("radio", { name: /Re-checked/ }).click();
    await expect(rows.locator(".gui-vd__id")).toHaveText(["R3"]);
    await filters.getByRole("radio", { name: /^All/ }).click();

    // Which requirements rest on a check: C3, and C7 inside "C6〜C8".
    const checks = vd.getByRole("group", { name: "Show the items that cite a check" });
    await checks.getByRole("button", { name: "C3", exact: true }).click();
    await expect(rows.locator(".gui-vd__id")).toHaveText(["R2", "R3", "R5"]);
    await expect(vd.getByText("3 of 5 cite C3")).toBeVisible();
    await checks.getByRole("button", { name: "C7", exact: true }).click();
    await expect(rows.locator(".gui-vd__id")).toHaveText(["R3"]);
    await vd.getByRole("button", { name: "Clear" }).click();
    await expect(rows).toHaveCount(5);

    // Opening a row shows its text with the re-check sentence marked.
    await rows.nth(2).getByRole("button", { name: /^R3: satisfied, re-checked/ }).click();
    await expect(rows.nth(2).locator(".gui-vd__mark[data-kind='rechecked']")).toHaveText(/^前回差し戻した点/);
    await expect(rows.nth(2).locator(".gui-vd__text")).toContainText("C6〜C8 で確認した");
  });
});
