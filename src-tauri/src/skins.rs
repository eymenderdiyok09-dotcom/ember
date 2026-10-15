//! Custom skins for offline play.
//!
//! The game only shows skins it gets from a session server, so for offline
//! players Ember runs a tiny Yggdrasil-compatible server on 127.0.0.1 and
//! points the game at it with authlib-injector (the approach HMCL uses). The
//! server answers only for the local player and signs the textures with a key
//! generated on this computer, whose public half it hands to authlib-injector.
//! Only the local player sees the skin; other players don't.

use base64::Engine;
use rsa::pkcs1v15::SigningKey;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey, EncodePublicKey, LineEnding};
use rsa::signature::{SignatureEncoding, Signer};
use rsa::RsaPrivateKey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::minecraft_java::minecraft_dir;

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// A saved offline skin.
#[derive(Serialize, Deserialize, Clone)]
struct SavedSkin {
    /// SHA-256 of the PNG; also its file name.
    hash: String,
    slim: bool,
}

/// What the UI sees.
#[derive(Serialize)]
pub struct SkinInfo {
    /// `data:image/png;base64,…` for previews.
    pub data_url: String,
    pub slim: bool,
}

fn skins_dir() -> Result<PathBuf, String> {
    Ok(minecraft_dir().ok_or("could not resolve .minecraft directory")?.join("ember").join("skins"))
}

fn index_path(dir: &Path) -> PathBuf {
    dir.join("offline.json")
}

fn load_index(dir: &Path) -> HashMap<String, SavedSkin> {
    fs::read_to_string(index_path(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_index(dir: &Path, index: &HashMap<String, SavedSkin>) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(index).map_err(|e| e.to_string())?;
    fs::write(index_path(dir), text).map_err(|e| e.to_string())
}

fn key_for(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

/// Checks the PNG signature and that it's a Minecraft skin size (64×64 or 64×32).
fn validate_skin(png: &[u8]) -> Result<(), String> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if png.len() < 24 || &png[..8] != SIGNATURE || &png[12..16] != b"IHDR" {
        return Err("That isn't a PNG image.".into());
    }
    let width = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(png[20..24].try_into().unwrap());
    if width != 64 || (height != 64 && height != 32) {
        return Err(format!("Skins are 64×64 (or old-style 64×32) pixels; this image is {width}×{height}."));
    }
    if png.len() > 256 * 1024 {
        return Err("That image is too large for a skin.".into());
    }
    Ok(())
}

fn info(dir: &Path, saved: &SavedSkin) -> Option<SkinInfo> {
    let png = fs::read(dir.join(format!("{}.png", saved.hash))).ok()?;
    Some(SkinInfo { data_url: format!("data:image/png;base64,{}", B64.encode(png)), slim: saved.slim })
}

/// The offline skin saved for `name`, if any.
#[tauri::command]
pub fn skin_get_offline(name: String) -> Option<SkinInfo> {
    let dir = skins_dir().ok()?;
    let index = load_index(&dir);
    info(&dir, index.get(&key_for(&name))?)
}

/// Saves a skin (PNG as base64) for the offline player `name`.
#[tauri::command]
pub fn skin_set_offline(name: String, png_base64: String, slim: bool) -> Result<SkinInfo, String> {
    if key_for(&name).is_empty() {
        return Err("Choose a username first.".into());
    }
    let png = B64.decode(png_base64.trim()).map_err(|_| "That image couldn't be read.")?;
    validate_skin(&png)?;
    let dir = skins_dir()?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let hash: String = Sha256::digest(&png).iter().map(|b| format!("{b:02x}")).collect();
    fs::write(dir.join(format!("{hash}.png")), &png).map_err(|e| e.to_string())?;
    let mut index = load_index(&dir);
    let saved = SavedSkin { hash, slim };
    index.insert(key_for(&name), saved.clone());
    save_index(&dir, &index)?;
    info(&dir, &saved).ok_or_else(|| "Couldn't save the skin.".into())
}

/// Switches a saved skin between the classic (Steve) and slim (Alex) arms.
#[tauri::command]
pub fn skin_set_offline_model(name: String, slim: bool) -> Result<(), String> {
    let dir = skins_dir()?;
    let mut index = load_index(&dir);
    let saved = index.get_mut(&key_for(&name)).ok_or("No skin saved for this name.")?;
    saved.slim = slim;
    save_index(&dir, &index)
}

/// Goes back to the default skin for `name`.
#[tauri::command]
pub fn skin_clear_offline(name: String) -> Result<(), String> {
    let dir = skins_dir()?;
    let mut index = load_index(&dir);
    if index.remove(&key_for(&name)).is_some() {
        save_index(&dir, &index)?;
    }
    Ok(())
}

/// Opens Ely.by's skin catalogue (Ely.by has no API to change skins from apps).
#[tauri::command]
pub fn ely_open_skins() -> Result<(), String> {
    crate::microsoft_auth::open_url("https://ely.by/skins")
}

// ---------- local skin server ----------

/// The player the server answers for.
#[derive(Clone)]
struct Player {
    name: String,
    /// Undashed lowercase UUID.
    uuid: String,
    hash: String,
    slim: bool,
}

struct Server {
    port: u16,
    key: SigningKey<sha1::Sha1>,
    public_pem: String,
    skins: PathBuf,
}

static SERVER: OnceLock<Server> = OnceLock::new();
static PLAYER: Mutex<Option<Player>> = Mutex::new(None);

/// This computer's texture-signing key, created on first use.
fn signing_key(dir: &Path) -> Result<RsaPrivateKey, String> {
    let path = dir.join("signing-key.pem");
    if let Some(key) = fs::read_to_string(&path).ok().and_then(|pem| RsaPrivateKey::from_pkcs8_pem(&pem).ok()) {
        return Ok(key);
    }
    let key = RsaPrivateKey::new(&mut rand::thread_rng(), 2048).map_err(|e| e.to_string())?;
    let pem = key.to_pkcs8_pem(LineEnding::LF).map_err(|e| e.to_string())?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    fs::write(&path, pem.as_bytes()).map_err(|e| e.to_string())?;
    Ok(key)
}

fn start_server(skins: PathBuf) -> Result<&'static Server, String> {
    if let Some(server) = SERVER.get() {
        return Ok(server);
    }
    let private = signing_key(&skins)?;
    let public_pem = private.to_public_key().to_public_key_pem(LineEnding::LF).map_err(|e| e.to_string())?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Skin server: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let server = SERVER.get_or_init(|| Server { port, key: SigningKey::new(private), public_pem, skins });
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || {
                let _ = handle(stream);
            });
        }
    });
    Ok(server)
}

