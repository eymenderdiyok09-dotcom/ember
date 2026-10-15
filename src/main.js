const tauri = window.__TAURI__ ?? previewBackend();
const { invoke } = tauri.core;
const { listen } = tauri.event;

const $ = (id) => document.getElementById(id);
const load = (key) => { try { return localStorage.getItem(key); } catch { return null; } };
const save = (key, value) => { try { localStorage.setItem(key, value); } catch {} };

const USERNAME_RE = /^[A-Za-z0-9_]{3,16}$/;
const KIND_KEY = { release: "kind_release", snapshot: "kind_snapshot", old_beta: "kind_beta", old_alpha: "kind_alpha" };
const kindLabel = (kind) => (KIND_KEY[kind] ? t(KIND_KEY[kind]) : kind);

// ---------- toast ----------
let toastTimer;
function toast(message, { error = false, sticky = false } = {}) {
  const el = $("toast");
  el.textContent = message;
  el.classList.toggle("error", error);
  el.hidden = false;
  clearTimeout(toastTimer);
  if (!sticky) toastTimer = setTimeout(() => { el.hidden = true; }, error ? 9000 : 4000);
}

// ---------- logo & backdrop ----------
const FLAME = [
  "......#.....", ".....##.....", "....###..#..", "....####.##.", "...#####.##.",
  "..##oo####..", "..#ooo#####.", ".##oyyo####.", ".#ooyyyoo###", ".#oyyyyyoo##",
  ".#oyyWWyyo##", "..#oyWWyyo#.", "..##oyyyo##.", "....######..",
];
const FLAME_COLORS = { "#": "#e8481c", o: "#ff8a24", y: "#ffcc40", W: "#fff4c8" };

function drawLogo(svg = $("logo")) {
  const ns = "http://www.w3.org/2000/svg";
  FLAME.forEach((row, y) => [...row].forEach((c, x) => {
    if (!FLAME_COLORS[c]) return;
    const rect = document.createElementNS(ns, "rect");
    rect.setAttribute("x", x);
    rect.setAttribute("y", y);
    rect.setAttribute("width", 1);
    rect.setAttribute("height", 1);
    rect.setAttribute("fill", FLAME_COLORS[c]);
    svg.append(rect);
  }));
}

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

/** Blocky landscape behind everything, drawn small and blurred once (same as the website). */
function drawLandscape() {
  const canvas = $("landscape");
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
      if (inWater) continue;
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
  }  blurCanvas(ctx, W, H, 3);
}

// ---------- avatar ----------
function hash(str) {
  let h = 2166136261;
  for (const c of str) h = Math.imul(h ^ c.charCodeAt(0), 16777619);
  return h >>> 0;
}

/** Symmetric 8×8 pixel face, unique per username. */
function drawAvatar(name, canvas = $("avatar")) {
  const ctx = canvas.getContext("2d");
  let seed = hash(name.toLowerCase() || "ember");
  const rand = () => ((seed = Math.imul(seed ^ (seed >>> 15), 2246822519) >>> 0) / 4294967296);
  const hue = Math.floor(rand() * 360);
  const skin = `hsl(${hue}, 45%, 58%)`;
  const shade = `hsl(${hue}, 40%, 42%)`;
  const hair = `hsl(${(hue + 180 + rand() * 60) % 360}, 35%, 24%)`;
  ctx.fillStyle = skin;
  ctx.fillRect(0, 0, 8, 8);
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 4; x++) {
      const r = rand();
      const color = y < 2 ? hair : r < 0.18 ? shade : null;
      if (!color) continue;
      ctx.fillStyle = color;
      ctx.fillRect(x, y, 1, 1);
      ctx.fillRect(7 - x, y, 1, 1);
    }
  }
  ctx.fillStyle = "#fff";
  ctx.fillRect(1, 4, 2, 1); ctx.fillRect(5, 4, 2, 1);
  ctx.fillStyle = `hsl(${(hue + 200) % 360}, 60%, 35%)`;
  ctx.fillRect(2, 4, 1, 1); ctx.fillRect(5, 4, 1, 1);
  ctx.fillStyle = shade;
  ctx.fillRect(3, 6, 2, 1);
}

// ---------- versions ----------
// An entry is anything the picker can show: an installed version, a vanilla
// version available to download, or "latest Fabric for <game version>".
const state = { installed: [], remote: { vanilla: [], fabric: [] }, entries: [], selected: null, tab: null, busy: false, running: new Set() };
const runningPids = new Map();

const TABS = [
  ["installed", () => t("tab_installed")],
  ["release", () => t("tab_release")],
  ["snapshot", () => t("tab_snapshot")],
  ["fabric", () => "Fabric"],
  ["forge", () => "Forge"],
  ["neoforge", () => "NeoForge"],
  ["old", () => t("tab_old")],
];
const BADGE_CLASS = { release: "release", snapshot: "snapshot", old_beta: "legacy", old_alpha: "legacy", modded: "modded" };

function installedEntry(v) {
  const modded = v.kind === "modded";
  return {
    key: v.id,
    tab: "installed",
    name: modded ? `${v.loader} ${v.base}` : v.id,
    sub: modded ? v.id : kindLabel(v.kind),
    badge: modded ? v.loader : kindLabel(v.kind),
    badgeClass: BADGE_CLASS[v.kind] ?? "legacy",
    installed: true,
    modLoader: modded ? v.loader.toLowerCase() : null,
    modGame: modded ? v.base : v.id,
  };
}

function buildEntries() {
  const installedIds = new Set(state.installed.map((v) => v.id));
  const loaderInstalled = (loader) => new Set(state.installed.filter((v) => v.loader === loader).map((v) => v.base));
  const entries = state.installed.map(installedEntry);
  for (const v of state.remote.vanilla) {
    const old = v.kind === "old_beta" || v.kind === "old_alpha";
    entries.push({
      key: v.id,
      tab: old ? "old" : v.kind,
      name: v.id,
      sub: installedIds.has(v.id) ? t("installed") : kindLabel(v.kind),
      badge: kindLabel(v.kind),
      badgeClass: BADGE_CLASS[v.kind] ?? "legacy",
      installed: installedIds.has(v.id),
      modLoader: null,
      modGame: v.id,
    });
  }
  for (const [tab, loader, prefix] of [["fabric", "Fabric", "fabric"], ["forge", "Forge", "forge"], ["neoforge", "NeoForge", "neoforge"]]) {
    const installed = loaderInstalled(loader);
    for (const game of state.remote[tab] ?? []) {
      entries.push({
        key: `${prefix}:${game}`,
        tab,
        name: `${loader} ${game}`,
        sub: installed.has(game) ? t("installed_latest", { loader }) : t("latest_loader", { loader }),
        badge: loader,
        badgeClass: "modded",
        installed: installed.has(game),
        modLoader: prefix,
        modGame: game,
      });
    }
  }
  state.entries = entries;
}

function findEntry(key) {
  return state.entries.find((e) => e.key === key && e.tab === "installed") ?? state.entries.find((e) => e.key === key);
}

