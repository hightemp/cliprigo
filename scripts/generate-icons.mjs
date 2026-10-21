#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { copyFileSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const root = fileURLToPath(new URL("../", import.meta.url));
const result = spawnSync(
  process.execPath,
  [
    require.resolve("@tauri-apps/cli/tauri.js"),
    "icon",
    "assets/cliprigo-app-icon.png",
    "--output",
    "src-tauri/icons/cliprigo",
  ],
  { cwd: root, stdio: "inherit" },
);

if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

// The CLI also emits mobile assets; Cliprigo currently targets desktop only.
for (const platform of ["android", "ios"]) {
  rmSync(new URL(`../src-tauri/icons/cliprigo/${platform}`, import.meta.url), {
    recursive: true,
    force: true,
  });
}

copyFileSync(
  new URL("../src-tauri/icons/cliprigo/32x32.png", import.meta.url),
  new URL("../static/cliprigo-favicon.png", import.meta.url),
);
console.log("Updated desktop icons and static/cliprigo-favicon.png");
