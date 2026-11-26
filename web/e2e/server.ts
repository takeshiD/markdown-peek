// Start an isolated `mdpeek serve` for one test.
import { spawn, execFileSync, type ChildProcess } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, writeFileSync, rmSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test as base, expect, type Page } from "@playwright/test";

export const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const DOCS = path.join(repoRoot, "web/e2e/docs");

export interface ServerOptions {
  /** File to open (relative to the workspace). */
  file?: string;
  /** `[select]` overrides (provider, fake script fields…). */
  provider?: "fake" | "rules" | "jev";
  fake?: Record<string, unknown>;
  debounceMs?: number;
  /** Make the workspace a git repo with a second worktree (explorer tests). */
  git?: boolean;
}

export interface Server {
  url: string;
  dir: string;
  /** Workspace directory that holds the documents. */
  ws: string;
  proc: ChildProcess;
  log: () => string;
  write: (rel: string, text: string) => void;
  read: (rel: string) => string;
  stop: () => Promise<void>;
}

const strip = (s: string) => s.replace(/\x1b\[[0-9;]*m/g, "");

export async function startServer(opts: ServerOptions = {}): Promise<Server> {
  const dir = mkdtempSync(path.join(tmpdir(), "mdpeek-e2e-"));
  const home = path.join(dir, "home");
  const ws = path.join(dir, "repo");
  mkdirSync(home);
  mkdirSync(ws);
  cpSync(DOCS, ws, { recursive: true });
  if (opts.git) {
    const g = (...a: string[]) =>
      execFileSync("git", ["-c", "user.name=e2e", "-c", "user.email=e2e@example.invalid", "-c", "init.defaultBranch=main", ...a], {
        cwd: ws,
        env: { ...process.env, HOME: home },
        stdio: "ignore",
      });
    g("init", "-q");
    g("add", "-A");
    g("commit", "-q", "-m", "init");
    g("worktree", "add", "-q", "-b", "feature-x", path.join(dir, "repo-feature"));
  }
  const fake = { model: "fake-e2e", delay_ms: 0, doc_type: { value: "runbook", confidence: 0.95 }, default: "rules", ...(opts.fake ?? {}) };
  writeFileSync(path.join(dir, "fake.json"), JSON.stringify(fake));
  const provider = opts.provider ?? "fake";
  writeFileSync(
    path.join(dir, "config.toml"),
    `[select]\nprovider = "${provider}"\nfake_script = "${path.join(dir, "fake.json")}"\ndebounce_ms = ${opts.debounceMs ?? 150}\n`,
  );
  const bin = process.env.MDPEEK_BIN ? path.resolve(process.env.MDPEEK_BIN) : path.join(repoRoot, "target/debug/mdpeek");
  const proc = spawn(bin, ["-c", path.join(dir, "config.toml"), "serve", opts.file ?? "deploy-guide.md", "--port", "0", "--host", "127.0.0.1"], {
    cwd: ws,
    env: { PATH: process.env.PATH ?? "", HOME: home, XDG_CONFIG_HOME: path.join(home, ".config"), RUST_LOG: "info" },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let out = "";
  proc.stdout!.on("data", (d) => (out += strip(String(d))));
  proc.stderr!.on("data", (d) => (out += strip(String(d))));
  const url = await new Promise<string>((resolve, reject) => {
    const t = setTimeout(() => reject(new Error(`mdpeek did not start:\n${out}`)), 15_000);
    const check = () => {
      const m = out.match(/Listening on (http:\/\/[0-9.:]+)/);
      if (m) {
        clearTimeout(t);
        resolve(m[1]);
      }
    };
    proc.stdout!.on("data", check);
    proc.stderr!.on("data", check);
    proc.on("exit", (c) => reject(new Error(`mdpeek exited (${c}):\n${out}`)));
  });
  return {
    url,
    dir,
    ws,
    proc,
    log: () => out,
    write: (rel, text) => writeFileSync(path.join(ws, rel), text),
    read: (rel) => readFileSync(path.join(ws, rel), "utf8"),
    stop: async () => {
      proc.kill("SIGTERM");
      await new Promise((r) => setTimeout(r, 50));
      rmSync(dir, { recursive: true, force: true });
    },
  };
}

/** `server` fixture: override per file with `test.use({ serverOptions: {...} })`. */
export const test = base.extend<{ serverOptions: ServerOptions; server: Server }>({
  serverOptions: [{}, { option: true }],
  server: async ({ serverOptions }, use) => {
    const s = await startServer(serverOptions);
    await use(s);
    await s.stop();
  },
});
export { expect };

/** Open the app and wait for the first reading-aid layout. */
export async function openApp(page: Page, server: Server, opts: { waitLayout?: boolean } = {}) {
  await page.goto(server.url);
  await expect(page.locator("article.markdown-body h1")).toBeVisible();
  if (opts.waitLayout !== false) {
    await expect(page.getByTestId("selector-status")).toHaveAttribute("data-state", /ready|fallback/);
  }
}

/** Edit a file the way an editor does (and make sure the mtime moves on). */
export async function editFile(server: Server, rel: string, fn: (s: string) => string) {
  // The watcher compares mtimes at 1 s resolution: wait for the next second.
  await new Promise((r) => setTimeout(r, 1100));
  server.write(rel, fn(server.read(rel)));
}
