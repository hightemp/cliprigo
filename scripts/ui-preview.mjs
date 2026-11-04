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
  rememberSearches: true,
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
  subtitles: [],
  // 2025-06-12 00:00 UTC.
  published: 1749686400,
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
  favorites_list: [],
  search_history_list: [],
  url_inspect: { normalized: VIDEO.url, isYoutube: true, singleVideo: true },
  ...extra,
});

const HOUR = 3600;
const DAY = 24 * HOUR;

const SEARCH_TITLES = [
  "Mountain sunrise timelapse in 4K",
  "Rocky Mountains — 3 hours of relaxing scenery",
  "How to shoot timelapses: complete guide",
  "Northern lights over Norway 8K",
  "Alpine lakes from a drone",
];

/** A search result as saved to Favorites `ago` seconds before NOW. */
const favorite = (entry, ago) => ({
  id: entry.id,
  url: entry.url,
  title: entry.title,
  uploader: entry.uploader,
  duration: entry.duration,
  thumbnail: entry.thumbnail,
  viewCount: entry.viewCount,
  published: entry.published,
  addedAt: NOW - ago,
});

/** The search result row with exactly this title. */
const resultRow = (page, title) =>
  page
    .locator("li.card:visible")
    .filter({ has: page.getByText(title, { exact: true }) });

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
      // As yt-dlp's approximate dates: "3 days ago", "2 weeks ago"…
      published:
        Math.floor(Date.now() / 1000) -
        [3 * DAY, 15 * DAY, 2 * HOUR, 400 * DAY, 70 * DAY][n % 5],
      ...overrides[n],
    };
  });
}

const SEARCH_RESULTS = searchPage(0, 20, { 3: { live: true, duration: null } });

/** A past search `ago` seconds before NOW. */
const pastSearch = (id, query, ago, filters = {}, resultCount = 20) => ({
  id,
  query,
  filters: {
    uploadDate: null,
    length: null,
    features: [],
    sort: "relevance",
    ...filters,
  },
  searchedAt: NOW - ago,
  resultCount,
});

const subsTrack = (lang, kind, name) => ({
  lang,
  name,
  kind,
  url: `https://www.youtube.com/api/timedtext?v=vid100&lang=${lang}&fmt=json3`,
});

const SUB_TRACKS = [
  subsTrack("en", "manual", "English"),
  subsTrack("de-DE", "manual", "German (Germany)"),
  subsTrack("en-orig", "auto", "English (Original)"),
  subsTrack("ru", "translated", "Russian"),
];

const SUB_PHRASES = [
  "[Music]",
  "The sun comes up over the ridge",
  "Clouds roll down into the valley",
  "This shot took six hours to film",
  "We set the camera every ten seconds",
  "Here the light turns orange",
  "Shadows move across the snow",
  "The lake is still in the morning",
  "Wind picks up after noon",
  "Night falls and the stars appear",
];

const SUB_CUES = SUB_PHRASES.map((text, i) => ({
  startMs: 1000 + i * 4200,
  endMs: 1000 + i * 4200 + 4000,
  text,
}));

