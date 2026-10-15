//! Downloads Minecraft: Java Edition from Mojang's public servers: version
//! metadata, client jar, libraries, assets and the matching Java runtime.
//! Mod loaders: Fabric (meta.fabricmc.net), Forge and NeoForge (their own
//! installers, run headless). FPS Boost adds performance mods from Modrinth
//! in a separate game folder.

use crate::minecraft_java::{
    find_java, instance_dir, java_component, maven_rel, minecraft_dir, native_classifier,
    read_version_json, resolve_version, rules_allow, runtime_home, runtime_java,
    runtime_platforms, str_at, validate_id,
};
use serde::Serialize;
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const VERSION_MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const RUNTIME_INDEX: &str = "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
const ASSETS_BASE: &str = "https://resources.download.minecraft.net";
const LIBRARIES_BASE: &str = "https://libraries.minecraft.net/";
const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
const FORGE_PROMOS: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net/net/minecraftforge/forge";
const NEOFORGE_VERSIONS: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";
const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const WORKERS: usize = 16;
const CANCELLED: &str = "Cancelled";

/// Performance mods FPS Boost installs (Modrinth slugs). Sodium is required;
/// the rest are added when available for the game version.
const FPS_MODS: &[&str] = &["sodium", "lithium", "ferrite-core", "entityculling", "immediatelyfast"];
/// Shader loader added on top of FPS Boost when shaders are on.
const SHADER_LOADER: &str = "iris";
/// Shader packs Ember installs (Modrinth projects), by quality setting.
const SHADER_PACK_FAST: &str = "makeup-ultra-fast-shaders";
const SHADER_PACK_FANCY: &str = "complementary-reimagined";
/// File name prefixes of the packs above, so Ember can tell its own packs
/// from ones the player picked in the game.
const EMBER_PACK_PREFIXES: &[&str] = &["MakeUp-UltraFast", "ComplementaryReimagined"];

/// The Modrinth shader pack for a quality setting ("fast" unless "fancy").
fn shader_pack_for(quality: &str) -> &'static str {
    if quality == "fancy" { SHADER_PACK_FANCY } else { SHADER_PACK_FAST }
}
/// Folders an FPS Boost instance shares with the main game folder.
const SHARED_FOLDERS: &[&str] = &["saves", "resourcepacks", "shaderpacks", "screenshots"];

static CANCEL: AtomicBool = AtomicBool::new(false);

fn check_cancel() -> Result<(), String> {
    if CANCEL.load(Ordering::Relaxed) { Err(CANCELLED.into()) } else { Ok(()) }
}

#[derive(Clone, Copy)]
enum ForgeLoader {
    Forge,
    NeoForge,
}

impl ForgeLoader {
    fn name(self) -> &'static str {
        match self {
            ForgeLoader::Forge => "Forge",
            ForgeLoader::NeoForge => "NeoForge",
        }
    }
}

