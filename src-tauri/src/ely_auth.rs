//! Ely.by account sign-in for online play on Ely.by-enabled servers.
//!
//! Ely.by speaks Mojang's old Yggdrasil auth protocol. The password is sent
//! once to authserver.ely.by and never stored; the access token is kept in the
//! OS credential store. The game runs with authlib-injector so it uses Ely.by
//! for skins and server joins.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const AUTH_URL: &str = "https://authserver.ely.by/auth";
const TEXTURES_URL: &str = "https://skinsystem.ely.by/textures";
const INJECTOR_INDEX_URL: &str = "https://authlib-injector.yushi.moe/artifact/latest.json";
const KEYRING_SERVICE: &str = "app.ember.launcher";
const KEYRING_USER: &str = "elyby-account";
const TWO_FACTOR_MESSAGE: &str = "Account protected with two factor auth.";

/// Stored in the credential store.
#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub uuid: String,
    pub access_token: String,
    pub client_token: String,
    pub skin_url: Option<String>,
}

/// What the UI is allowed to see.
#[derive(Serialize)]
pub struct AccountInfo {
    pub name: String,
    pub uuid: String,
    pub skin_url: Option<String>,
}

impl From<&Account> for AccountInfo {
    fn from(a: &Account) -> Self {
        AccountInfo { name: a.name.clone(), uuid: a.uuid.clone(), skin_url: a.skin_url.clone() }
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .user_agent(concat!("Ember/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// POSTs to the auth server; returns (status, JSON body or `null`).
fn post(agent: &ureq::Agent, action: &str, body: &Value) -> Result<(u16, Value), String> {
    let mut response = agent
        .post(&format!("{AUTH_URL}/{action}"))
        .send_json(body)
        .map_err(|e| format!("Couldn't reach Ely.by: {e}"))?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_json::<Value>().unwrap_or(Value::Null);
    Ok((status, body))
}

/// A random-enough identifier for this launcher install.
fn new_client_token() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let digest = Sha256::digest(format!("ember-{nanos}-{}", std::process::id()));
    digest[..16].iter().map(|b| format!("{b:02x}")).collect()
}

// ---------- credential store ----------

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())
}

fn load() -> Option<Account> {
    serde_json::from_str(&entry().ok()?.get_password().ok()?).ok()
}

fn store(account: &Account) -> Result<(), String> {
    let text = serde_json::to_string(account).map_err(|e| e.to_string())?;
    entry()?.set_password(&text).map_err(|e| format!("Couldn't save the account: {e}"))
}

fn forget() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// The player's skin on Ely.by, as an https URL.
fn skin_url(agent: &ureq::Agent, name: &str) -> Option<String> {
    let mut response = agent.get(&format!("{TEXTURES_URL}/{name}")).call().ok()?;
    if response.status() != 200 {
        return None;
    }
    let body: Value = response.body_mut().read_json().ok()?;
    let url = body["SKIN"]["url"].as_str()?;
    Some(url.replacen("http://", "https://", 1))
}

/// Builds the stored account from an authenticate/refresh response.
fn account_from(agent: &ureq::Agent, body: &Value, client_token: &str) -> Result<Account, String> {
    let profile = &body["selectedProfile"];
    let name = profile["name"].as_str().ok_or("Ely.by returned no player profile")?;
    Ok(Account {
        name: name.to_owned(),
        uuid: profile["id"].as_str().ok_or("Ely.by returned no player id")?.to_owned(),
        access_token: body["accessToken"].as_str().ok_or("Ely.by returned no access token")?.to_owned(),
        client_token: client_token.to_owned(),
        skin_url: skin_url(agent, name),
    })
}

// ---------- commands ----------

/// The signed-in account, if any (no network).
#[tauri::command]
pub fn ely_status() -> Option<AccountInfo> {
    load().as_ref().map(AccountInfo::from)
}

/// Signs in with an Ely.by nickname or email and password. Fails with
/// `"TWO_FACTOR"` when the account needs a code from an authenticator app.
#[tauri::command]
pub async fn ely_login(login: String, password: String, code: Option<String>) -> Result<AccountInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let login = login.trim();
        if login.is_empty() || password.is_empty() {
            return Err("Enter your Ely.by email or nickname and password.".into());
        }
        let code = code.as_deref().map(str::trim).filter(|c| !c.is_empty());
        let password = match code {
            Some(code) => format!("{password}:{code}"),
            None => password,
        };
        let client_token = load().map_or_else(new_client_token, |a| a.client_token);
        let agent = agent();
        let (status, body) = post(
            &agent,
            "authenticate",
            &json!({ "username": login, "password": password, "clientToken": client_token, "requestUser": true }),
        )?;
        if status != 200 {
            let message = body["errorMessage"].as_str().unwrap_or("");
            return Err(if message == TWO_FACTOR_MESSAGE {
                "TWO_FACTOR".into()
            } else if message.starts_with("Invalid credentials") {
                if code.is_some() {
                    "Wrong password or code. Codes change every 30 seconds — try the newest one.".into()
                } else {
                    "Wrong email, nickname or password.".into()
                }
            } else if status == 429 {
                "Too many attempts. Wait a minute and try again.".into()
            } else if message.is_empty() {
                format!("Ely.by sign-in failed (HTTP {status}).")
            } else {
                format!("Ely.by: {message}")
            });
        }
        let account = account_from(&agent, &body, &client_token)?;
        store(&account)?;
        Ok(AccountInfo::from(&account))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Re-reads the player's current skin from Ely.by (it may have changed since
/// sign-in) and returns the updated account.
#[tauri::command]
pub async fn ely_refresh_skin() -> Result<Option<AccountInfo>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(mut account) = load() else { return Ok(None) };
        let skin = skin_url(&agent(), &account.name);
        if skin != account.skin_url {
            account.skin_url = skin;
            store(&account)?;
        }
        Ok(Some(AccountInfo::from(&account)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn ely_logout() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        if let Some(account) = load() {
            // Best effort: the token is forgotten locally either way.
            let _ = post(
                &agent(),
                "invalidate",
                &json!({ "accessToken": account.access_token, "clientToken": account.client_token }),
            );
        }
        forget()
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Opens Ely.by's sign-up page in the default browser.
#[tauri::command]
pub fn ely_open_register() -> Result<(), String> {
    crate::microsoft_auth::open_url("https://account.ely.by/register")
}

/// A signed-in account with a working access token, refreshed if needed.
pub(crate) fn session() -> Result<Account, String> {
    let account = load().ok_or("Sign in with Ely.by to play online.")?;
    let agent = agent();
    let (status, _) = post(&agent, "validate", &json!({ "accessToken": account.access_token }))?;
    if status == 200 {
        return Ok(account);
    }
    let (status, body) = post(
        &agent,
        "refresh",
        &json!({ "accessToken": account.access_token, "clientToken": account.client_token, "requestUser": true }),
    )?;
    if status != 200 {
        let _ = forget();
        return Err("Your Ely.by sign-in expired. Sign in again.".into());
    }
    let account = account_from(&agent, &body, &account.client_token)?;
    store(&account)?;
    Ok(account)
}

// ---------- authlib-injector ----------

fn sha256_file(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Any previously downloaded injector, newest first by file name.
fn cached_injector(dir: &Path) -> Option<PathBuf> {
    let mut jars: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jar"))
        .collect();
    jars.sort();
    jars.pop()
}

/// Path to authlib-injector, downloading the latest release (SHA-256
/// verified) into `<mc>/ember/authlib-injector/` when needed.
pub(crate) fn authlib_injector(mc: &Path) -> Result<PathBuf, String> {
    let dir = mc.join("ember").join("authlib-injector");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let agent = agent();
    let index: Option<Value> = agent
        .get(INJECTOR_INDEX_URL)
        .call()
        .ok()
        .filter(|r| r.status() == 200)
        .and_then(|mut r| r.body_mut().read_json().ok());
    let Some(index) = index else {
        // Offline: fall back to whatever we downloaded before.
        return cached_injector(&dir).ok_or_else(|| "Couldn't download authlib-injector (needed for Ely.by). Check your internet connection.".into());
    };
    let version = index["version"].as_str().ok_or("authlib-injector index has no version")?;
    let url = index["download_url"].as_str().ok_or("authlib-injector index has no download URL")?;
    let sha256 = index["checksums"]["sha256"].as_str().ok_or("authlib-injector index has no checksum")?;
    if !url.starts_with("https://") || version.contains(['/', '\\']) {
        return Err("Unexpected authlib-injector download".into());
    }
    let path = dir.join(format!("authlib-injector-{version}.jar"));
    if sha256_file(&path).as_deref() == Some(sha256) {
        return Ok(path);
    }
    let mut response = agent.get(url).call().map_err(|e| format!("Downloading authlib-injector: {e}"))?;
    if response.status() != 200 {
        return Err(format!("Downloading authlib-injector failed (HTTP {}).", response.status()));
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("Downloading authlib-injector: {e}"))?;
    let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if actual != sha256 {
        return Err("authlib-injector download was corrupted. Try again.".into());
    }
    let part = path.with_extension("jar.part");
    fs::write(&part, &bytes).map_err(|e| e.to_string())?;
    fs::rename(&part, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn client_tokens_are_hex_and_unique() {
        let a = super::new_client_token();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = super::new_client_token();
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    /// Downloads the real injector into a temp folder.
    #[test]
    #[ignore]
    fn downloads_authlib_injector() {
        let dir = std::env::temp_dir().join("ember-injector-test");
        let path = super::authlib_injector(&dir).unwrap();
        assert!(path.exists());
        // Second call reuses the verified file.
        assert_eq!(super::authlib_injector(&dir).unwrap(), path);
    }
}