function selectEntry(key) {
  const entry = findEntry(key)
    ?? state.entries.find((e) => e.tab === "installed")
    ?? state.entries.find((e) => e.tab === "release");
  state.selected = entry ?? null;
  const badge = $("version-badge");
  if (!entry) {
    $("version-name").textContent = state.remoteLoaded ? t("no_versions") : t("loading_versions");
    badge.hidden = true;
  } else {
    $("version-name").textContent = entry.name;
    badge.textContent = entry.installed ? entry.badge : t("badge_download");
    badge.className = `badge ${entry.installed ? entry.badgeClass : "download"}`;
    badge.hidden = false;
    save("version", entry.key);
  }
  updatePlayButton();
}

function renderTabs() {
  const tabs = $("version-tabs");
  tabs.replaceChildren();
  for (const [id, title] of TABS) {
    const count = state.entries.filter((e) => e.tab === id).length;
    if (!count && id !== "installed") continue;
    const tab = document.createElement("button");
    tab.className = "menu-tab";
    tab.dataset.tab = id;
    tab.setAttribute("aria-pressed", String(state.tab === id));
    tab.textContent = title();
    tabs.append(tab);
  }
}

function renderMenu() {
  const list = $("version-list");
  const q = $("version-search").value.trim().toLowerCase();
  // Searching looks across every tab (installed copies win over remote ones).
  const seen = new Set();
  const matches = state.entries.filter((e) => {
    if (!q) return e.tab === state.tab;
    if (!e.name.toLowerCase().includes(q) && !e.sub.toLowerCase().includes(q)) return false;
    if (seen.has(e.key)) return false;
    seen.add(e.key);
    return true;
  });
  renderTabs();
  list.replaceChildren();
  if (!matches.length) {
    const empty = document.createElement("div");
    empty.className = "menu-empty";
    empty.textContent = q ? t("no_matching")
      : state.tab === "installed" ? t("nothing_installed")
      : state.remoteLoaded ? t("nothing_here") : t("loading_versions");
    list.append(empty);
    return;
  }
  for (const e of matches.slice(0, 300)) {
    const item = document.createElement("button");
    item.className = "menu-item";
    item.setAttribute("role", "option");
    item.setAttribute("aria-selected", String(e.key === state.selected?.key));
    item.dataset.key = e.key;
    item.innerHTML = `<span class="item-text"><span class="item-name"></span><span class="item-sub"></span></span>
      <svg class="item-icon ${e.installed ? "is-installed" : "is-download"}" viewBox="0 0 14 14">${
        e.installed ? '<path d="M3 7.5 6 10.5 11 4" />' : '<path d="M7 2.5v7M4 6.5l3 3 3-3M3 11.5h8" />'}</svg>`;
    item.querySelector(".item-name").textContent = e.name;
    item.querySelector(".item-sub").textContent = e.sub;
    list.append(item);
  }
}

function setMenuOpen(open) {
  const menu = $("version-menu");
  const button = $("version-button");
  if (open === !menu.hidden) return;
  menu.hidden = !open;
  button.setAttribute("aria-expanded", String(open));
  if (open) {
    $("version-search").value = "";
    state.tab = state.selected?.tab ?? (state.installed.length ? "installed" : "release");
    renderMenu();
    $("version-search").focus({ preventScroll: true });
    // Scroll only the list (scrollIntoView would also scroll a page embedding the app).
    const selected = menu.querySelector('[aria-selected="true"]');
    const list = $("version-list");
    if (selected) list.scrollTop = selected.offsetTop - list.clientHeight / 2 + selected.offsetHeight / 2;
  }
}

function initPicker() {
  $("version-button").addEventListener("click", () => setMenuOpen($("version-menu").hidden));
  $("version-search").addEventListener("input", renderMenu);
  $("version-tabs").addEventListener("click", (e) => {
    const tab = e.target.closest(".menu-tab");
    if (!tab) return;
    state.tab = tab.dataset.tab;
    $("version-search").value = "";
    renderMenu();
    $("version-list").scrollTop = 0;
  });
  $("version-list").addEventListener("click", (e) => {
    const item = e.target.closest(".menu-item");
    if (!item) return;
    selectEntry(item.dataset.key);
    setMenuOpen(false);
    $("version-button").focus({ preventScroll: true });
  });
  document.addEventListener("pointerdown", (e) => {
    if (!e.target.closest(".picker-wrap")) setMenuOpen(false);
  });
  $("version-menu").addEventListener("keydown", (e) => {
    const items = [...$("version-list").querySelectorAll(".menu-item")];
    const current = items.indexOf(document.activeElement);
    if (e.key === "Escape") {
      setMenuOpen(false);
      $("version-button").focus({ preventScroll: true });
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = e.key === "ArrowDown" ? Math.min(current + 1, items.length - 1) : current - 1;
      (next < 0 ? $("version-search") : items[next])?.focus({ preventScroll: true });
    } else if (e.key === "Enter" && document.activeElement === $("version-search") && items[0]) {
      items[0].click();
    }
  });
}

async function refreshInstalled() {
  try {
    state.installed = await invoke("java_list_versions");
  } catch (e) {
    toast(String(e), { error: true });
  }
  buildEntries();
}

// ---------- memory ----------
const GB = 1024;
// Wrapped in Unicode isolates so "4 GB" stays in order inside right-to-left sentences.
const formatGB = (mb) => `\u2066${Number.isInteger(mb / GB) ? mb / GB : (mb / GB).toFixed(1)} GB\u2069`;

async function initMemory() {
  const slider = $("memory");
  const totalMb = await invoke("system_memory_mb");
  // Leave at least 2 GB for the OS; snap to 512 MB steps.
  const max = Math.max(2 * GB, Math.floor((totalMb - 2 * GB) / 512) * 512);
  const recommended = Math.min(Math.max(Math.round(totalMb / 4 / 512) * 512, 2 * GB), 8 * GB, max);
  slider.min = 1 * GB;
  slider.max = max;
  slider.step = 512;
  const saved = Number(load("memoryMb"));
  slider.value = saved >= slider.min && saved <= max ? saved : recommended;
  const update = () => {
    const mb = Number(slider.value);
    $("memory-value").textContent = formatGB(mb);
    $("memory-hint").textContent = t("memory_hint", { recommended: formatGB(recommended), total: formatGB(Math.round(totalMb / GB) * GB) });
    slider.style.setProperty("--fill", `${((mb - slider.min) / (max - slider.min)) * 100}%`);
    save("memoryMb", mb);
  };
  slider.addEventListener("input", update);
  update();
}
const selectedMemory = () => Number($("memory").value);

// ---------- FPS Boost ----------
function initFpsBoost() {
  const fps = $("fps-boost");
  const shaders = $("shaders");
  fps.checked = load("fpsBoost") !== "false";
  shaders.checked = fps.checked && load("shaders") === "true";
  // Shaders run on top of FPS Boost (Iris needs Sodium), so they go together.
  fps.addEventListener("change", () => {
    save("fpsBoost", fps.checked);
    if (!fps.checked && shaders.checked) {
      shaders.checked = false;
      save("shaders", false);
    }
  });
  shaders.addEventListener("change", () => {
    save("shaders", shaders.checked);
    if (shaders.checked && !fps.checked) {
      fps.checked = true;
      save("fpsBoost", true);
    }
  });
}

// ---------- accounts ----------
// mode: offline | microsoft | elyby. `info` is the Microsoft account, `ely` the Ely.by one.
const account = { mode: "offline", info: null, ely: null };
const ACCOUNT_MODES = ["offline", "microsoft", "elyby"];
const hasAccount = () => account.mode === "offline" || Boolean(account.mode === "microsoft" ? account.info : account.ely);