/// The `-javaagent` argument that gives offline player `name` their saved skin,
/// or `None` when they have no skin (or the skin system can't start).
pub(crate) fn offline_skin_agent(mc: &Path, name: &str, uuid: &str) -> Option<String> {
    let skins = mc.join("ember").join("skins");
    let saved = load_index(&skins).get(&key_for(name))?.clone();
    if !skins.join(format!("{}.png", saved.hash)).is_file() {
        return None;
    }
    let injector = crate::ely_auth::authlib_injector(mc).ok()?;
    let server = start_server(skins).ok()?;
    *PLAYER.lock().unwrap() = Some(Player {
        name: name.to_owned(),
        uuid: uuid.replace('-', "").to_ascii_lowercase(),
        hash: saved.hash,
        slim: saved.slim,
    });
    Some(format!("-javaagent:{}=http://127.0.0.1:{}", injector.display(), server.port))
}

fn texture_url(server: &Server, hash: &str) -> String {
    format!("http://127.0.0.1:{}/textures/{hash}", server.port)
}

/// Mojang-style profile with a signed `textures` property.
fn profile_json(server: &Server, player: &Player) -> Value {
    let mut skin = json!({ "url": texture_url(server, &player.hash) });
    if player.slim {
        skin["metadata"] = json!({ "model": "slim" });
    }
    let textures = json!({
        "timestamp": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64),
        "profileId": player.uuid,
        "profileName": player.name,
        "textures": { "SKIN": skin },
    });
    let value = B64.encode(textures.to_string());
    let signature = B64.encode(server.key.sign(value.as_bytes()).to_bytes());
    json!({
        "id": player.uuid,
        "name": player.name,
        "properties": [{ "name": "textures", "value": value, "signature": signature }],
    })
}

