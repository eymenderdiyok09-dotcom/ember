// Website translations. English lives here; other languages are files in
// lang/ (loaded only when picked) that call I18N.add("xx", {...}). The choice is
// stored as "language", which the live demo (same site) reads too.

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
  page_title: "Ember — Minecraft launcher",
  language: "Language",
  nav_features: "Features",
  nav_try: "Try it",
  nav_download: "Download",
  nav_faq: "FAQ",

  hero_title: "Minecraft launcher",
  hero_tagline: "Tiny, fast and free",
  download_ember: "Download Ember",
  download_mac: "Download for macOS",
  download_windows: "Download for Windows",
  all_downloads: "All downloads",
  menu_mac_sub: "Apple Silicon & Intel",
  menu_x64_sub: "Intel & AMD",
  menu_arm_sub: "Snapdragon",
  all_releases: "All releases",
  on_github: "on GitHub",

  stat_versions: "Minecraft versions",
  stat_size: "app size",
  stat_click: "1 click",
  stat_languages: "languages",

  features: "Features",
  f_versions_t: "Every version",
  f_versions_d: "Any release or snapshot, back to the Alpha days of 2010. Ember downloads it straight from Mojang — no official launcher needed.",
  f_fps_t: "FPS Boost",
  f_fps_d: "On by default. Sodium, Lithium, FerriteCore and more in their own folder, plus Java tuned for smooth frames. Your worlds stay shared.",
  f_shaders_t: "Shaders in one click",
  f_shaders_d: "Switch on Shaders and Ember adds Iris and Complementary Reimagined. Change packs any time in the game's video settings.",
  f_loaders_t: "Fabric, Forge, NeoForge",
  f_loaders_d: "Install any mod loader in one click. Ember picks the latest stable build for your game version.",
  f_skins_t: "Your skin",
  f_skins_d: "Pick any skin PNG for offline play and see it in-game, or bring your Ely.by skin to Ely.by servers.",
  f_accounts_t: "Offline, Ely.by & Microsoft",
  f_accounts_d: "Play offline with any name, or sign in with a free Ely.by account for online servers. Microsoft sign-in is built in and coming soon.",
  f_updates_t: "Updates itself",
  f_updates_d: "Rekindle spots new versions and installs them in one click. Every update is signed, so only real Ember updates get in.",
  f_java_t: "No Java setup",
  f_java_d: "Each version gets the exact Java runtime it needs, downloaded and checked automatically.",
  f_languages_t: "26 languages",
  f_languages_d: "English, Türkçe, Español, Русский, 日本語 and many more — Ember picks yours automatically.",

  try_it: "Try it",
  demo_title: "Interactive demo of the Ember launcher",
  demo_caption: "This is the real Ember interface, running in your browser. Type a name, pick a version and press Play — downloads here are simulated.",

  download: "Download",
  for_you: "For you",
  mac_sub: "macOS 11 or later · Apple Silicon & Intel · 4.7 MB",
  mac_step1: "Open the downloaded <b>.dmg</b> and drag <b>Ember</b> into <b>Applications</b>.",
  mac_step2: "Open Ember. It's signed and notarized by Apple, so it opens like any other app — and keeps itself up to date.",
  win_sub: "Windows 10 & 11 · portable, no installer · about 2 MB",
  x64_sub: "Intel & AMD — most PCs",
  arm_sub: "Snapdragon, Surface Pro",
  download_button: "Download",
  win_hint: "Not sure which? Open <b>Settings → System → About</b> and look at “System type”.",
  win_step1: "Right-click the downloaded <b>.zip</b> → <b>Extract All</b>, and put the folder anywhere.",
  win_step2: "Open <b>Ember.exe</b>, keeping any files next to it in the same folder.",
  win_step3: "If you see “Windows protected your PC”, click <b>More info → Run anyway</b>.",
  win_bedrock: "On Windows, Ember can also start <b>Bedrock Edition</b> if you have it installed.",

  faq: "FAQ",
  q_free: "Is it free?",
  a_free: "Yes. No ads, no accounts, no tracking — and the code is open on GitHub under the MIT license.",
  q_safe: "Is it safe?",
  a_safe: "The macOS app is signed and notarized by Apple. Game files come straight from Mojang, mods from Modrinth, and every file is checked against its official checksum. Windows builds aren't code-signed yet, so Windows shows a warning the first time.",
  q_updates: "How do I update Ember?",
  a_updates: "You don't have to. Ember 0.5.0 and newer check for updates when they open and ask before installing. Older versions need one last download from this page.",
  q_skins: "How do I change my skin?",
  a_skins: "Click your player's face next to the name in Ember. Offline: choose any 64×64 PNG — you'll see it in-game, other players won't. Ely.by: change it on ely.by and Ember picks it up.",
  q_mods: "Can I install mods?",
  a_mods: "Yes. Pick a Fabric, Forge or NeoForge version and put your mods in the <code>mods</code> folder of your Minecraft directory.",
  q_fps: "Does FPS Boost touch my mods or worlds?",
  a_fps: "No. FPS Boost runs from its own folder (<code>ember/instances</code>) and links to your real worlds, resource packs, shader packs and screenshots. Your own <code>mods</code> folder is never changed.",
  q_online: "Can I play on online servers?",
  a_online: "Yes, with a free <a href=\"https://account.ely.by/register\">Ely.by account</a>: pick <b>Ely.by</b> next to Player, sign in, and join any server that supports Ely.by. Official online-mode servers and Realms need a Microsoft account — Ember's Microsoft sign-in is built and waiting for approval from Mojang.",
  q_crash: "Minecraft doesn't start. What should I do?",
  a_crash: "Try turning FPS Boost or Shaders off, or lowering the memory slider. If it still won't start, open an issue on GitHub and attach <code>logs/mc-launcher-output.log</code> from your Minecraft folder.",

  support: "Support",
  support_text: "Something not working the way it should? Let us know and we'll help you out.",
  github_issues: "GitHub Issues",
  footer_pegi: "The game has a recommended PEGI rating of 7 and an ESRB rating of Everyone 10+.",
  footer_affiliation: "Ember is not affiliated with or endorsed by Mojang Studios. Minecraft is a trademark of Mojang AB.",
  source_on_github: "Source on GitHub",
};

