const RELEASE = "https://github.com/eymenderdiyok09-dotcom/ember/releases/latest/download/";

/** Blurs the canvas in place (three box-blur passes ≈ Gaussian). Done once, so
 * the page doesn't pay for a live CSS blur while scrolling. */
function blurCanvas(ctx, W, H, radius) {
  const img = ctx.getImageData(0, 0, W, H);
  const src = img.data;
  const tmp = new Uint8ClampedArray(src.length);
  const pass = (from, to, horizontal) => {
    const len = horizontal ? W : H, lines = horizontal ? H : W, span = radius * 2 + 1;
    for (let line = 0; line < lines; line++) {
      const at = (i) => 4 * (horizontal ? line * W + Math.min(len - 1, Math.max(0, i)) : Math.min(len - 1, Math.max(0, i)) * W + line);
      for (let c = 0; c < 3; c++) {
        let sum = 0;
        for (let i = -radius; i <= radius; i++) sum += from[at(i) + c];
        for (let i = 0; i < len; i++) {
          to[at(i) + c] = sum / span;
          sum += from[at(i + radius + 1) + c] - from[at(i - radius) + c];
        }
      }
      for (let i = 0; i < len; i++) to[at(i) + 3] = 255;
    }
  };
  for (let n = 0; n < 3; n++) {
    pass(src, tmp, true);
    pass(tmp, src, false);
  }
  ctx.putImageData(img, 0, 0);
}

// Blocky landscape behind the hero, drawn small and blurred once.
function drawLandscape(canvas) {
  const W = 384, H = 216;
  canvas.width = W;
  canvas.height = H;
  const ctx = canvas.getContext("2d");
  let seed = 7;
  const rand = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);
  const noise = (n) => {
    const pts = Array.from({ length: n + 2 }, rand);
    return (x) => {
      const i = Math.floor(x), f = x - i, t = (1 - Math.cos(f * Math.PI)) / 2;
      return pts[i % pts.length] * (1 - t) + pts[(i + 1) % pts.length] * t;
    };
  };
  const haze = [196, 222, 240];
  const mix = (c, k) => `rgb(${c.map((v, i) => Math.round(v + (haze[i] - v) * k)).join(",")})`;

  const sky = ctx.createLinearGradient(0, 0, 0, H * 0.6);
  sky.addColorStop(0, "#5b9be3");
  sky.addColorStop(1, "#c4def2");
  ctx.fillStyle = sky;
  ctx.fillRect(0, 0, W, H);

  for (let i = 0; i < 11; i++) {
    const x = rand() * W, y = 6 + rand() * 62, w = 24 + rand() * 60, h = 4 + rand() * 6;
    ctx.fillStyle = `rgba(255,255,255,${0.75 + rand() * 0.2})`;
    ctx.fillRect(x, y, w, h);
    ctx.fillRect(x + w * 0.2, y - h * 0.6, w * 0.5, h);
  }

  const layers = 7;
  for (let L = 0; L < layers; L++) {
    const t = L / (layers - 1);
    const far = 1 - t;
    const bs = 1 + Math.round(t * 6);
    const base = H * 0.5 + t * t * H * 0.42;
    const amp = 3 + t * 16;
    const n = noise(12);
    const freq = 0.012 + far * 0.02;
    const water = L === 4 ? [W * 0.3, W * 0.62] : null;
    for (let x = 0; x < W; x += bs) {
      let y = Math.round((base - n(x * freq) * amp) / bs) * bs;
      const inWater = water && x > water[0] && x < water[1];
      if (inWater) y = Math.round(base / bs) * bs + bs;
      ctx.fillStyle = inWater ? mix([70, 120, 220], far * 0.6) : mix([96, 160, 70], far * 0.7);
      ctx.fillRect(x, y, bs, H - y);
      if (!inWater) {
        ctx.fillStyle = mix([118, 184, 82], far * 0.7);
        ctx.fillRect(x, y, bs, bs);
        if (rand() < 0.25) {
          ctx.fillStyle = mix([78, 136, 58], far * 0.7);
          ctx.fillRect(x, y + bs * (1 + Math.floor(rand() * 4)), bs, bs);
        }
        if (rand() < 0.05 + t * 0.03 && x % (bs * 6) < bs) {
          ctx.fillStyle = mix([102, 74, 44], far * 0.7);
          ctx.fillRect(x + bs, y - bs * 3, bs, bs * 3);
          ctx.fillStyle = mix([58, 112, 46], far * 0.7);
          ctx.fillRect(x - bs, y - bs * 6, bs * 5, bs * 3);
          ctx.fillRect(x, y - bs * 7, bs * 3, bs);
        }
      }
    }
  }
  blurCanvas(ctx, W, H, 3);
}
drawLandscape(document.getElementById("landscape"));

// Download links all point at the latest GitHub release.
for (const a of document.querySelectorAll("[data-file]")) a.href = RELEASE + a.dataset.file;

// Recommend the visitor's platform.
const platform = (navigator.userAgentData?.platform || navigator.platform || navigator.userAgent).toLowerCase();
const os = platform.includes("win") ? "win" : platform.includes("mac") ? "mac" : null;
const hero = document.getElementById("hero-download");
if (os) {
  const row = document.getElementById(`row-${os}`);
  row.classList.add("is-detected");
  row.open = true;
  row.parentElement.prepend(row);
  document.getElementById("hero-label").dataset.i18n = os === "mac" ? "download_mac" : "download_windows";
  document.querySelector("#hero-icon use").setAttribute("href", os === "mac" ? "#i-apple" : "#i-windows");
  hero.href = (os === "mac" ? row.querySelector("[data-file]") : document.getElementById("win-x64")).href;
  if (os === "win") {
    navigator.userAgentData?.getHighEntropyValues(["architecture"])
      .then(({ architecture }) => {
        const pick = document.getElementById(architecture === "arm" ? "win-arm64" : "win-x64");
        hero.href = pick.href;
        pick.classList.add("is-recommended");
      })
      .catch(() => {});
  }
} else {
  document.getElementById("hero-icon").remove();
}

// "More" menu next to the main button.
const moreBtn = document.getElementById("more-btn");
const menu = document.getElementById("more-menu");
const setMenu = (open) => {
  menu.hidden = !open;
  moreBtn.setAttribute("aria-expanded", String(open));
};
moreBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  setMenu(menu.hidden);
});
document.addEventListener("click", (e) => { if (!menu.contains(e.target)) setMenu(false); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape") setMenu(false); });

// Live count of versions Ember can install.
addEventListener("load", () => {
  fetch("https://piston-meta.mojang.com/mc/game/version_manifest_v2.json")
    .then((r) => r.json())
    .then((m) => { document.getElementById("stat-versions").textContent = m.versions.length.toLocaleString("en-US"); })
    .catch(() => {});
});

// Language: load the picked (or system) language, then fill the picker.
initI18n().then((lang) => {
  const select = document.getElementById("language");
  for (const [code, name] of LANGUAGES) select.append(new Option(name, code, false, code === lang));
  select.addEventListener("change", () => {
    try { localStorage.setItem("language", select.value); } catch {}
    const url = new URL(location.href);
    url.searchParams.delete("lang");
    location.replace(url);
  });
});
