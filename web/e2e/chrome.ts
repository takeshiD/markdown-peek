import { existsSync } from "node:fs";
import { execSync } from "node:child_process";

/** System Chrome: $MDPEEK_CHROME, the Nix profile Chrome, or one on PATH. */
export function chromePath(): string {
  const env = process.env.MDPEEK_CHROME;
  if (env) return env;
  const home = process.env.HOME ?? "";
  const candidates = ["/home/tkcd/.nix-profile/bin/google-chrome", `${home}/.nix-profile/bin/google-chrome`];
  for (const c of candidates) if (existsSync(c)) return c;
  for (const name of ["google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]) {
    try {
      const p = execSync(`command -v ${name}`, { encoding: "utf8", shell: "/bin/sh" }).trim();
      if (p) return p;
    } catch {
      /* next */
    }
  }
  throw new Error("No Chrome found. Set MDPEEK_CHROME to a Chrome/Chromium binary.");
}
