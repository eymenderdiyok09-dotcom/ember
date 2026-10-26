// Translations. English lives here; every other language is a file in lang/
// (loaded only when picked) that adds itself with I18N.add("xx", {...}).
// Missing keys fall back to English.

const LANGUAGES = [
  ["en", "English"],
  ["ar", "العربية"],
  ["cs", "Čeština"],
  ["de", "Deutsch"],
  ["el", "Ελληνικά"],
  ["es", "Español"],
  ["fa", "فارسی"],
  ["fr", "Français"],
  ["hi", "हिन्दी"],
  ["hu", "Magyar"],
  ["id", "Bahasa Indonesia"],
  ["it", "Italiano"],
  ["ja", "日本語"],
  ["ko", "한국어"],
  ["nl", "Nederlands"],
  ["pl", "Polski"],
  ["pt-BR", "Português (Brasil)"],
  ["ro", "Română"],
  ["ru", "Русский"],
  ["sv", "Svenska"],
  ["th", "ไทย"],
  ["tr", "Türkçe"],
  ["uk", "Українська"],
  ["vi", "Tiếng Việt"],
  ["zh-CN", "简体中文"],
  ["zh-TW", "繁體中文"],
];
const RTL_LANGUAGES = new Set(["ar", "fa"]);

const EN = {
  tagline: "Minecraft, lit fast.",
  edition_java: "Java Edition",
  edition_bedrock: "Bedrock Edition",
  language: "Language",

  player: "Player",
  account_type: "Account type",
  mode_offline: "Offline",
  mode_offline_tip: "Play offline with any name",
  mode_microsoft_tip: "Play online with your Minecraft account",
  mode_ely_tip: "Play online with a free Ely.by account",
  username_placeholder: "Choose a username",
  username_invalid: "Pick a username: 3–16 letters, numbers or _",
  change_skin: "Change skin",
  ms_signin: "Sign in with Microsoft",
  ely_signin: "Sign in with Ely.by",
  ms_account_sub: "Microsoft account · online play",
  ely_account_sub: "Ely.by account · online play",
  sign_out: "Sign out",
  signed_in_as: "Signed in as {name}",
  sign_in_first: "Sign in with {service} first",

  version: "Version",
  loading: "Loading…",
  loading_versions: "Loading versions…",
  no_versions: "No versions available",
  search_versions: "Search all versions",
  no_matching: "No matching versions",
  nothing_installed: "Nothing installed yet — pick a version from Releases",
  nothing_here: "Nothing here",
  tab_installed: "Installed",
  tab_release: "Releases",
  tab_snapshot: "Snapshots",
  tab_old: "Old",
  kind_release: "Release",
  kind_snapshot: "Snapshot",
  kind_beta: "Beta",
  kind_alpha: "Alpha",
  installed: "Installed",
  installed_latest: "Installed · latest {loader}",
  latest_loader: "Latest {loader}",
  badge_download: "Download",
  mojang_unreachable: "Couldn't reach Mojang — showing installed versions only.",

  memory: "Memory",
  memory_hint: "{recommended} recommended · {total} in this computer",
  fps_boost: "FPS Boost",
  fps_boost_sub: "Sodium, Lithium & more + tuned Java",
  shaders: "Shaders",
  shaders_sub: "Iris + Complementary Reimagined",
  shaders_quality: "Shader quality",
  shaders_fast: "Fast",
  shaders_fancy: "Fancy",

  play: "Play",
  playing: "Playing",
  install_play: "Install & Play",
  pick_version: "Pick a version",
  downloads_from_mojang: "{name} · downloads from Mojang",
  cancel: "Cancel",
  preparing: "Preparing…",
  launching: "Launching…",
  cancelling: "Cancelling…",
  downloading_pct: "Downloading {pct}%",
  fps_mods_pct: "FPS mods {pct}%",
  of_mb: "{done} of {total}",
  of_files: "{done} of {total} files",
  stage_fetching: "Fetching version info",
  stage_assets: "Preparing assets",
  stage_installer: "Downloading installer",
  stage_fps: "Setting up FPS Boost",
  fps_on: "FPS Boost on: {mods}",
  shaders_on: "Shaders on — change the pack in Options → Video Settings → Shader Packs",
  starting: "Starting {name} — the game window will open in a moment.",
  download_cancelled: "Download cancelled — press Play to resume.",
  game_folder: "Game folder · {dir}",
  game_folder_new: "Game folder will be created on first install",

  bedrock_checking: "Checking installation…",
  bedrock_ready: "Installed and ready",
  bedrock_missing: "Not installed — get it from the Microsoft Store",
  bedrock_starting: "Starting Bedrock Edition…",

  ms_preparing: "Preparing Microsoft sign-in. First use may take a moment while Ember downloads what it needs.",
  ms_title: "Microsoft Account",
  ms_enter_code: "Go to the Microsoft sign-in page and enter this code:",
  click_to_copy: "Click to copy",
  click_code_hint: "Click the code to copy it",
  ms_open: "Open {url}",
  code_copied: "Code copied",
  ms_browser: "Finish signing in in the browser window that just opened.",
  ms_reopen: "Open the sign-in page again",
  ms_waiting: "Waiting for you to finish signing in…",
  ms_not_configured: "Microsoft sign-in is not supported on this platform.",
  back: "Back",

  ely_title: "Ely.by Account",
  ely_intro: "Sign in to play on servers that support Ely.by, with your Ely.by skin.",
  ely_login_placeholder: "Email or nickname",
  password: "Password",
  code_placeholder: "6-digit code",
  sign_in: "Sign in",
  signing_in: "Signing in…",
  ely_register: "Create a free account",
  ely_missing: "Enter your email or nickname and password.",
  ely_need_code: "Enter the code from your authenticator app.",
  ely_two_factor: "This account uses two-factor sign-in. Enter the code from your authenticator app.",

  skin_title: "Skin",
  skin_front: "Skin, front",
  skin_back: "Skin, back",
  skin_default: "Default skin",
  arms_style: "Arm style",
  arms_classic: "Classic arms",
  arms_slim: "Slim arms",
  skin_choose: "Choose skin file…",
  skin_note: "Click a skin to wear it. Offline skins show in your game only — other players see the default skin.",
  skin_reset: "Use the default skin",
  skin_ely_open: "Change on Ely.by",
  skin_ely_text: "Ely.by skins are changed on ely.by — pick one from the catalogue or upload your own there. Ember shows it the next time it opens.",
  skin_ms_text: "Microsoft skins are managed on minecraft.net — your active skin is loaded automatically in Ember.",
  skin_need_name: "Pick a username first (3–16 letters, numbers or _)",
  skin_saved: "Skin saved — you'll see it in-game next time you press Play",
  done: "Done",

  skin_library: "Library",
  skin_add: "Add skin",
  skin_import_placeholder: "Player name",
  skin_import: "Import",
  skin_importing: "Importing…",
  skin_remove: "Remove from library",
  skin_source_minecraft: "Minecraft",

  mods: "Mods",
  mods_title: "Mods for {name}",
  mods_close: "Close",
  mods_browse: "Browse",
  mods_installed_tab: "Installed",
  mods_search: "Search mods",
  mods_install: "Install",
  mods_installing: "Installing…",
  mods_installed_badge: "Installed",
  mods_by: "by {author}",
  mods_downloads: "{count} downloads",
  mods_load_more: "Load more",
  mods_none_found: "No mods found",
  mods_none_installed: "No mods yet — find some under Browse",
  mods_open_folder: "Open mods folder",
  mods_remove: "Remove",
  mods_dependency: "needed by another mod",
  mods_installed_toast: "Installed {mods}",
  mods_need_loader: "Mods need Fabric, Forge or NeoForge, and {game} is plain Minecraft.",
  mods_use_fabric: "Switch to Fabric {game}",
  mods_cf_unavailable: "CurseForge isn't switched on in this build of Ember yet. Use Modrinth for now.",

  rk_ready: "Ember {version} is ready",
  rk_sub: "You have {current}. The update is {size} and Ember restarts by itself.",
  rk_update: "Update now",
  rk_later: "Later",
  rk_downloading: "Downloading… {pct}%",
  rk_installing: "Installing…",
  rk_restarting: "Restarting…",
  rk_retry: "Try again",
  rk_busy: "Wait for the current download to finish first.",
};