/** The face from a skin texture; players without a skin get a generated face. */
function drawSkinFace(canvas, skinUrl, name = "") {
  const ctx = canvas.getContext("2d");
  ctx.imageSmoothingEnabled = false;
  drawAvatar(name, canvas);
  if (!skinUrl) return;
  const img = new Image();
  img.onload = () => {
    ctx.clearRect(0, 0, 8, 8);
    ctx.drawImage(img, 8, 8, 8, 8, 0, 0, 8, 8);
    ctx.drawImage(img, 40, 8, 8, 8, 0, 0, 8, 8);
  };
  img.src = skinUrl;
}

function renderAccount() {
  const ms = account.mode === "microsoft";
  const ely = account.mode === "elyby";
  $("account-mode").querySelectorAll("button").forEach((b) => b.setAttribute("aria-checked", String(b.dataset.mode === account.mode)));
  $("offline-row").hidden = account.mode !== "offline";
  $("ms-signin").hidden = !ms || Boolean(account.info);
  $("ms-row").hidden = !ms || !account.info;
  $("ms-name").textContent = account.info?.name ?? "";
  drawSkinFace($("ms-avatar"), account.info?.skin_url, account.info?.name);
  $("ely-signin").hidden = !ely || Boolean(account.ely);
  $("ely-row").hidden = !ely || !account.ely;
  $("ely-name").textContent = account.ely?.name ?? "";
  drawSkinFace($("ely-avatar"), account.ely?.skin_url, account.ely?.name);
  updatePlayButton();
}

// ---------- Ely.by account ----------
function elyError(message) {
  $("ely-error").textContent = message;
  $("ely-error").hidden = !message;
}

function openElyLogin() {
  elyError("");
  $("ely-password").value = "";
  $("ely-code").value = "";
  $("ely-code-row").hidden = true;
  $("ely-submit").disabled = false;
  $("ely-submit").textContent = t("sign_in");
  $("ely-modal").hidden = false;
  $("ely-login").focus({ preventScroll: true });
}

function closeElyLogin() {
  $("ely-modal").hidden = true;
  $("ely-password").value = "";
  $("ely-code").value = "";
}

async function submitElyLogin(e) {
  e.preventDefault();
  const login = $("ely-login").value.trim();
  const password = $("ely-password").value;
  const needsCode = !$("ely-code-row").hidden;
  const code = $("ely-code").value.trim();
  if (!login || !password) return elyError(t("ely_missing"));
  if (needsCode && !code) return elyError(t("ely_need_code"));
  elyError("");
  $("ely-submit").disabled = true;
  $("ely-submit").textContent = t("signing_in");
  try {
    account.ely = await invoke("ely_login", { login, password, code: needsCode ? code : null });
    save("elyLogin", login);
    closeElyLogin();
    toast(t("signed_in_as", { name: account.ely.name }));
    renderAccount();
  } catch (err) {
    if (String(err) === "TWO_FACTOR") {
      $("ely-code-row").hidden = false;
      elyError("");
      $("ely-code").focus({ preventScroll: true });
      toast(t("ely_two_factor"));
    } else {
      elyError(String(err));
    }
  } finally {
    $("ely-submit").disabled = false;
    $("ely-submit").textContent = t("sign_in");
  }
}

async function initEly() {
  account.ely = await invoke("ely_status");
  // Pick up skin changes made on ely.by since the last sign-in.
  if (account.ely) {
    invoke("ely_refresh_skin").then((info) => {
      if (info && account.ely) { account.ely = info; renderAccount(); }
    }).catch(() => {});
  }
  $("ely-login").value = load("elyLogin") ?? "";
  $("ely-signin").addEventListener("click", openElyLogin);
  $("ely-form").addEventListener("submit", submitElyLogin);
  $("ely-cancel").addEventListener("click", closeElyLogin);
  $("ely-modal").addEventListener("keydown", (e) => { if (e.key === "Escape") closeElyLogin(); });
  $("ely-register").addEventListener("click", () => invoke("ely_open_register").catch((e) => toast(String(e), { error: true })));
  $("ely-signout").addEventListener("click", async () => {
    await invoke("ely_logout").catch(() => {});
    account.ely = null;
    renderAccount();
  });
}

function showLoginStep(step, message = "") {
  $("ms-step-code").hidden = step !== "code";
  $("ms-step-error").hidden = step !== "error";
  $("ms-error").textContent = message;
  $("ms-cancel").textContent = step === "error" ? t("back") : t("cancel");
  $("ms-modal").hidden = false;
}

let loginActive = false;
async function signIn() {
  if (!(await invoke("msa_configured"))) {
    showLoginStep("error", t("ms_not_configured"));
    $("ms-cancel").focus({ preventScroll: true });
    return;
  }
  loginActive = true;
  try {
    const code = await invoke("msa_begin_login");
    if (!loginActive) return;
    $("ms-code").textContent = code.user_code;
    $("ms-open-label").textContent = t("ms_open", { url: code.verification_uri.replace(/^https:\/\/(www\.)?/, "") });
    showLoginStep("code");
    $("ms-open").focus({ preventScroll: true });
    account.info = await invoke("msa_finish_login");
    $("ms-modal").hidden = true;
    toast(t("signed_in_as", { name: account.info.name }));
  } catch (e) {
    // Errors after the user pressed Cancel are expected; anything else is shown.
    if (loginActive) showLoginStep("error", String(e));
  } finally {
    loginActive = false;
    renderAccount();
  }
}

async function initAccount() {
  account.mode = ACCOUNT_MODES.includes(load("accountMode")) ? load("accountMode") : "offline";
  account.info = await invoke("msa_status");
  await initEly();
  $("account-mode").addEventListener("click", (e) => {
    const b = e.target.closest("button");
    if (!b) return;
    account.mode = b.dataset.mode;
    save("accountMode", account.mode);
    renderAccount();
  });
  $("ms-signin").addEventListener("click", signIn);
  $("ms-signout").addEventListener("click", async () => {
    await invoke("msa_logout");
    account.info = null;
    renderAccount();
  });
  $("ms-open").addEventListener("click", () => invoke("msa_open_verification").catch((e) => toast(String(e), { error: true })));
  $("ms-code").addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText($("ms-code").textContent);
      toast(t("code_copied"));
    } catch {}
  });
  const cancel = () => {
    loginActive = false;
    invoke("msa_cancel_login");
    $("ms-modal").hidden = true;
  };
  $("ms-cancel").addEventListener("click", cancel);
  $("ms-modal").addEventListener("keydown", (e) => { if (e.key === "Escape") cancel(); });
  renderAccount();
}

// ---------- skins ----------
const skin = { offline: null, library: [] }; // offline: { data_url, slim } for the current username

