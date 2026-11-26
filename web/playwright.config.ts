import { defineConfig } from "@playwright/test";
import { chromePath } from "./e2e/chrome";

// E2E for the mdpeek web UI. Each test starts its own `mdpeek serve` (fake
// selector, no network, isolated HOME) on a free port — see e2e/server.ts.
// Browser: the system Chrome (Playwright's browser download does not run on
// NixOS). Override with MDPEEK_CHROME=/path/to/chrome.
export default defineConfig({
  testDir: "e2e",
  timeout: 45_000,
  expect: { timeout: 8_000 },
  fullyParallel: true,
  workers: process.env.CI ? 2 : 4,
  reporter: [["list"]],
  globalSetup: "./e2e/global-setup.ts",
  outputDir: "../target/playwright",
  use: {
    browserName: "chromium",
    launchOptions: { executablePath: chromePath(), args: ["--disable-gpu"] },
    viewport: { width: 1440, height: 900 },
    trace: "retain-on-failure",
  },
});