const SEARCH_HISTORY = [
  pastSearch(3, "горы таймлапс", 600, { features: ["4k"], sort: "views" }),
  pastSearch(2, "котики смешные", 86400),
  pastSearch(1, "lofi для работы", 3 * 86400, { length: "long" }, 40),
];

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
      await check.text("Nature Frames · 12 июн. 2025 г. · 12:34");
      const quality = page.getByLabel("Качество");
      check.equal((await quality.textContent()).trim(), "Лучшее доступное");
      await check.choose(quality, { label: "1080p" });
      // A long folder is cut at the start (the folder name stays visible) and must
      // not overflow the card.
      const folder = page.locator("[data-tip^='/home/user/Videos']");
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

      await check.choose(page.getByLabel("Simultaneous downloads"), {
        value: "3",
      });
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
      check.equal(await codec.getAttribute("data-value"), "av1");
      await check.text("AV1 Video Extension");
      await check.choose(codec, { value: "h264" });
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
      await check.choose(page.getByLabel("Default codec"), { value: "vp9" });
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
      check.equal(search.args.query, "timelapse mountains");
      check.equal(search.args.page, 1);
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
      check.equal(await page.locator("li.card").count(), 22);
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
    name: "24-open-on-youtube",
    theme: "light",
    responses: ready("ru", {
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      // No in-app YouTube window: a video opens in the system browser (or the
      // YouTube app on Android).
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
    },
    noFinalShot: true,
  },
  {
    name: "26-search-filters",
    theme: "light",
    responses: ready("ru", {
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      await page.getByLabel("Поиск на YouTube").fill("timelapse");
      await page.keyboard.press("Enter");
      const plain = await check.call("youtube_search");
      check.equal(plain.args.filters, {
        uploadDate: null,
        length: null,
        features: [],
        sort: "relevance",
      });

      await page.getByRole("button", { name: "Фильтры" }).click();
      await check.choose(page.getByLabel("Дата загрузки"), { value: "week" });
      await page.getByRole("checkbox", { name: "4K" }).check();
      await check.choose(page.getByLabel("Сортировка"), { value: "views" });
      // Each change searches again with the same text.
      await page.waitForFunction(
        () =>
          window.__calls.filter((c) => c.cmd === "youtube_search").length === 4,
      );
      const filtered = await check.call("youtube_search");
      check.equal(filtered.args, {
        query: "timelapse",
        filters: {
          uploadDate: "week",
          length: null,
          features: ["4k"],
          sort: "views",
        },
        page: 1,
      });
      await check.text("3");
      await page.screenshot({ path: `${OUT}/26-search-filters.png` });

      await page.getByRole("button", { name: "Сбросить" }).click();
      await page.waitForFunction(
        () =>
          window.__calls.filter((c) => c.cmd === "youtube_search").length === 5,
      );
      const reset = await check.call("youtube_search");
      check.equal(reset.args.filters, plain.args.filters);
    },
    noFinalShot: true,
  },
  {
    name: "27-custom-controls",
    theme: "light",
    responses: ready("en", {
      history_list: [record(1, { status: "completed", title: "Sunset" })],
      settings_set: settings("en"),
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Settings" }).click();
      const codec = page.getByLabel("Default codec");
      const saved = async () =>
        (await check.call("settings_set")).args.settings.defaultCodec;
      // Keyboard: arrows open and move, Enter picks, Esc closes without a change.
      await codec.focus();
      await page.keyboard.press("ArrowDown");
      check.equal(await codec.getAttribute("aria-expanded"), "true", "opened");
      await page.screenshot({ path: `${OUT}/27-select-open.png` });
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("Enter");
      check.equal(await saved(), "h264");
      check.equal(await codec.getAttribute("aria-expanded"), "false", "picked");
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("End");
      await page.keyboard.press("Escape");
      check.equal(await check.count("settings_set"), 1);
      // Typing a letter jumps to the option starting with it.
      await page.keyboard.press("Enter");
      await page.keyboard.press("v");
      await page.keyboard.press("Enter");
      check.equal(await saved(), "vp9");
      // A click outside closes the list.
      await codec.click();
      await page.mouse.click(5, 300);
      // The popover reports closing with an asynchronous toggle event.
      await page.waitForFunction(
        () => document.querySelector("#default-codec").ariaExpanded === "false",
        null,
        { timeout: 2000 },
      );

      await page.getByRole("tab", { name: "History" }).click();
      const search = page.getByLabel("Search by title or channel");
      await search.fill("sun");
      await page.getByRole("button", { name: "Clear" }).click();
      check.equal(await search.inputValue(), "");

      if (!MOBILE) {
        // The app's tooltip after a short hover.
        await page.getByRole("button", { name: "Copy link" }).hover();
        const tip = page.locator(".tip");
        await tip.waitFor({ state: "visible" });
        check.equal(await tip.textContent(), "Copy link");
        await page.mouse.move(5, 300);
        await tip.waitFor({ state: "hidden" });
      }
    },
  },
  {
    name: "28-favorites",
    theme: "light",
    responses: ready("ru", {
      favorites_list: [
        favorite(SEARCH_RESULTS[5], 200),
        favorite(SEARCH_RESULTS[6], 100),
      ],
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
      favorites_add: favorite(SEARCH_RESULTS[0], 300),
      queue_add: [record(12)],
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      await page.getByLabel("Поиск на YouTube").fill("timelapse");
      await page.keyboard.press("Enter");
      // Saved videos show as saved in the results too.
      const saved = resultRow(page, SEARCH_RESULTS[5].title).getByRole(
        "button",
        {
          name: "В избранное",
        },
      );
      check.equal(
        await saved.getAttribute("aria-pressed"),
        "true",
        "saved star",
      );
      const star = resultRow(page, SEARCH_RESULTS[0].title).getByRole(
        "button",
        {
          name: "В избранное",
        },
      );
      check.equal(await star.getAttribute("aria-pressed"), "false", "new star");
      await star.click();
      const add = await check.call("favorites_add");
      check.equal(add.args.video.id, SEARCH_RESULTS[0].id);
      check.equal(add.args.video.title, SEARCH_RESULTS[0].title);
      check.equal(await star.getAttribute("aria-pressed"), "true", "starred");

      await page.getByRole("tab", { name: "Избранное" }).click();
      check.equal(
        await page.locator("li.card:visible").count(),
        3,
        "three saved",
      );
      await page.screenshot({ path: `${OUT}/28-favorites.png` });

      const filter = page.getByLabel("Найти в избранном");
      await filter.fill("rocky");
      check.equal(await page.locator("li.card:visible").count(), 1, "filtered");
      await filter.fill("");

      await page.getByRole("button", { name: "Все в очередь" }).click();
      const queued = await check.call("queue_add");
      check.equal(queued.args.items.length, 3);

      // Unstarring removes it from the list.
      await resultRow(page, SEARCH_RESULTS[6].title)
        .getByRole("button", { name: "В избранное" })
        .click();
      const removed = await check.call("favorites_remove");
      check.equal(removed.args.id, SEARCH_RESULTS[6].id);
      check.equal(
        await page.locator("li.card:visible").count(),
        2,
        "after remove",
      );
    },
    noFinalShot: true,
  },
  {
    // English screenshots of search and favorites for the README and the site.
    name: "29-search-en",
    theme: "light",
    responses: ready("en", {
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
      favorites_list: [favorite(SEARCH_RESULTS[2], 100)],
      find_duplicates: [
        record(9, {
          videoId: SEARCH_RESULTS[1].id,
          status: "completed",
          completedAt: NOW,
        }),
      ],
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Search" }).click();
      await page.getByLabel("Search YouTube").fill("timelapse mountains");
      await page.keyboard.press("Enter");
      await check.text(SEARCH_RESULTS[0].title);
      await page.getByRole("button", { name: "Filters" }).click();
      await check.choose(page.getByLabel("Length"), { value: "medium" });
      await page.getByRole("checkbox", { name: "4K" }).check();
      await check.text("1.2M views");
      await check.text("Nature Frames · 1.2M views · 3 days ago · 3:00");
      // Let the checkbox finish its animation before the screenshot.
      await page.waitForTimeout(300);
    },
  },
  {
    name: "30-favorites-en",
    theme: "light",
    responses: ready("en", {
      favorites_list: [0, 2, 4, 1].map((i, n) =>
        favorite(SEARCH_RESULTS[i], 100 * (n + 1)),
      ),
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Favorites" }).click();
      await check.text("Queue all");
      // Saved with the date it was found with.
      await check.text("Nature Frames · 1.2M views · 3 days ago");
      check.equal(await page.locator("li.card:visible").count(), 4);
    },
  },
  {
    name: "31-search-suggestions",
    theme: "light",
    responses: ready("ru", {
      search_history_list: SEARCH_HISTORY,
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      // Before the first search the latest queries are one tap away.
      const recent = page.getByRole("list", { name: "Недавние запросы" });
      check.equal(
        (await recent.getByRole("button").allTextContents()).map((s) =>
          s.trim(),
        ),
        SEARCH_HISTORY.map((s) => s.query),
        "recent chips",
      );

      const field = page.getByRole("combobox", { name: "Поиск на YouTube" });
      await field.click();
      const list = page.getByRole("listbox");
      check.equal(await list.getByRole("option").count(), 3, "on focus");
      check.equal(await field.getAttribute("aria-expanded"), "true");
      await page.screenshot({ path: `${OUT}/31-search-suggestions.png` });

      await field.fill("КОТ");
      check.equal(
        (await list.getByRole("option").allTextContents()).map((s) => s.trim()),
        ["котики смешные"],
        "filtered",
      );
      await page.keyboard.press("ArrowDown");
      check.equal(
        await field.getAttribute("aria-activedescendant"),
        await list.getByRole("option").first().getAttribute("id"),
        "active option",
      );
      await page.keyboard.press("Enter");
      const searched = await check.call("youtube_search");
      check.equal(searched.args.query, "котики смешные");
      check.equal(await field.inputValue(), "котики смешные");
      check.equal(await field.getAttribute("aria-expanded"), "false");
      // The list reloads to show the new search on top.
      await page.waitForFunction(
        () =>
          window.__calls.filter((c) => c.cmd === "search_history_list")
            .length >= 2,
      );

      // Esc closes the list; Shift+Delete removes the active suggestion.
      await field.fill("");
      check.equal(await list.getByRole("option").count(), 3, "again");
      await page.keyboard.press("Escape");
      check.equal(await field.getAttribute("aria-expanded"), "false", "esc");
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("ArrowDown");
      await page.keyboard.press("Shift+Delete");
      const removed = await check.call("search_history_remove");
      check.equal(removed.args.id, SEARCH_HISTORY[1].id);
      check.equal(await list.getByRole("option").count(), 2, "removed");

      // A click on a suggestion searches it too.
      await list.getByRole("option", { name: "lofi для работы" }).click();
      check.equal(
        (await check.call("youtube_search")).args.query,
        "lofi для работы",
      );
    },
    noFinalShot: true,
  },
  {
    name: "32-search-history",
    theme: "light",
    responses: ready("ru", {
      search_history_list: SEARCH_HISTORY,
      search_history_results: searchPage(0, 6),
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
      queue_add: [record(12)],
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      await page.getByRole("button", { name: "История", exact: true }).click();
      const dialog = page.getByRole("dialog", { name: "История поиска" });
      const queries = dialog.getByRole("list", { name: "Запросы" });
      check.equal(await queries.getByRole("listitem").count(), 3, "queries");
      await check.text("4K · По популярности");

      await queries.getByRole("button", { name: /котики смешные/ }).click();
      check.equal((await check.call("search_history_results")).args.id, 2);
      await resultRow(page, SEARCH_RESULTS[1].title).waitFor();
      check.equal(
        await dialog.locator("li.card:visible").count(),
        6,
        "saved results",
      );
      if (MOBILE) {
        // One pane at a time on a phone: Back returns to the queries.
        await dialog.getByRole("button", { name: "Назад" }).click();
        await queries.getByRole("button", { name: /горы таймлапс/ }).click();
      } else {
        await queries.getByRole("button", { name: /горы таймлапс/ }).click();
      }
      check.equal((await check.call("search_history_results")).args.id, 3);
      await page.screenshot({ path: `${OUT}/32-search-history.png` });

      await resultRow(page, SEARCH_RESULTS[0].title)
        .getByRole("button", { name: "В очередь" })
        .click();
      check.equal(
        (await check.call("queue_add")).args.items[0].url,
        SEARCH_RESULTS[0].url,
      );

      // "Search again" runs it with the saved filters and closes the dialog.
      await dialog.getByRole("button", { name: "Искать снова" }).click();
      const again = await check.call("youtube_search");
      check.equal(again.args.query, "горы таймлапс");
      check.equal(again.args.filters.features, ["4k"]);
      check.equal(again.args.filters.sort, "views");
      check.equal(await dialog.count(), 0, "closed");
      check.equal(
        await page
          .getByRole("combobox", { name: "Поиск на YouTube" })
          .inputValue(),
        "горы таймлапс",
      );
    },
    noFinalShot: true,
  },
  {
    name: "33-search-history-edit",
    responses: ready("ru", {
      search_history_list: SEARCH_HISTORY,
      search_history_results: searchPage(0, 3),
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      await page.getByRole("button", { name: "История", exact: true }).click();
      const dialog = page.getByRole("dialog", { name: "История поиска" });
      const queries = dialog.getByRole("list", { name: "Запросы" });
      await queries
        .getByRole("listitem")
        .filter({ hasText: "котики смешные" })
        .getByRole("button", { name: "Удалить из истории" })
        .click();
      check.equal((await check.call("search_history_remove")).args.id, 2);
      check.equal(await queries.getByRole("listitem").count(), 2, "removed");

      await dialog.getByRole("button", { name: "Очистить историю" }).click();
      await check.call("search_history_clear");
      await check.text("Здесь появятся ваши поиски");
      await dialog.getByRole("button", { name: "Закрыть окно" }).click();
      check.equal(
        await page.getByRole("list", { name: "Недавние запросы" }).count(),
        0,
        "no chips",
      );
    },
  },
  {
    name: "34-settings-search-history",
    responses: ready("ru", {
      search_history_list: SEARCH_HISTORY,
      settings_set: { ...settings("ru"), rememberSearches: false },
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Настройки" }).click();
      const remember = page.getByRole("checkbox", {
        name: "Запоминать поисковые запросы",
      });
      check.equal(await remember.isChecked(), true, "on by default");
      await remember.uncheck();
      const saved = await check.call("settings_set");
      check.equal(saved.args.settings.rememberSearches, false);
      await page
        .getByRole("button", { name: "Очистить историю поиска" })
        .click();
      await check.call("search_history_clear");
      await check.text("История поиска очищена");
    },
  },
  {
    // English screenshot of the search history for the README and the site.
    name: "35-search-history-en",
    theme: "light",
    responses: ready("en", {
      search_history_list: [
        pastSearch(3, "mountain timelapse", 600, {
          features: ["4k"],
          sort: "views",
        }),
        pastSearch(2, "northern lights", 86400, { uploadDate: "month" }),
        pastSearch(1, "drone footage alps", 3 * 86400, { length: "long" }, 40),
      ],
      search_history_results: SEARCH_RESULTS.slice(0, 8),
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Search" }).click();
      await page.getByRole("button", { name: "History", exact: true }).click();
      const dialog = page.getByRole("dialog", { name: "Search history" });
      await dialog
        .getByRole("list", { name: "Searches" })
        .getByRole("button", { name: /mountain timelapse/ })
        .click();
      await resultRow(page, SEARCH_RESULTS[0].title).waitFor();
      check.equal(await dialog.locator("li.card:visible").count(), 8);
    },
  },
  {
    name: "36-subtitles",
    theme: "light",
    responses: ready("ru", {
      youtube_search: { entries: SEARCH_RESULTS, hasMore: true },
      video_probe: { ...VIDEO, subtitles: SUB_TRACKS },
      subtitles_fetch: SUB_CUES,
      "plugin:dialog|save": "/home/user/Downloads/sunrise.en.srt",
      subtitles_save: "/home/user/Downloads/sunrise.en.srt",
    }),
    async run(page, check) {
      await page.getByRole("tab", { name: "Поиск" }).click();
      await page.getByLabel("Поиск на YouTube").fill("timelapse");
      await page.keyboard.press("Enter");
      await resultRow(page, SEARCH_RESULTS[0].title)
        .getByRole("button", { name: "Субтитры" })
        .click();
      const dialog = page.getByRole("dialog", { name: "Субтитры" });
      check.equal(
        (await check.call("video_probe")).args.url,
        SEARCH_RESULTS[0].url,
      );
      // No Russian by the author: what is spoken, by the author.
      const language = dialog.getByRole("combobox", { name: "Язык" });
      check.equal((await language.textContent()).trim(), "English");
      check.equal(
        (await check.call("subtitles_fetch")).args.url,
        SUB_TRACKS[0].url,
      );
      await dialog.getByText(SUB_PHRASES[3]).waitFor();

      await dialog.getByRole("checkbox", { name: "Показывать время" }).check();
      await check.text("0:13");

      await dialog.getByLabel("Найти в субтитрах").fill("SHADOWS");
      await check.text("1 совпадение");
      check.equal(await dialog.locator("mark").count(), 1, "highlighted");
      await page.screenshot({ path: `${OUT}/36-subtitles.png` });
      await dialog.getByLabel("Найти в субтитрах").fill("");

      await dialog.getByRole("button", { name: "Копировать всё" }).click();
      const all = await check.call("plugin:clipboard-manager|write_text");
      const copied = JSON.stringify(all.args);
      for (const needle of ["[0:01] [Music]", SUB_PHRASES[9]]) {
        if (!copied.includes(needle))
          throw new Error(`copy all lacks ${needle}: ${copied}`);
      }

      // Copy what is selected with the mouse.
      const selected = dialog.getByRole("button", {
        name: "Копировать выделенное",
      });
      check.equal(await selected.isDisabled(), true, "nothing selected");
      await dialog.getByText(SUB_PHRASES[5]).selectText();
      await selected.click();
      const part = await check.call("plugin:clipboard-manager|write_text");
      if (!JSON.stringify(part.args).includes(SUB_PHRASES[5]))
        throw new Error(`selection not copied: ${JSON.stringify(part.args)}`);

      await check.choose(language, { label: "Russian · автоперевод" });
      check.equal(
        (await check.call("subtitles_fetch")).args.url,
        SUB_TRACKS[3].url,
      );

      await check.choose(
        dialog.getByRole("combobox", { name: "Формат файла" }),
        {
          value: "srt",
        },
      );
      await dialog.getByRole("button", { name: "Сохранить в файл" }).click();
      const saved = await check.call("subtitles_save");
      // Android has no save dialog: the file goes to the download folder.
      check.equal(
        saved.args.path,
        MOBILE ? null : "/home/user/Downloads/sunrise.en.srt",
      );
      if (!saved.args.text.startsWith("1\n00:00:01,000 --> 00:00:05,000\n"))
        throw new Error(`not SRT: ${saved.args.text.slice(0, 60)}`);
      check.equal(saved.args.name, `${SEARCH_RESULTS[0].title}.ru.srt`);
      await check.text("Сохранено");

      // Every window has a cross at the top.
      await dialog.getByRole("button", { name: "Закрыть окно" }).click();
      check.equal(await dialog.count(), 0, "closed by the cross");
    },
    noFinalShot: true,
  },
  {
    name: "37-subtitles-none",
    responses: ready("en", {
      video_probe: { ...VIDEO, subtitles: [] },
    }),
    async run(page, check) {
      await page.fill("input[type=url]", VIDEO.url);
      await page.keyboard.press("Enter");
      await check.text(VIDEO.title);
      await page.getByRole("button", { name: "Subtitles" }).click();
      await check.text("This video has no subtitles.");
      check.equal(await check.count("subtitles_fetch"), 0);
    },
  },
  {
    name: "38-download-subtitles",
    responses: ready("ru", {
      video_probe: { ...VIDEO, subtitles: SUB_TRACKS },
      queue_add: [record(12)],
    }),
    async run(page, check) {
      await page.fill("input[type=url]", VIDEO.url);
      await page.keyboard.press("Enter");
      await check.text(VIDEO.title);
      const subs = page.getByRole("combobox", { name: /^Субтитры/ });
      check.equal((await subs.textContent()).trim(), "Не сохранять");
      await check.choose(subs, { label: "English" });
      await page.screenshot({ path: `${OUT}/38-download-subtitles-video.png` });
      await page.getByRole("button", { name: "Скачать" }).last().click();
      const added = await check.call("queue_add");
      check.equal(added.args.items[0].subtitles, ["en"]);

      // Audio has no subtitles to go with.
      await page.fill("input[type=url]", VIDEO.url);
      await page.keyboard.press("Enter");
      await check.text(VIDEO.title);
      await page.getByRole("radio", { name: "Только звук" }).click();
      check.equal(await subs.count(), 0, "no picker for audio");
    },
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
    /** Picks an option in the app's Select by `value` or visible `label`. */
    async choose(combobox, { value, label }) {
      await combobox.click();
      const list = page.locator(
        `#${await combobox.getAttribute("aria-controls")}`,
      );
      const option =
        value === undefined
          ? list.getByRole("option", { name: label, exact: true })
          : list.locator(`[role=option][data-value="${value}"]`);
      await option.click();
      if ((await combobox.getAttribute("aria-expanded")) !== "false")
        throw new Error("the list stayed open");
    },
    equal(actual, expected, what = "") {
      const a = JSON.stringify(actual);
      const e = JSON.stringify(expected);
      if (a !== e)
        throw new Error(`${what ? `${what}: ` : ""}expected ${e}, got ${a}`);
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
let ran = 0;
const browser = await chromium
  .launch({ executablePath: CHROME, args: ["--no-proxy-server"] })
  .catch((e) => {
    server.kill();
    throw e;
  });
try {
  await waitForServer(BASE);
  const only = process.env.PREVIEW_ONLY;
  for (const scenario of scenarios) {
    if (only && !scenario.name.startsWith(only)) continue;
    ran++;
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
      // The app draws its own controls: no native selects or tooltips, and no
      // browser context menu outside text fields.
      const native = await page.evaluate(() => {
        const found = [];
        if (document.querySelector("select")) found.push("select");
        const titled = document.querySelector(
          "[title]:not(svg [title], iframe)",
        );
        if (titled) found.push(`title on ${titled.tagName}`);
        const menu = new MouseEvent("contextmenu", {
          bubbles: true,
          cancelable: true,
        });
        document.body.dispatchEvent(menu);
        if (!menu.defaultPrevented) found.push("context menu");
        return found;
      });
      if (native.length) throw new Error(`native UI: ${native.join(", ")}`);
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
console.log(`${ran - failed}/${ran} scenarios passed, screenshots in ${OUT}`);
process.exit(failed ? 1 : 0);
