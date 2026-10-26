//! A pinned Node runtime, fetched only when Microsoft sign-in needs it.
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, Ordering}, Mutex}, time::Duration};

const VERSION: &str = "v22.23.3"; // Node 22 supports macOS 11.
static INSTALL: Mutex<()> = Mutex::new(());

struct Release {
    asset: &'static str,
    archive_hash: &'static str,
    binary_hash: &'static str,
}

fn release(os: &str, arch: &str) -> Result<Release, String> {
    let (asset, archive_hash, binary_hash) = match (os, arch) {
        ("macos", "aarch64") => ("node-v22.23.3-darwin-arm64.tar.gz", "23b25245dcfb9af7262f8ff142e9e2e0af025368117329e7a7458a51e5922f53", "68f4d07ca49e0500cc135c7e0a445093e228e42e126ac22306d045f0a8c2636b"),
        ("macos", "x86_64") => ("node-v22.23.3-darwin-x64.tar.gz", "8a677b0219178efd6eb0e475457c4afb452b521a92f6e67845a73bd85727f2a8", "fdb8d4da9332c2c99c3b2ae4bad4672bfef225ecd603e656cd1378b3644583a2"),
        ("windows", "x86_64") => ("win-x64/node.exe", "9c9245166b4a8e182e0b797da9c20136117ff24368eaff1fec8343a123c8db0e", "9c9245166b4a8e182e0b797da9c20136117ff24368eaff1fec8343a123c8db0e"),
        ("windows", "aarch64") => ("win-arm64/node.exe", "b5a3165ec6f24c0b1fe2a10fed58e03f6c1d7d141beb3b225ff030ceac6c5eb5", "b5a3165ec6f24c0b1fe2a10fed58e03f6c1d7d141beb3b225ff030ceac6c5eb5"),
        _ => return Err("Microsoft sign-in is supported on macOS and Windows (Intel/AMD and ARM64).".into()),
    };
    Ok(Release { asset, archive_hash, binary_hash })
}

pub(super) fn supported() -> bool { release(std::env::consts::OS, std::env::consts::ARCH).is_ok() }

fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

pub(super) fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) { Err("Sign-in cancelled".into()) } else { Ok(()) }
}

pub(super) fn directory() -> Result<PathBuf, String> {
    let dir = crate::minecraft_java::minecraft_dir().ok_or("Couldn't locate the Minecraft folder")?
        .join("ember/microsoft-auth");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

fn valid_binary(path: &Path, expected: &str) -> bool {
    let Ok(mut file) = fs::File::open(path) else { return false };
    let mut digest = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        match file.read(&mut buf) {
            Ok(0) => return format!("{:x}", digest.finalize()) == expected,
            Ok(n) => digest.update(&buf[..n]),
            Err(_) => return false,
        }
    }
}

pub(super) fn node(cancel: &AtomicBool) -> Result<PathBuf, String> {
    let _lock = INSTALL.lock().unwrap();
    let release = release(std::env::consts::OS, std::env::consts::ARCH)?;
    let dir = directory()?.join(format!("node-{VERSION}-{}", std::env::consts::ARCH));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(if cfg!(windows) { "node.exe" } else { "node" });
    check_cancel(cancel)?;
    if valid_binary(&path, release.binary_hash) { return Ok(path) }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(180)))
        .timeout_recv_body(Some(Duration::from_secs(20)))
        .build().into();
    let mut response = agent.get(&format!("https://nodejs.org/dist/{VERSION}/{}", release.asset))
        .call().map_err(|_| "Couldn't download the Microsoft sign-in helper. Check your internet connection and try again.")?;
    let mut reader = response.body_mut().as_reader();
    let mut bytes = Vec::new();
    let mut chunk = [0; 65536];
    loop {
        check_cancel(cancel)?;
        let n = reader.read(&mut chunk).map_err(|_| "Downloading the sign-in helper failed. Try again.")?;
        if n == 0 { break }
        if bytes.len() + n > 128 * 1024 * 1024 { return Err("The sign-in helper download is too large.".into()) }
        bytes.extend_from_slice(&chunk[..n]);
    }
    if hash(&bytes) != release.archive_hash { return Err("The sign-in helper download failed verification. Try again.".into()) }
    let binary = if release.asset.ends_with(".tar.gz") {
        let name = format!("{}/bin/node", release.asset.trim_end_matches(".tar.gz"));
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes.as_slice()));
        let mut binary = None;
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            if entry.path().map_err(|e| e.to_string())?.as_ref() == Path::new(&name) && entry.header().entry_type().is_file() {
                let mut out = Vec::new();
                (&mut entry).take(128 * 1024 * 1024).read_to_end(&mut out).map_err(|e| e.to_string())?;
                binary = Some(out);
                break;
            }
        }
        binary.ok_or("The sign-in helper archive has no Node executable")?
    } else { bytes };
    if hash(&binary) != release.binary_hash { return Err("The sign-in helper executable failed verification.".into()) }
    check_cancel(cancel)?;
    let part = path.with_extension("part");
    fs::write(&part, binary).map_err(|e| e.to_string())?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&part, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    }
    fs::rename(&part, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub(super) fn helper(source: &str) -> Result<PathBuf, String> {
    let _lock = INSTALL.lock().unwrap();
    let path = directory()?.join(format!("helper-{}.cjs", hash(source.as_bytes())));
    if fs::read(&path).ok().as_deref() != Some(source.as_bytes()) {
        let part = path.with_extension("part");
        fs::write(&part, source).map_err(|e| e.to_string())?;
        fs::rename(part, &path).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_all_distributed_platforms() {
        for os in ["macos", "windows"] { for arch in ["aarch64", "x86_64"] {
            let r = release(os, arch).unwrap();
            assert_eq!(r.binary_hash.len(), 64);
            assert_eq!(r.archive_hash.len(), 64);
        }}
        assert!(release("linux", "x86_64").is_err());
    }
    #[test]
    #[ignore = "downloads a verified Node runtime from nodejs.org"]
    fn downloads_and_reuses_node() {
        let cancel = AtomicBool::new(false);
        let path = node(&cancel).unwrap();
        assert_eq!(node(&cancel).unwrap(), path);
        let output = std::process::Command::new(path).arg("--version").output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), VERSION);
    }
}