struct Response {
    status: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Response {
    fn json(value: &Value) -> Self {
        Response { status: "200 OK", content_type: "application/json; charset=utf-8", body: value.to_string().into_bytes() }
    }
    fn empty() -> Self {
        Response { status: "204 No Content", content_type: "text/plain", body: Vec::new() }
    }
    fn not_found() -> Self {
        Response { status: "404 Not Found", content_type: "text/plain", body: Vec::new() }
    }
}

fn route(method: &str, path: &str, body: &[u8]) -> Response {
    let Some(server) = SERVER.get() else { return Response::not_found() };
    let player = PLAYER.lock().unwrap().clone();
    let path = path.split('?').next().unwrap_or("");
    match (method, path) {
        ("GET", "/") => Response::json(&json!({
            "meta": {
                "serverName": "Ember",
                "implementationName": "Ember offline skins",
                "implementationVersion": env!("CARGO_PKG_VERSION"),
            },
            "skinDomains": ["127.0.0.1"],
            "signaturePublickey": server.public_pem,
        })),
        ("GET", p) if p.starts_with("/sessionserver/session/minecraft/profile/") => {
            let uuid = p.rsplit('/').next().unwrap_or("").replace('-', "").to_ascii_lowercase();
            match player.filter(|pl| pl.uuid == uuid) {
                Some(pl) => Response::json(&profile_json(server, &pl)),
                None => Response::empty(),
            }
        }
        ("GET", p) if p.starts_with("/api/users/profiles/minecraft/") => {
            let name = p.rsplit('/').next().unwrap_or("");
            match player.filter(|pl| pl.name.eq_ignore_ascii_case(name)) {
                Some(pl) => Response::json(&json!({ "id": pl.uuid, "name": pl.name })),
                None => Response::empty(),
            }
        }
        ("POST", "/api/profiles/minecraft") => {
            let names: Vec<String> = serde_json::from_slice(body).unwrap_or_default();
            let found: Vec<Value> = player
                .filter(|pl| names.iter().any(|n| n.eq_ignore_ascii_case(&pl.name)))
                .map(|pl| json!({ "id": pl.uuid, "name": pl.name }))
                .into_iter()
                .collect();
            Response::json(&Value::from(found))
        }
        ("GET", p) if p.starts_with("/textures/") => {
            let hash = &p["/textures/".len()..];
            if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Response::not_found();
            }
            match fs::read(server.skins.join(format!("{hash}.png"))) {
                Ok(png) => Response { status: "200 OK", content_type: "image/png", body: png },
                Err(_) => Response::not_found(),
            }
        }
        // Joining online-mode servers can't work for offline players; say so politely.
        ("POST", "/sessionserver/session/minecraft/join") | ("GET", "/sessionserver/session/minecraft/hasJoined") => Response::empty(),
        _ => Response::not_found(),
    }
}

fn handle(stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or("").to_owned());
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0).min(64 * 1024);
            }
        }
    }
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    let response = route(method, &path, &body);
    if std::env::var_os("EMBER_SKIN_DEBUG").is_some() {
        eprintln!("[skin server] {method} {path} -> {}", response.status);
    }
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )?;
    stream.write_all(&response.body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 6, 0, 0, 0]);
        bytes
    }

    #[test]
    fn validates_skin_sizes() {
        assert!(validate_skin(&png(64, 64)).is_ok());
        assert!(validate_skin(&png(64, 32)).is_ok());
        assert!(validate_skin(&png(128, 128)).is_err());
        assert!(validate_skin(b"GIF89a not a png at all").is_err());
    }

    /// Starts the real server and checks the signed profile verifies with the
    /// public key it advertises.
    #[test]
    fn serves_signed_profile() {
        use rsa::pkcs1v15::{Signature, VerifyingKey};
        use rsa::pkcs8::DecodePublicKey;
        use rsa::signature::Verifier;

        let dir = std::env::temp_dir().join(format!("ember-skins-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let server = start_server(dir.clone()).unwrap();
        *PLAYER.lock().unwrap() = Some(Player { name: "Steve".into(), uuid: "abc123".into(), hash: "f".repeat(64), slim: true });

        let get = |path: &str| -> (String, Vec<u8>) {
            let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
            let mut out = Vec::new();
            s.read_to_end(&mut out).unwrap();
            let split = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
            (String::from_utf8_lossy(&out[..split]).into_owned(), out[split + 4..].to_vec())
        };

        let (head, body) = get("/");
        assert!(head.starts_with("HTTP/1.1 200"));
        let meta: Value = serde_json::from_slice(&body).unwrap();
        let public = rsa::RsaPublicKey::from_public_key_pem(meta["signaturePublickey"].as_str().unwrap()).unwrap();

        let (head, body) = get("/sessionserver/session/minecraft/profile/abc123?unsigned=false");
        assert!(head.starts_with("HTTP/1.1 200"));
        let profile: Value = serde_json::from_slice(&body).unwrap();
        let prop = &profile["properties"][0];
        let value = prop["value"].as_str().unwrap();
        let signature = Signature::try_from(B64.decode(prop["signature"].as_str().unwrap()).unwrap().as_slice()).unwrap();
        VerifyingKey::<sha1::Sha1>::new(public).verify(value.as_bytes(), &signature).unwrap();
        let textures: Value = serde_json::from_slice(&B64.decode(value).unwrap()).unwrap();
        assert_eq!(textures["textures"]["SKIN"]["metadata"]["model"], "slim");

        let (head, _) = get("/sessionserver/session/minecraft/profile/someoneelse");
        assert!(head.starts_with("HTTP/1.1 204"));
        let _ = fs::remove_dir_all(&dir);
    }
}
