// #55 / #56: local images (Markdown `![]()` and raw `<img>`) render in the
// browser preview; paths escaping the repository are not served.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test, expect, openApp } from "./server";

// 2×3 opaque PNG.
const PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAIAAAADCAIAAAA2iEnWAAAADklEQVR4nGNoAAMGFAoAXpUJAdY1eq0AAAAASUVORK5CYII=",
  "base64",
);
const SVG = '<svg xmlns="http://www.w3.org/2000/svg" width="7" height="5"><rect width="7" height="5" fill="red"/></svg>';

test("relative Markdown and raw <img> images load; external URLs are kept; escapes are refused", async ({ page, server }) => {
  const docs = path.join(server.ws, "pics");
  mkdirSync(path.join(docs, "img"), { recursive: true });
  writeFileSync(path.join(docs, "img", "dot.png"), PNG);
  writeFileSync(path.join(docs, "img", "my logo.svg"), SVG);
  writeFileSync(path.join(server.ws, "top.png"), PNG);
  writeFileSync(path.join(server.dir, "outside.png"), PNG);
  writeFileSync(
    path.join(docs, "doc.md"),
    [
      "# Images",
      "",
      "![md](img/dot.png)",
      "",
      '<img id="raw" src="img/my%20logo.svg" />',
      "",
      "![parent](../top.png)",
      "",
      "inline <img id=\"inline\" src='./img/dot.png'> html",
      "",
      "![ext](https://example.invalid/x.png)",
      "",
    ].join("\n"),
  );
  expect((await page.request.post(`${server.url}/api/select`, { data: { path: path.join(docs, "doc.md") } })).status()).toBe(200);
  await openApp(page, server, { waitLayout: false });
  await expect(page.locator("article h1")).toHaveText(/Images/);

  const loaded = async (sel: string) =>
    page.locator(sel).evaluate(async (img: HTMLImageElement) => {
      if (!img.complete) await new Promise((r) => { img.addEventListener("load", r, { once: true }); img.addEventListener("error", r, { once: true }); });
      return [img.naturalWidth, img.naturalHeight];
    });
  expect(await loaded('article img[alt="md"]')).toEqual([2, 3]);
  expect(await loaded("article img#raw")).toEqual([7, 5]);
  expect(await loaded('article img[alt="parent"]')).toEqual([2, 3]);
  expect(await loaded("article img#inline")).toEqual([2, 3]);
  await expect(page.locator('article img[alt="ext"]')).toHaveAttribute("src", "https://example.invalid/x.png");

  // Outside the repository, absolute paths and non-images: 404.
  for (const p of ["../../outside.png", path.join(server.dir, "outside.png"), "doc.md", "../../../../../../etc/passwd"]) {
    const res = await page.request.get(`${server.url}/asset?path=${encodeURIComponent(p)}`);
    expect(res.status(), p).toBe(404);
  }
});
