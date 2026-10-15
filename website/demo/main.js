const tauri = window.__TAURI__ ?? previewBackend();
const { invoke } = tauri.core;
const { listen } = tauri.event;

const $ = (id) => document.getElementById(id);
const load = (key) => { try { return localStorage.getItem(key); } catch { return null; } };
const save = (key, value) => { try { localStorage.setItem(key, value); } catch {} };

const USERNAME_RE = /^[A-Za-z0-9_]{3,16}$/;
const KIND_LABEL = { release: "Release", snapshot: "Snapshot", old_beta: "Beta", old_alpha: "Alpha" };

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

function drawLogo() {
  const ns = "http://www.w3.org/2000/svg";
  const svg = $("logo");
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

/** Blocky landscape behind everything, drawn small and blurred by CSS (same as the website). */
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
  }
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
  ["installed", "Installed"],
  ["release", "Releases"],
  ["snapshot", "Snapshots"],
  ["fabric", "Fabric"],
  ["forge", "Forge"],
  ["neoforge", "NeoForge"],
  ["old", "Old"],
];
const BADGE_CLASS = { release: "release", snapshot: "snapshot", old_beta: "legacy", old_alpha: "legacy", modded: "modded" };

function installedEntry(v) {
  const modded = v.kind === "modded";
  return {
    key: v.id,
    tab: "installed",
    name: modded ? `${v.loader} ${v.base}` : v.id,
    sub: modded ? v.id : KIND_LABEL[v.kind] ?? v.kind,
    badge: modded ? v.loader : KIND_LABEL[v.kind] ?? v.kind,
    badgeClass: BADGE_CLASS[v.kind] ?? "legacy",
    installed: true,
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
      sub: installedIds.has(v.id) ? "Installed" : KIND_LABEL[v.kind] ?? v.kind,
      badge: KIND_LABEL[v.kind] ?? v.kind,
      badgeClass: BADGE_CLASS[v.kind] ?? "legacy",
      installed: installedIds.has(v.id),
    });
  }
  for (const [tab, loader, prefix] of [["fabric", "Fabric", "fabric"], ["forge", "Forge", "forge"], ["neoforge", "NeoForge", "neoforge"]]) {
    const installed = loaderInstalled(loader);
    for (const game of state.remote[tab] ?? []) {
      entries.push({
        key: `${prefix}:${game}`,
        tab,
        name: `${loader} ${game}`,
        sub: installed.has(game) ? `Installed · latest ${loader}` : `Latest ${loader}`,
        badge: loader,
        badgeClass: "modded",
        installed: installed.has(game),
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
    $("version-name").textContent = state.remoteLoaded ? "No versions available" : "Loading versions…";
    badge.hidden = true;
  } else {
    $("version-name").textContent = entry.name;
    badge.textContent = entry.installed ? entry.badge : "Download";
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
    tab.textContent = title;
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
    empty.textContent = q ? "No matching versions"
      : state.tab === "installed" ? "Nothing installed yet — pick a version from Releases"
      : state.remoteLoaded ? "Nothing here" : "Loading versions…";
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
const formatGB = (mb) => `${Number.isInteger(mb / GB) ? mb / GB : (mb / GB).toFixed(1)} GB`;

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
    $("memory-hint").textContent = `${formatGB(recommended)} recommended · ${formatGB(Math.round(totalMb / GB) * GB)} in this computer`;
    slider.style.setProperty("--fill", `${((mb - slider.min) / (max - slider.min)) * 100}%`);
    save("memoryMb", mb);
  };
  slider.addEventListener("input", update);
  update();
}
const selectedMemory = () => Number($("memory").value);

// ---------- FPS Boost ----------
function initFpsBoost() {
  const toggle = $("fps-boost");
  toggle.checked = load("fpsBoost") !== "false";
  toggle.addEventListener("change", () => save("fpsBoost", toggle.checked));
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
  $("ely-submit").textContent = "Sign in";
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
  if (!login || !password) return elyError("Enter your email or nickname and password.");
  if (needsCode && !code) return elyError("Enter the code from your authenticator app.");
  elyError("");
  $("ely-submit").disabled = true;
  $("ely-submit").textContent = "Signing in…";
  try {
    account.ely = await invoke("ely_login", { login, password, code: needsCode ? code : null });
    save("elyLogin", login);
    closeElyLogin();
    toast(`Signed in as ${account.ely.name}`);
    renderAccount();
  } catch (err) {
    if (String(err) === "TWO_FACTOR") {
      $("ely-code-row").hidden = false;
      elyError("");
      $("ely-code").focus({ preventScroll: true });
      toast("This account uses two-factor sign-in. Enter the code from your authenticator app.");
    } else {
      elyError(String(err));
    }
  } finally {
    $("ely-submit").disabled = false;
    $("ely-submit").textContent = "Sign in";
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
  $("ms-cancel").textContent = step === "error" ? "Back" : "Cancel";
  $("ms-modal").hidden = false;
}

let loginActive = false;
async function signIn() {
  if (!(await invoke("msa_configured"))) {
    showLoginStep("error",
      "Microsoft sign-in isn't switched on in this build of Ember yet. It needs an app ID approved by Mojang; until then, use Offline mode.");
    $("ms-cancel").focus({ preventScroll: true });
    return;
  }
  loginActive = true;
  try {
    const code = await invoke("msa_begin_login");
    if (!loginActive) return;
    $("ms-code").textContent = code.user_code;
    $("ms-open-label").textContent = `Open ${code.verification_uri.replace(/^https:\/\/(www\.)?/, "")}`;
    showLoginStep("code");
    $("ms-open").focus({ preventScroll: true });
    account.info = await invoke("msa_finish_login");
    $("ms-modal").hidden = true;
    toast(`Signed in as ${account.info.name}`);
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
      toast("Code copied");
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
  $("java-play-label").textContent = running ? "Playing" : e && !e.installed ? "Install & Play" : "Play";
  $("java-play-sub").textContent = !e ? "Pick a version"
    : needsAccount ? `Sign in with ${account.mode === "elyby" ? "Ely.by" : "Microsoft"} first`
    : e.installed ? e.name : `${e.name} · downloads from Mojang`;
}

function showProgress(p) {
  if (!state.busy) return;
  if (!p.files_total) {
    updatePlayButton({ label: "Preparing…", sub: p.stage });
    return;
  }
  const progress = p.bytes_total ? p.bytes_done / p.bytes_total : p.files_done / p.files_total;
  const amount = p.bytes_total
    ? `${formatMB(p.bytes_done)} of ${formatMB(p.bytes_total)}`
    : `${p.files_done.toLocaleString()} of ${p.files_total.toLocaleString()} files`;
  updatePlayButton({
    label: `${p.stage === "Downloading game files" ? "Downloading" : p.stage} ${Math.floor(progress * 100)}%`.replace("Downloading FPS Boost mods", "FPS mods"),
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
    toast("Pick a username: 3–16 letters, numbers or _", { error: true });
    $("username").focus({ preventScroll: true });
    return;
  }
  if (offline) save("username", username);
  const fpsBoost = $("fps-boost").checked;
  setMenuOpen(false);
  state.busy = true;
  updatePlayButton({ label: "Preparing…", sub: entry.name });
  let runKey = entry.key;
  try {
    const prepared = await invoke("java_prepare", { target: entry.key, fpsBoost });
    if (!entry.installed) {
      await refreshInstalled();
      runKey = entry.key.includes(":") ? prepared.version : entry.key;
      selectEntry(runKey);
    }
    updatePlayButton({ label: "Launching…", sub: state.selected.name, progress: 1 });
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
    else if (prepared.fps_mods.length) toast(`FPS Boost on: ${prepared.fps_mods.filter((m) => m !== "Fabric API").join(", ")}`);
    else toast(`Starting ${state.selected.name} — the game window will open in a moment.`);
  } catch (e) {
    if (String(e) === "Cancelled") toast("Download cancelled — press Play to resume.");
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
  username.addEventListener("input", () => drawAvatar(username.value));
  username.addEventListener("keydown", (e) => { if (e.key === "Enter") playJava(); });

  initPicker();
  initFpsBoost();
  await Promise.all([initMemory(), initAccount()]);
  $("java-play").addEventListener("click", playJava);
  $("java-cancel").addEventListener("click", () => {
    invoke("java_cancel_prepare");
    updatePlayButton({ label: "Cancelling…", sub: state.selected?.name ?? "" });
  });

  await listen("java-progress", ({ payload }) => showProgress(payload));
  await listen("java-exited", ({ payload: pid }) => {
    const id = runningPids.get(pid);
    runningPids.delete(pid);
    if (id && ![...runningPids.values()].includes(id)) state.running.delete(id);
    updatePlayButton();
  });

  const dir = await invoke("java_game_dir");
  $("game-dir").textContent = dir ? `Game folder · ${dir.replace(/^\/Users\/[^/]+/, "~")}` : "Game folder will be created on first install";

  await refreshInstalled();
  selectEntry(load("version"));

  try {
    state.remote = await invoke("java_available_versions");
  } catch (e) {
    toast("Couldn't reach Mojang — showing installed versions only.", { error: true });
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
  $("bedrock-status").textContent = installed ? "Installed and ready" : "Not installed — get it from the Microsoft Store";
  $("bedrock-dot").className = `dot ${installed ? "ok" : "bad"}`;
  $("bedrock-play").disabled = !installed;
  $("bedrock-play").addEventListener("click", async () => {
    try {
      await invoke("bedrock_launch");
      toast("Starting Bedrock Edition…");
    } catch (e) {
      toast(String(e), { error: true });
    }
  });
}

drawLogo();
drawLandscape();
initJava().catch((e) => toast(String(e), { error: true }));
initBedrock().catch((e) => toast(String(e), { error: true }));

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
