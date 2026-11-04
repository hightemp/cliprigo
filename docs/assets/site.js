// Points download buttons at the latest release and suggests the visitor's system.
// Without JS (or if the GitHub API is unavailable) the links in the HTML still
// lead to a working release.
(() => {
  const REPO = "hightemp/cliprigo";

  function detectOs() {
    const ua = navigator.userAgent;
    const platform =
      navigator.userAgentData?.platform ?? navigator.platform ?? "";
    if (/android/i.test(ua)) return "android";
    if (/iphone|ipad|ipod/i.test(ua)) return null;
    if (/win/i.test(platform) || /windows/i.test(ua)) return "windows";
    if (/mac/i.test(platform) || /mac os/i.test(ua)) return "macos";
    if (/linux|x11|cros/i.test(platform + ua)) return "linux";
    return null;
  }

  async function isArm() {
    try {
      const data = await navigator.userAgentData?.getHighEntropyValues([
        "architecture",
      ]);
      return data?.architecture === "arm";
    } catch {
      return false;
    }
  }

  function formatSize(bytes) {
    const lang = document.documentElement.lang;
    const mb = bytes / 1024 / 1024;
    const unit = lang === "ru" ? "МБ" : "MB";
    return `${mb.toLocaleString(lang, { maximumFractionDigits: 0 })} ${unit}`;
  }

  async function suggest() {
    const os = detectOs();
    const button = document.getElementById("get");
    if (!os || !button) return null;
    const card = document.getElementById(`dl-${os}`);
    card?.classList.add("recommended");
    const arm = os === "windows" || os === "linux" ? await isArm() : false;
    const link =
      (arm && card?.querySelector("a[data-arm]")) ||
      card?.querySelector("a.main");
    const label = button.dataset[os];
    if (!link || !label) return;
    button.href = link.href;
    button.querySelector("span").textContent = label;
    return link;
  }

  async function latest() {
    let release;
    try {
      const response = await fetch(
        `https://api.github.com/repos/${REPO}/releases/latest`,
        {
          headers: { Accept: "application/vnd.github+json" },
        },
      );
      if (!response.ok) return;
      release = await response.json();
    } catch {
      return;
    }
    const assets = release.assets ?? [];
    for (const a of document.querySelectorAll("a[data-asset]")) {
      const pattern = new RegExp(a.dataset.asset);
      const asset = assets.find((x) => pattern.test(x.name));
      if (!asset) continue;
      a.href = asset.browser_download_url;
      const size = a.querySelector("small");
      // ".exe" becomes ".exe · 4 MB"; the type stays for those who know it.
      if (size)
        size.textContent = `${size.textContent.split(" · ")[0]} · ${formatSize(asset.size)}`;
    }
    for (const el of document.querySelectorAll("[data-version]")) {
      el.textContent = release.tag_name;
    }
  }

  // The hero button follows its card link once that points at the latest release.
  Promise.all([suggest(), latest()]).then(([link]) => {
    if (link) document.getElementById("get").href = link.href;
  });
})();