/** Draws a whole player (front or back) from a skin texture onto a 16×32 canvas. */
function drawSkinBody(canvas, img, slim, back) {
  const ctx = canvas.getContext("2d");
  ctx.imageSmoothingEnabled = false;
  ctx.clearRect(0, 0, 16, 32);
  const modern = img.height >= 64;
  const aw = slim ? 3 : 4;
  const part = (sx, sy, w, h, dx, dy, mirror = false) => {
    if (!mirror) return ctx.drawImage(img, sx, sy, w, h, dx, dy, w, h);
    ctx.save();
    ctx.translate(dx + w, dy);
    ctx.scale(-1, 1);
    ctx.drawImage(img, sx, sy, w, h, 0, 0, w, h);
    ctx.restore();
  };
  // [front x, back x] of each texture block; old 64×32 skins mirror the right limbs.
  const armR = back ? 52 - (4 - aw) : 44, armL = back ? 44 - (4 - aw) : 36;
  const legR = back ? 12 : 4, legL = back ? 28 : 20;
  const [leftX, rightX] = [4 - aw, 12]; // viewer's left/right
  // Head and body.
  part(back ? 24 : 8, 8, 8, 8, 4, 0);
  part(back ? 32 : 20, 20, 8, 12, 4, 8);
  // Arms: front view shows the right arm on the viewer's left; back view the opposite.
  const rightArmX = back ? rightX : leftX, leftArmX = back ? leftX : rightX;
  part(armR, 20, aw, 12, rightArmX, 8);
  if (modern) part(armL, 52, aw, 12, leftArmX, 8);
  else part(armR, 20, aw, 12, leftArmX, 8, true);
  // Legs.
  const rightLegX = back ? 8 : 4, leftLegX = back ? 4 : 8;
  part(legR, 20, 4, 12, rightLegX, 20);
  if (modern) part(legL, 52, 4, 12, leftLegX, 20);
  else part(legR, 20, 4, 12, leftLegX, 20, true);
  // Outer layer.
  part(back ? 56 : 40, 8, 8, 8, 4, 0);
  if (modern) {
    part(back ? 32 : 20, 36, 8, 12, 4, 8);
    part(armR, 36, aw, 12, rightArmX, 8);
    part(armL + 16, 52, aw, 12, leftArmX, 8);
    part(legR, 36, 4, 12, rightLegX, 20);
    part(legR, 52, 4, 12, leftLegX, 20);
  }
}

/** The skin texture for the current account mode, if any. */
function currentSkin() {
  if (account.mode === "offline") return skin.offline && { url: skin.offline.data_url, slim: skin.offline.slim };
  const info = account.mode === "elyby" ? account.ely : account.info;
  return info?.skin_url ? { url: info.skin_url, slim: false } : null;
}

function renderSkinPreview() {
  const current = currentSkin();
  const front = $("skin-front"), back = $("skin-back");
  $("skin-empty").hidden = Boolean(current);
  front.hidden = back.hidden = !current;
  if (current) {
    const img = new Image();
    img.onload = () => {
      drawSkinBody(front, img, current.slim, false);
      drawSkinBody(back, img, current.slim, true);
    };
    img.src = current.url;
  }
  const offline = account.mode === "offline";
  $("skin-offline").hidden = !offline;
  $("skin-online").hidden = offline;
  $("skin-model").hidden = !offline || !skin.offline;
  $("skin-reset").hidden = !offline || !skin.offline;
  $("skin-model").querySelectorAll("button").forEach((b) => b.setAttribute("aria-checked", String((b.dataset.slim === "true") === Boolean(skin.offline?.slim))));
  if (account.mode === "elyby") {
    $("skin-online-text").textContent = t("skin_ely_text");
    $("skin-ely-open").hidden = false;
  } else if (account.mode === "microsoft") {
    $("skin-online-text").textContent = t("skin_ms_text");
    $("skin-ely-open").hidden = true;
  }
  if (offline) renderLibrary();
}

/** The library grid: one tile per saved skin, plus an "add" tile. */
function renderLibrary() {
  const grid = $("library-grid");
  grid.replaceChildren();
  for (const item of skin.library) {
    const tile = document.createElement("button");
    tile.className = "library-tile";
    tile.classList.toggle("is-active", item.data_url === skin.offline?.data_url);
    tile.title = item.name;
    const canvas = document.createElement("canvas");
    canvas.width = 16;
    canvas.height = 32;
    const img = new Image();
    img.onload = () => drawSkinBody(canvas, img, item.slim, false);
    img.src = item.data_url;
    const name = document.createElement("span");
    name.className = "tile-name";
    name.textContent = item.name;
    const remove = document.createElement("span");
    remove.className = "tile-remove";
    remove.setAttribute("role", "button");
    remove.setAttribute("aria-label", t("skin_remove"));
    remove.title = t("skin_remove");
    remove.textContent = "×";
    remove.addEventListener("click", async (e) => {
      e.stopPropagation();
      await invoke("skin_library_remove", { id: item.id }).catch((err) => skinError(String(err)));
      await loadLibrary();
      renderLibrary();
    });
    tile.append(canvas, name, remove);
    tile.addEventListener("click", () => wearSkin(item.id));
    grid.append(tile);
  }
  const add = document.createElement("button");
  add.className = "library-tile is-add";
  add.innerHTML = "<b>+</b>";
  const label = document.createElement("span");
  label.textContent = t("skin_add");
  add.append(label);
  add.addEventListener("click", () => $("skin-file").click());
  grid.append(add);
}

async function loadLibrary() {
  skin.library = await invoke("skin_library").catch(() => []) ?? [];
}

/** Puts a library skin on the current offline player. */
async function wearSkin(id) {
  skinError();
  try {
    const name = $("username").value.trim();
    skin.offline = await invoke("skin_use", { name, id });
    drawSkinFace($("avatar"), skin.offline.data_url, name);
    renderSkinPreview();
    toast(t("skin_saved"));
  } catch (e) {
    skinError(String(e));
  }
}

function skinError(message = "") {
  $("skin-error").textContent = message;
  $("skin-error").hidden = !message;
}

/** Loads the offline skin saved for the typed username and updates the avatar. */
let skinLookup = 0;
async function refreshOfflineSkin() {
  const name = $("username").value.trim();
  const ticket = ++skinLookup;
  const found = USERNAME_RE.test(name) ? await invoke("skin_get_offline", { name }).catch(() => null) : null;
  if (ticket !== skinLookup) return; // a newer lookup started
  skin.offline = found;
  if (found) drawSkinFace($("avatar"), found.data_url, name);
  else drawAvatar(name);
}

async function openSkinModal() {
  if (account.mode === "offline" && !USERNAME_RE.test($("username").value.trim())) {
    toast(t("skin_need_name"), { error: true });
    $("username").focus({ preventScroll: true });
    return;
  }
  skinError();
  if (account.mode === "offline") await loadLibrary();
  renderSkinPreview();
  $("skin-modal").hidden = false;
  $("skin-close").focus({ preventScroll: true });
}

async function addSkinFile(file) {
  if (!file) return;
  skinError();
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    let binary = "";
    for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    const name = file.name.replace(/\.png$/i, "");
    const item = await invoke("skin_library_add", { name, pngBase64: btoa(binary), slim: Boolean(skin.offline?.slim) });
    await loadLibrary();
    await wearSkin(item.id);
  } catch (e) {
    skinError(String(e));
  }
}

async function importSkin(e) {
  e.preventDefault();
  const username = $("import-name").value.trim();
  if (!username) return $("import-name").focus({ preventScroll: true });
  skinError();
  const button = $("import-button");
  button.disabled = true;
  button.textContent = t("skin_importing");
  try {
    const item = await invoke("skin_library_import", { source: $("import-source").value, username });
    $("import-name").value = "";
    await loadLibrary();
    await wearSkin(item.id);
  } catch (err) {
    skinError(String(err));
  } finally {
    button.disabled = false;
    button.textContent = t("skin_import");
  }
}

