//! Mod browser: search and install mods from Modrinth and CurseForge into the
//! game folder of one loader + game version (e.g. `ember/instances/fabric-1.20.1/mods`).
//!
//! Required dependencies are installed too. Ember remembers what it installed
//! in `ember-mods.json` next to the mods folder, so it can show titles and
//! avoid duplicates; any other jars the player drops in are listed as-is.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::minecraft_java::{minecraft_dir, str_at};
use crate::minecraft_java_install::{agent, encode, get_json, mod_instance, write_json};

const MODRINTH_API: &str = "https://api.modrinth.com/v2";
const CURSEFORGE_API: &str = "https://api.curseforge.com/v1";
/// CurseForge's API needs a key from console.curseforge.com, set at build time.
const CURSEFORGE_KEY: Option<&str> = option_env!("EMBER_CURSEFORGE_KEY");
const CF_MINECRAFT: u32 = 432;
const CF_MODS_CLASS: u32 = 6;
const PAGE_SIZE: u32 = 20;
const MAX_MOD_SIZE: u64 = 256 * 1024 * 1024;

#[derive(Serialize)]
pub struct ModHit {
    pub source: String,
    pub id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub downloads: u64,
    pub icon: Option<String>,
    pub url: String,
    pub installed: bool,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub hits: Vec<ModHit>,
    pub total: u64,
}

#[derive(Serialize)]
pub struct InstalledMod {
    pub file: String,
    pub title: String,
    pub source: Option<String>,
    pub enabled: bool,
    /// Installed only because another mod needs it.
    pub dependency: bool,
}

/// What Ember installed, kept in `ember-mods.json`.
#[derive(Serialize, Deserialize, Clone)]
struct Record {
    file: String,
    title: String,
    source: String,
    project: String,
    #[serde(default)]
    dependency: bool,
}

/// A file chosen for download.
struct ModFile {
    title: String,
    project: String,
    file: String,
    url: String,
    sha1: Option<String>,
    size: Option<u64>,
    /// (source, project id) of required dependencies.
    requires: Vec<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum Source {
    Modrinth,
    CurseForge,
}

impl Source {
    fn parse(s: &str) -> Result<Self, String> {
        match s {
            "modrinth" => Ok(Source::Modrinth),
            "curseforge" => Ok(Source::CurseForge),
            _ => Err(format!("Unknown mod source '{s}'")),
        }
    }
    fn key(self) -> &'static str {
        match self {
            Source::Modrinth => "modrinth",
            Source::CurseForge => "curseforge",
        }
    }
}

/// CurseForge's numeric mod loader ids.
fn cf_loader(loader: &str) -> Result<u32, String> {
    match loader {
        "forge" => Ok(1),
        "fabric" => Ok(4),
        "quilt" => Ok(5),
        "neoforge" => Ok(6),
        _ => Err(format!("CurseForge has no mods for '{loader}'")),
    }
}

fn cf_key() -> Result<&'static str, String> {
    CURSEFORGE_KEY.filter(|k| !k.is_empty()).ok_or_else(|| {
        "CurseForge isn't switched on in this build of Ember yet (it needs a CurseForge API key). Use Modrinth for now.".into()
    })
}

fn cf_get(agent: &ureq::Agent, path: &str) -> Result<Value, String> {
    let key = cf_key()?;
    let mut response = agent
        .get(&format!("{CURSEFORGE_API}{path}"))
        .header("x-api-key", key)
        .header("Accept", "application/json")
        .call()
        .map_err(|e| format!("CurseForge: {e}"))?;
    response.body_mut().read_json::<Value>().map_err(|e| format!("CurseForge: {e}"))
}

fn folder(loader: &str, game: &str) -> Result<PathBuf, String> {
    let mc = minecraft_dir().ok_or("could not resolve .minecraft directory")?;
    Ok(mod_instance(&mc, loader, game)?.1)
}

fn records_path(dir: &Path) -> PathBuf {
    dir.join("ember-mods.json")
}

