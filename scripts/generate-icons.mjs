#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { copyFileSync, cpSync, rmSync } from "node:fs";
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

// Launcher icons go into the Android project; there is no iOS build.
cpSync(
  new URL("../src-tauri/icons/cliprigo/android", import.meta.url),
  new URL("../src-tauri/gen/android/app/src/main/res", import.meta.url),
  { recursive: true },
);
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
console.log(
  "Updated desktop and Android icons and static/cliprigo-favicon.png",
);