function initSkins() {
  document.querySelectorAll("[data-skin]").forEach((b) => b.addEventListener("click", openSkinModal));
  $("skin-file").addEventListener("change", (e) => {
    addSkinFile(e.target.files[0]);
    e.target.value = "";
  });
  $("library-import").addEventListener("submit", importSkin);
  $("skin-model").addEventListener("click", async (e) => {
    const b = e.target.closest("button");
    if (!b || !skin.offline) return;
    const slim = b.dataset.slim === "true";
    const item = skin.library.find((i) => i.data_url === skin.offline.data_url);
    const done = item
      ? invoke("skin_library_set_slim", { id: item.id, slim })
      : invoke("skin_set_offline_model", { name: $("username").value.trim(), slim });
    await done.catch((err) => skinError(String(err)));
    skin.offline.slim = slim;
    if (item) item.slim = slim;
    renderSkinPreview();
  });
  $("skin-reset").addEventListener("click", async () => {
    await invoke("skin_clear_offline", { name: $("username").value.trim() }).catch(() => {});
    skin.offline = null;
    drawAvatar($("username").value.trim());
    renderSkinPreview();
  });
  $("skin-ely-open").addEventListener("click", () => invoke("ely_open_skins").catch((e) => toast(String(e), { error: true })));
  const close = () => { $("skin-modal").hidden = true; };
  $("skin-close").addEventListener("click", close);
  $("skin-modal").addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
}

// ---------- mods ----------
const MOD_LOADERS = new Set(["fabric", "forge", "neoforge", "quilt"]);
const mods = { tab: "browse", source: "modrinth", query: "", offset: 0, hits: [], total: 0, loading: false, cfAvailable: false, ticket: 0 };

/** Loader + game version the mod browser works on, from the selected version. */
function modTarget() {
  const e = state.selected;
  if (!e) return null;
  return { loader: e.modLoader ?? null, game: e.modGame, name: e.name };
}

const compact = (n) => {
  try { return new Intl.NumberFormat(I18N.lang, { notation: "compact", maximumFractionDigits: 1 }).format(n); } catch { return String(n); }
};

function openMods() {
  const target = modTarget();
  if (!target) return;
  const ok = target.loader && MOD_LOADERS.has(target.loader);
  $("mods-title").textContent = ok ? t("mods_title", { name: target.name }) : t("mods");
  $("mods-need").hidden = ok;
  $("mods-body").hidden = !ok;
  if (!ok) {
    const fabric = findEntry(`fabric:${target.game}`) ?? state.entries.find((e) => e.modLoader === "fabric" && e.modGame === target.game);
    $("mods-need-text").textContent = t("mods_need_loader", { game: target.game ?? target.name });
    $("mods-use-fabric").hidden = !fabric;
    $("mods-use-fabric").textContent = t("mods_use_fabric", { game: target.game });
    $("mods-use-fabric").onclick = () => {
      selectEntry(fabric.key);
      openMods();
    };
  } else {
    renderModTabs();
    if (mods.tab === "browse") searchMods(true);
    else loadInstalledMods();
  }
  $("mods-modal").hidden = false;
  $("mods-close").focus({ preventScroll: true });
}

function renderModTabs() {
  $("mods-tabs").querySelectorAll("button").forEach((b) => b.setAttribute("aria-selected", String(b.dataset.tab === mods.tab)));
  $("mods-tabs").querySelectorAll("button").forEach((b) => b.setAttribute("aria-checked", String(b.dataset.tab === mods.tab)));
  $("mods-source").querySelectorAll("button").forEach((b) => b.setAttribute("aria-checked", String(b.dataset.source === mods.source)));
  $("mods-source").hidden = mods.tab !== "browse";
  $("mods-browse").hidden = mods.tab !== "browse";
  $("mods-installed").hidden = mods.tab !== "installed";
}

function modsMessage(list, text) {
  const p = document.createElement("p");
  p.className = "mods-empty";
  p.textContent = text;
  list.replaceChildren(p);
}

async function searchMods(reset) {
  const target = modTarget();
  const list = $("mods-results");
  if (mods.source === "curseforge" && !mods.cfAvailable) return modsMessage(list, t("mods_cf_unavailable"));
  if (reset) {
    mods.offset = 0;
    mods.hits = [];
    modsMessage(list, t("loading"));
  }
  const ticket = ++mods.ticket;
  mods.loading = true;
  try {
    const result = await invoke("mods_search", { source: mods.source, query: mods.query, loader: target.loader, game: target.game, offset: mods.offset });
    if (ticket !== mods.ticket) return;
    mods.hits = reset ? result.hits : mods.hits.concat(result.hits);
    mods.total = result.total;
    mods.offset = mods.hits.length;
    renderModResults();
  } catch (e) {
    if (ticket === mods.ticket) modsMessage(list, String(e));
  } finally {
    mods.loading = false;
  }
}

function modIcon(url) {
  const img = document.createElement("img");
  img.className = "mod-icon";
  img.alt = "";
  img.loading = "lazy";
  if (url) img.src = url;
  return img;
}

function renderModResults() {
  const list = $("mods-results");
  if (!mods.hits.length) return modsMessage(list, t("mods_none_found"));
  list.replaceChildren();
  for (const hit of mods.hits) {
    const row = document.createElement("div");
    row.className = "mod-row";
    const text = document.createElement("div");
    text.className = "mod-text";
    const title = document.createElement("button");
    title.className = "mod-title";
    title.textContent = hit.title;
    title.addEventListener("click", () => invoke("mods_open_page", { url: hit.url }).catch((e) => toast(String(e), { error: true })));
    const meta = document.createElement("span");
    meta.className = "mod-meta";
    meta.textContent = `${t("mods_by", { author: hit.author })} · ${t("mods_downloads", { count: compact(hit.downloads) })}`;
    const desc = document.createElement("span");
    desc.className = "mod-desc";
    desc.textContent = hit.description;
    text.append(title, meta, desc);
    const action = document.createElement("button");
    action.className = "mod-action";
    const markDone = () => {
      action.disabled = true;
      action.classList.add("is-done");
      action.textContent = `✓ ${t("mods_installed_badge")}`;
    };
    if (hit.installed) markDone();
    else action.textContent = t("mods_install");
    action.addEventListener("click", async () => {
      const target = modTarget();
      action.disabled = true;
      action.textContent = t("mods_installing");
      try {
        const titles = await invoke("mods_install", { source: hit.source, id: hit.id, loader: target.loader, game: target.game });
        hit.installed = true;
        markDone();
        if (titles.length) toast(t("mods_installed_toast", { mods: titles.join(", ") }));
      } catch (e) {
        action.disabled = false;
        action.textContent = t("mods_install");
        toast(String(e), { error: true, sticky: true });
      }
    });
    row.append(modIcon(hit.icon), text, action);
    list.append(row);
  }
  if (mods.hits.length < mods.total) {
    const more = document.createElement("button");
    more.className = "link-button mods-more";
    more.textContent = t("mods_load_more");
    more.addEventListener("click", () => { more.remove(); searchMods(false); });
    list.append(more);
  }
}