fn load_records(dir: &Path) -> Vec<Record> {
    fs::read_to_string(records_path(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_records(dir: &Path, records: &[Record]) -> Result<(), String> {
    write_json(&records_path(dir), &serde_json::to_value(records).map_err(|e| e.to_string())?)
}

/// A jar file name that's safe to use inside the mods folder.
fn safe_file(name: &str) -> Result<&str, String> {
    let ok = !name.is_empty()
        && !name.contains(['/', '\\'])
        && !name.starts_with('.')
        && (name.ends_with(".jar") || name.ends_with(".jar.disabled"));
    if ok {
        Ok(name)
    } else {
        Err(format!("Unexpected mod file name '{name}'"))
    }
}

// ---------- search ----------

/// Whether this build can talk to CurseForge.
#[tauri::command]
pub fn mods_curseforge_available() -> bool {
    cf_key().is_ok()
}

/// Mods for `loader` + `game`, most popular first when `query` is empty.
#[tauri::command]
pub async fn mods_search(source: String, query: String, loader: String, game: String, offset: u32) -> Result<SearchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let source = Source::parse(&source)?;
        let dir = folder(&loader, &game)?;
        let installed: HashSet<String> = load_records(&dir)
            .into_iter()
            .filter(|r| r.source == source.key() && dir.join("mods").join(&r.file).exists())
            .map(|r| r.project)
            .collect();
        let agent = agent();
        let mut result = match source {
            Source::Modrinth => modrinth_search(&agent, &query, &loader, &game, offset)?,
            Source::CurseForge => curseforge_search(&agent, &query, &loader, &game, offset)?,
        };
        for hit in &mut result.hits {
            hit.installed = installed.contains(&hit.id);
        }
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn modrinth_search(agent: &ureq::Agent, query: &str, loader: &str, game: &str, offset: u32) -> Result<SearchResult, String> {
    let facets = format!(r#"[["project_type:mod"],["categories:{loader}"],["versions:{game}"]]"#);
    let index = if query.trim().is_empty() { "downloads" } else { "relevance" };
    let url = format!(
        "{MODRINTH_API}/search?query={}&facets={}&index={index}&limit={PAGE_SIZE}&offset={offset}",
        encode(query.trim()),
        encode(&facets)
    );
    let body = get_json(agent, &url)?;
    let hits = body["hits"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| ModHit {
            source: "modrinth".into(),
            id: str_at(h, "project_id").unwrap_or_default().into(),
            title: str_at(h, "title").unwrap_or_default().into(),
            author: str_at(h, "author").unwrap_or_default().into(),
            description: str_at(h, "description").unwrap_or_default().into(),
            downloads: h["downloads"].as_u64().unwrap_or(0),
            icon: str_at(h, "icon_url").filter(|u| u.starts_with("https://")).map(str::to_owned),
            url: format!("https://modrinth.com/mod/{}", str_at(h, "slug").unwrap_or_default()),
            installed: false,
        })
        .collect();
    Ok(SearchResult { hits, total: body["total_hits"].as_u64().unwrap_or(0) })
}

fn curseforge_search(agent: &ureq::Agent, query: &str, loader: &str, game: &str, offset: u32) -> Result<SearchResult, String> {
    let path = format!(
        "/mods/search?gameId={CF_MINECRAFT}&classId={CF_MODS_CLASS}&searchFilter={}&gameVersion={}&modLoaderType={}&sortField=2&sortOrder=desc&pageSize={PAGE_SIZE}&index={offset}",
        encode(query.trim()),
        encode(game),
        cf_loader(loader)?
    );
    let body = cf_get(agent, &path)?;
    let hits = body["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| ModHit {
            source: "curseforge".into(),
            id: m["id"].as_u64().unwrap_or(0).to_string(),
            title: str_at(m, "name").unwrap_or_default().into(),
            author: m["authors"][0]["name"].as_str().unwrap_or_default().into(),
            description: str_at(m, "summary").unwrap_or_default().into(),
            downloads: m["downloadCount"].as_f64().unwrap_or(0.0) as u64,
            icon: m["logo"]["thumbnailUrl"].as_str().filter(|u| u.starts_with("https://")).map(str::to_owned),
            url: m["links"]["websiteUrl"].as_str().unwrap_or("https://www.curseforge.com/minecraft/mc-mods").into(),
            installed: false,
        })
        .collect();
    Ok(SearchResult { hits, total: body["pagination"]["totalCount"].as_u64().unwrap_or(0) })
}

// ---------- install ----------

/// Installs a mod and its required dependencies; returns the titles installed.
#[tauri::command]
pub async fn mods_install(source: String, id: String, loader: String, game: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || install(Source::parse(&source)?, &id, &loader, &game))
        .await
        .map_err(|e| e.to_string())?
}

fn install(source: Source, id: &str, loader: &str, game: &str) -> Result<Vec<String>, String> {
    let dir = folder(loader, game)?;
    let mods = dir.join("mods");
    let agent = agent();
    let mut records = load_records(&dir);
    let mut queue = vec![(id.to_owned(), false)];
    let mut seen = HashSet::new();
    let mut installed = Vec::new();
    while let Some((project, dependency)) = queue.pop() {
        if !seen.insert(project.clone()) {
            continue;
        }
        let already = records.iter().any(|r| r.source == source.key() && r.project == project && mods.join(&r.file).exists());
        if already {
            continue;
        }
        let file = match source {
            Source::Modrinth => modrinth_file(&agent, &project, loader, game)?,
            Source::CurseForge => curseforge_file(&agent, &project, loader, game)?,
        };
        let Some(file) = file else {
            if dependency {
                continue; // A dependency without a build for this version: let the game report it.
            }
            return Err(format!("No version of this mod for {loader} {game}."));
        };
        // Modrinth dependencies sometimes name the slug and sometimes the id.
        seen.insert(file.project.clone());
        if records.iter().any(|r| r.source == source.key() && r.project == file.project && mods.join(&r.file).exists()) {
            continue;
        }
        let name = safe_file(&file.file)?.to_owned();
        download(&agent, &file, &mods.join(&name))?;
        records.retain(|r| !(r.source == source.key() && r.project == file.project));
        records.push(Record { file: name, title: file.title.clone(), source: source.key().into(), project: file.project.clone(), dependency });
        installed.push(file.title);
        queue.extend(file.requires.into_iter().map(|p| (p, true)));
    }
    save_records(&dir, &records)?;
    Ok(installed)
}

fn modrinth_file(agent: &ureq::Agent, project: &str, loader: &str, game: &str) -> Result<Option<ModFile>, String> {
    let url = format!(
        "{MODRINTH_API}/project/{}/version?loaders={}&game_versions={}",
        encode(project),
        encode(&format!(r#"["{loader}"]"#)),
        encode(&format!(r#"["{game}"]"#))
    );
    let versions = get_json(agent, &url)?;
    let versions = versions.as_array().cloned().unwrap_or_default();
    let Some(version) = versions.iter().find(|v| v["version_type"] == "release").or(versions.first()) else {
        return Ok(None);
    };
    let files = version["files"].as_array().cloned().unwrap_or_default();
    let Some(file) = files.iter().find(|f| f["primary"] == true).or(files.first()) else { return Ok(None) };
    let project_id = str_at(version, "project_id").unwrap_or(project).to_owned();
    let title = get_json(agent, &format!("{MODRINTH_API}/project/{}", encode(&project_id)))
        .ok()
        .and_then(|p| str_at(&p, "title").map(str::to_owned))
        .unwrap_or_else(|| str_at(file, "filename").unwrap_or(project).to_owned());
    let requires = version["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["dependency_type"] == "required")
        .filter_map(|d| str_at(d, "project_id").map(str::to_owned))
        .collect();
    Ok(Some(ModFile {
        title,
        project: project_id,
        file: str_at(file, "filename").unwrap_or_default().into(),
        url: str_at(file, "url").unwrap_or_default().into(),
        sha1: str_at(&file["hashes"], "sha1").map(str::to_owned),
        size: file["size"].as_u64(),
        requires,
    }))
}

fn curseforge_file(agent: &ureq::Agent, project: &str, loader: &str, game: &str) -> Result<Option<ModFile>, String> {
    if !project.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Unexpected CurseForge project id".into());
    }
    let info = cf_get(agent, &format!("/mods/{project}"))?;
    let title = info["data"]["name"].as_str().unwrap_or(project).to_owned();
    let page = info["data"]["links"]["websiteUrl"].as_str().unwrap_or("https://www.curseforge.com").to_owned();
    let files = cf_get(
        agent,
        &format!("/mods/{project}/files?gameVersion={}&modLoaderType={}&pageSize=50", encode(game), cf_loader(loader)?),
    )?;
    let files = files["data"].as_array().cloned().unwrap_or_default();
    // Newest first; prefer releases (releaseType 1) over betas and alphas.
    let mut sorted: Vec<&Value> = files.iter().collect();
    sorted.sort_by(|a, b| str_at(b, "fileDate").cmp(&str_at(a, "fileDate")));
    let Some(file) = sorted.iter().find(|f| f["releaseType"] == 1).or(sorted.first()).copied() else {
        return Ok(None);
    };
    let Some(url) = str_at(file, "downloadUrl") else {
        return Err(format!("“{title}” can only be downloaded from the CurseForge website: {page}"));
    };
    let sha1 = file["hashes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| h["algo"] == 1)
        .and_then(|h| str_at(h, "value"))
        .map(str::to_owned);
    let requires = file["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["relationType"] == 3)
        .filter_map(|d| d["modId"].as_u64().map(|id| id.to_string()))
        .collect();
    Ok(Some(ModFile {
        title,
        project: project.to_owned(),
        file: str_at(file, "fileName").unwrap_or_default().into(),
        url: url.into(),
        sha1,
        size: file["fileLength"].as_u64(),
        requires,
    }))
}

fn download(agent: &ureq::Agent, file: &ModFile, path: &Path) -> Result<(), String> {
    if !file.url.starts_with("https://") {
        return Err("Unexpected download address".into());
    }
    let mut response = agent.get(&file.url).call().map_err(|e| format!("Downloading {}: {e}", file.title))?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .with_config()
        .limit(MAX_MOD_SIZE)
        .reader()
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Downloading {}: {e}", file.title))?;
    if file.size.is_some_and(|s| s != bytes.len() as u64) {
        return Err(format!("{} downloaded incompletely. Try again.", file.title));
    }
    if let Some(expected) = &file.sha1 {
        let actual: String = Sha1::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
        if &actual != expected {
            return Err(format!("{} was corrupted while downloading. Try again.", file.title));
        }
    }
    let part = path.with_extension("part");
    fs::write(&part, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&part, path).map_err(|e| e.to_string())
}

// ---------- installed mods ----------

/// Every jar in the mods folder (disabled ones end in `.jar.disabled`).
#[tauri::command]
pub fn mods_list(loader: String, game: String) -> Result<Vec<InstalledMod>, String> {
    let dir = folder(&loader, &game)?;
    let records = load_records(&dir);
    let mut list: Vec<InstalledMod> = fs::read_dir(dir.join("mods"))
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| safe_file(name).is_ok())
        .map(|file| {
            let base = file.trim_end_matches(".disabled");
            let record = records.iter().find(|r| r.file == base);
            InstalledMod {
                title: record.map_or_else(|| base.trim_end_matches(".jar").to_owned(), |r| r.title.clone()),
                source: record.map(|r| r.source.clone()),
                enabled: !file.ends_with(".disabled"),
                dependency: record.is_some_and(|r| r.dependency),
                file,
            }
        })
        .collect();
    list.sort_by_key(|m| m.title.to_lowercase());
    Ok(list)
}

/// Turns a mod on or off without deleting it.
#[tauri::command]
pub fn mods_set_enabled(loader: String, game: String, file: String, enabled: bool) -> Result<(), String> {
    let mods = folder(&loader, &game)?.join("mods");
    let file = safe_file(&file)?;
    let base = file.trim_end_matches(".disabled");
    let (from, to) = if enabled {
        (format!("{base}.disabled"), base.to_owned())
    } else {
        (base.to_owned(), format!("{base}.disabled"))
    };
    if mods.join(&to).exists() {
        return Ok(());
    }
    fs::rename(mods.join(from), mods.join(to)).map_err(|e| e.to_string())
}

/// Deletes one mod file.
#[tauri::command]
pub fn mods_remove(loader: String, game: String, file: String) -> Result<(), String> {
    let dir = folder(&loader, &game)?;
    let file = safe_file(&file)?;
    fs::remove_file(dir.join("mods").join(file)).map_err(|e| e.to_string())?;
    let base = file.trim_end_matches(".disabled");
    let mut records = load_records(&dir);
    records.retain(|r| r.file != base);
    save_records(&dir, &records)
}

/// Shows the mods folder in Finder / Explorer.
#[tauri::command]
pub fn mods_open_folder(loader: String, game: String) -> Result<(), String> {
    let mods = folder(&loader, &game)?.join("mods");
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&mods).status();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(&mods).status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(&mods).status();
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Opens a mod's page on Modrinth or CurseForge.
#[tauri::command]
pub fn mods_open_page(url: String) -> Result<(), String> {
    let allowed = ["https://modrinth.com/", "https://www.curseforge.com/", "https://curseforge.com/"];
    if !allowed.iter().any(|p| url.starts_with(p)) {
        return Err("Unexpected link".into());
    }
    crate::microsoft_auth::open_url(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_file_names() {
        assert!(safe_file("sodium-0.5.jar").is_ok());
        assert!(safe_file("sodium-0.5.jar.disabled").is_ok());
        assert!(safe_file("../evil.jar").is_err());
        assert!(safe_file("a\\b.jar").is_err());
        assert!(safe_file("notes.txt").is_err());
        assert!(safe_file(".hidden.jar").is_err());
    }

    /// Real Modrinth search and install (with dependencies) into a temp home.
    /// Run with HOME set to a scratch folder.
    #[test]
    #[ignore]
    fn installs_from_modrinth() {
        let found = modrinth_search(&agent(), "sodium extra", "fabric", "1.20.1", 0).unwrap();
        assert!(found.hits.iter().any(|h| h.title == "Sodium Extra"));
        let id = found.hits.iter().find(|h| h.title == "Sodium Extra").unwrap().id.clone();
        let titles = install(Source::Modrinth, &id, "fabric", "1.20.1").unwrap();
        println!("installed {titles:?}");
        assert!(titles.contains(&"Sodium Extra".to_string()));
        assert!(titles.contains(&"Sodium".to_string()), "required dependency missing");
        let list = mods_list("fabric".into(), "1.20.1".into()).unwrap();
        assert!(list.iter().any(|m| m.title == "Sodium" && m.dependency));
        // Installing again is a no-op.
        assert!(install(Source::Modrinth, &id, "fabric", "1.20.1").unwrap().is_empty());
    }
}
