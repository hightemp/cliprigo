#!/usr/bin/env node
// UI smoke test and screenshots without the Rust backend.
//
// Starts the Vite dev server, opens the app in headless Chrome with a fake
// `window.__TAURI_INTERNALS__` that answers commands with canned data and can emit
// backend events, walks through the main screens, checks what the UI sends back,
// and saves screenshots.
//
//   node scripts/ui-preview.mjs [output-dir]
//
// Env: CHROME_PATH (default /usr/bin/google-chrome), PREVIEW_PORT (default 1430),
// PREVIEW_MOBILE=1 (an Android phone: narrow touch screen, Android user agent),
// PREVIEW_THEME=light|dark (one theme for every scenario, e.g. for site screenshots).

import { spawn } from "node:child_process";
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";
import { chromium } from "playwright-core";

const OUT = resolve(process.argv[2] ?? "ui-preview");
const PORT = Number(process.env.PREVIEW_PORT ?? 1430);
const CHROME = process.env.CHROME_PATH ?? "/usr/bin/google-chrome";
const BASE = `http://127.0.0.1:${PORT}/`;
const MOBILE = process.env.PREVIEW_MOBILE === "1";
const THEME = process.env.PREVIEW_THEME;
const VIEWPORT = MOBILE
  ? {
      viewport: { width: 412, height: 915 },
      isMobile: true,
      hasTouch: true,
      userAgent:
        "Mozilla/5.0 (Linux; Android 15; Pixel 6) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/140.0.0.0 Mobile Safari/537.36",
    }
  : { viewport: { width: 900, height: 640 } };

// ---------- fake data ----------

/** A 16:9 gradient placeholder, so screenshots don't depend on the network. */
function thumb(hue) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${hue},70%,55%)"/><stop offset="1" stop-color="hsl(${hue + 40},70%,35%)"/></linearGradient></defs><rect width="320" height="180" fill="url(#g)"/><circle cx="160" cy="90" r="28" fill="rgba(255,255,255,.85)"/><path d="M152 76l22 14-22 14z" fill="hsl(${hue},60%,40%)"/></svg>`;
  return `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
}

const NOW = 1792600000;

function record(id, overrides = {}) {
  return {
    id,
    videoId: `vid${id}`,
    url: `https://www.youtube.com/watch?v=vid${id}`,
    title: `Video ${id}`,
    uploader: "Demo Channel",
    duration: 245,
    thumbnailUrl: thumb(id * 47),
    format: "video",
    quality: null,
    codec: "auto",
    filePath: null,
    fileSize: null,
    status: "queued",
    error: null,
    playlistId: null,
    playlistTitle: null,
    createdAt: NOW - id * 3600,
    completedAt: null,
    outputDir: "/home/user/Downloads",
    ...overrides,
  };
}

const settings = (language) => ({
  downloadDir: "/home/user/Downloads",
  defaultFormat: "video",
  defaultQuality: null,
  defaultCodec: "auto",
  concurrency: 2,
  filenameTemplate: "%(title)s [%(id)s].%(ext)s",
  language,
  autoUpdateYtdlp: true,
  proxy: "",
});

const installed = [
  { component: "ytdlp", installed: true, version: "2026.08.19" },
  { component: "ffmpeg", installed: true, version: "n8.1.2-1" },
  { component: "deno", installed: true, version: "v2.9.8" },
];

const VIDEO = {
  type: "video",
  id: "vid100",
  url: "https://www.youtube.com/watch?v=vid100",
  title: "Sunset timelapse over the mountains — 4K",
  uploader: "Nature Frames",
  duration: 754,
  thumbnail: thumb(20),
  heights: [2160, 1440, 1080, 720, 480, 360, 240, 144],
};

const PLAYLIST = {
  type: "playlist",
  id: "PLdemo",
  url: "https://www.youtube.com/playlist?list=PLdemo",
  title: "Learn to cook: weeknight dinners",
  uploader: "Kitchen Basics",
  entries: [
    ["Five-minute garlic noodles", 312, true],
    ["One-pan roasted vegetables", 498, true],
    ["Crispy chickpea salad", 405, true],
    ["[Private video]", null, false],
    ["Lemon herb chicken thighs", 611, true],
    ["Quick tomato soup", 287, true],
  ].map(([title, duration, available], i) => ({
    id: `pl${i}`,
    url: `https://www.youtube.com/watch?v=pl${i}`,
    title,
    uploader: "Kitchen Basics",
    duration,
    thumbnail: available ? thumb(100 + i * 30) : null,
    available,
  })),
};

const progress = (percent, overrides = {}) => ({
  stage: "downloading",
  part: 1,
  percent,
  downloaded: Math.round(percent * 1.8e6),
  total: 180e6,
  speed: 4.2e6,
  eta: Math.round((100 - percent) * 0.4),
  postprocessor: null,
  ...overrides,
});

// ---------- browser-side fake bridge ----------