async function loadInstalledMods() {
  const target = modTarget();
  const list = $("mods-installed-list");
  let installed;
  try {
    installed = await invoke("mods_list", { loader: target.loader, game: target.game });
  } catch (e) {
    return modsMessage(list, String(e));
  }
  if (!installed.length) return modsMessage(list, t("mods_none_installed"));
  list.replaceChildren();
  for (const mod of installed) {
    const row = document.createElement("div");
    row.className = "mod-row";
    const text = document.createElement("div");
    text.className = "mod-text";
    const title = document.createElement("span");
    title.className = "mod-title";
    title.textContent = mod.title;
    const meta = document.createElement("span");
    meta.className = "mod-meta";
    meta.textContent = [mod.source === "curseforge" ? "CurseForge" : mod.source === "modrinth" ? "Modrinth" : null, mod.dependency ? t("mods_dependency") : null]
      .filter(Boolean).join(" · ") || mod.file;
    text.append(title, meta);
    const toggle = document.createElement("input");
    toggle.type = "checkbox";
    toggle.className = "switch";
    toggle.setAttribute("role", "switch");
    toggle.checked = mod.enabled;
    toggle.setAttribute("aria-label", mod.title);
    toggle.addEventListener("change", async () => {
      await invoke("mods_set_enabled", { loader: target.loader, game: target.game, file: mod.file, enabled: toggle.checked })
        .catch((e) => toast(String(e), { error: true }));
      loadInstalledMods();
    });
    const remove = document.createElement("button");
    remove.className = "icon-button mod-remove";
    remove.title = t("mods_remove");
    remove.setAttribute("aria-label", `${t("mods_remove")}: ${mod.title}`);
    remove.innerHTML = '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.7 8.5h5.6l.7-8.5" /></svg>';
    remove.addEventListener("click", async () => {
      await invoke("mods_remove", { loader: target.loader, game: target.game, file: mod.file }).catch((e) => toast(String(e), { error: true }));
      loadInstalledMods();
    });
    row.append(text, toggle, remove);
    list.append(row);
  }
}

async function initMods() {
  mods.cfAvailable = await invoke("mods_curseforge_available").catch(() => false);
  $("mods-button").addEventListener("click", openMods);
  $("mods-tabs").addEventListener("click", (e) => {
    const b = e.target.closest("button");
    if (!b) return;
    mods.tab = b.dataset.tab;
    renderModTabs();
    if (mods.tab === "browse") searchMods(true);
    else loadInstalledMods();
  });
  $("mods-source").addEventListener("click", (e) => {
    const b = e.target.closest("button");
    if (!b || b.dataset.source === mods.source) return;
    mods.source = b.dataset.source;
    renderModTabs();
    searchMods(true);
  });
  let timer;
  $("mods-search").addEventListener("input", () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      mods.query = $("mods-search").value;
      searchMods(true);
    }, 300);
  });
  $("mods-folder").addEventListener("click", () => {
    const target = modTarget();
    invoke("mods_open_folder", { loader: target.loader, game: target.game }).catch((e) => toast(String(e), { error: true }));
  });
  const close = () => { $("mods-modal").hidden = true; };
  $("mods-close").addEventListener("click", close);
  $("mods-modal").addEventListener("keydown", (e) => { if (e.key === "Escape") close(); });
}

// ---------- Rekindle (updates) ----------
const formatSize = (bytes) => `${(bytes / 1048576).toFixed(1)} MB`;

async function checkForUpdate() {
  let update;
  try {
    update = await invoke("rekindle_check");
  } catch {
    return; // Offline or GitHub unreachable: try again next launch.
  }
  if (!update) return;
  $("rekindle-title").textContent = t("rk_ready", { version: update.version });
  $("rekindle-sub").textContent = t("rk_sub", { current: update.current, size: formatSize(update.size) });
  const notes = $("rekindle-notes");
  notes.replaceChildren(...update.notes.split("\n").map((l) => l.replace(/^[-•*]\s*/, "").trim()).filter(Boolean).map((line) => {
    const li = document.createElement("li");
    li.textContent = line;
    return li;
  }));
  $("rekindle-modal").hidden = false;
  $("rekindle-update").focus({ preventScroll: true });
}

async function installUpdate() {
  if (state.busy) return toast(t("rk_busy"), { error: true });
  $("rekindle-update").disabled = true;
  $("rekindle-later").hidden = true;
  $("rekindle-error").hidden = true;
  $("rekindle-progress").hidden = false;
  $("rekindle-update").textContent = t("rk_downloading", { pct: 0 });
  try {
    await invoke("rekindle_install");
    $("rekindle-update").textContent = t("rk_restarting");
  } catch (e) {
    $("rekindle-error").textContent = String(e);
    $("rekindle-error").hidden = false;
    $("rekindle-update").disabled = false;
    $("rekindle-update").textContent = t("rk_retry");
    $("rekindle-later").hidden = false;
    $("rekindle-progress").hidden = true;
  }
}

function initRekindle() {
  drawLogo($("rekindle-logo"));
  $("rekindle-update").addEventListener("click", installUpdate);
  $("rekindle-later").addEventListener("click", () => { $("rekindle-modal").hidden = true; });
  listen("rekindle-progress", ({ payload }) => {
    const done = payload.total ? payload.done / payload.total : 0;
    $("rekindle-bar").style.width = `${Math.round(done * 100)}%`;
    $("rekindle-update").textContent = t("rk_downloading", { pct: Math.floor(done * 100) });
    if (done >= 1) $("rekindle-update").textContent = t("rk_installing");
  });
  // Let the window settle before checking.
  setTimeout(checkForUpdate, 2500);
}

// ---------- play ----------
const formatMB = (bytes) => `${Math.round(bytes / 1048576).toLocaleString()} MB`;

function updatePlayButton(status = null) {
  const button = $("java-play");
  const e = state.selected;
  const running = e?.installed && state.running.has(e.key);
  const needsAccount = !hasAccount();
  button.classList.toggle("is-launching", Boolean(status));
  button.classList.toggle("is-running", Boolean(running) && !status);
  button.disabled = !e || state.busy || running;
  $("java-cancel").hidden = !state.busy;
  if (status) {
    $("java-play-label").textContent = status.label;
    $("java-play-sub").textContent = status.sub;
    button.style.setProperty("--progress", status.progress ?? 0);
    return;
  }
  button.style.setProperty("--progress", 0);
  $("java-play-label").textContent = running ? t("playing") : e && !e.installed ? t("install_play") : t("play");
  $("java-play-sub").textContent = !e ? t("pick_version")
    : needsAccount ? t("sign_in_first", { service: account.mode === "elyby" ? "Ely.by" : "Microsoft" })
    : e.installed ? e.name : `${e.name} · downloads from Mojang`;
}

/** Translates the backend's progress stage names. */
const STAGE_KEYS = {
  "Fetching version info": "stage_fetching",
  "Preparing assets": "stage_assets",
  "Downloading installer": "stage_installer",
  "Setting up FPS Boost": "stage_fps",
};
const stageLabel = (stage) => (STAGE_KEYS[stage] ? t(STAGE_KEYS[stage]) : stage);
function progressLabel(stage, pct) {
  if (stage === "Downloading game files") return t("downloading_pct", { pct });
  if (stage === "Downloading FPS Boost mods") return t("fps_mods_pct", { pct });
  return `${stageLabel(stage)} ${pct}%`;
}

