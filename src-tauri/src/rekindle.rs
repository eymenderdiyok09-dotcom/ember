//! Rekindle: Ember's self-updater.
//!
//! Each GitHub release carries `latest.json` (version, notes, and a download
//! with SHA-256 per platform) plus `latest.json.sig`, an Ed25519 signature made
//! with the release key. Ember only installs an update whose manifest verifies
//! against the public key below and whose download matches the manifest hash,
//! so nobody but the key holder can ship an update.

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const MANIFEST_URL: &str = "https://github.com/eymenderdiyok09-dotcom/ember/releases/latest/download/latest.json";
/// Downloads must come from this repository's releases.
const DOWNLOAD_PREFIX: &str = "https://github.com/eymenderdiyok09-dotcom/ember/releases/download/";
/// Ed25519 public key (base64) matching the private release key.
const PUBLIC_KEY: &str = "dwwIHuSc86rVyI/u5jPqoZ5o0TUSBo9FMi4DZ4TR5Pg=";
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    platforms: HashMap<String, Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    url: String,
    sha256: String,
    size: u64,
}

/// What the UI shows in the update pop-up.
#[derive(Serialize, Clone)]
pub struct Update {
    pub version: String,
    pub current: String,
    pub notes: String,
    pub size: u64,
}

#[derive(Serialize, Clone)]
struct Progress {
    done: u64,
    total: u64,
}

