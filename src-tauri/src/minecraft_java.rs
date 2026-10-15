//! Minecraft: Java Edition support (all platforms).
//!
//! Launches versions installed in `.minecraft` (by Ember's installer in
//! `minecraft_java_install.rs` or by the official launcher), in offline mode. Handles `inheritsFrom` (Fabric, Quilt,
//! Forge), OS/arch rules, legacy natives extraction and Mojang's bundled
//! Java runtimes.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::{AppHandle, Emitter};

#[cfg(target_os = "windows")]
const OS_NAME: &str = "windows";
#[cfg(target_os = "macos")]
const OS_NAME: &str = "osx";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const OS_NAME: &str = "linux";

#[cfg(target_os = "windows")]
const CP_SEP: &str = ";";
#[cfg(not(target_os = "windows"))]
const CP_SEP: &str = ":";

const LAUNCHER_NAME: &str = "mc-launcher";
const LAUNCHER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
pub struct VersionEntry {
    pub id: String,
    /// `release`, `snapshot`, `old_beta`, `old_alpha` or `modded`.
    pub kind: String,
    /// Mod loader name for modded versions (`Fabric`, `Forge`, …).
    pub loader: Option<String>,
    /// Vanilla version a modded version is built on.
    pub base: Option<String>,
    #[serde(skip)]
    release_time: String,
}

#[derive(Serialize)]
pub struct LaunchInfo {
    pub pid: u32,
    pub java: String,
    pub log: String,
}

/// Default `.minecraft` directory for the current OS.
pub fn minecraft_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join(".minecraft"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join("Library/Application Support/minecraft"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".minecraft"))
    }
}