const I18N = {
  strings: { en: EN },
  lang: "en",
  add(code, strings) {
    this.strings[code] = strings;
  },
};

/** Translated string for `key`, with {placeholders} filled from `vars`. */
function t(key, vars = {}) {
  const text = I18N.strings[I18N.lang]?.[key] ?? EN[key] ?? key;
  return text.replace(/\{(\w+)\}/g, (_, name) => (name in vars ? vars[name] : `{${name}}`));
}

/** Best supported language for a BCP 47 tag like "pt-BR" or "tr". */
function matchLanguage(tag) {
  if (!tag) return null;
  const codes = LANGUAGES.map(([code]) => code);
  const lower = tag.toLowerCase();
  const exact = codes.find((c) => c.toLowerCase() === lower);
  if (exact) return exact;
  if (lower.startsWith("zh")) return /tw|hk|mo|hant/.test(lower) ? "zh-TW" : "zh-CN";
  if (lower.startsWith("pt")) return "pt-BR";
  const base = lower.split("-")[0];
  return codes.find((c) => c === base) ?? null;
}

/** Fills elements marked with data-i18n (text), data-i18n-placeholder,
 * data-i18n-title and data-i18n-aria (aria-label). */
function applyI18n(root = document) {
  for (const el of root.querySelectorAll("[data-i18n]")) el.textContent = t(el.dataset.i18n);
  for (const el of root.querySelectorAll("[data-i18n-placeholder]")) el.placeholder = t(el.dataset.i18nPlaceholder);
  for (const el of root.querySelectorAll("[data-i18n-title]")) el.title = t(el.dataset.i18nTitle);
  for (const el of root.querySelectorAll("[data-i18n-aria]")) el.setAttribute("aria-label", t(el.dataset.i18nAria));
}

/** Picks the saved or system language and loads its file (English needs none). */
async function initI18n(saved) {
  const lang = matchLanguage(saved) ?? (navigator.languages ?? [navigator.language]).map(matchLanguage).find(Boolean) ?? "en";
  if (lang !== "en") {
    await new Promise((resolve) => {
      const script = document.createElement("script");
      script.src = `lang/${lang}.js`;
      script.onload = script.onerror = resolve;
      document.head.append(script);
    });
  }
  I18N.lang = I18N.strings[lang] ? lang : "en";
  document.documentElement.lang = I18N.lang;
  document.documentElement.dir = RTL_LANGUAGES.has(I18N.lang) ? "rtl" : "ltr";
  applyI18n();
  return I18N.lang;
}