function showProgress(p) {
  if (!state.busy) return;
  if (!p.files_total) {
    updatePlayButton({ label: t("preparing"), sub: stageLabel(p.stage) });
    return;
  }
  const progress = p.bytes_total ? p.bytes_done / p.bytes_total : p.files_done / p.files_total;
  const amount = p.bytes_total
    ? t("of_mb", { done: formatMB(p.bytes_done), total: formatMB(p.bytes_total) })
    : t("of_files", { done: p.files_done.toLocaleString(), total: p.files_total.toLocaleString() });
  updatePlayButton({
    label: progressLabel(p.stage, Math.floor(progress * 100)),
    sub: amount,
    progress,
  });
}

async function playJava() {
  const entry = state.selected;
  if (!entry || state.busy) return;
  const offline = account.mode === "offline";
  const username = $("username").value.trim();
  if (!hasAccount()) {
    if (account.mode === "elyby") openElyLogin();
    else signIn();
    return;
  }
  if (offline && !USERNAME_RE.test(username)) {
    toast(t("username_invalid"), { error: true });
    $("username").focus({ preventScroll: true });
    return;
  }
  if (offline) save("username", username);
  const fpsBoost = $("fps-boost").checked;
  setMenuOpen(false);
  state.busy = true;
  updatePlayButton({ label: t("preparing"), sub: entry.name });
  let runKey = entry.key;
  try {
    const prepared = await invoke("java_prepare", { target: entry.key, fpsBoost, shaders: fpsBoost && $("shaders").checked });
    if (!entry.installed) {
      await refreshInstalled();
      runKey = entry.key.includes(":") ? prepared.version : entry.key;
      selectEntry(runKey);
    }
    updatePlayButton({ label: t("launching"), sub: state.selected.name, progress: 1 });
    const info = await invoke("java_launch", {
      options: {
        version: prepared.version,
        username,
        maxMemoryMb: selectedMemory(),
        online: account.mode === "microsoft",
        ely: account.mode === "elyby",
        fpsBoost,
        instance: prepared.instance,
      },
    });
    state.running.add(runKey);
    runningPids.set(info.pid, runKey);
    if (prepared.notice) toast(prepared.notice);
    else if (prepared.fps_mods.includes("Iris")) toast(t("shaders_on"));
    else if (prepared.fps_mods.length) toast(t("fps_on", { mods: prepared.fps_mods.filter((m) => m !== "Fabric API").join(", ") }));
    else toast(t("starting", { name: state.selected.name }));
  } catch (e) {
    if (String(e) === "Cancelled") toast(t("download_cancelled"));
    else toast(String(e), { error: true, sticky: true });
  } finally {
    state.busy = false;
    updatePlayButton();
  }
}

async function initJava() {
  const username = $("username");
  username.pattern = USERNAME_RE.source.slice(1, -1);
  username.value = load("username") ?? "";
  drawAvatar(username.value);
  refreshOfflineSkin();
  let skinTimer;
  username.addEventListener("input", () => {
    drawAvatar(username.value);
    clearTimeout(skinTimer);
    skinTimer = setTimeout(refreshOfflineSkin, 200);
  });
  username.addEventListener("keydown", (e) => { if (e.key === "Enter") playJava(); });

  initPicker();
  initFpsBoost();
  await Promise.all([initMemory(), initAccount()]);
  $("java-play").addEventListener("click", playJava);
  $("java-cancel").addEventListener("click", () => {
    invoke("java_cancel_prepare");
    updatePlayButton({ label: t("cancelling"), sub: state.selected?.name ?? "" });
  });

  await listen("java-progress", ({ payload }) => showProgress(payload));
  await listen("java-exited", ({ payload: pid }) => {
    const id = runningPids.get(pid);
    runningPids.delete(pid);
    if (id && ![...runningPids.values()].includes(id)) state.running.delete(id);
    updatePlayButton();
  });

  const dir = await invoke("java_game_dir");
  $("game-dir").textContent = dir ? t("game_folder", { dir: dir.replace(/^\/Users\/[^/]+/, "~") }) : t("game_folder_new");

  await refreshInstalled();
  selectEntry(load("version"));

  try {
    state.remote = await invoke("java_available_versions");
  } catch (e) {
    toast(t("mojang_unreachable"), { error: true });
  }
  state.remoteLoaded = true;
  buildEntries();
  selectEntry(load("version"));
  if (!$("version-menu").hidden) renderMenu();
}

// ---------- bedrock (Windows only) ----------
async function initBedrock() {
  if (!(await invoke("bedrock_supported"))) return;

  const tabs = $("edition-switch");
  tabs.hidden = false;
  tabs.addEventListener("click", (e) => {
    const tab = e.target.closest("button");
    if (!tab) return;
    tabs.querySelectorAll("button").forEach((b) => b.classList.toggle("is-active", b === tab));
    $("java-panel").hidden = tab.dataset.edition !== "java";
    $("bedrock-panel").hidden = tab.dataset.edition !== "bedrock";
  });

  const installed = await invoke("bedrock_is_installed");
  $("bedrock-status").textContent = installed ? t("bedrock_ready") : t("bedrock_missing");
  $("bedrock-dot").className = `dot ${installed ? "ok" : "bad"}`;
  $("bedrock-play").disabled = !installed;
  $("bedrock-play").addEventListener("click", async () => {
    try {
      await invoke("bedrock_launch");
      toast(t("bedrock_starting"));
    } catch (e) {
      toast(String(e), { error: true });
    }
  });
}

function initLanguagePicker() {
  const select = $("language");
  for (const [code, name] of LANGUAGES) select.append(new Option(name, code, false, code === I18N.lang));
  select.addEventListener("change", () => {
    save("language", select.value);
    location.reload();
  });
}

drawLogo();
drawLandscape();
initI18n(load("language")).then(() => {
  initLanguagePicker();
  initJava().catch((e) => toast(String(e), { error: true }));
  initBedrock().catch((e) => toast(String(e), { error: true }));
  initRekindle();
  initSkins();
  initMods();
});

