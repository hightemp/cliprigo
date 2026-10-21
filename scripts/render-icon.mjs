#!/usr/bin/env node
// Renders assets/icon.svg to a 1024×1024 PNG; then `pnpm tauri icon assets/icon.png`.
//   node scripts/render-icon.mjs [out.png]
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { chromium } from "playwright-core";

const out = resolve(process.argv[2] ?? "assets/icon.png");
const svg = readFileSync(
  new URL("../assets/icon.svg", import.meta.url),
  "utf8",
);
const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
});
const page = await browser.newPage({ viewport: { width: 1024, height: 1024 } });
await page.setContent(
  `<html><body style="margin:0;background:transparent">${svg}</body></html>`,
);
await page.locator("svg").screenshot({ path: out, omitBackground: true });
await browser.close();
console.log(out);