enum Target<'a> {
    Vanilla(&'a str),
    Fabric(&'a str),
    Forge(ForgeLoader, &'a str),
}

impl<'a> Target<'a> {
    fn parse(target: &'a str) -> Result<Self, String> {
        let parsed = match target.split_once(':') {
            Some(("fabric", game)) => Target::Fabric(game),
            Some(("forge", game)) => Target::Forge(ForgeLoader::Forge, game),
            Some(("neoforge", game)) => Target::Forge(ForgeLoader::NeoForge, game),
            Some(_) => return Err(format!("Unknown version '{target}'")),
            None => Target::Vanilla(target),
        };
        let (Target::Vanilla(id) | Target::Fabric(id) | Target::Forge(_, id)) = parsed;
        validate_id(id)?;
        Ok(parsed)
    }
}

/// What to launch after `java_prepare`.
#[derive(Serialize)]
pub struct Prepared {
    pub version: String,
    /// Ember-managed game folder to run in (FPS Boost).
    pub instance: Option<String>,
    /// FPS Boost mods in use.
    pub fps_mods: Vec<String>,
    /// Something the user should know (e.g. FPS Boost unavailable).
    pub notice: Option<String>,
}

#[derive(Serialize)]
pub struct RemoteVersion {
    pub id: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct AvailableVersions {
    pub vanilla: Vec<RemoteVersion>,
    /// Game versions each loader supports, newest first.
    pub fabric: Vec<String>,
    pub forge: Vec<String>,
    pub neoforge: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct Progress {
    pub stage: &'static str,
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Clone)]
struct Download {
    url: String,
    path: PathBuf,
    sha1: Option<String>,
    size: Option<u64>,
    /// Unix permission bit for runtime binaries; Windows has no equivalent.
    #[cfg_attr(not(unix), allow(dead_code))]
    executable: bool,
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .user_agent(concat!("Ember/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

pub(crate) fn get_json(agent: &ureq::Agent, url: &str) -> Result<Value, String> {
    agent
        .get(url)
        .call()
        .map_err(|e| format!("{url}: {e}"))?
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_json()
        .map_err(|e| format!("{url}: {e}"))
}

pub(crate) fn write_json(path: &Path, json: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(json).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Versions available to download.
#[tauri::command]
pub async fn java_available_versions() -> Result<AvailableVersions, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let agent = agent();
        let manifest = get_json(&agent, VERSION_MANIFEST)?;
        let vanilla = manifest["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                Some(RemoteVersion {
                    id: str_at(v, "id")?.to_owned(),
                    kind: str_at(v, "type")?.to_owned(),
                })
            })
            .collect();
        // Loaders are optional: a failure here shouldn't hide vanilla versions.
        let fabric = get_json(&agent, &format!("{FABRIC_META}/versions/game"))
            .map(|list| {
                list.as_array()
                    .into_iter()
                    .flatten()
                    .filter(|v| v["stable"].as_bool() == Some(true))
                    .filter_map(|v| str_at(v, "version").map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let forge = get_json(&agent, FORGE_PROMOS)
            .map(|promos| {
                let keys = promos["promos"].as_object().map(|m| m.keys().cloned().collect::<Vec<_>>());
                let games = keys.into_iter().flatten().filter_map(|k| {
                    let game = k.rsplit_once('-')?.0.to_owned();
                    forge_has_cli_installer(&game).then_some(game)
                });
                newest_first_unique(games)
            })
            .unwrap_or_default();
        let neoforge = get_json(&agent, NEOFORGE_VERSIONS)
            .map(|list| {
                let versions = list["versions"].as_array().cloned().unwrap_or_default();
                newest_first_unique(versions.iter().filter_map(|v| neoforge_game_version(v.as_str()?)))
            })
            .unwrap_or_default();
        Ok(AvailableVersions { vanilla, fabric, forge, neoforge })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Makes sure everything needed to play `target` is on disk and returns what
/// to launch. `target` is a version id, or `fabric:`/`forge:`/`neoforge:` +
/// game version for the latest loader. With `fps_boost`, vanilla versions run
/// on Fabric with performance mods in a separate game folder.
#[tauri::command]
pub async fn java_prepare(
    app: AppHandle,
    target: String,
    fps_boost: bool,
    shaders: Option<bool>,
    shader_quality: Option<String>,
) -> Result<Prepared, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let pack = shaders.unwrap_or(false).then(|| shader_pack_for(shader_quality.as_deref().unwrap_or("fast")));
        prepare(&target, fps_boost, pack, &|p| {
            let _ = app.emit("java-progress", p);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stops a running `java_prepare` (it returns "Cancelled").
#[tauri::command]
pub fn java_cancel_prepare() {
    CANCEL.store(true, Ordering::Relaxed);
}

type Report<'a> = &'a (dyn Fn(Progress) + Sync);

fn report_stage(report: Report, stage: &'static str) {
    report(Progress { stage, files_done: 0, files_total: 0, bytes_done: 0, bytes_total: 0 });
}

/// `shaders` is the Modrinth shader pack to use, or `None` for no shaders.
pub(crate) fn prepare(target: &str, fps_boost: bool, shaders: Option<&str>, report: Report) -> Result<Prepared, String> {
    CANCEL.store(false, Ordering::Relaxed);
    let mc = minecraft_dir().ok_or("could not resolve .minecraft directory")?;
    let agent = agent();
    report_stage(report, "Fetching version info");

    let target = Target::parse(target)?;
    let id = match target {
        Target::Vanilla(id) => id.to_owned(),
        Target::Fabric(game) => install_fabric_profile(&agent, &mc, game)?,
        Target::Forge(loader, game) => install_forge(&agent, &mc, loader, game, report)?,
    };
    install_version(&agent, &mc, &id, report)?;

    let mut prepared = Prepared { version: id.clone(), instance: None, fps_mods: Vec::new(), notice: None };
    let base = read_version_json(&mc, &id)?.get("inheritsFrom").and_then(Value::as_str).map(str::to_owned);
    let is_vanilla = matches!(target, Target::Vanilla(_)) && base.is_none();
    // Modded versions run in their own folder so each loader + game version
    // keeps its own mods; worlds and packs stay shared.
    if let Some(game) = &base {
        let loader = crate::minecraft_java::loader_name(&id).to_ascii_lowercase();
        let (instance, _) = mod_instance(&mc, &loader, game)?;
        prepared.instance = Some(instance);
    }
    if fps_boost && is_vanilla {
        match fps_boost_instance(&agent, &mc, &id, shaders, report) {
            Ok(Some(boosted)) => prepared = boosted,
            Ok(None) => {
                prepared.notice = Some(format!(
                    "FPS Boost mods aren't available for {id}, so only the Java tuning is applied."
                ))
            }
            Err(e) if e == CANCELLED => return Err(e),
            Err(e) => prepared.notice = Some(format!("FPS Boost mods couldn't be set up ({e}); playing without them.")),
        }
    } else if shaders.is_some() {
        prepared.notice = Some("Shaders work with FPS Boost on a plain Minecraft version, so they're off for this one.".into());
    }
    Ok(prepared)
}

/// Downloads everything an installed or Mojang-listed version needs.
fn install_version(agent: &ureq::Agent, mc: &Path, id: &str, report: Report) -> Result<(), String> {
    let mut manifest = None;
    // Version JSONs for the whole inheritsFrom chain.
    let mut next = Some(id.to_owned());
    while let Some(current) = next.take() {
        ensure_version_json(agent, mc, &current, &mut manifest)?;
        let json = read_version_json(mc, &current)?;
        next = str_at(&json, "inheritsFrom").map(str::to_owned);
    }
    let json = resolve_version(mc, id)?;

    let mut downloads = Vec::new();
    client_download(mc, &json, &mut downloads)?;
    library_downloads(mc, &json, &mut downloads);
    let assets = asset_downloads(agent, mc, &json, &mut downloads)?;
    let links = runtime_downloads(agent, mc, &json, &mut downloads)?;

    // Skip what's already on disk.
    let mut seen = HashSet::new();
    downloads.retain(|d| seen.insert(d.path.clone()) && needs_download(d));
    run_downloads(agent, downloads, "Downloading game files", report)?;

    finish_runtime(&links)?;
    if let Some(assets) = assets {
        report_stage(report, "Preparing assets");
        copy_legacy_assets(mc, &assets)?;
    }
    Ok(())
}

fn ensure_version_json(
    agent: &ureq::Agent,
    mc: &Path,
    id: &str,
    manifest: &mut Option<Value>,
) -> Result<(), String> {
    let path = mc.join("versions").join(id).join(format!("{id}.json"));
    if path.is_file() {
        return Ok(());
    }
    if manifest.is_none() {
        *manifest = Some(get_json(agent, VERSION_MANIFEST)?);
    }
    let entry = manifest.as_ref().unwrap()["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|v| str_at(v, "id") == Some(id))
        .ok_or_else(|| format!("Unknown version '{id}'"))?;
    let url = str_at(entry, "url").ok_or("version entry has no url")?;
    write_json(&path, &get_json(agent, url)?)
}

/// Writes the Fabric profile JSON for the latest stable loader and returns its id.
fn install_fabric_profile(agent: &ureq::Agent, mc: &Path, game: &str) -> Result<String, String> {
    let loaders = get_json(agent, &format!("{FABRIC_META}/versions/loader/{game}"))?;
    let loaders = loaders.as_array().filter(|l| !l.is_empty()).ok_or_else(|| format!("Fabric doesn't support {game} yet"))?;
    let loader = loaders
        .iter()
        .find(|l| l["loader"]["stable"].as_bool() == Some(true))
        .unwrap_or(&loaders[0]);
    let loader = loader["loader"]["version"].as_str().ok_or("bad Fabric loader entry")?;
    let profile = get_json(agent, &format!("{FABRIC_META}/versions/loader/{game}/{loader}/profile/json"))?;
    let id = str_at(&profile, "id").ok_or("Fabric profile has no id")?.to_owned();
    write_json(&mc.join("versions").join(&id).join(format!("{id}.json")), &profile)?;
    Ok(id)
}

fn client_download(mc: &Path, json: &Value, out: &mut Vec<Download>) -> Result<(), String> {
    let jar = str_at(json, "jar").ok_or("version has no jar")?;
    let client = &json["downloads"]["client"];
    let url = str_at(client, "url").ok_or("version has no client download")?;
    out.push(Download {
        url: url.to_owned(),
        path: mc.join("versions").join(jar).join(format!("{jar}.jar")),
        sha1: str_at(client, "sha1").map(str::to_owned),
        size: client["size"].as_u64(),
        executable: false,
    });
    Ok(())
}

fn library_downloads(mc: &Path, json: &Value, out: &mut Vec<Download>) {
    let libs = mc.join("libraries");
    let from_meta = |meta: &Value| {
        Some(Download {
            url: str_at(meta, "url").filter(|u| !u.is_empty())?.to_owned(),
            path: libs.join(str_at(meta, "path")?),
            sha1: str_at(meta, "sha1").map(str::to_owned),
            size: meta["size"].as_u64(),
            executable: false,
        })
    };
    for lib in json["libraries"].as_array().into_iter().flatten() {
        if !rules_allow(lib.get("rules")) {
            continue;
        }
        let Some(name) = str_at(lib, "name") else { continue };

        if let Some(classifier) = native_classifier(lib) {
            if let Some(d) = from_meta(&lib["downloads"]["classifiers"][&classifier]) {
                out.push(d);
            }
        }
        if lib["downloads"]["artifact"].is_object() {
            out.extend(from_meta(&lib["downloads"]["artifact"]));
        } else if lib.get("natives").is_none() || lib.get("url").is_some() {
            // Maven-style entry (Fabric etc.): repository URL + coordinates.
            let rel = maven_rel(name, None);
            let base = str_at(lib, "url").unwrap_or(LIBRARIES_BASE);
            out.push(Download {
                url: format!("{}/{rel}", base.trim_end_matches('/')),
                path: libs.join(&rel),
                sha1: str_at(lib, "sha1").map(str::to_owned),
                size: lib["size"].as_u64(),
                executable: false,
            });
        }
    }
}

struct AssetIndex {
    id: String,
    json: Value,
}

fn asset_downloads(
    agent: &ureq::Agent,
    mc: &Path,
    json: &Value,
    out: &mut Vec<Download>,
) -> Result<Option<AssetIndex>, String> {
    let meta = &json["assetIndex"];
    let (Some(id), Some(url)) = (str_at(meta, "id"), str_at(meta, "url")) else {
        return Ok(None);
    };
    let path = mc.join("assets").join("indexes").join(format!("{id}.json"));
    let index = match fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()) {
        Some(index) if meta["size"].as_u64().map_or(true, |s| file_len(&path) == Some(s)) => index,
        _ => {
            let index = get_json(agent, url)?;
            write_json(&path, &index)?;
            index
        }
    };
    let objects = mc.join("assets").join("objects");
    for object in index["objects"].as_object().into_iter().flatten().map(|(_, o)| o) {
        let Some(hash) = str_at(object, "hash").filter(|h| h.len() > 2) else { continue };
        out.push(Download {
            url: format!("{ASSETS_BASE}/{}/{hash}", &hash[..2]),
            path: objects.join(&hash[..2]).join(hash),
            sha1: Some(hash.to_owned()),
            size: object["size"].as_u64(),
            executable: false,
        });
    }
    Ok(Some(AssetIndex { id: id.to_owned(), json: index }))
}

/// Pre-1.7 versions read assets by name from `assets/virtual/<id>` or `resources/`.
fn copy_legacy_assets(mc: &Path, index: &AssetIndex) -> Result<(), String> {
    let json = &index.json;
    let dest = if json["map_to_resources"].as_bool() == Some(true) {
        mc.join("resources")
    } else if json["virtual"].as_bool() == Some(true) {
        mc.join("assets").join("virtual").join(&index.id)
    } else {
        return Ok(());
    };
    let objects = mc.join("assets").join("objects");
    for (name, object) in json["objects"].as_object().into_iter().flatten() {
        let Some(hash) = str_at(object, "hash").filter(|h| h.len() > 2) else { continue };
        let target = dest.join(name);
        if target.is_file() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(objects.join(&hash[..2]).join(hash), &target).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Symlinks inside a Java runtime, created after its files are downloaded.
type Links = Vec<(PathBuf, String)>;

fn runtime_downloads(
    agent: &ureq::Agent,
    mc: &Path,
    json: &Value,
    out: &mut Vec<Download>,
) -> Result<Links, String> {
    let component = java_component(json);
    if runtime_platforms()
        .iter()
        .any(|p| runtime_java(&runtime_home(mc, component, p)).is_file())
    {
        return Ok(Vec::new());
    }

    let index = get_json(agent, RUNTIME_INDEX)?;
    let (platform, entry) = runtime_platforms()
        .iter()
        .find_map(|p| Some((*p, index[*p][component].as_array()?.first()?)))
        .ok_or_else(|| format!("Mojang has no '{component}' Java runtime for this computer"))?;
    let manifest_url = entry["manifest"]["url"].as_str().ok_or("bad runtime entry")?;
    let manifest = get_json(agent, manifest_url)?;

    let home = runtime_home(mc, component, platform);
    let mut links = Vec::new();
    for (rel, file) in manifest["files"].as_object().into_iter().flatten() {
        let path = home.join(rel);
        match str_at(file, "type") {
            Some("directory") => fs::create_dir_all(&path).map_err(|e| e.to_string())?,
            Some("file") => {
                let raw = &file["downloads"]["raw"];
                let Some(url) = str_at(raw, "url") else { continue };
                out.push(Download {
                    url: url.to_owned(),
                    path,
                    sha1: str_at(raw, "sha1").map(str::to_owned),
                    size: raw["size"].as_u64(),
                    executable: file["executable"].as_bool() == Some(true),
                });
            }
            Some("link") => links.extend(str_at(file, "target").map(|t| (path, t.to_owned()))),
            _ => {}
        }
    }
    Ok(links)
}

fn finish_runtime(links: &Links) -> Result<(), String> {
    #[cfg(unix)]
    for (path, target) in links {
        if path.symlink_metadata().is_ok() {
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::os::unix::fs::symlink(target, path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    #[cfg(not(unix))]
    let _ = links;
    Ok(())
}

fn file_len(path: &Path) -> Option<u64> {
    fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len())
}

fn needs_download(d: &Download) -> bool {
    match (file_len(&d.path), d.size) {
        (None, _) => true,
        (Some(len), Some(size)) => len != size,
        (Some(_), None) => false,
    }
}

fn run_downloads(
    agent: &ureq::Agent,
    downloads: Vec<Download>,
    stage: &'static str,
    report: Report,
) -> Result<(), String> {
    check_cancel()?;
    let files_total = downloads.len();
    let bytes_total = downloads.iter().filter_map(|d| d.size).sum();
    let next = AtomicUsize::new(0);
    let files_done = AtomicUsize::new(0);
    let bytes_done = AtomicU64::new(0);
    let failed = AtomicBool::new(false);
    let error = Mutex::new(None);
    let last_report = Mutex::new(Instant::now() - Duration::from_secs(1));

    let emit = |force: bool| {
        let mut last = last_report.lock().unwrap();
        if force || last.elapsed() >= Duration::from_millis(100) {
            *last = Instant::now();
            report(Progress {
                stage,
                files_done: files_done.load(Ordering::Relaxed),
                files_total,
                bytes_done: bytes_done.load(Ordering::Relaxed),
                bytes_total,
            });
        }
    };
    emit(true);

    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(files_total) {
            scope.spawn(|| {
                while !failed.load(Ordering::Relaxed) {
                    if CANCEL.load(Ordering::Relaxed) {
                        failed.store(true, Ordering::Relaxed);
                        error.lock().unwrap().get_or_insert(CANCELLED.to_owned());
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(d) = downloads.get(i) else { break };
                    let result = (0..3)
                        .map(|attempt| {
                            if attempt > 0 {
                                std::thread::sleep(Duration::from_millis(500 * attempt));
                            }
                            download_file(agent, d, &bytes_done)
                        })
                        .find(Result::is_ok)
                        .unwrap_or_else(|| download_file(agent, d, &bytes_done));
                    match result {
                        Ok(()) => {
                            files_done.fetch_add(1, Ordering::Relaxed);
                            emit(false);
                        }
                        Err(e) => {
                            let e = if CANCEL.load(Ordering::Relaxed) { CANCELLED.to_owned() } else { e };
                            failed.store(true, Ordering::Relaxed);
                            error.lock().unwrap().get_or_insert(e);
                        }
                    }
                }
            });
        }
    });
    emit(true);
    match error.into_inner().unwrap() {
        Some(e) if e == CANCELLED => Err(e),
        Some(e) => Err(format!("Download failed: {e}")),
        None => Ok(()),
    }
}

fn download_file(agent: &ureq::Agent, d: &Download, bytes_done: &AtomicU64) -> Result<(), String> {
    let fail = |e: &dyn std::fmt::Display| format!("{}: {e}", d.url);
    if let Some(parent) = d.path.parent() {
        fs::create_dir_all(parent).map_err(|e| fail(&e))?;
    }
    let mut part = d.path.clone().into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);

    let mut response = agent.get(&d.url).call().map_err(|e| fail(&e))?;
    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(&part).map_err(|e| fail(&e))?;
    let mut hasher = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    let mut written = 0u64;
    let result = (|| -> io::Result<()> {
        loop {
            if CANCEL.load(Ordering::Relaxed) {
                return Err(io::Error::other(CANCELLED));
            }
            let n = reader.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            io::Write::write_all(&mut file, &buf[..n])?;
            hasher.update(&buf[..n]);
            written += n as u64;
            if d.size.is_some() {
                bytes_done.fetch_add(n as u64, Ordering::Relaxed);
            }
        }
    })();
    drop(file);
    let cleanup = |msg: String| {
        let _ = fs::remove_file(&part);
        if d.size.is_some() {
            bytes_done.fetch_sub(written, Ordering::Relaxed);
        }
        msg
    };
    result.map_err(|e| cleanup(fail(&e)))?;

    if let Some(expected) = &d.sha1 {
        let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(cleanup(fail(&"checksum mismatch")));
        }
    }
    #[cfg(unix)]
    if d.executable {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&part, fs::Permissions::from_mode(0o755)).map_err(|e| fail(&e))?;
    }
    fs::rename(&part, &d.path).map_err(|e| fail(&e))
}

// ---------- Forge & NeoForge ----------

/// Forge's installer supports `--installClient` from 1.13 on.
fn forge_has_cli_installer(game: &str) -> bool {
    let mut parts = game.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    match (parts.next(), parts.next()) {
        (Some(1), Some(minor)) => minor >= 13,
        (Some(major), _) => major >= 26,
        _ => false,
    }
}

/// NeoForge `21.1.77` → `1.21.1`, `20.4.x` → `1.20.4`, `26.3.0.77` → `26.3`.
fn neoforge_game_version(version: &str) -> Option<String> {
    let version = version.split('-').next()?;
    let parts: Vec<u32> = version.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    match parts[..] {
        [0, ..] => None,
        [major, minor, patch, _] if major >= 26 => {
            Some(if patch == 0 { format!("{major}.{minor}") } else { format!("{major}.{minor}.{patch}") })
        }
        [major, minor, _] if major < 26 => {
            Some(if minor == 0 { format!("1.{major}") } else { format!("1.{major}.{minor}") })
        }
        _ => None,
    }
}

/// Dedupes an oldest-first list and returns it newest first.
fn newest_first_unique(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut list: Vec<String> = items.filter(|i| seen.insert(i.clone())).collect();
    list.reverse();
    list
}

/// Latest recommended (Forge) or stable (NeoForge) loader version for `game`.
fn forge_loader_version(agent: &ureq::Agent, loader: ForgeLoader, game: &str) -> Result<String, String> {
    let unsupported = || format!("{} isn't available for {game}", loader.name());
    match loader {
        ForgeLoader::Forge => {
            let promos = get_json(agent, FORGE_PROMOS)?;
            [format!("{game}-recommended"), format!("{game}-latest")]
                .iter()
                .find_map(|k| str_at(&promos["promos"], k).map(str::to_owned))
                .ok_or_else(unsupported)
        }
        ForgeLoader::NeoForge => {
            let list = get_json(agent, NEOFORGE_VERSIONS)?;
            let matching: Vec<&str> = list["versions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|v| neoforge_game_version(v).as_deref() == Some(game))
                .collect();
            matching
                .iter()
                .rev()
                .find(|v| !v.contains("-beta"))
                .or(matching.last())
                .map(|v| v.to_string())
                .ok_or_else(unsupported)
        }
    }
}

/// Runs the Forge/NeoForge installer for `game` and returns the new version id.
fn install_forge(agent: &ureq::Agent, mc: &Path, loader: ForgeLoader, game: &str, report: Report) -> Result<String, String> {
    let version = forge_loader_version(agent, loader, game)?;
    validate_id(&version)?;
    let (id, url) = match loader {
        ForgeLoader::Forge => (
            format!("{game}-forge-{version}"),
            format!("{FORGE_MAVEN}/{game}-{version}/forge-{game}-{version}-installer.jar"),
        ),
        ForgeLoader::NeoForge => (
            format!("neoforge-{version}"),
            format!("{NEOFORGE_MAVEN}/{version}/neoforge-{version}-installer.jar"),
        ),
    };
    if mc.join("versions").join(&id).join(format!("{id}.json")).is_file() {
        return Ok(id);
    }

    // The installer needs the vanilla game and a Java runtime first.
    install_version(agent, mc, game, report)?;
    let java = find_java(mc, &resolve_version(mc, game)?);

    // The installers refuse to run without a launcher profile file.
    let profiles = mc.join("launcher_profiles.json");
    if !profiles.is_file() {
        write_json(&profiles, &serde_json::json!({ "profiles": {} }))?;
    }

    let installers = mc.join("ember").join("installers");
    let installer = installers.join(url.rsplit('/').next().unwrap_or("installer.jar"));
    let download = Download { url, path: installer.clone(), sha1: None, size: None, executable: false };
    let pending: Vec<Download> = Some(download).into_iter().filter(needs_download).collect();
    run_downloads(agent, pending, "Downloading installer", report)?;

    report_stage(report, match loader {
        ForgeLoader::Forge => "Installing Forge (about a minute)",
        ForgeLoader::NeoForge => "Installing NeoForge (about a minute)",
    });
    let log_path = mc.join("logs").join("ember-installer.log");
    fs::create_dir_all(mc.join("logs")).map_err(|e| e.to_string())?;
    let log = File::create(&log_path).map_err(|e| e.to_string())?;
    let mut child = Command::new(&java)
        .arg("-Djava.awt.headless=true")
        .arg("-jar")
        .arg(&installer)
        .arg("--installClient")
        .arg(mc)
        .current_dir(&installers)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .spawn()
        .map_err(|e| format!("couldn't start the {} installer: {e}", loader.name()))?;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if CANCEL.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CANCELLED.into());
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    if !status.success() || !mc.join("versions").join(&id).join(format!("{id}.json")).is_file() {
        return Err(format!("The {} installer failed. Details: {}", loader.name(), log_path.display()));
    }
    Ok(id)
}

// ---------- FPS Boost ----------

#[derive(Serialize, serde::Deserialize, Default)]
struct InstanceRecord {
    version: String,
    mods: Vec<String>,
}

/// Percent-encodes a query value (game ids can contain spaces).
pub(crate) fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Newest Fabric build of a Modrinth project for `game` (releases preferred,
/// except Iris: its betas are the builds that match the newest Sodium).
fn modrinth_version(agent: &ureq::Agent, project: &str, game: &str) -> Result<Option<Value>, String> {
    let url = format!(
        "{MODRINTH_API}/project/{}/version?loaders={}&game_versions={}",
        encode(project),
        encode(r#"["fabric"]"#),
        encode(&format!(r#"["{game}"]"#)),
    );
    let versions = get_json(agent, &url)?;
    let versions = versions.as_array().cloned().unwrap_or_default();
    if project == SHADER_LOADER {
        return Ok(versions.first().cloned());
    }
    Ok(versions
        .iter()
        .find(|v| v["version_type"] == "release")
        .or(versions.first())
        .cloned())
}

/// FPS mods (plus their required dependencies) as downloads into `mods`.
fn resolve_fps_mods(agent: &ureq::Agent, game: &str, mods: &Path, shaders: bool) -> Result<Option<Vec<(String, Download)>>, String> {
    let mut queue: Vec<String> = FPS_MODS.iter().map(|m| m.to_string()).collect();
    if shaders {
        queue.insert(0, SHADER_LOADER.to_owned());
    }
    let mut seen = HashSet::new();
    // Dependencies name projects by id ("AANobbMI") while FPS_MODS uses slugs
    // ("sodium"), so also dedupe on the id each version reports.
    let mut seen_ids = HashSet::new();
    let mut found = Vec::new();
    while let Some(project) = queue.pop() {
        check_cancel()?;
        if !seen.insert(project.clone()) || seen_ids.contains(&project) {
            continue;
        }
        let Some(version) = modrinth_version(agent, &project, game)? else {
            if project == "sodium" {
                return Ok(None);
            }
            continue;
        };
        if let Some(id) = str_at(&version, "project_id") {
            if !seen_ids.insert(id.to_owned()) {
                continue;
            }
        }
        for dep in version["dependencies"].as_array().into_iter().flatten() {
            if dep["dependency_type"] == "required" {
                queue.extend(str_at(dep, "project_id").map(str::to_owned));
            }
        }
        let files = version["files"].as_array().cloned().unwrap_or_default();
        let Some(file) = files.iter().find(|f| f["primary"] == true).or(files.first()) else { continue };
        let (Some(url), Some(filename)) = (str_at(file, "url"), str_at(file, "filename")) else { continue };
        if filename.contains(['/', '\\']) || !filename.ends_with(".jar") {
            continue;
        }
        let name = match project.as_str() {
            "sodium" => "Sodium",
            "lithium" => "Lithium",
            "ferrite-core" => "FerriteCore",
            "entityculling" => "EntityCulling",
            "immediatelyfast" => "ImmediatelyFast",
            "iris" => "Iris",
            "P7dR8mSH" | "fabric-api" => "Fabric API",
            _ => filename.trim_end_matches(".jar"),
        }
        .to_owned();
        found.push((name, Download {
            url: url.to_owned(),
            path: mods.join(filename),
            sha1: str_at(&file["hashes"], "sha1").map(str::to_owned),
            size: file["size"].as_u64(),
            executable: false,
        }));
    }
    Ok(Some(found))
}

/// Sets up `ember/instances/fps-boost-<game>`: Fabric + performance mods, sharing
/// worlds and packs with the main folder. `None` if Sodium doesn't support `game`.
fn fps_boost_instance(agent: &ureq::Agent, mc: &Path, game: &str, shaders: Option<&str>, report: Report) -> Result<Option<Prepared>, String> {
    let instance = format!("fps-boost-{game}");
    let dir = instance_dir(mc, &instance)?;
    let record_path = dir.join("ember-instance.json");
    let previous: InstanceRecord = fs::read_to_string(&record_path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();

    report_stage(report, "Setting up FPS Boost");
    let mods_dir = dir.join("mods");
    let resolved = install_fabric_profile(agent, mc, game)
        .and_then(|fabric| Ok((fabric, resolve_fps_mods(agent, game, &mods_dir, shaders.is_some())?)));
    let (fabric_id, mods) = match resolved {
        Ok((_, None)) => return Ok(None),
        Ok((fabric_id, Some(mods))) => (fabric_id, mods),
        Err(e) if e == CANCELLED => return Err(e),
        // Offline: reuse the last working setup if there is one.
        Err(_) if !previous.version.is_empty() && previous.mods.iter().all(|m| mods_dir.join(m).is_file()) => {
            install_version(agent, mc, &previous.version, report)?;
            link_shared_folders(mc, &dir)?;
            return Ok(Some(Prepared {
                version: previous.version,
                instance: Some(instance),
                fps_mods: previous.mods,
                notice: None,
            }));
        }
        Err(e) => return Err(e),
    };
    install_version(agent, mc, &fabric_id, report)?;

    fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;
    let names: Vec<String> = mods.iter().map(|(name, _)| name.clone()).collect();
    let files: Vec<String> = mods
        .iter()
        .filter_map(|(_, d)| d.path.file_name()?.to_str().map(str::to_owned))
        .collect();
    let mut pending: Vec<Download> = mods.into_iter().map(|(_, d)| d).filter(needs_download).collect();
    let has_iris = names.iter().any(|n| n == "Iris");
    let mut notice = None;
    let pack = if has_iris {
        match shader_pack(agent, mc, shaders.unwrap_or(SHADER_PACK_FAST)) {
            Ok(pack) => Some(pack),
            Err(e) if e == CANCELLED => return Err(e),
            Err(e) => {
                notice = Some(format!("The shader pack couldn't be downloaded ({e})."));
                None
            }
        }
    } else {
        if shaders.is_some() {
            notice = Some(format!("Shaders (Iris) aren't available for {game} yet."));
        }
        None
    };
    if let Some((_, download)) = &pack {
        if needs_download(download) {
            pending.push(download.clone());
        }
    }
    run_downloads(agent, pending, "Downloading FPS Boost mods", report)?;
    if let Some((file, _)) = &pack {
        enable_shader_pack(&dir, file)?;
    }

    // Remove mod versions Ember installed before that are no longer current.
    for old in previous.mods.iter().filter(|m| !files.contains(m)) {
        if !old.contains(['/', '\\']) {
            let _ = fs::remove_file(mods_dir.join(old));
        }
    }
    let record = InstanceRecord { version: fabric_id.clone(), mods: files };
    write_json(&record_path, &serde_json::to_value(&record).map_err(|e| e.to_string())?)?;
    link_shared_folders(mc, &dir)?;

    Ok(Some(Prepared { version: fabric_id, instance: Some(instance), fps_mods: names, notice }))
}

/// The newest release of the default shader pack, as a download into the
/// shared `shaderpacks` folder.
fn shader_pack(agent: &ureq::Agent, mc: &Path, project: &str) -> Result<(String, Download), String> {
    check_cancel()?;
    let url = format!("{MODRINTH_API}/project/{project}/version?loaders={}", encode(r#"["iris"]"#));
    let versions = get_json(agent, &url)?;
    let versions = versions.as_array().cloned().unwrap_or_default();
    let version = versions.iter().find(|v| v["version_type"] == "release").or(versions.first()).ok_or("no shader pack release")?;
    let files = version["files"].as_array().cloned().unwrap_or_default();
    let file = files.iter().find(|f| f["primary"] == true).or(files.first()).ok_or("shader pack has no files")?;
    let (Some(url), Some(filename)) = (str_at(file, "url"), str_at(file, "filename")) else {
        return Err("shader pack file is incomplete".into());
    };
    if filename.contains(['/', '\\']) || !filename.ends_with(".zip") {
        return Err("unexpected shader pack file".into());
    }
    Ok((filename.to_owned(), Download {
        url: url.to_owned(),
        path: mc.join("shaderpacks").join(filename),
        sha1: str_at(&file["hashes"], "sha1").map(str::to_owned),
        size: file["size"].as_u64(),
        executable: false,
    }))
}

/// Turns shaders on in Iris's settings. A pack the player picked themselves
/// in the game (and still has) is kept; otherwise — or if the current pack is
/// one Ember installed — `pack` is selected, so the Fast/Fancy choice applies.
fn enable_shader_pack(instance: &Path, pack: &str) -> Result<(), String> {
    let config = instance.join("config");
    fs::create_dir_all(&config).map_err(|e| e.to_string())?;
    let path = config.join("iris.properties");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let chosen = existing
        .lines()
        .find_map(|l| l.strip_prefix("shaderPack="))
        .map(str::trim)
        .filter(|p| !p.is_empty() && instance.join("shaderpacks").join(p).exists())
        .filter(|p| !EMBER_PACK_PREFIXES.iter().any(|prefix| p.starts_with(prefix)))
        .unwrap_or(pack)
        .to_owned();
    let mut lines: Vec<String> = existing
        .lines()
        .filter(|l| !l.starts_with("enableShaders=") && !l.starts_with("shaderPack="))
        .map(str::to_owned)
        .collect();
    lines.push("enableShaders=true".into());
    lines.push(format!("shaderPack={chosen}"));
    fs::write(&path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

/// The game folder for `loader` + `game` (e.g. `fabric-1.20.1`), created and
/// linked to the shared worlds and packs on first use.
pub(crate) fn mod_instance(mc: &Path, loader: &str, game: &str) -> Result<(String, std::path::PathBuf), String> {
    if loader.is_empty() || !loader.bytes().all(|b| b.is_ascii_lowercase()) {
        return Err(format!("Unknown mod loader '{loader}'"));
    }
    validate_id(game)?;
    let name = format!("{loader}-{game}");
    let dir = instance_dir(mc, &name)?;
    fs::create_dir_all(dir.join("mods")).map_err(|e| e.to_string())?;
    link_shared_folders(mc, &dir)?;
    Ok((name, dir))
}

/// Points an instance's world/pack folders at the main game folder, and seeds
/// its settings from the main ones.
pub(crate) fn link_shared_folders(mc: &Path, dir: &Path) -> Result<(), String> {
    for name in SHARED_FOLDERS {
        let target = mc.join(name);
        let link = dir.join(name);
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        if link.symlink_metadata().is_ok() {
            continue;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).map_err(|e| format!("{}: {e}", link.display()))?;
        #[cfg(windows)]
        {
            // Junctions need no admin rights, unlike symlinks.
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let ok = Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&target)
                .creation_flags(CREATE_NO_WINDOW)
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                return Err(format!("couldn't link {}", link.display()));
            }
        }
    }
    for file in ["options.txt", "servers.dat"] {
        let (from, to) = (mc.join(file), dir.join(file));
        if from.is_file() && !to.exists() {
            fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn switches_between_embers_own_packs() {
        let dir = std::env::temp_dir().join(format!("ember-iris-switch-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("shaderpacks/ComplementaryReimagined_r5.9.3.zip")).unwrap();
        super::enable_shader_pack(&dir, "ComplementaryReimagined_r5.9.3.zip").unwrap();
        // Picking Fast later replaces Ember's Fancy pack.
        super::enable_shader_pack(&dir, "MakeUp-UltraFast-9.5g.zip").unwrap();
        let text = fs::read_to_string(dir.join("config/iris.properties")).unwrap();
        assert!(text.contains("shaderPack=MakeUp-UltraFast-9.5g.zip"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn enables_shaders_and_keeps_players_pack() {
        let dir = std::env::temp_dir().join(format!("ember-iris-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        super::enable_shader_pack(&dir, "Default.zip").unwrap();
        let text = fs::read_to_string(dir.join("config/iris.properties")).unwrap();
        assert!(text.contains("enableShaders=true") && text.contains("shaderPack=Default.zip"));

        // The player switched to a pack they still have: keep it, turn shaders back on.
        fs::create_dir_all(dir.join("shaderpacks/Mine.zip")).unwrap();
        fs::write(dir.join("config/iris.properties"), "enableShaders=false\nshaderPack=Mine.zip\nmaxShadowRenderDistance=4\n").unwrap();
        super::enable_shader_pack(&dir, "Default.zip").unwrap();
        let text = fs::read_to_string(dir.join("config/iris.properties")).unwrap();
        assert!(text.contains("enableShaders=true") && text.contains("shaderPack=Mine.zip") && text.contains("maxShadowRenderDistance=4"));
        let _ = fs::remove_dir_all(&dir);
    }

    use super::*;

    #[test]
    fn maps_neoforge_versions() {
        assert_eq!(neoforge_game_version("21.1.77").as_deref(), Some("1.21.1"));
        assert_eq!(neoforge_game_version("20.4.237").as_deref(), Some("1.20.4"));
        assert_eq!(neoforge_game_version("21.0.167").as_deref(), Some("1.21"));
        assert_eq!(neoforge_game_version("26.3.0.77").as_deref(), Some("26.3"));
        assert_eq!(neoforge_game_version("26.1.1.4-beta").as_deref(), Some("26.1.1"));
        assert_eq!(neoforge_game_version("0.25w14craftmine.3-beta"), None);
    }

    #[test]
    fn forge_cli_installer_versions() {
        assert!(forge_has_cli_installer("1.20.1"));
        assert!(forge_has_cli_installer("26.3"));
        assert!(!forge_has_cli_installer("1.12.2"));
    }

    #[test]
    fn parses_targets() {
        assert!(matches!(Target::parse("fabric:1.20.1"), Ok(Target::Fabric("1.20.1"))));
        assert!(matches!(Target::parse("neoforge:1.21.1"), Ok(Target::Forge(ForgeLoader::NeoForge, "1.21.1"))));
        assert!(Target::parse("forge:../x").is_err());
        assert!(Target::parse("bogus:1.0").is_err());
    }

    /// Starts a real download and cancels it. Run with `--ignored`.
    #[test]
    #[ignore]
    fn cancel_stops_download() {
        let worker = std::thread::spawn(|| prepare("1.17.1", false, None, &|_| {}));
        std::thread::sleep(Duration::from_secs(1));
        java_cancel_prepare();
        let started = Instant::now();
        let result = worker.join().unwrap();
        println!("result={:?} stopped_in={:?}", result.as_ref().map(|p| &p.version), started.elapsed());
        assert_eq!(result.err().as_deref(), Some(CANCELLED));
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(encode(r#"["1.14 Pre-Release 1"]"#), "%5B%221.14%20Pre-Release%201%22%5D");
    }
}