const I18N = {
  strings: { en: EN },
  lang: "en",
  add(code, strings) {
    this.strings[code] = strings;
  },
};

function t(key, vars = {}) {
  const text = I18N.strings[I18N.lang]?.[key] ?? EN[key] ?? key;
  return text.replace(/\{(\w+)\}/g, (_, name) => (name in vars ? vars[name] : `{${name}}`));
}

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

/** data-i18n → text, data-i18n-html → trusted markup from our own translations,
 * data-i18n-title / data-i18n-aria → attributes. */
function applyI18n(root = document) {
  for (const el of root.querySelectorAll("[data-i18n]")) el.textContent = t(el.dataset.i18n);
  for (const el of root.querySelectorAll("[data-i18n-html]")) el.innerHTML = t(el.dataset.i18nHtml);
  for (const el of root.querySelectorAll("[data-i18n-title]")) el.title = t(el.dataset.i18nTitle);
  for (const el of root.querySelectorAll("[data-i18n-aria]")) el.setAttribute("aria-label", t(el.dataset.i18nAria));
  document.title = t("page_title");
}

async function initI18n() {
  let saved = null;
  try { saved = localStorage.getItem("language"); } catch {}
  const query = new URLSearchParams(location.search).get("lang");
  const lang = matchLanguage(query) ?? matchLanguage(saved) ?? (navigator.languages ?? [navigator.language]).map(matchLanguage).find(Boolean) ?? "en";
  if (lang !== "en") {
    await new Promise((resolve) => {
      const script = document.createElement("script");
      script.src = `lang/${lang}.js?v=1`;
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