/** Stand-in backend so the UI can be previewed in a regular browser. */
function previewBackend() {
  const installed = [
    { id: "fabric-loader-0.19.5-1.20.1", kind: "modded", loader: "Fabric", base: "1.20.1" },
    { id: "1.20.1", kind: "release" },
  ];
  const vanilla = [
    ["26.4-snapshot-2", "snapshot"], ["26.3", "release"], ["26.3-snapshot-7", "snapshot"], ["26.2", "release"],
    ["1.21.11", "release"], ["1.21.8", "release"], ["1.20.1", "release"], ["1.8.9", "release"],
    ["b1.7.3", "old_beta"], ["a1.2.6", "old_alpha"],
  ].map(([id, kind]) => ({ id, kind }));
  let progress;
  const listeners = {};
  const demoSkins = {};
  const demoLibrary = [];
  const demoMods = [];
  const DEMO_MODS = [
    { id: "sodium", title: "Sodium", author: "jellysquid3", downloads: 245e6, description: "The fastest and most compatible rendering optimization mod for Minecraft.", icon: null, url: "https://modrinth.com/mod/sodium" },
    { id: "iris", title: "Iris Shaders", author: "coderbot", downloads: 120e6, description: "A modern shader pack loader for Minecraft intended to be compatible with existing OptiFine shader packs.", icon: null, url: "https://modrinth.com/mod/iris", needs: ["Sodium"] },
    { id: "jei", title: "Just Enough Items", author: "mezz", downloads: 95e6, description: "View items and recipes in game.", icon: null, url: "https://modrinth.com/mod/jei" },
    { id: "journeymap", title: "JourneyMap", author: "techbrew", downloads: 40e6, description: "Real-time mapping in game or in a web browser as you explore.", icon: null, url: "https://modrinth.com/mod/journeymap", needs: ["Fabric API"] },
    { id: "modmenu", title: "Mod Menu", author: "Prospector", downloads: 80e6, description: "Adds a mod menu to view the list of mods you have installed.", icon: null, url: "https://modrinth.com/mod/modmenu", needs: ["Fabric API"] },
  ];
  const handlers = {
    java_game_dir: () => "/Users/you/Library/Application Support/minecraft",
    java_list_versions: () => [...installed],
    java_available_versions: () => ({ vanilla, fabric: ["26.3", "26.2", "1.21.11", "1.20.1"], forge: ["26.3", "1.21.1", "1.20.1"], neoforge: ["26.3", "1.21.1"] }),
    system_memory_mb: () => 16384,
    msa_status: () => null,
    msa_configured: () => new URLSearchParams(location.search).has("msa"),
    msa_begin_login: () => ({ user_code: "EMBR-7Q2K", verification_uri: "https://www.microsoft.com/link", expires_in: 900 }),
    msa_finish_login: () => new Promise((resolve) => setTimeout(() => resolve({ name: "Notch", uuid: "x", skin_url: null }), 60000)),
    msa_cancel_login: () => null,
    msa_open_verification: () => null,
    ely_status: () => null,
    // Demo: any login works; the password "2fa" asks for a code first.
    ely_login: ({ login, password, code }) => new Promise((resolve, reject) => setTimeout(() => {
      if (password === "2fa" && !code) reject("TWO_FACTOR");
      else resolve({ name: login.split("@")[0].slice(0, 16), uuid: "x", skin_url: null });
    }, 700)),
    ely_logout: () => null,
    ely_refresh_skin: () => null,
    // Demo: skins live in memory only.
    skin_get_offline: ({ name }) => demoSkins[name.toLowerCase()] ?? null,
    skin_set_offline: ({ name, pngBase64, slim }) => (demoSkins[name.toLowerCase()] = { data_url: `data:image/png;base64,${pngBase64}`, slim }),
    skin_set_offline_model: ({ name, slim }) => { if (demoSkins[name.toLowerCase()]) demoSkins[name.toLowerCase()].slim = slim; },
    skin_clear_offline: ({ name }) => { delete demoSkins[name.toLowerCase()]; },
    ely_open_skins: () => window.open("https://ely.by/skins", "_blank"),
    skin_library: () => demoLibrary.map((i) => ({ ...i })),
    skin_library_add: ({ name, pngBase64, slim }) => {
      const item = { id: String(Date.now()).padStart(64, "0"), name, slim, data_url: `data:image/png;base64,${pngBase64}` };
      demoLibrary.push(item);
      return item;
    },
    skin_library_import: () => Promise.reject("Importing skins works in the real app."),
    skin_library_remove: ({ id }) => { demoLibrary.splice(demoLibrary.findIndex((i) => i.id === id), 1); },
    skin_library_set_slim: ({ id, slim }) => { const i = demoLibrary.find((x) => x.id === id); if (i) i.slim = slim; },
    skin_use: ({ name, id }) => {
      const i = demoLibrary.find((x) => x.id === id);
      return (demoSkins[name.toLowerCase()] = { data_url: i.data_url, slim: i.slim });
    },
    // Demo mod browser with a few well-known Fabric mods.
    mods_curseforge_available: () => false,
    mods_search: ({ query }) => {
      const hits = DEMO_MODS.filter((m) => !query || m.title.toLowerCase().includes(query.toLowerCase()))
        .map((m) => ({ ...m, source: "modrinth", installed: demoMods.some((d) => d.title === m.title) }));
      return new Promise((r) => setTimeout(() => r({ hits, total: hits.length }), 250));
    },
    mods_install: ({ id }) => new Promise((resolve) => setTimeout(() => {
      const m = DEMO_MODS.find((x) => x.id === id);
      const added = [m.title, ...(m.needs ?? [])].filter((title) => !demoMods.some((d) => d.title === title));
      added.forEach((title) => demoMods.push({ file: `${title.toLowerCase().replace(/\W+/g, "-")}.jar`, title, source: "modrinth", enabled: true, dependency: title !== m.title }));
      resolve(added);
    }, 900)),
    mods_list: () => demoMods.map((m) => ({ ...m })),
    mods_set_enabled: ({ file, enabled }) => { const m = demoMods.find((x) => x.file === file); if (m) m.enabled = enabled; },
    mods_remove: ({ file }) => { demoMods.splice(demoMods.findIndex((x) => x.file === file), 1); },
    mods_open_folder: () => null,
    mods_open_page: ({ url }) => window.open(url, "_blank"),
    // Demo: ?update shows the Rekindle pop-up.
    rekindle_check: () => (new URLSearchParams(location.search).has("update")
      ? { version: "0.5.1", current: "0.5.0", size: 4.7 * 1048576, notes: "- Faster version downloads\n- New shader pack picker\n- Bug fixes" }
      : null),
    rekindle_install: () => new Promise((resolve) => {
      let done = 0;
      const total = 4.7 * 1048576;
      const t = setInterval(() => {
        done = Math.min(total, done + total / 25);
        listeners["rekindle-progress"]?.({ payload: { done, total } });
        if (done >= total) { clearInterval(t); resolve(); }
      }, 80);
    }),
    ely_open_register: () => window.open("https://account.ely.by/register", "_blank"),
    java_cancel_prepare: () => null,
    java_prepare: ({ target }) => new Promise((resolve) => {
      // Record the pretend install so the UI moves on to "Playing".
      const [loader, game] = target.includes(":") ? target.split(":") : [null, target];
      const names = { fabric: "Fabric", forge: "Forge", neoforge: "NeoForge" };
      const id = loader ? `${loader}-demo-${game}` : game;
      const finish = () => {
        if (!installed.some((v) => v.id === id)) {
          installed.unshift(loader
            ? { id, kind: "modded", loader: names[loader], base: game }
            : { id, kind: vanilla.find((v) => v.id === game)?.kind ?? "release" });
        }
        resolve({ version: id, instance: null, fps_mods: loader ? [] : ["Sodium", "Lithium", "FerriteCore"], notice: null });
      };
      if (installed.some((v) => v.id === id)) return setTimeout(finish, 300);
      let done = 0;
      const timer = setInterval(() => {
        done = Math.min(done + 37, 791);
        progress?.({ payload: { stage: "Downloading game files", files_done: done * 4, files_total: 3776, bytes_done: done * 1048576, bytes_total: 791 * 1048576 } });
        if (done >= 791) { clearInterval(timer); finish(); }
      }, 120);
    }),
    // Pretend the game runs for a few seconds, then closes.
    java_launch: () => {
      const pid = Math.floor(Math.random() * 1e6);
      setTimeout(() => listeners["java-exited"]?.({ payload: pid }), 6000);
      return { pid };
    },
    bedrock_supported: () => false,
  };
  return {
    core: { invoke: async (cmd, args) => handlers[cmd]?.(args) },
    event: { listen: async (name, cb) => { listeners[name] = cb; if (name === "java-progress") progress = cb; return () => {}; } },
  };
}
