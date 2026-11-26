// Widgets add an action the text cannot: ticks with progress (kept in this
// browser, file untouched), a stepper, copyable commands, sortable tables.
import { test, expect, openApp } from "./server";

const FAKE = {
  sections: {
    前提条件: { widget: "Checklist", confidence: 0.9, helps: 0.9 },
    手順: { widget: "StepNavigator", confidence: 0.9, helps: 0.9 },
    コマンド: { widget: "ConfigViewer:commands", confidence: 0.9, helps: 0.9 },
  },
};

test.use({ serverOptions: { fake: FAKE } });

test("checklist ticks show progress, survive reload, and reset to the file", async ({ page, server }) => {
  await openApp(page, server);
  const note = page.getByTestId("widgets-pane").locator('[data-kind="Checklist"]');
  await expect(note).toContainText("1 of 4 done");
  const first = note.getByRole("checkbox").first();
  await expect(first).toHaveAttribute("aria-checked", "false");
  await first.press("Space");
  await expect(first).toHaveAttribute("aria-checked", "true");
  await expect(note).toContainText("2 of 4 done");
  await page.reload();
  await expect(note.getByRole("checkbox").first()).toHaveAttribute("aria-checked", "true");
  await note.getByRole("button", { name: "Reset to file" }).click();
  await expect(note).toContainText("1 of 4 done");
  await expect(note.getByRole("button", { name: "Reset to file" })).toHaveCount(0);
  // The Markdown body is never modified by ticking.
  await expect(page.locator("article input[type=checkbox]").first()).not.toBeChecked();
});

test("the stepper moves with buttons and keeps its place per document", async ({ page, server }) => {
  await openApp(page, server);
  const note = page.getByTestId("widgets-pane").locator('[data-kind="StepNavigator"]');
  await expect(note).toContainText("Step 1 of 5");
  await expect(note.getByRole("button", { name: "Previous step" })).toBeDisabled();
  await note.getByRole("button", { name: "Next step" }).click();
  await note.getByRole("button", { name: "Next step" }).click();
  await expect(note).toContainText("Step 3 of 5");
  await expect(note.locator('li[aria-current="step"]')).toContainText("デプロイパイプラインを実行する");
  await expect(note.locator("li.is-done")).toHaveCount(2);
  await page.reload();
  await expect(page.getByTestId("widgets-pane").locator('[data-kind="StepNavigator"]')).toContainText("Step 3 of 5");
  await note.locator("li").nth(4).getByRole("button").first().click();
  await expect(note).toContainText("Step 5 of 5");
  await expect(note.getByRole("button", { name: "Next step" })).toBeDisabled();
});

test("commands are copied line by line or all at once, never run", async ({ page, server, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await openApp(page, server);
  const note = page.getByTestId("widgets-pane").locator('[data-kind="ConfigViewer"]');
  await expect(note.locator("header h3")).toHaveText(/^Commands/);
  await note.getByRole("button", { name: "Copy command 2" }).click();
  await expect(note.getByRole("button", { name: "Copied command 2" })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe("git push origin v2.3.0");
  await note.getByRole("button", { name: "Copy all commands" }).click();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
    'git tag -a v2.3.0 -m "release"\ngit push origin v2.3.0\n./scripts/deploy.sh production',
  );
  await expect(page.getByTestId("live-region")).toHaveText(/Copied all commands/);
});
