import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** Build the mdpeek binary (with the freshly built web/dist embedded). */
export default function globalSetup() {
  if (process.env.MDPEEK_BIN) return;
  if (!existsSync(path.join(root, "web/dist/app.js"))) throw new Error("web/dist missing: run `npm run build` first");
  execFileSync("sh", [path.join(root, "scripts/cargo-isolated-home.sh"), "build", "--quiet", "--bin", "mdpeek"], {
    cwd: root,
    stdio: "inherit",
  });
}