static PENDING: Mutex<Option<Asset>> = Mutex::new(None);

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos-universal"
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        "windows-aarch64"
    } else if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else {
        "unsupported"
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .http_status_as_error(false)
        .user_agent(concat!("Ember/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn fetch(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    if response.status() != 200 {
        return Err(format!("HTTP {}", response.status()));
    }
    response.body_mut().with_config().limit(limit).read_to_vec().map_err(|e| e.to_string())
}

/// Numeric dotted-version comparison ("0.10.0" > "0.9.3").
fn is_newer(candidate: &str, current: &str) -> bool {
    let parse = |v: &str| v.split(['.', '-']).take(3).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    parse(candidate) > parse(current)
}

fn verify(manifest: &[u8], signature: &[u8]) -> Result<(), String> {
    use base64::Engine;
    let key = base64::engine::general_purpose::STANDARD.decode(PUBLIC_KEY).map_err(|e| e.to_string())?;
    let key = VerifyingKey::from_bytes(key.as_slice().try_into().map_err(|_| "bad public key")?).map_err(|e| e.to_string())?;
    let signature = Signature::from_slice(signature).map_err(|_| "Update signature is malformed")?;
    key.verify_strict(manifest, &signature).map_err(|_| "Update signature doesn't match — refusing to install".to_string())
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The newest correctly signed release, if it's newer than `current`.
fn find_update(current: &str) -> Result<Option<(Update, Asset)>, String> {
    let agent = agent();
    let manifest_bytes = fetch(&agent, MANIFEST_URL, 1024 * 1024).map_err(|e| format!("Couldn't check for updates: {e}"))?;
    // GitHub's "latest" links are cached separately and can briefly disagree
    // right after a release, so fetch the signature of exactly this version.
    let version = serde_json::from_slice::<serde_json::Value>(&manifest_bytes)
        .ok()
        .and_then(|m| m["version"].as_str().map(str::to_owned))
        .filter(|v| v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'))
        .ok_or("Update info is unreadable")?;
    let signature_url = format!("{DOWNLOAD_PREFIX}v{version}/latest.json.sig");
    let signature = fetch(&agent, &signature_url, 1024).map_err(|e| format!("Couldn't check for updates: {e}"))?;
    verify(&manifest_bytes, &signature)?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|e| format!("Update info is unreadable: {e}"))?;
    if !is_newer(&manifest.version, current) {
        return Ok(None);
    }
    let Some(asset) = manifest.platforms.get(platform()).cloned() else { return Ok(None) };
    if !asset.url.starts_with(DOWNLOAD_PREFIX) || asset.size > MAX_DOWNLOAD {
        return Err("Update info points somewhere unexpected — refusing to install".into());
    }
    let update = Update { version: manifest.version, current: current.into(), notes: manifest.notes, size: asset.size };
    Ok(Some((update, asset)))
}

/// Downloads an update and checks it against the manifest's size and hash.
fn download(asset: &Asset, progress: impl Fn(u64)) -> Result<Vec<u8>, String> {
    let mut response = agent().get(&asset.url).call().map_err(|e| format!("Download failed: {e}"))?;
    if response.status() != 200 {
        return Err(format!("Download failed (HTTP {}).", response.status()));
    }
    let mut reader = response.body_mut().with_config().limit(MAX_DOWNLOAD).reader();
    let mut bytes = Vec::with_capacity(asset.size as usize);
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        progress(bytes.len() as u64);
    }
    if bytes.len() as u64 != asset.size || hex_sha256(&bytes) != asset.sha256 {
        return Err("The update download was corrupted. Try again.".into());
    }
    Ok(bytes)
}

/// Checks GitHub for a newer, correctly signed Ember for this platform.
#[tauri::command]
pub async fn rekindle_check() -> Result<Option<Update>, String> {
    if cfg!(debug_assertions) {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(|| {
        cleanup_previous_update();
        let found = find_update(env!("CARGO_PKG_VERSION"))?;
        *PENDING.lock().unwrap() = found.as_ref().map(|(_, asset)| asset.clone());
        Ok(found.map(|(update, _)| update))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Downloads, verifies and installs the update found by `rekindle_check`,
/// then restarts Ember. Emits `rekindle-progress` while downloading.
#[tauri::command]
pub async fn rekindle_install(app: AppHandle) -> Result<(), String> {
    let asset = PENDING.lock().unwrap().clone().ok_or("No update to install")?;
    let handle = app.clone();
    let relaunch = tauri::async_runtime::spawn_blocking(move || -> Result<Box<dyn FnOnce() + Send>, String> {
        let bytes = download(&asset, |done| {
            let _ = handle.emit("rekindle-progress", Progress { done, total: asset.size });
        })?;
        install(&bytes)
    })
    .await
    .map_err(|e| e.to_string())??;
    relaunch();
    app.exit(0);
    Ok(())
}

// ---------- platform install ----------

fn not_writable(dir: &Path) -> String {
    format!(
        "Ember can't update itself in {} (no permission). Move Ember to a folder you own, or download the update from emberlauncher.app.",
        dir.display()
    )
}

/// The running Ember.app bundle.
#[cfg(target_os = "macos")]
fn app_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .ok_or_else(|| "Ember isn't running from an app bundle".into())
}

#[cfg(target_os = "macos")]
fn install(zip: &[u8]) -> Result<Box<dyn FnOnce() + Send>, String> {
    let bundle = app_bundle()?;
    install_into(&bundle, zip)?;
    Ok(Box::new(move || {
        let _ = Command::new("/bin/sh").arg("-c").arg("sleep 1; /usr/bin/open \"$0\"").arg(&bundle).spawn();
    }))
}

/// Replaces the app bundle at `bundle` with the one inside the update zip.
#[cfg(target_os = "macos")]
fn install_into(bundle: &Path, zip: &[u8]) -> Result<(), String> {
    let bundle = bundle.to_path_buf();
    let parent = bundle.parent().ok_or("bad bundle path")?.to_path_buf();
    let work = parent.join(format!(".ember-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work).map_err(|_| not_writable(&parent))?;
    let result = (|| {
        let archive = work.join("update.zip");
        fs::write(&archive, zip).map_err(|e| e.to_string())?;
        // ditto keeps permissions and the code signature intact.
        let extracted = work.join("new");
        let ok = Command::new("/usr/bin/ditto").arg("-x").arg("-k").arg(&archive).arg(&extracted).status().map_err(|e| e.to_string())?;
        if !ok.success() {
            return Err("Couldn't unpack the update".to_string());
        }
        let new_app = fs::read_dir(&extracted)
            .map_err(|e| e.to_string())?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e == "app"))
            .ok_or("The update has no app inside")?;
        let verified = Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(&new_app).status().map_err(|e| e.to_string())?;
        if !verified.success() {
            return Err("The update's Apple signature is invalid — refusing to install".to_string());
        }
        let old = work.join("old.app");
        fs::rename(&bundle, &old).map_err(|_| not_writable(&parent))?;
        if let Err(e) = fs::rename(&new_app, &bundle) {
            let _ = fs::rename(&old, &bundle);
            return Err(format!("Couldn't replace Ember: {e}"));
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&work);
    result
}

#[cfg(target_os = "windows")]
fn install(zip: &[u8]) -> Result<Box<dyn FnOnce() + Send>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("bad exe path")?.to_path_buf();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).map_err(|e| e.to_string())?;
    // Unpack everything first, so a bad archive changes nothing.
    let mut files = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        // Entries look like "Ember-windows-x64/Ember.exe"; keep the file name only.
        let name = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(PathBuf::from))
            .ok_or("The update contains an unsafe path")?;
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        files.push((name, data));
    }
    if !files.iter().any(|(n, _)| n.as_os_str().eq_ignore_ascii_case("Ember.exe")) {
        return Err("The update has no Ember.exe inside".into());
    }
    for (name, data) in &files {
        // The new exe always lands next to the running one, under its current name.
        let target = if name.as_os_str().eq_ignore_ascii_case("Ember.exe") { exe.clone() } else { dir.join(name) };
        let staged = target.with_extension("new");
        fs::write(&staged, data).map_err(|_| not_writable(&dir))?;
        if target.exists() {
            // A running exe can't be overwritten, but it can be renamed.
            let old = target.with_extension("old");
            let _ = fs::remove_file(&old);
            fs::rename(&target, &old).map_err(|_| not_writable(&dir))?;
        }
        fs::rename(&staged, &target).map_err(|e| format!("Couldn't replace {}: {e}", name.display()))?;
    }
    Ok(Box::new(move || {
        let _ = Command::new(&exe).current_dir(&dir).spawn();
    }))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn install(_zip: &[u8]) -> Result<Box<dyn FnOnce() + Send>, String> {
    Err("Updates aren't supported on this system yet".into())
}

/// Removes files left behind by the previous update (Windows renames the
/// running exe to `.old` because it can't delete it).
fn cleanup_previous_update() {
    #[cfg(target_os = "windows")]
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            if entry.path().extension().is_some_and(|e| e == "old") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("0.5.0", "0.3.2"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "0.99.0"));
        assert!(!is_newer("0.5.0", "0.5.0"));
        assert!(!is_newer("0.4.9", "0.5.0"));
    }

    #[test]
    fn rejects_tampered_manifests() {
        assert!(verify(b"{\"version\":\"9.9.9\"}", &[0u8; 64]).is_err());
        assert!(verify(b"anything", b"short").is_err());
    }

    /// End-to-end against the real GitHub release: finds the latest signed
    /// update, downloads and verifies it, and installs it over a copy of an
    /// app bundle. Run with REKINDLE_TEST_APP=/path/to/Ember.app.
    #[test]
    #[ignore]
    #[cfg(target_os = "macos")]
    fn updates_a_real_bundle() {
        let source = std::env::var("REKINDLE_TEST_APP").expect("set REKINDLE_TEST_APP");
        let dir = std::env::temp_dir().join(format!("rekindle-e2e-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let bundle = dir.join("Ember.app");
        assert!(Command::new("/usr/bin/ditto").arg(&source).arg(&bundle).status().unwrap().success());
        let (update, asset) = find_update("0.0.1").unwrap().expect("no release found");
        println!("found {} ({} bytes)", update.version, update.size);
        let bytes = download(&asset, |_| {}).unwrap();
        install_into(&bundle, &bytes).unwrap();
        let plist = fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap();
        assert!(plist.contains(&format!("<string>{}</string>", update.version)), "bundle wasn't replaced");
        assert!(Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(&bundle).status().unwrap().success());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Set REKINDLE_MANIFEST and REKINDLE_SIG to a signed pair to check the
    /// release script's output verifies.
    #[test]
    #[ignore]
    fn verifies_release_signature() {
        let manifest = fs::read(std::env::var("REKINDLE_MANIFEST").unwrap()).unwrap();
        let sig = fs::read(std::env::var("REKINDLE_SIG").unwrap()).unwrap();
        verify(&manifest, &sig).unwrap();
    }
}