/** Runs in the page before the app. `scenario.responses[cmd]` is returned for `cmd`. */
function installFakeTauri(scenario) {
  const callbacks = {};
  const listeners = {};
  let nextId = 1;
  window.__calls = [];
  window.__TAURI_INTERNALS__ = {
    transformCallback(callback, once) {
      const id = nextId++;
      callbacks[id] = (payload) => {
        if (once) delete callbacks[id];
        return callback(payload);
      };
      return id;
    },
    unregisterCallback(id) {
      delete callbacks[id];
    },
    convertFileSrc: (path) => path,
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
    async invoke(cmd, args = {}) {
      window.__calls.push({
        cmd,
        args: JSON.parse(JSON.stringify(args ?? {})),
      });
      if (cmd === "plugin:event|listen") {
        (listeners[args.event] ??= []).push(args.handler);
        return args.handler;
      }
      if (cmd === "plugin:event|unlisten") return null;
      let response = scenario.responses[cmd];
      // `{ __sequence: [a, b] }` answers a, then b, then b again.
      if (
        response &&
        typeof response === "object" &&
        "__sequence" in response
      ) {
        const calls = window.__calls.filter((c) => c.cmd === cmd).length;
        const list = response.__sequence;
        response = list[Math.min(calls, list.length) - 1];
      }
      if (response && typeof response === "object" && "__error" in response) {
        throw response.__error;
      }
      return response === undefined ? null : structuredClone(response);
    },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__emit = (event, payload) =>
    (listeners[event] ?? []).forEach((id) =>
      callbacks[id]?.({ event, id, payload }),
    );
}

// ---------- scenarios ----------

const ready = (language, extra = {}) => ({
  settings_get: settings(language),
  components_status: installed,
  queue_list: [],
  history_list: [],
  find_duplicates: [],
  url_inspect: { normalized: VIDEO.url, isYoutube: true, singleVideo: true },
  ...extra,
});

const SEARCH_TITLES = [
  "Mountain sunrise timelapse in 4K",
  "Rocky Mountains — 3 hours of relaxing scenery",
  "How to shoot timelapses: complete guide",
  "Northern lights over Norway 8K",
  "Alpine lakes from a drone",
];

/** The search result row with exactly this title. */
const resultRow = (page, title) =>
  page.locator("li").filter({ has: page.getByText(title, { exact: true }) });

/** One page of search results: `count` videos from `start`. */
function searchPage(start, count, overrides = {}) {
  return Array.from({ length: count }, (_, i) => {
    const n = start + i;
    return {
      id: `res${String(n).padStart(8, "0")}`,
      url: `https://www.youtube.com/watch?v=res${String(n).padStart(8, "0")}`,
      title: SEARCH_TITLES[n % SEARCH_TITLES.length] + (n >= 5 ? ` #${n}` : ""),
      uploader: ["Nature Frames", "4K Planet Earth", "Sky Watch"][n % 3],
      duration: 180 + n * 97,
      thumbnail: thumb(n * 37),
      available: true,
      viewCount: [1222317, 338921, 950, 15600000, 21][n % 5],
      live: false,
      ...overrides[n],
    };
  });
}

const SEARCH_RESULTS = searchPage(0, 20, { 3: { live: true, duration: null } });

const scenarios = [
  {
    name: "01-setup-error",
    theme: "light",
    responses: {
      settings_get: settings("en"),
      components_status: installed.map((c) => ({
        ...c,
        installed: false,
        version: null,
      })),
      queue_list: [],
      components_install: {
        __error: { code: "rate_limited", message: "rate limited" },
      },
    },
    async run(page, check) {
      await check.text("Try again");
      await check.text("GitHub limited the requests");
    },
  },
  {
    name: "02-empty",
    theme: "light",
    responses: ready("ru"),
    async run(page, check) {
      await check.text("Сейчас ничего не скачивается");
    },
  },
  {
    name: "03-video-card",
    theme: "light",
    responses: ready("ru", {
      video_probe: VIDEO,
      settings_get: {
        ...settings("ru"),
        downloadDir:
          "/home/user/Videos/YouTube/Downloads/a very long folder name to check truncation",
      },
    }),
    async run(page, check) {
      await page.fill("input[type=url]", VIDEO.url);
      await page.keyboard.press("Enter");
      await check.text(VIDEO.title);
      const quality = page.getByLabel("Качество");
      check.equal(
        await quality.locator("option:checked").textContent(),
        "Лучшее доступное",
      );
      await quality.selectOption({ label: "1080p" });
      // A long folder is cut at the start (the folder name stays visible) and must
      // not overflow the card.
      const folder = page.locator("[title^='/home/user/Videos']");
      await folder.waitFor({ timeout: 5000 });
      const fits = await folder.evaluate((el) => {
        const card = el.closest(".card").getBoundingClientRect();
        return el.getBoundingClientRect().right <= card.right;
      });
      check.equal(fits, true);
      await page.screenshot({ path: `${OUT}/03-video-card.png` });

      await page.getByRole("button", { name: "Скачать" }).last().click();
      const add = await check.call("queue_add");
      check.equal(add.args.items[0].format, {
        kind: "video",
        maxHeight: 1080,
        codec: "auto",
      });
      check.equal(add.args.items[0].videoId, VIDEO.id);
      await check.text("Добавлено в очередь");
    },
    noFinalShot: true,
  },
  {
    name: "04-queue",
    theme: "light",
    responses: ready("ru"),
    async run(page, check) {
      await check.listening("download://status");
      const emit = (event, payload) =>
        page.evaluate(([e, p]) => window.__emit(e, p), [event, payload]);
      const a = record(1, { title: VIDEO.title, status: "downloading" });
      const b = record(2, {
        title: "Morning jazz for focus",
        format: "audio_mp3",
        status: "processing",
      });
      const c = record(3, { title: "Street food tour of the old town" });
      for (const r of [a, b, c])
        await emit("download://status", { type: "status", record: r });
      await emit("download://progress", {
        type: "progress",
        id: 1,
        snapshot: progress(42, { part: 2 }),
      });
      await emit("download://progress", {
        type: "progress",
        id: 2,
        snapshot: progress(100, {
          stage: "processing",
          postprocessor: "ExtractAudio",
        }),
      });
      await check.text("часть 2");
      await check.text("Конвертация звука…");
      await check.text("Ожидает");
      await check.text("3 загрузки");

      // Finished downloads leave the queue.
      await emit("download://status", {
        type: "status",
        record: { ...c, status: "canceled" },
      });
      await check.text("2 загрузки");

      if (MOBILE) {
        // Android keeps the app alive with a notification while anything runs,
        // updated when the count changes, not on every progress event.
        const texts = await page.evaluate(() =>
          window.__calls
            .filter((x) => x.cmd === "keep_alive")
            .map((x) => x.args.text),
        );
        check.equal(texts, [
          null,
          "Скачивается 1 видео",
          "Скачиваются 2 видео",
          "Скачиваются 3 видео",
          "Скачиваются 2 видео",
        ]);
      }
    },
  },
  {
    name: "05-playlist",
    theme: "dark",
    responses: ready("en", {
      url_inspect: {
        normalized: PLAYLIST.url,
        isYoutube: true,
        singleVideo: false,
      },
      video_probe: PLAYLIST,
      find_duplicates: [record(9, { videoId: "pl1", status: "completed" })],
    }),
    async run(page, check) {
      await page.fill("input[type=url]", PLAYLIST.url);
      await page.keyboard.press("Enter");
      await check.text(PLAYLIST.title);
      await check.text("Already downloaded");
      await check.text("Unavailable");
      // Available minus the duplicate are preselected.
      await check.text("4 of 5 selected");
      await check.text("Download 4 videos");
      await page.screenshot({ path: `${OUT}/05-playlist.png` });

      await page.getByRole("radio", { name: "Audio only" }).click();
      await page.getByRole("button", { name: "Download 4 videos" }).click();
      const add = await check.call("queue_add");
      check.equal(add.args.items.length, 4);
      check.equal(add.args.items[0].format, { kind: "audio_mp3" });
      check.equal(add.args.items[0].playlistId, PLAYLIST.id);
    },
    noFinalShot: true,
  },
  {
    name: "06-duplicate",
    theme: "light",
    responses: ready("ru", {
      video_probe: VIDEO,
      find_duplicates: [
        record(5, {
          videoId: VIDEO.id,
          title: VIDEO.title,
          status: "completed",
        }),
      ],
    }),
    async run(page, check) {
      await page.fill("input[type=url]", VIDEO.url);
      await page.keyboard.press("Enter");
      await check.text("Уже скачано");
      await page.getByRole("button", { name: "Скачать" }).last().click();
      await check.text("уже скачано в этом формате");
      check.equal(await check.count("queue_add"), 0);
      await page.screenshot({ path: `${OUT}/06-duplicate.png` });
      await page.getByRole("button", { name: "Отмена" }).click();

      // Audio is another file: no warning, downloads at once.
      // Picked from the keyboard: arrows select and move focus, wrapping around.
      const video = page.getByRole("radio", { name: "Видео" });
      const audio = page.getByRole("radio", { name: "Только звук" });
      check.equal(await audio.getAttribute("tabindex"), "-1");
      await video.focus();
      await page.keyboard.press("ArrowLeft");
      check.equal(await audio.getAttribute("aria-checked"), "true");
      check.equal(
        await audio.evaluate((el) => el === document.activeElement),
        true,
      );
      await page.keyboard.press("ArrowRight");
      check.equal(await video.getAttribute("aria-checked"), "true");
      await page.keyboard.press("End");
      await page.getByText("Уже скачано").waitFor({ state: "detached" });
      await page.getByRole("button", { name: "Скачать" }).last().click();
      await check.call("queue_add");
    },
    noFinalShot: true,
  },
  {
    name: "07-not-youtube",
    theme: "light",
    responses: ready("en", {
      url_inspect: {
        normalized: "https://example.com/v",
        isYoutube: false,
        singleVideo: false,
      },
      video_probe: {
        __error: { code: "ytdlp_unsupported", message: "Unsupported URL" },
      },
      diagnostics: "DIAG REPORT",
    }),
    async run(page, check) {
      await page.fill("input[type=url]", "https://example.com/v");
      await page.keyboard.press("Enter");
      await check.text("doesn't look like a YouTube link");
      await check.text("This link is not supported.");

      await page.getByRole("button", { name: "Copy details" }).click();
      const copy = await check.call("plugin:clipboard-manager|write_text");
      const text = JSON.stringify(copy.args);
      for (const needle of [
        "Action: probe https://example.com/v",
        "Error code: ytdlp_unsupported",
        "Message: Unsupported URL",
        "DIAG REPORT",
      ]) {
        if (!text.includes(needle))
          throw new Error(`clipboard lacks ${needle}: ${text}`);
      }
      await check.text("Details copied");
    },
  },
  {
    name: "08-history",
    theme: "light",
    responses: ready("ru", {
      history_list: [
        {
          ...record(11, {
            title: VIDEO.title,
            status: "completed",
            filePath: "/d/a.mp4",
            fileSize: 1.2e9,
            completedAt: NOW,
          }),
          fileExists: true,
        },
        {
          ...record(12, {
            title: "Morning jazz for focus",
            format: "audio_mp3",
            status: "completed",
            filePath: "/d/b.mp3",
            fileSize: 8.4e6,
            completedAt: NOW - 86400,
          }),
          fileExists: false,
        },
        {
          ...record(13, {
            title: "Street food tour of the old town",
            status: "failed",
            error: JSON.stringify({
              code: "ytdlp_bot_check",
              message: "Sign in to confirm you're not a bot",
            }),
          }),
          fileExists: false,
        },
        {
          ...record(14, {
            title: "Five-minute garlic noodles",
            status: "interrupted",
            playlistTitle: "Learn to cook",
          }),
          fileExists: false,
        },
      ],
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "История" }).click();
      await check.text("Файл не найден");
      await check.text("YouTube попросил подтвердить");
      // The status filter also lists "Прервано", so look at the badge.
      await page
        .locator(".badge", { hasText: "Прервано" })
        .waitFor({ timeout: 5000 });
      await page.screenshot({ path: `${OUT}/08-history.png` });

      await page.getByRole("button", { name: "Повторить" }).first().click();
      const retry = await check.call("queue_retry");
      check.equal(retry.args.id, 13);
      await page.getByRole("button", { name: "Открыть", exact: true }).click();
      check.equal((await check.call("history_open")).args.id, 11);
    },
    noFinalShot: true,
  },
  {
    name: "09-history-remove",
    theme: "dark",
    responses: ready("en", {
      history_list: [
        {
          ...record(11, {
            title: VIDEO.title,
            status: "completed",
            filePath: "/d/a.mp4",
            fileSize: 1.2e9,
            completedAt: NOW,
          }),
          fileExists: true,
        },
      ],
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "History" }).click();
      await page.getByRole("button", { name: "Remove" }).click();
      await check.text("Also delete the file from disk");
      await page.screenshot({ path: `${OUT}/09-history-remove.png` });
      await page.getByLabel("Also delete the file from disk").check();
      await page
        .getByRole("dialog")
        .getByRole("button", { name: "Remove" })
        .click();
      const del = await check.call("history_delete");
      check.equal(del.args, { id: 11, deleteFile: true });
    },
    noFinalShot: true,
  },
  {
    name: "10-settings",
    theme: "light",
    responses: ready("en", {
      settings_get: {
        ...settings("en"),
        downloadDir:
          "/home/user/Videos/YouTube/Downloads/a very long folder name to check truncation",
      },
      components_check_updates: [
        {
          component: "ytdlp",
          current: "2026.08.19",
          latest: "2026.10.15",
          updateAvailable: true,
        },
        {
          component: "ffmpeg",
          current: "n8.1.2-1",
          latest: "n8.1.2-1",
          updateAvailable: false,
        },
        {
          component: "deno",
          current: "v2.9.8",
          latest: "v2.9.8",
          updateAvailable: false,
        },
      ],
      settings_set: settings("en"),
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Settings" }).click();
      await page.getByRole("button", { name: "Check for updates" }).click();
      await check.text("Update to 2026.10.15");
      await page.screenshot({ path: `${OUT}/10-settings.png` });

      await page.getByLabel("Simultaneous downloads").selectOption("3");
      const set = await check.call("settings_set");
      check.equal(set.args.settings.concurrency, 3);
    },
    noFinalShot: true,
  },
  {
    name: "11-close-dialog",
    theme: "dark",
    responses: ready("ru", {
      queue_list: [
        {
          ...record(1, { title: VIDEO.title, status: "downloading" }),
          progress: progress(63),
        },
        { ...record(2, { title: "Morning jazz for focus" }), progress: null },
      ],
    }),
    async run(page, check) {
      await check.text("2 загрузки");
      await check.listening("app://close-requested");
      await page.evaluate(() => window.__emit("app://close-requested", null));
      await check.text("Если закрыть сейчас, 2 загрузки будут остановлены");
      await page.screenshot({ path: `${OUT}/11-close-dialog.png` });
      await page.getByRole("button", { name: "Остановить и выйти" }).click();
      await check.call("app_quit");
    },
    noFinalShot: true,
  },
  {
    name: "12-video-card-en",
    theme: "dark",
    responses: ready("en", { video_probe: VIDEO }),
    async run(page, check) {
      await page.fill("input[type=url]", VIDEO.url);
      await page.keyboard.press("Enter");
      await check.text(VIDEO.title);
      await check.text("Audio only");
    },
  },
  {
    name: "13-history-en",
    theme: "light",
    responses: ready("en", {
      queue_list: [
        {
          ...record(1, {
            title: "Street food tour of the old town",
            status: "downloading",
          }),
          progress: progress(57),
        },
      ],
      history_list: [
        {
          ...record(11, {
            title: VIDEO.title,
            status: "completed",
            filePath: "/d/a.mp4",
            fileSize: 1.2e9,
            completedAt: NOW,
          }),
          fileExists: true,
        },
        {
          ...record(12, {
            title: "Morning jazz for focus",
            format: "audio_mp3",
            status: "completed",
            filePath: "/d/b.mp3",
            fileSize: 8.4e6,
            completedAt: NOW - 86400,
          }),
          fileExists: true,
        },
        {
          ...record(13, {
            title: "Five-minute garlic noodles",
            status: "completed",
            filePath: "/d/c.mp4",
            fileSize: 96e6,
            completedAt: NOW - 2 * 86400,
          }),
          fileExists: false,
        },
      ],
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "History" }).click();
      await check.text("File not found");
      await check.text("Morning jazz for focus");
    },
  },
  {
    name: "14-proxy-saved",
    theme: "light",
    responses: ready("ru", {
      settings_set: { ...settings("ru"), proxy: "http://127.0.0.1:8888" },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Настройки" }).click();
      const proxy = page.getByLabel("Прокси");
      await proxy.fill("http://127.0.0.1:8888");
      await proxy.press("Enter");
      await proxy.blur();
      const set = await check.call("settings_set");
      check.equal(set.args.settings.proxy, "http://127.0.0.1:8888");
      await check.text("Сохранено");
    },
  },
  {
    name: "15-proxy-invalid",
    theme: "light",
    responses: ready("ru", {
      settings_set: {
        __error: {
          code: "invalid_proxy",
          message: "invalid proxy URL: 127.0.0.1:8888",
        },
      },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Настройки" }).click();
      const proxy = page.getByLabel("Прокси");
      await proxy.fill("127.0.0.1:8888");
      await proxy.blur();
      await check.text("Неверный адрес прокси");
      // The field shows the stored value again.
      await page.waitForFunction(() => {
        const el = [...document.querySelectorAll("input")].find(
          (i) => i.placeholder === "http://127.0.0.1:8888",
        );
        return el && el.value === "";
      });
    },
  },
  {
    name: "16-proxy-check-ok",
    theme: "dark",
    responses: ready("ru", {
      // Clicking "Check" blurs the field, which saves it.
      settings_set: { ...settings("ru"), proxy: "http://127.0.0.1:8888" },
      proxy_check: {
        ok: true,
        viaProxy: true,
        elapsedMs: 385,
        failure: null,
        detail: "204 No Content",
      },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Настройки" }).click();
      await page.getByLabel("Прокси").fill("http://127.0.0.1:8888");
      await page
        .getByRole("button", { name: "Проверить", exact: true })
        .click();
      // The typed value is checked, even before it is saved.
      const call = await check.call("proxy_check");
      check.equal(call.args.proxy, "http://127.0.0.1:8888");
      await check.text("YouTube доступен через прокси · 385 мс");
    },
  },
  {
    name: "17-proxy-check-refused",
    theme: "light",
    responses: ready("ru", {
      // Clicking "Check" blurs the field, which saves it.
      settings_set: { ...settings("ru"), proxy: "http://127.0.0.1:8888" },
      proxy_check: {
        ok: false,
        viaProxy: true,
        elapsedMs: 120,
        failure: "proxy_refused",
        detail: "tunnel error: unsuccessful",
      },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Настройки" }).click();
      await page.getByLabel("Прокси").fill("http://127.0.0.1:8888");
      await page
        .getByRole("button", { name: "Проверить", exact: true })
        .click();
      await check.text("Прокси отказался соединять с YouTube");
      // The mark sits on the first line of the message, not on a line of its own.
      const [mark, message] = await page
        .locator(".proxy-result > svg, .proxy-result > span")
        .evaluateAll((els) => els.map((e) => e.getBoundingClientRect().top));
      if (Math.abs(mark - message) > 6)
        throw new Error(`mark at ${mark}, message at ${message}`);
      await page
        .getByRole("button", { name: "Скопировать подробности" })
        .click();
      await check.call("diagnostics");
    },
  },
  {
    name: "18-codec-picker",
    theme: "light",
    responses: ready("ru", {
      video_probe: VIDEO,
      settings_get: { ...settings("ru"), defaultCodec: "av1" },
    }),
    async run(page, check) {
      await page.fill("input[type=url]", VIDEO.url);
      await page.keyboard.press("Enter");
      await check.text(VIDEO.title);
      // The default comes from the settings.
      const codec = page.getByLabel("Кодек");
      check.equal(await codec.inputValue(), "av1");
      await check.text("AV1 Video Extension");
      await codec.selectOption("h264");
      await check.text("открывается в любом плеере");
      await page.screenshot({ path: `${OUT}/18-codec-picker.png` });
      await page.getByRole("button", { name: "Скачать" }).last().click();
      const add = await check.call("queue_add");
      check.equal(add.args.items[0].format, {
        kind: "video",
        maxHeight: null,
        codec: "h264",
      });
    },
    noFinalShot: true,
  },
  {
    name: "19-settings-codec",
    theme: "dark",
    responses: ready("en", {
      settings_set: { ...settings("en"), defaultCodec: "vp9" },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Settings" }).click();
      await page.getByLabel("Default codec").selectOption("vp9");
      const set = await check.call("settings_set");
      check.equal(set.args.settings.defaultCodec, "vp9");
      await check.text("VP9 Video Extensions");
    },
  },
  {
    name: "20-url-without-scheme",
    theme: "light",
    responses: ready("en", { video_probe: VIDEO }),
    async run(page, check) {
      // Typed, not pasted: the form submit must not be blocked by URL validation.
      await page.fill("input[type=url]", "youtube.com/watch?v=dQw4w9WgXcQ");
      await page.keyboard.press("Enter");
      const probe = await check.call("video_probe");
      check.equal(probe.args.url, "youtube.com/watch?v=dQw4w9WgXcQ");
      await check.text(VIDEO.title);
    },
  },
  {
    name: "21-filename-template",
    theme: "light",
    responses: ready("ru", {
      settings_get: { ...settings("ru"), filenameTemplate: "%(title)s.mp4" },
      settings_set: {
        ...settings("ru"),
        filenameTemplate: "%(title)s.%(ext)s",
      },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Настройки" }).click();
      await check.text("Шаблон не заканчивается на");
      // Without .%(ext)s the name has no quality and no extension.
      const result = page.locator(".preview bdi");
      check.equal(await result.textContent(), "Название ролика.mp4");
      await page.getByText("Как составить шаблон").click();
      await check.text("название канала");
      await page.screenshot({
        path: `${OUT}/21-filename-template.png`,
        fullPage: true,
      });

      await page.getByRole("button", { name: "Исправить" }).click();
      const fixed = await check.call("settings_set");
      check.equal(fixed.args.settings.filenameTemplate, "%(title)s.%(ext)s");
      await page
        .getByText("Шаблон не заканчивается")
        .waitFor({ state: "detached" });
      check.equal(await result.textContent(), "Название ролика 720p.mp4");

      // An example replaces the template and is saved at once.
      const example = "%(uploader)s/%(title)s [%(id)s].%(ext)s";
      await page.getByRole("button", { name: example }).click();
      await page.waitForFunction(
        () =>
          window.__calls.filter((x) => x.cmd === "settings_set").length === 2,
      );
      const picked = await check.call("settings_set");
      check.equal(picked.args.settings.filenameTemplate, example);
    },
    noFinalShot: true,
  },
  {
    name: "22-search",
    theme: "light",
    responses: ready("ru", {
      youtube_search: {
        __sequence: [
          { entries: SEARCH_RESULTS, hasMore: true },
          { entries: searchPage(18, 4), hasMore: false },
        ],
      },
      // The second result is already downloaded in the default format.
      find_duplicates: [
        record(9, {
          videoId: SEARCH_RESULTS[1].id,
          status: "completed",
          completedAt: NOW,
        }),
      ],
      queue_add: [record(10, { videoId: SEARCH_RESULTS[0].id })],
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      await check.text("Найдите ролик");
      await page.getByLabel("Поиск на YouTube").fill("timelapse mountains");
      await page.keyboard.press("Enter");
      const search = await check.call("youtube_search");
      check.equal(search.args, { query: "timelapse mountains", page: 1 });
      await check.text("1,2 млн просмотров");
      // "339 тыс. просмотров", not "просмотр" although 338 921 ends in 1.
      await check.text("339 тыс. просмотров");
      await check.text("В эфире");
      await check.text("Скачано");
      await page.screenshot({ path: `${OUT}/22-search.png` });

      // A live stream has nothing to download yet.
      const live = resultRow(page, SEARCH_RESULTS[3].title);
      check.equal(
        await live.getByRole("button", { name: "В очередь" }).count(),
        0,
      );

      const first = resultRow(page, SEARCH_RESULTS[0].title);
      await first.getByRole("button", { name: "В очередь" }).click();
      const add = await check.call("queue_add");
      check.equal(add.args.items[0].videoId, SEARCH_RESULTS[0].id);
      check.equal(add.args.items[0].format, {
        kind: "video",
        maxHeight: null,
        codec: "auto",
      });
      await check.text("Добавлено в очередь");

      // Already downloaded in the default format: not queued again.
      const second = resultRow(page, SEARCH_RESULTS[1].title);
      await second.getByRole("button", { name: "В очередь" }).click();
      await check.text("Уже скачано в формате по умолчанию");
      check.equal(await check.count("queue_add"), 1);

      await page.getByRole("button", { name: "Показать ещё" }).click();
      await page.waitForFunction(
        () =>
          window.__calls.filter((c) => c.cmd === "youtube_search").length === 2,
      );
      const more = await check.call("youtube_search");
      check.equal(more.args.page, 2);
      // Page 2 repeats two results; each video is listed once.
      await check.text("#21");
      check.equal(await page.locator("ul > li").count(), 22);
    },
    noFinalShot: true,
  },
  {
    name: "23-search-watch",
    theme: "dark",
    responses: ready("en", {
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
      player_url: "about:blank",
      queue_add: [record(11, { videoId: SEARCH_RESULTS[0].id })],
      video_probe: VIDEO,
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Search" }).click();
      await page.getByLabel("Search YouTube").fill("timelapse");
      await page.getByRole("button", { name: "Search", exact: true }).click();
      await check.text(SEARCH_RESULTS[0].title);
      const first = resultRow(page, SEARCH_RESULTS[0].title);
      await first.getByRole("button", { name: "Watch", exact: true }).click();
      const player = await check.call("player_url");
      check.equal(player.args, { videoId: SEARCH_RESULTS[0].id, lang: "en" });
      const frame = page.locator("dialog iframe");
      await frame.waitFor();
      check.equal(await frame.getAttribute("src"), "about:blank");
      await page.screenshot({ path: `${OUT}/23-search-watch.png` });

      // A toast shows above the open player, not hidden behind it.
      await page
        .locator("dialog")
        .getByRole("button", { name: "Add to queue" })
        .click();
      await check.text("Added to the queue");
      // In the top layer, shown after the dialog. (Hit testing can't tell: a modal
      // dialog makes everything outside it inert.)
      check.equal(
        await page
          .locator(".toast")
          .evaluate((t) => t.matches(":popover-open")),
        true,
      );

      // "Download…" opens the video in the Download tab to pick a format.
      await page
        .locator("dialog")
        .getByRole("button", { name: "Download…" })
        .click();
      const probe = await check.call("video_probe");
      check.equal(probe.args.url, SEARCH_RESULTS[0].url);
      await check.text(VIDEO.title);
      check.equal(await page.locator("dialog[open]").count(), 0);
    },
    noFinalShot: true,
  },
  {
    name: "24-youtube-window",
    theme: "light",
    responses: ready("ru", {
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
      video_probe: VIDEO,
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      if (MOBILE) {
        // Android has no YouTube window: links go to the YouTube app.
        check.equal(
          await page.getByRole("button", { name: "Открыть YouTube" }).count(),
          0,
        );
        await page.getByLabel("Поиск на YouTube").fill("timelapse");
        await page.keyboard.press("Enter");
        await page
          .getByRole("button", { name: "Открыть на YouTube" })
          .first()
          .click();
        const open = await check.call("plugin:opener|open_url");
        check.equal(open.args.url, SEARCH_RESULTS[0].url);
        return;
      }
      await page.getByRole("button", { name: "Открыть YouTube" }).click();
      const open = await check.call("youtube_open");
      check.equal(open.args, {
        url: "https://www.youtube.com/",
        label: "Скачать в Cliprigo",
      });
      // The window's download button sends the open video to the Download tab.
      await check.listening("app://open-url");
      await page.evaluate(
        (url) => window.__emit("app://open-url", url),
        VIDEO.url,
      );
      const probe = await check.call("video_probe");
      check.equal(probe.args.url, VIDEO.url);
      await check.text(VIDEO.title);
      check.equal(
        await page
          .getByRole("tab", { name: "Загрузка" })
          .getAttribute("aria-selected"),
        "true",
      );
    },
    noFinalShot: true,
  },
  {
    name: "25-android-share",
    theme: "light",
    responses: ready("ru", {
      shared_take: `${VIDEO.title}\n${VIDEO.url}`,
      video_probe: VIDEO,
    }),
    async run(page, check) {
      if (!MOBILE) {
        // Only Android receives shared links.
        await check.text("Сейчас ничего не скачивается");
        check.equal(await check.count("shared_take"), 0);
        return;
      }
      const probe = await check.call("video_probe");
      check.equal(probe.args.url, VIDEO.url);
      await check.text(VIDEO.title);
    },
  },
];

// ---------- runner ----------

function checks(page) {
  return {
    async text(text) {
      await page
        .getByText(text, { exact: false })
        .first()
        .waitFor({ timeout: 5000 });
    },
    /** Waits until the app subscribed to `event`; emitting earlier would be lost. */
    async listening(event) {
      await page.waitForFunction(
        (e) =>
          window.__calls.some(
            (x) => x.cmd === "plugin:event|listen" && x.args.event === e,
          ),
        event,
        { timeout: 5000 },
      );
    },
    async call(cmd) {
      const handle = await page.waitForFunction(
        (c) => [...window.__calls].reverse().find((x) => x.cmd === c),
        cmd,
        { timeout: 5000 },
      );
      return handle.jsonValue();
    },
    async count(cmd) {
      return page.evaluate(
        (c) => window.__calls.filter((x) => x.cmd === c).length,
        cmd,
      );
    },
    equal(actual, expected) {
      const a = JSON.stringify(actual);
      const e = JSON.stringify(expected);
      if (a !== e) throw new Error(`expected ${e}, got ${a}`);
    },
  };
}

async function waitForServer(url, timeoutMs = 30000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`dev server did not start at ${url}`);
}

mkdirSync(OUT, { recursive: true });
const server = spawn(
  "pnpm",
  [
    "exec",
    "vite",
    "dev",
    "--port",
    String(PORT),
    "--strictPort",
    "--host",
    "127.0.0.1",
  ],
  {
    stdio: "ignore",
    env: {
      ...process.env,
      NO_PROXY: "127.0.0.1,localhost",
      no_proxy: "127.0.0.1,localhost",
    },
  },
);

let failed = 0;
const browser = await chromium
  .launch({ executablePath: CHROME, args: ["--no-proxy-server"] })
  .catch((e) => {
    server.kill();
    throw e;
  });
try {
  await waitForServer(BASE);
  for (const scenario of scenarios) {
    const context = await browser.newContext({
      ...VIEWPORT,
      colorScheme: THEME ?? scenario.theme,
    });
    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.addInitScript(installFakeTauri, {
      responses: scenario.responses,
    });
    try {
      await page.goto(BASE);
      await scenario.run(page, checks(page));
      if (errors.length) throw new Error(`page errors: ${errors.join("; ")}`);
      if (MOBILE) {
        // A phone screen must never scroll sideways; name what sticks out.
        const wide = await page.evaluate(() => {
          if (document.documentElement.scrollWidth <= innerWidth) return null;
          const el = [...document.querySelectorAll("body *")]
            .filter((e) => e.getBoundingClientRect().right > innerWidth + 1)
            .pop();
          return `${el?.tagName}.${el?.className} ${el?.textContent?.slice(0, 40)}`;
        });
        if (wide) throw new Error(`wider than the screen: ${wide}`);
        // Fingers need targets of 44 px or more; name the ones that are smaller.
        const small = await page.evaluate(() =>
          [...document.querySelectorAll("button, summary, [role=radio]")]
            .filter((e) => e.checkVisibility() && !e.closest("[inert]"))
            .map((e) => [e, e.getBoundingClientRect()])
            .filter(([, r]) => r.width < 44 || r.height < 44)
            .map(
              ([e, r]) =>
                `${e.getAttribute("aria-label") ?? e.textContent.trim().slice(0, 30)} ${Math.round(r.width)}×${Math.round(r.height)}`,
            ),
        );
        if (small.length) throw new Error(`small targets: ${small.join(", ")}`);
        // A badge wrapping to two lines squeezes the title next to it.
        const tall = await page.evaluate(() =>
          [...document.querySelectorAll(".badge")]
            .filter((e) => e.getBoundingClientRect().height > 24)
            .map((e) => e.textContent.trim()),
        );
        if (tall.length)
          throw new Error(`badges on two lines: ${tall.join(", ")}`);
      }
      if (!scenario.noFinalShot) {
        await page.screenshot({ path: `${OUT}/${scenario.name}.png` });
      }
      console.log(`ok   ${scenario.name}`);
    } catch (e) {
      failed++;
      await page.screenshot({ path: `${OUT}/${scenario.name}.FAILED.png` });
      console.log(`FAIL ${scenario.name}: ${e.message.split("\n")[0]}`);
    }
    await context.close();
  }
} finally {
  await browser.close();
  server.kill();
}
console.log(
  `${scenarios.length - failed}/${scenarios.length} scenarios passed, screenshots in ${OUT}`,
);
process.exit(failed ? 1 : 0);