/// Returns the game directory path, if it exists.
#[tauri::command]
pub fn java_game_dir() -> Option<String> {
    minecraft_dir()
        .filter(|p| p.is_dir())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Lists installed versions (folders in `.minecraft/versions` with a version
/// JSON), newest first.
#[tauri::command]
pub fn java_list_versions() -> Result<Vec<VersionEntry>, String> {
    let mc = minecraft_dir().ok_or("could not resolve .minecraft directory")?;
    let dir = mc.join("versions");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut versions: Vec<VersionEntry> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|id| {
            let json = read_version_json(&mc, &id).ok()?;
            let base = str_at(&json, "inheritsFrom").map(str::to_owned);
            let kind = if base.is_some() {
                "modded".to_owned()
            } else {
                str_at(&json, "type").unwrap_or("release").to_owned()
            };
            Some(VersionEntry {
                loader: base.as_ref().map(|_| loader_name(&id)),
                base,
                kind,
                release_time: str_at(&json, "releaseTime").unwrap_or_default().to_owned(),
                id,
            })
        })
        .collect();
    versions.sort_by(|a, b| b.release_time.cmp(&a.release_time));
    Ok(versions)
}

pub(crate) fn loader_name(id: &str) -> String {
    let id = id.to_ascii_lowercase();
    let known = [
        ("neoforge", "NeoForge"),
        ("forge", "Forge"),
        ("fabric", "Fabric"),
        ("quilt", "Quilt"),
        ("optifine", "OptiFine"),
    ];
    known
        .iter()
        .find(|(key, _)| id.contains(key))
        .map_or("Modded", |(_, name)| name)
        .to_owned()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchOptions {
    pub version: String,
    /// Offline-mode name; ignored when `online` is set.
    pub username: String,
    pub max_memory_mb: u32,
    /// Use the signed-in Microsoft account.
    pub online: bool,
    /// Use the signed-in Ely.by account.
    #[serde(default)]
    pub ely: bool,
    /// Tuned JVM flags (the mods are handled by `java_prepare`).
    pub fps_boost: bool,
    /// Ember-managed game folder (`ember/instances/<name>`), e.g. for FPS Boost.
    pub instance: Option<String>,
}

/// Who the game runs as.
struct Identity {
    name: String,
    uuid: String,
    access_token: String,
    xuid: String,
    user_type: &'static str,
}

/// The official launcher's default JVM flags.
const DEFAULT_JVM_FLAGS: &[&str] = &[
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+UseG1GC",
    "-XX:G1NewSizePercent=20",
    "-XX:G1ReservePercent=20",
    "-XX:MaxGCPauseMillis=50",
    "-XX:G1HeapRegionSize=32M",
];

/// G1 tuned for short pauses (fewer frame-time spikes) on a client.
const FPS_BOOST_JVM_FLAGS: &[&str] = &[
    "-XX:+UnlockExperimentalVMOptions",
    "-XX:+UseG1GC",
    "-XX:+ParallelRefProcEnabled",
    "-XX:MaxGCPauseMillis=37",
    "-XX:+DisableExplicitGC",
    "-XX:G1NewSizePercent=23",
    "-XX:G1MaxNewSizePercent=50",
    "-XX:G1HeapRegionSize=16M",
    "-XX:G1ReservePercent=20",
    "-XX:G1MixedGCCountTarget=3",
    "-XX:InitiatingHeapOccupancyPercent=20",
    "-XX:G1MixedGCLiveThresholdPercent=90",
    "-XX:SurvivorRatio=32",
    "-XX:MaxTenuringThreshold=1",
    "-XX:+PerfDisableSharedMem",
];

/// Launches an installed version.
#[tauri::command]
pub async fn java_launch(app: AppHandle, options: LaunchOptions) -> Result<LaunchInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        launch(&options, move |pid| {
            let _ = app.emit("java-exited", pid);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Total physical memory, so the UI can offer sensible memory limits.
#[tauri::command]
pub fn system_memory_mb() -> u64 {
    total_memory_bytes().map_or(8192, |b| b / (1024 * 1024))
}

#[cfg(target_os = "macos")]
fn total_memory_bytes() -> Option<u64> {
    let mut size: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: hw.memsize is a u64 and `len` matches its size.
    let rc = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut size as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then_some(size)
}

#[cfg(target_os = "windows")]
fn total_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // SAFETY: MEMORYSTATUSEX is plain data; dwLength must be set before the call.
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        (GlobalMemoryStatusEx(&mut status) != 0).then_some(status.ullTotalPhys)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn total_memory_bytes() -> Option<u64> {
    let info = fs::read_to_string("/proc/meminfo").ok()?;
    let kb: u64 = info
        .lines()
        .find(|l| l.starts_with("MemTotal:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some(kb * 1024)
}

fn identity(options: &LaunchOptions) -> Result<Identity, String> {
    if options.ely {
        let account = crate::ely_auth::session()?;
        return Ok(Identity {
            name: account.name,
            uuid: account.uuid,
            access_token: account.access_token,
            xuid: "0".into(),
            user_type: "mojang",
        });
    }
    if options.online {
        let account = crate::microsoft_auth::session()?;
        return Ok(Identity {
            name: account.name,
            uuid: account.uuid,
            access_token: account.mc_token,
            xuid: account.xuid,
            user_type: "msa",
        });
    }
    let name = options.username.trim();
    let valid_name = (3..=16).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid_name {
        return Err("Username must be 3–16 characters: letters, digits or _".into());
    }
    Ok(Identity {
        name: name.to_owned(),
        uuid: offline_uuid(name),
        access_token: "0".into(),
        xuid: "0".into(),
        user_type: "legacy",
    })
}

/// Game folder for an Ember-managed instance.
pub(crate) fn instance_dir(mc: &Path, name: &str) -> Result<PathBuf, String> {
    validate_id(name)?;
    Ok(mc.join("ember").join("instances").join(name))
}

fn launch(options: &LaunchOptions, on_exit: impl FnOnce(u32) + Send + 'static) -> Result<LaunchInfo, String> {
    let version = options.version.as_str();
    let who = identity(options)?;
    let mc = minecraft_dir().ok_or("could not resolve .minecraft directory")?;
    let json = resolve_version(&mc, version)?;
    let game_dir = match &options.instance {
        Some(name) => instance_dir(&mc, name)?,
        None => mc.clone(),
    };
    fs::create_dir_all(&game_dir).map_err(|e| e.to_string())?;
    let versions_dir = mc.join("versions");
    let libs_dir = mc.join("libraries");

    // Client jar. Inherited versions run on the parent's jar, copied under
    // their own name like the official launcher does (Forge's bootstrap
    // ignores `<version>.jar` but would clash with the vanilla name).
    let jar_id = str_at(&json, "jar").unwrap_or(version);
    let parent_jar = versions_dir.join(jar_id).join(format!("{jar_id}.jar"));
    if !parent_jar.is_file() {
        return Err(format!(
            "Missing client jar for '{jar_id}'. Press Play again to repair the install."
        ));
    }
    let client_jar = versions_dir.join(version).join(format!("{version}.jar"));
    if client_jar != parent_jar {
        let stale = fs::metadata(&client_jar).map(|m| m.len()).ok() != fs::metadata(&parent_jar).map(|m| m.len()).ok();
        if stale {
            fs::copy(&parent_jar, &client_jar).map_err(|e| format!("copying client jar: {e}"))?;
        }
    }

    // Libraries and natives.
    let natives_dir = versions_dir.join(version).join("natives");
    let _ = fs::remove_dir_all(&natives_dir);
    fs::create_dir_all(&natives_dir).map_err(|e| e.to_string())?;

    let mut classpath = Vec::new();
    let mut missing = Vec::new();
    let mut seen = HashSet::new();
    for lib in json["libraries"].as_array().into_iter().flatten() {
        if !rules_allow(lib.get("rules")) {
            continue;
        }
        let Some(name) = str_at(lib, "name") else { continue };

        if let Some(classifier) = native_classifier(lib) {
            let path = lib["downloads"]["classifiers"][&classifier]["path"]
                .as_str()
                .map(PathBuf::from)
                .unwrap_or_else(|| maven_path(name, Some(&classifier)));
            let path = libs_dir.join(path);
            if !path.is_file() {
                missing.push(path);
                continue;
            }
            let excludes: Vec<&str> = lib["extract"]["exclude"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            extract_natives(&path, &natives_dir, &excludes)
                .map_err(|e| format!("extracting {}: {e}", path.display()))?;
        }

        // Legacy native-only entries have no main artifact.
        let has_artifact = lib["downloads"]["artifact"].is_object();
        if lib.get("natives").is_some() && !has_artifact {
            continue;
        }

        // Child versions list their libraries first, so the first entry wins
        // (e.g. Fabric's newer ASM over the vanilla one).
        let mut parts = name.split(':');
        let key = match (parts.next(), parts.next(), parts.nth(1)) {
            (Some(g), Some(a), classifier) => format!("{g}:{a}:{}", classifier.unwrap_or("")),
            _ => name.to_owned(),
        };
        if !seen.insert(key) {
            continue;
        }

        let path = lib["downloads"]["artifact"]["path"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| maven_path(name, None));
        let path = libs_dir.join(path);
        if path.is_file() {
            classpath.push(path);
        } else {
            missing.push(path);
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "{} libraries are missing (first: {}). Press Play again to repair the install.",
            missing.len(),
            missing[0].display()
        ));
    }
    classpath.push(client_jar);
    let classpath = classpath
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join(CP_SEP);

    // Placeholder values.
    let assets_root = mc.join("assets");
    let assets_index = json["assetIndex"]["id"]
        .as_str()
        .or_else(|| str_at(&json, "assets"))
        .unwrap_or("legacy")
        .to_owned();
    let p = |path: &Path| path.to_string_lossy().into_owned();
    let vars: HashMap<&str, String> = HashMap::from([
        ("auth_player_name", who.name.clone()),
        ("auth_uuid", who.uuid.clone()),
        ("auth_access_token", who.access_token.clone()),
        ("auth_session", who.access_token.clone()),
        ("auth_xuid", who.xuid.clone()),
        ("clientid", "0".into()),
        ("user_type", who.user_type.into()),
        ("user_properties", "{}".into()),
        ("version_name", version.to_owned()),
        ("version_type", str_at(&json, "type").unwrap_or("release").to_owned()),
        ("game_directory", p(&game_dir)),
        ("assets_root", p(&assets_root)),
        ("game_assets", p(&assets_root.join("virtual").join(&assets_index))),
        ("assets_index_name", assets_index),
        ("natives_directory", p(&natives_dir)),
        ("library_directory", p(&libs_dir)),
        ("classpath_separator", CP_SEP.into()),
        ("classpath", classpath),
        ("launcher_name", LAUNCHER_NAME.into()),
        ("launcher_version", LAUNCHER_VERSION.into()),
    ]);

    // JVM and game arguments: modern `arguments` object or legacy string.
    let flags = if options.fps_boost { FPS_BOOST_JVM_FLAGS } else { DEFAULT_JVM_FLAGS };
    let mut jvm_args = vec![format!("-Xmx{}M", options.max_memory_mb)];
    // Ely.by: authlib-injector points skins and server joins at Ely.by.
    if options.ely {
        let injector = crate::ely_auth::authlib_injector(&mc)?;
        jvm_args.insert(0, format!("-javaagent:{}=ely.by", injector.display()));
    } else if !options.online {
        // Offline with a custom skin: serve it from Ember's local skin server.
        if let Some(agent) = crate::skins::offline_skin_agent(&mc, &who.name, &who.uuid) {
            jvm_args.insert(0, agent);
        }
    }
    jvm_args.extend(flags.iter().map(|f| f.to_string()));
    let mut game_args = Vec::new();
    if json["arguments"].is_object() {
        collect_args(&json["arguments"]["jvm"], &mut jvm_args);
        collect_args(&json["arguments"]["game"], &mut game_args);
    }
    if let Some(legacy) = str_at(&json, "minecraftArguments") {
        game_args.extend(legacy.split_whitespace().map(str::to_owned));
    }
    if json["arguments"]["jvm"].as_array().map_or(true, Vec::is_empty) {
        jvm_args.extend(
            ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"].map(String::from),
        );
    }
    let main_class = str_at(&json, "mainClass").ok_or("version JSON has no mainClass")?;

    let java = find_java(&mc, &json);
    let log_path = mc.join("logs").join("mc-launcher-output.log");
    fs::create_dir_all(mc.join("logs")).map_err(|e| e.to_string())?;
    let mut log = File::create(&log_path).map_err(|e| e.to_string())?;

    let jvm_args: Vec<String> = jvm_args.iter().map(|a| substitute(a, &vars)).collect();
    let game_args: Vec<String> = game_args.iter().map(|a| substitute(a, &vars)).collect();
    let command_line = format!(
        "{} {} {main_class} {}",
        java.display(),
        jvm_args.join(" "),
        game_args.join(" ")
    );
    // Never write a real access token to disk.
    let command_line = match who.access_token.as_str() {
        "0" => command_line,
        token => command_line.replace(token, "<redacted>"),
    };
    let _ = writeln!(log, "[Ember] {command_line}\n");

    let mut child = Command::new(&java)
        .args(&jvm_args)
        .arg(main_class)
        .args(&game_args)
        .current_dir(&game_dir)
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .spawn()
        .map_err(|e| format!("failed to start {}: {e}", java.display()))?;

    let pid = child.id();
    std::thread::spawn(move || {
        let _ = child.wait();
        on_exit(pid);
    });

    Ok(LaunchInfo {
        pid,
        java: java.to_string_lossy().into_owned(),
        log: p(&log_path),
    })
}

pub(crate) fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

pub(crate) fn read_version_json(mc: &Path, id: &str) -> Result<Value, String> {
    validate_id(id)?;
    let path = mc.join("versions").join(id).join(format!("{id}.json"));
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Loads a version JSON merged over its `inheritsFrom` parent chain, with
/// compatibility fixes applied.
pub(crate) fn resolve_version(mc: &Path, id: &str) -> Result<Value, String> {
    let mut json = resolve_chain(mc, id)?;
    apply_library_overrides(&mut json);
    Ok(json)
}

/// JNA 5.x releases before 5.19 abort the game on recent macOS: looking up
/// IOKit produces a dlopen error longer than JNA's message buffer, which trips
/// an assertion. Swap them for a fixed release from Maven Central.
fn apply_library_overrides(json: &mut Value) {
    const JNA_FIXED: (u32, u32, u32) = (5, 19, 1);
    if OS_NAME != "osx" {
        return;
    }
    let parse = |v: &str| -> Option<(u32, u32, u32)> {
        let mut it = v.split('.').map(|n| n.parse().ok());
        Some((it.next()??, it.next()??, it.next().flatten().unwrap_or(0)))
    };
    for lib in json["libraries"].as_array_mut().into_iter().flatten() {
        let Some(name) = str_at(lib, "name") else { continue };
        let parts: Vec<&str> = name.split(':').collect();
        let [group @ "net.java.dev.jna", artifact @ ("jna" | "jna-platform"), version] = parts[..] else {
            continue;
        };
        if !matches!(parse(version), Some(v) if v.0 == 5 && v < JNA_FIXED) {
            continue;
        }
        let (a, b, c) = JNA_FIXED;
        let name = format!("{group}:{artifact}:{a}.{b}.{c}");
        let rel = maven_rel(&name, None);
        *lib = serde_json::json!({
            "name": name,
            "downloads": { "artifact": {
                "path": rel,
                "url": format!("https://repo1.maven.org/maven2/{rel}"),
            }},
        });
    }
}

/// A version id that is safe to use as a folder name.
pub(crate) fn validate_id(id: &str) -> Result<(), String> {
    let ok = !id.is_empty()
        && id != "."
        && id != ".."
        && !id.contains(['/', '\\', ':'])
        && !id.contains("..");
    if ok { Ok(()) } else { Err(format!("Invalid version id '{id}'")) }
}

fn resolve_chain(mc: &Path, id: &str) -> Result<Value, String> {
    validate_id(id)?;
    let child = read_version_json(mc, id)?;
    let Some(parent_id) = str_at(&child, "inheritsFrom") else {
        let mut json = child;
        if json.get("jar").is_none() {
            json["jar"] = Value::from(id);
        }
        return Ok(json);
    };

    let mut merged = resolve_chain(mc, parent_id)?;
    for (key, value) in child.as_object().into_iter().flatten() {
        match key.as_str() {
            "inheritsFrom" => {}
            "libraries" => {
                let mut libs = value.as_array().cloned().unwrap_or_default();
                libs.extend(merged["libraries"].as_array().cloned().unwrap_or_default());
                merged["libraries"] = Value::from(libs);
            }
            "arguments" => {
                for kind in ["game", "jvm"] {
                    let mut args = merged["arguments"][kind].as_array().cloned().unwrap_or_default();
                    args.extend(value[kind].as_array().cloned().unwrap_or_default());
                    merged["arguments"][kind] = Value::from(args);
                }
            }
            _ => merged[key] = value.clone(),
        }
    }
    Ok(merged)
}

/// Evaluates Mojang's `rules` array. No optional features are enabled.
pub(crate) fn rules_allow(rules: Option<&Value>) -> bool {
    let Some(rules) = rules.and_then(Value::as_array) else { return true };
    let mut allowed = false;
    for rule in rules {
        let os = &rule["os"];
        let os_ok = str_at(os, "name").map_or(true, |n| n == OS_NAME)
            && str_at(os, "arch").map_or(true, |a| a == "x86" && cfg!(target_arch = "x86"));
        let features_ok = rule["features"]
            .as_object()
            .map_or(true, |f| f.values().all(|v| v.as_bool() == Some(false)));
        if os_ok && features_ok {
            allowed = str_at(rule, "action") == Some("allow");
        }
    }
    allowed
}

fn collect_args(list: &Value, out: &mut Vec<String>) {
    for arg in list.as_array().into_iter().flatten() {
        match arg {
            Value::String(s) => out.push(s.clone()),
            Value::Object(_) if rules_allow(arg.get("rules")) => match &arg["value"] {
                Value::String(s) => out.push(s.clone()),
                Value::Array(values) => {
                    out.extend(values.iter().filter_map(Value::as_str).map(str::to_owned))
                }
                _ => {}
            },
            _ => {}
        }
    }
}

/// Replaces `${name}` placeholders; unknown ones are left untouched.
fn substitute(arg: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        match rest[start..].find('}') {
            Some(len) => {
                let key = &rest[start + 2..start + len];
                match vars.get(key) {
                    Some(value) => out.push_str(value),
                    None => out.push_str(&rest[start..=start + len]),
                }
                rest = &rest[start + len + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// `group:artifact:version[:classifier]` → relative Maven path (`/`-separated,
/// usable both as a URL suffix and a file path).
pub(crate) fn maven_rel(name: &str, classifier: Option<&str>) -> String {
    let mut parts = name.split(':');
    let group = parts.next().unwrap_or_default().replace('.', "/");
    let artifact = parts.next().unwrap_or_default();
    let version = parts.next().unwrap_or_default();
    match classifier.or(parts.next()) {
        Some(c) => format!("{group}/{artifact}/{version}/{artifact}-{version}-{c}.jar"),
        None => format!("{group}/{artifact}/{version}/{artifact}-{version}.jar"),
    }
}

fn maven_path(name: &str, classifier: Option<&str>) -> PathBuf {
    PathBuf::from(maven_rel(name, classifier))
}

/// Legacy (pre-1.19) native classifier for this OS, e.g. `natives-osx`.
pub(crate) fn native_classifier(lib: &Value) -> Option<String> {
    let arch = if cfg!(target_pointer_width = "64") { "64" } else { "32" };
    lib["natives"][OS_NAME]
        .as_str()
        .map(|c| c.replace("${arch}", arch))
}

fn extract_natives(jar: &Path, dest: &Path, excludes: &[&str]) -> io::Result<()> {
    let mut archive = zip::ZipArchive::new(File::open(jar)?).map_err(io::Error::other)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(io::Error::other)?;
        if entry.is_dir() || excludes.iter().any(|ex| entry.name().starts_with(ex)) {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else { continue };
        let out_path = dest.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entry, &mut File::create(out_path)?)?;
    }
    Ok(())
}

/// Same UUID the vanilla server assigns offline players:
/// `UUID.nameUUIDFromBytes("OfflinePlayer:<name>")`.
fn offline_uuid(name: &str) -> String {
    use md5::{Digest, Md5};
    let mut hash: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}")).into();
    hash[6] = (hash[6] & 0x0f) | 0x30;
    hash[8] = (hash[8] & 0x3f) | 0x80;
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(target_os = "windows")]
pub(crate) const JAVA_EXE: &str = "javaw.exe";
#[cfg(not(target_os = "windows"))]
pub(crate) const JAVA_EXE: &str = "java";

/// Mojang runtime platform keys to try, best match first (Apple Silicon and
/// Windows on ARM fall back to x64 builds for runtimes without native ones).
pub(crate) fn runtime_platforms() -> &'static [&'static str] {
    match (OS_NAME, std::env::consts::ARCH) {
        ("osx", "aarch64") => &["mac-os-arm64", "mac-os"],
        ("osx", _) => &["mac-os"],
        ("windows", "aarch64") => &["windows-arm64", "windows-x64"],
        ("windows", "x86") => &["windows-x86"],
        ("windows", _) => &["windows-x64"],
        (_, "x86") => &["linux-i386"],
        _ => &["linux"],
    }
}

/// Where a Mojang runtime component lives inside `.minecraft/runtime`.
pub(crate) fn runtime_home(mc: &Path, component: &str, platform: &str) -> PathBuf {
    mc.join("runtime").join(component).join(platform).join(component)
}

pub(crate) fn runtime_java(home: &Path) -> PathBuf {
    if OS_NAME == "osx" {
        home.join("jre.bundle/Contents/Home/bin").join(JAVA_EXE)
    } else {
        home.join("bin").join(JAVA_EXE)
    }
}

pub(crate) fn java_component(json: &Value) -> &str {
    json["javaVersion"]["component"].as_str().unwrap_or("jre-legacy")
}

/// Prefers the Java runtime Mojang's launcher (or Ember) installed for this
/// version, then `JAVA_HOME`, then `java` on PATH.
pub(crate) fn find_java(mc: &Path, json: &Value) -> PathBuf {
    let component = java_component(json);
    for platform in runtime_platforms() {
        let java = runtime_java(&runtime_home(mc, component, platform));
        if java.is_file() {
            return java;
        }
    }
    if let Some(java_home) = std::env::var_os("JAVA_HOME") {
        let path = PathBuf::from(java_home).join("bin").join(JAVA_EXE);
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from(JAVA_EXE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offline_uuid_matches_java() {
        // UUID.nameUUIDFromBytes("OfflinePlayer:Notch".getBytes(UTF_8))
        assert_eq!(offline_uuid("Notch"), "b50ad385829d3141a2167e7d7539ba7f");
    }

    #[test]
    fn maven_paths() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.19.5", None),
            PathBuf::from("net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar")
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-macos", None),
            PathBuf::from("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-macos.jar")
        );
    }

    #[test]
    fn rejects_unsafe_ids() {
        assert!(validate_id("1.20.1").is_ok());
        assert!(validate_id("fabric-loader-0.19.5-26.3").is_ok());
        for bad in ["", "..", "../x", "a/b", "a\\b", "C:x"] {
            assert!(validate_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn substitutes_known_placeholders_only() {
        let vars = HashMap::from([("a", "1".to_owned())]);
        assert_eq!(substitute("x=${a},y=${b}", &vars), "x=1,y=${b}");
    }
}

#[cfg(test)]
mod smoke {
    /// Launches a real installed version. Run with:
    /// MC_SMOKE_VERSION=<id> cargo test smoke -- --ignored --nocapture
    #[test]
    #[ignore]
    fn install_version() {
        let target = std::env::var("MC_SMOKE_VERSION").expect("set MC_SMOKE_VERSION");
        let fps = std::env::var("MC_SMOKE_FPS").is_ok();
        let shaders = std::env::var("MC_SMOKE_SHADERS").is_ok();
        let prepared = crate::minecraft_java_install::prepare(&target, fps, shaders, &|p| {
            if p.files_done == p.files_total || p.files_done % 500 == 0 {
                println!("{} {}/{} files, {}/{} MB", p.stage, p.files_done, p.files_total, p.bytes_done >> 20, p.bytes_total >> 20);
            }
        })
        .expect("install failed");
        println!(
            "installed {} instance={:?} mods={:?} notice={:?}",
            prepared.version, prepared.instance, prepared.fps_mods, prepared.notice
        );
    }

    #[test]
    #[ignore]
    fn launch_installed_version() {
        let version = std::env::var("MC_SMOKE_VERSION").expect("set MC_SMOKE_VERSION");
        let options = super::LaunchOptions {
            version,
            username: "Tester".into(),
            max_memory_mb: 2048,
            online: false,
            ely: false,
            fps_boost: std::env::var("MC_SMOKE_FPS").is_ok(),
            instance: std::env::var("MC_SMOKE_INSTANCE").ok(),
        };
        // MC_SMOKE_SKIN=<png>: give "Tester" an offline skin first.
        if let Ok(path) = std::env::var("MC_SMOKE_SKIN") {
            use base64::Engine;
            let png = base64::engine::general_purpose::STANDARD.encode(std::fs::read(path).unwrap());
            crate::skins::skin_set_offline("Tester".into(), png, false).expect("saving skin failed");
        }
        let info = super::launch(&options, |_| {}).expect("launch failed");
        println!("pid={} java={} log={}", info.pid, info.java, info.log);
        // MC_SMOKE_WAIT=<seconds>: keep Ember's skin server alive while the game runs.
        if let Some(secs) = std::env::var("MC_SMOKE_WAIT").ok().and_then(|s| s.parse().ok()) {
            std::thread::sleep(std::time::Duration::from_secs(secs));
        }
    }
}
